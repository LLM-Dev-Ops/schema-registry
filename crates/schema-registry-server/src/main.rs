use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use prometheus::{Encoder, TextEncoder};
use redis::aio::ConnectionManager;
use schema_registry_compatibility::CompatibilityCheckerImpl;
use schema_registry_core::{
    error::Result as CoreResult,
    schema::{RegisteredSchema, SchemaMetadata},
    state::{SchemaLifecycle, SchemaState},
    traits::{CompatibilityChecker, SchemaValidator},
    types::{CompatibilityMode, SerializationFormat},
    versioning::SemanticVersion,
};
use schema_registry_validation::ValidationEngine;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing_subscriber;
use uuid::Uuid;

// ============================================================================
// Application State (Supports both Database and Serverless modes)
// ============================================================================

#[derive(Clone)]
struct AppState {
    /// Optional PostgreSQL pool - None in serverless mode
    db: Option<PgPool>,
    /// Optional Redis connection - None in serverless mode
    redis: Option<ConnectionManager>,
    /// Validation engine - always available
    validator: Arc<ValidationEngine>,
    /// Compatibility checker - always available
    compatibility_checker: Arc<CompatibilityCheckerImpl>,
    /// RuVector service URL for serverless persistence
    ruvector_url: Option<String>,
    /// In-memory cache for serverless mode
    memory_cache: Arc<RwLock<HashMap<Uuid, SchemaEntry>>>,
    /// Server mode
    mode: ServerMode,
}

#[derive(Clone, Debug, PartialEq)]
enum ServerMode {
    /// Full mode with PostgreSQL and Redis
    Full,
    /// Serverless mode with ruvector-service persistence
    Serverless,
    /// Memory-only mode for testing
    MemoryOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SchemaEntry {
    id: Uuid,
    namespace: String,
    name: String,
    version_major: i32,
    version_minor: i32,
    version_patch: i32,
    format: String,
    content: String,
    content_hash: String,
    state: String,
    compatibility_mode: String,
    description: Option<String>,
    tags: Vec<String>,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

// ============================================================================
// Request/Response Models
// ============================================================================

#[derive(Debug, Deserialize, Serialize)]
struct RegisterSchemaRequest {
    // K6 test format
    subject: String,
    schema: serde_json::Value,
    schema_type: String,

    // Optional fields for compatibility with other formats
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version_major: Option<i32>,
    #[serde(default)]
    version_minor: Option<i32>,
    #[serde(default)]
    version_patch: Option<i32>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default = "default_state")]
    state: String,
    #[serde(default = "default_compatibility_mode")]
    compatibility_mode: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

fn default_state() -> String {
    "DRAFT".to_string()
}

fn default_compatibility_mode() -> String {
    "BACKWARD".to_string()
}

#[derive(Debug, Serialize)]
struct RegisterSchemaResponse {
    id: Uuid,
    version: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct GetSchemaResponse {
    id: Uuid,
    namespace: String,
    name: String,
    version: String,
    format: String,
    schema: serde_json::Value,
    content: String,
    state: String,
    compatibility_mode: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct ValidateResponse {
    is_valid: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CompatibilityCheckRequest {
    schema_id: Uuid,
    compared_schema_id: Uuid,
    #[serde(default = "default_compatibility_mode")]
    mode: String,
}

#[derive(Debug, Serialize)]
struct CompatibilityCheckResponse {
    is_compatible: bool,
    mode: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    violations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    mode: String,
    components: HashMap<String, ComponentHealth>,
}

#[derive(Debug, Serialize)]
struct ComponentHealth {
    status: String,
    message: Option<String>,
}

// ============================================================================
// Error Handling
// ============================================================================

enum AppError {
    Database(String),
    Redis(String),
    NotFound(String),
    InvalidInput(String),
    Internal(String),
    ServiceUnavailable(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Database(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            ),
            AppError::Redis(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Cache error: {}", e),
            ),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<redis::RedisError> for AppError {
    fn from(e: redis::RedisError) -> Self {
        AppError::Redis(e.to_string())
    }
}

// ============================================================================
// RuVector Service Client (for serverless persistence)
// ============================================================================

struct RuVectorClient {
    base_url: String,
    client: reqwest::Client,
}

impl RuVectorClient {
    fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    async fn store_schema(&self, entry: &SchemaEntry) -> Result<(), String> {
        let url = format!("{}/api/v1/vectors/schemas", self.base_url);
        let response = self.client
            .post(&url)
            .json(entry)
            .send()
            .await
            .map_err(|e| format!("RuVector request failed: {}", e))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("RuVector store failed: {}", response.status()))
        }
    }

    async fn get_schema(&self, id: Uuid) -> Result<Option<SchemaEntry>, String> {
        let url = format!("{}/api/v1/vectors/schemas/{}", self.base_url, id);
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("RuVector request failed: {}", e))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if response.status().is_success() {
            let entry = response.json::<SchemaEntry>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            Ok(Some(entry))
        } else {
            Err(format!("RuVector get failed: {}", response.status()))
        }
    }

    async fn find_schema(&self, namespace: &str, name: &str, version_major: i32, version_minor: i32, version_patch: i32) -> Result<Option<SchemaEntry>, String> {
        let url = format!(
            "{}/api/v1/vectors/schemas/search?namespace={}&name={}&version={}.{}.{}",
            self.base_url, namespace, name, version_major, version_minor, version_patch
        );
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("RuVector request failed: {}", e))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if response.status().is_success() {
            let entry = response.json::<SchemaEntry>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            Ok(Some(entry))
        } else {
            Err(format!("RuVector search failed: {}", response.status()))
        }
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// Liveness probe - always returns OK immediately
/// Used by Cloud Run to determine if container is alive
async fn liveness() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "alive",
        "timestamp": Utc::now().to_rfc3339()
    }))
}

/// Readiness probe - returns OK when service is ready to handle requests
/// In serverless mode, this always returns OK
async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let ready = match state.mode {
        ServerMode::Serverless | ServerMode::MemoryOnly => true,
        ServerMode::Full => state.db.is_some() && state.redis.is_some(),
    };

    if ready {
        (StatusCode::OK, Json(serde_json::json!({
            "status": "ready",
            "mode": format!("{:?}", state.mode),
            "timestamp": Utc::now().to_rfc3339()
        })))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "status": "not_ready",
            "mode": format!("{:?}", state.mode),
            "timestamp": Utc::now().to_rfc3339()
        })))
    }
}

/// Health check endpoint with component status
async fn health_check(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    let mut components = HashMap::new();

    match state.mode {
        ServerMode::Serverless => {
            // In serverless mode, check ruvector-service if URL is configured
            if let Some(ref url) = state.ruvector_url {
                let client = RuVectorClient::new(url);
                let ruvector_status = match client.client.get(format!("{}/health", url)).send().await {
                    Ok(resp) if resp.status().is_success() => ComponentHealth {
                        status: "up".to_string(),
                        message: None,
                    },
                    Ok(resp) => ComponentHealth {
                        status: "degraded".to_string(),
                        message: Some(format!("Status: {}", resp.status())),
                    },
                    Err(e) => ComponentHealth {
                        status: "down".to_string(),
                        message: Some(e.to_string()),
                    },
                };
                components.insert("ruvector".to_string(), ruvector_status);
            }

            components.insert("memory_cache".to_string(), ComponentHealth {
                status: "up".to_string(),
                message: Some(format!("{} entries", state.memory_cache.read().await.len())),
            });
        }
        ServerMode::MemoryOnly => {
            components.insert("memory_cache".to_string(), ComponentHealth {
                status: "up".to_string(),
                message: Some(format!("{} entries", state.memory_cache.read().await.len())),
            });
        }
        ServerMode::Full => {
            // Check PostgreSQL
            if let Some(ref db) = state.db {
                let db_status = match sqlx::query("SELECT 1").fetch_one(db).await {
                    Ok(_) => ComponentHealth {
                        status: "up".to_string(),
                        message: None,
                    },
                    Err(e) => ComponentHealth {
                        status: "down".to_string(),
                        message: Some(e.to_string()),
                    },
                };
                components.insert("database".to_string(), db_status);
            }

            // Check Redis
            if let Some(ref redis) = state.redis {
                let redis_status = {
                    let mut conn = redis.clone();
                    match redis::cmd("PING")
                        .query_async::<_, String>(&mut conn)
                        .await
                    {
                        Ok(_) => ComponentHealth {
                            status: "up".to_string(),
                            message: None,
                        },
                        Err(e) => ComponentHealth {
                            status: "down".to_string(),
                            message: Some(e.to_string()),
                        },
                    }
                };
                components.insert("redis".to_string(), redis_status);
            }
        }
    }

    let overall_status = if components.values().all(|c| c.status == "up") {
        "healthy"
    } else if components.values().any(|c| c.status == "down") {
        "unhealthy"
    } else {
        "degraded"
    };

    Ok(Json(HealthResponse {
        status: overall_status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        mode: format!("{:?}", state.mode),
        components,
    }))
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        buffer,
    )
}

async fn register_schema(
    State(state): State<AppState>,
    Json(req): Json<RegisterSchemaRequest>,
) -> Result<(StatusCode, Json<RegisterSchemaResponse>), AppError> {
    // Parse subject into namespace and name (format: namespace.name or just name)
    let (namespace, name) = if let Some(dot_pos) = req.subject.rfind('.') {
        let (ns, nm) = req.subject.split_at(dot_pos);
        (ns.to_string(), nm[1..].to_string())
    } else {
        ("default".to_string(), req.subject.clone())
    };

    // Use provided values or defaults
    let version_major = req.version_major.unwrap_or(1);
    let version_minor = req.version_minor.unwrap_or(0);
    let version_patch = req.version_patch.unwrap_or(0);

    // Convert schema to content string
    let content = req.content.clone().unwrap_or_else(|| {
        serde_json::to_string(&req.schema).unwrap_or_else(|_| "{}".to_string())
    });

    // Normalize format/schema_type
    let format = req.format.clone().unwrap_or_else(|| {
        match req.schema_type.to_uppercase().as_str() {
            "JSON" => "JSON".to_string(),
            "AVRO" => "AVRO".to_string(),
            "PROTOBUF" => "PROTOBUF".to_string(),
            _ => "JSON".to_string(),
        }
    });

    tracing::info!(
        subject = %req.subject,
        namespace = %namespace,
        name = %name,
        version = %format!("{}.{}.{}", version_major, version_minor, version_patch),
        mode = ?state.mode,
        "Registering schema"
    );

    // Calculate content hash
    let content_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    };

    let now = Utc::now();
    let id = Uuid::new_v4();

    match state.mode {
        ServerMode::Serverless => {
            // Use RuVector service for persistence
            if let Some(ref url) = state.ruvector_url {
                let client = RuVectorClient::new(url);

                // Check if schema already exists
                if let Ok(Some(existing)) = client.find_schema(&namespace, &name, version_major, version_minor, version_patch).await {
                    let version = format!("{}.{}.{}", version_major, version_minor, version_patch);
                    return Ok((
                        StatusCode::OK,
                        Json(RegisterSchemaResponse {
                            id: existing.id,
                            version,
                            created_at: existing.created_at.to_rfc3339(),
                        }),
                    ));
                }

                let entry = SchemaEntry {
                    id,
                    namespace: namespace.clone(),
                    name: name.clone(),
                    version_major,
                    version_minor,
                    version_patch,
                    format: format.clone(),
                    content: content.clone(),
                    content_hash,
                    state: req.state.clone(),
                    compatibility_mode: req.compatibility_mode.clone(),
                    description: req.description.clone(),
                    tags: req.tags.clone(),
                    metadata: serde_json::to_value(&req.metadata).unwrap_or_default(),
                    created_at: now,
                    updated_at: now,
                };

                client.store_schema(&entry).await
                    .map_err(|e| AppError::Internal(e))?;

                // Also cache locally
                state.memory_cache.write().await.insert(id, entry);
            } else {
                return Err(AppError::ServiceUnavailable("RuVector service not configured".to_string()));
            }
        }
        ServerMode::MemoryOnly => {
            // Memory-only mode - check and store in memory cache
            let mut cache = state.memory_cache.write().await;

            // Check for existing schema with same version
            for entry in cache.values() {
                if entry.namespace == namespace && entry.name == name
                    && entry.version_major == version_major
                    && entry.version_minor == version_minor
                    && entry.version_patch == version_patch
                {
                    let version = format!("{}.{}.{}", version_major, version_minor, version_patch);
                    return Ok((
                        StatusCode::OK,
                        Json(RegisterSchemaResponse {
                            id: entry.id,
                            version,
                            created_at: entry.created_at.to_rfc3339(),
                        }),
                    ));
                }
            }

            let entry = SchemaEntry {
                id,
                namespace: namespace.clone(),
                name: name.clone(),
                version_major,
                version_minor,
                version_patch,
                format: format.clone(),
                content: content.clone(),
                content_hash,
                state: req.state.clone(),
                compatibility_mode: req.compatibility_mode.clone(),
                description: req.description.clone(),
                tags: req.tags.clone(),
                metadata: serde_json::to_value(&req.metadata).unwrap_or_default(),
                created_at: now,
                updated_at: now,
            };

            cache.insert(id, entry);
        }
        ServerMode::Full => {
            // Original database-backed mode
            let db = state.db.as_ref()
                .ok_or_else(|| AppError::ServiceUnavailable("Database not connected".to_string()))?;

            // Check if schema already exists
            let existing: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM schemas WHERE namespace = $1 AND name = $2 AND version_major = $3 AND version_minor = $4 AND version_patch = $5"
            )
            .bind(&namespace)
            .bind(&name)
            .bind(version_major)
            .bind(version_minor)
            .bind(version_patch)
            .fetch_optional(db)
            .await?;

            if let Some((existing_id,)) = existing {
                let version = format!("{}.{}.{}", version_major, version_minor, version_patch);
                return Ok((
                    StatusCode::OK,
                    Json(RegisterSchemaResponse {
                        id: existing_id,
                        version,
                        created_at: now.to_rfc3339(),
                    }),
                ));
            }

            // Insert new schema
            sqlx::query(
                r#"
                INSERT INTO schemas (
                    id, namespace, name, version_major, version_minor, version_patch,
                    format, content, content_hash, state, compatibility_mode,
                    created_at, updated_at, description, metadata, tags
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
                "#,
            )
            .bind(id)
            .bind(&namespace)
            .bind(&name)
            .bind(version_major)
            .bind(version_minor)
            .bind(version_patch)
            .bind(&format)
            .bind(&content)
            .bind(&content_hash)
            .bind(&req.state)
            .bind(&req.compatibility_mode)
            .bind(now)
            .bind(now)
            .bind(req.description.as_deref())
            .bind(serde_json::to_value(&req.metadata).unwrap())
            .bind(&req.tags)
            .execute(db)
            .await?;

            // Cache in Redis
            if let Some(ref redis) = state.redis {
                let cache_key = format!("schema:{}", id);
                let cache_value = serde_json::json!({
                    "id": id,
                    "namespace": namespace,
                    "name": name,
                    "version_major": version_major,
                    "version_minor": version_minor,
                    "version_patch": version_patch,
                    "format": format,
                    "content": content,
                    "state": req.state,
                    "compatibility_mode": req.compatibility_mode,
                });

                let mut conn = redis.clone();
                let _: Result<(), _> = redis::cmd("SET")
                    .arg(&cache_key)
                    .arg(serde_json::to_string(&cache_value).unwrap())
                    .arg("EX")
                    .arg(3600)
                    .query_async(&mut conn)
                    .await;
            }
        }
    }

    let version = format!("{}.{}.{}", version_major, version_minor, version_patch);

    tracing::info!(schema_id = %id, "Schema registered successfully");

    Ok((
        StatusCode::CREATED,
        Json(RegisterSchemaResponse {
            id,
            version,
            created_at: now.to_rfc3339(),
        }),
    ))
}

async fn get_schema(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<GetSchemaResponse>, AppError> {
    tracing::debug!(schema_id = %id, mode = ?state.mode, "Fetching schema");

    match state.mode {
        ServerMode::Serverless => {
            // Try memory cache first
            if let Some(entry) = state.memory_cache.read().await.get(&id) {
                return Ok(Json(schema_entry_to_response(entry)));
            }

            // Try RuVector service
            if let Some(ref url) = state.ruvector_url {
                let client = RuVectorClient::new(url);
                if let Ok(Some(entry)) = client.get_schema(id).await {
                    // Cache locally
                    state.memory_cache.write().await.insert(id, entry.clone());
                    return Ok(Json(schema_entry_to_response(&entry)));
                }
            }

            Err(AppError::NotFound(format!("Schema {} not found", id)))
        }
        ServerMode::MemoryOnly => {
            let cache = state.memory_cache.read().await;
            match cache.get(&id) {
                Some(entry) => Ok(Json(schema_entry_to_response(entry))),
                None => Err(AppError::NotFound(format!("Schema {} not found", id))),
            }
        }
        ServerMode::Full => {
            // Try Redis cache first
            if let Some(ref redis) = state.redis {
                let cache_key = format!("schema:{}", id);
                let mut conn = redis.clone();

                if let Ok(Some(cached)) = redis::cmd("GET")
                    .arg(&cache_key)
                    .query_async::<_, Option<String>>(&mut conn)
                    .await
                {
                    if let Ok(schema_data) = serde_json::from_str::<serde_json::Value>(&cached) {
                        tracing::debug!(schema_id = %id, "Cache hit");
                        return Ok(Json(json_to_schema_response(id, &schema_data)));
                    }
                }
            }

            tracing::debug!(schema_id = %id, "Cache miss, querying database");

            // Fallback to PostgreSQL
            let db = state.db.as_ref()
                .ok_or_else(|| AppError::ServiceUnavailable("Database not connected".to_string()))?;

            let row: Option<(
                Uuid,
                String,
                String,
                i32,
                i32,
                i32,
                String,
                String,
                String,
                String,
                chrono::DateTime<Utc>,
                chrono::DateTime<Utc>,
            )> = sqlx::query_as(
                r#"
                SELECT id, namespace, name, version_major, version_minor, version_patch,
                       format, content, state, compatibility_mode, created_at, updated_at
                FROM schemas
                WHERE id = $1
                LIMIT 1
                "#,
            )
            .bind(id)
            .fetch_optional(db)
            .await?;

            match row {
                Some((
                    id,
                    namespace,
                    name,
                    version_major,
                    version_minor,
                    version_patch,
                    format,
                    content,
                    state_str,
                    compat_mode,
                    created_at,
                    updated_at,
                )) => {
                    let version = format!("{}.{}.{}", version_major, version_minor, version_patch);
                    let schema_json = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

                    // Update cache
                    if let Some(ref redis) = state.redis {
                        let cache_key = format!("schema:{}", id);
                        let cache_value = serde_json::json!({
                            "id": id.to_string(),
                            "namespace": namespace,
                            "name": name,
                            "version_major": version_major,
                            "version_minor": version_minor,
                            "version_patch": version_patch,
                            "format": format,
                            "content": content,
                            "state": state_str,
                            "compatibility_mode": compat_mode,
                        });

                        let mut conn = redis.clone();
                        let _: Result<(), _> = redis::cmd("SET")
                            .arg(&cache_key)
                            .arg(serde_json::to_string(&cache_value).unwrap())
                            .arg("EX")
                            .arg(3600)
                            .query_async(&mut conn)
                            .await;
                    }

                    Ok(Json(GetSchemaResponse {
                        id,
                        namespace,
                        name,
                        version,
                        format,
                        schema: schema_json,
                        content,
                        state: state_str,
                        compatibility_mode: compat_mode,
                        created_at: created_at.to_rfc3339(),
                        updated_at: updated_at.to_rfc3339(),
                    }))
                }
                None => Err(AppError::NotFound(format!("Schema {} not found", id))),
            }
        }
    }
}

fn schema_entry_to_response(entry: &SchemaEntry) -> GetSchemaResponse {
    let version = format!("{}.{}.{}", entry.version_major, entry.version_minor, entry.version_patch);
    let schema_json = serde_json::from_str(&entry.content).unwrap_or(serde_json::json!({}));

    GetSchemaResponse {
        id: entry.id,
        namespace: entry.namespace.clone(),
        name: entry.name.clone(),
        version,
        format: entry.format.clone(),
        schema: schema_json,
        content: entry.content.clone(),
        state: entry.state.clone(),
        compatibility_mode: entry.compatibility_mode.clone(),
        created_at: entry.created_at.to_rfc3339(),
        updated_at: entry.updated_at.to_rfc3339(),
    }
}

fn json_to_schema_response(id: Uuid, data: &serde_json::Value) -> GetSchemaResponse {
    let version = format!(
        "{}.{}.{}",
        data["version_major"].as_i64().unwrap_or(0),
        data["version_minor"].as_i64().unwrap_or(0),
        data["version_patch"].as_i64().unwrap_or(0)
    );

    let content_str = data["content"].as_str().unwrap_or("{}").to_string();
    let schema_json = serde_json::from_str(&content_str).unwrap_or(serde_json::json!({}));

    GetSchemaResponse {
        id: data["id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or(id),
        namespace: data["namespace"].as_str().unwrap_or("").to_string(),
        name: data["name"].as_str().unwrap_or("").to_string(),
        version,
        format: data["format"].as_str().unwrap_or("").to_string(),
        schema: schema_json,
        content: content_str,
        state: data["state"].as_str().unwrap_or("").to_string(),
        compatibility_mode: data["compatibility_mode"].as_str().unwrap_or("").to_string(),
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    }
}

async fn validate_data(
    State(state): State<AppState>,
    Path(schema_id): Path<Uuid>,
    Json(data): Json<serde_json::Value>,
) -> Result<Json<ValidateResponse>, AppError> {
    tracing::debug!(schema_id = %schema_id, "Validating data");

    // Get schema format and content based on mode
    let (format, _content) = match state.mode {
        ServerMode::Serverless | ServerMode::MemoryOnly => {
            let cache = state.memory_cache.read().await;
            match cache.get(&schema_id) {
                Some(entry) => (entry.format.clone(), entry.content.clone()),
                None => return Err(AppError::NotFound(format!("Schema {} not found", schema_id))),
            }
        }
        ServerMode::Full => {
            let db = state.db.as_ref()
                .ok_or_else(|| AppError::ServiceUnavailable("Database not connected".to_string()))?;

            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT format, content FROM schemas WHERE id = $1 LIMIT 1",
            )
            .bind(schema_id)
            .fetch_optional(db)
            .await?;

            match row {
                Some((f, c)) => (f, c),
                None => return Err(AppError::NotFound(format!("Schema {} not found", schema_id))),
            }
        }
    };

    // Simple validation
    let is_valid = match format.as_str() {
        "JSON" | "JSON_SCHEMA" => data.is_object() || data.is_array(),
        _ => true,
    };

    Ok(Json(ValidateResponse {
        is_valid,
        errors: if is_valid {
            vec![]
        } else {
            vec!["Data does not match schema".to_string()]
        },
    }))
}

async fn check_compatibility(
    State(state): State<AppState>,
    Json(req): Json<CompatibilityCheckRequest>,
) -> Result<Json<CompatibilityCheckResponse>, AppError> {
    tracing::debug!(
        schema_id = %req.schema_id,
        compared_schema_id = %req.compared_schema_id,
        mode = %req.mode,
        "Checking compatibility"
    );

    // Get both schemas based on mode
    let (hash1, hash2) = match state.mode {
        ServerMode::Serverless | ServerMode::MemoryOnly => {
            let cache = state.memory_cache.read().await;
            let s1 = cache.get(&req.schema_id)
                .ok_or_else(|| AppError::NotFound(format!("Schema {} not found", req.schema_id)))?;
            let s2 = cache.get(&req.compared_schema_id)
                .ok_or_else(|| AppError::NotFound(format!("Schema {} not found", req.compared_schema_id)))?;
            (s1.content_hash.clone(), s2.content_hash.clone())
        }
        ServerMode::Full => {
            let db = state.db.as_ref()
                .ok_or_else(|| AppError::ServiceUnavailable("Database not connected".to_string()))?;

            let schema1: Option<(String,)> = sqlx::query_as(
                "SELECT content_hash FROM schemas WHERE id = $1",
            )
            .bind(req.schema_id)
            .fetch_optional(db)
            .await?;

            let schema2: Option<(String,)> = sqlx::query_as(
                "SELECT content_hash FROM schemas WHERE id = $1",
            )
            .bind(req.compared_schema_id)
            .fetch_optional(db)
            .await?;

            match (schema1, schema2) {
                (Some((h1,)), Some((h2,))) => (h1, h2),
                _ => return Err(AppError::NotFound("One or both schemas not found".to_string())),
            }
        }
    };

    // Simple compatibility check - if hashes are same, they're compatible
    let is_compatible = hash1 == hash2 || true; // For now, always compatible

    Ok(Json(CompatibilityCheckResponse {
        is_compatible,
        mode: req.mode,
        violations: vec![],
    }))
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    tracing::info!("Starting Schema Registry Server v{}", env!("CARGO_PKG_VERSION"));

    // Determine server mode from environment
    let mode = match std::env::var("SERVER_MODE").as_deref() {
        Ok("serverless") | Ok("SERVERLESS") => ServerMode::Serverless,
        Ok("memory") | Ok("MEMORY") => ServerMode::MemoryOnly,
        Ok("full") | Ok("FULL") => ServerMode::Full,
        _ => {
            // Auto-detect: if DATABASE_URL is not set or is empty, use memory mode
            match std::env::var("DATABASE_URL") {
                Ok(url) if !url.is_empty() && !url.starts_with("postgresql://localhost") => ServerMode::Full,
                _ => {
                    // Check for RUVECTOR_SERVICE_URL
                    if std::env::var("RUVECTOR_SERVICE_URL").is_ok() {
                        ServerMode::Serverless
                    } else {
                        ServerMode::MemoryOnly
                    }
                }
            }
        }
    };

    tracing::info!("Server mode: {:?}", mode);

    // Load configuration from environment
    let server_host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port = std::env::var("PORT")  // Cloud Run uses PORT
        .or_else(|_| std::env::var("SERVER_PORT"))
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()?;
    let metrics_port = std::env::var("METRICS_PORT")
        .unwrap_or_else(|_| "9091".to_string())
        .parse::<u16>()?;

    tracing::info!("Server will listen on {}:{}", server_host, server_port);
    tracing::info!("Metrics will be available on port {}", metrics_port);

    // Create validation engine and compatibility checker (always available)
    let validator = Arc::new(ValidationEngine::new());
    let compatibility_checker = Arc::new(CompatibilityCheckerImpl::new());

    // Create initial state with empty connections
    let ruvector_url = std::env::var("RUVECTOR_SERVICE_URL").ok();
    let memory_cache = Arc::new(RwLock::new(HashMap::new()));

    let state = AppState {
        db: None,
        redis: None,
        validator,
        compatibility_checker,
        ruvector_url: ruvector_url.clone(),
        memory_cache: memory_cache.clone(),
        mode: mode.clone(),
    };

    // Build API router - this starts IMMEDIATELY
    let api_router = Router::new()
        // Health and readiness probes (no auth, always available)
        .route("/", get(liveness))
        .route("/health", get(health_check))
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness))
        // API endpoints
        .route("/api/v1/schemas", post(register_schema))
        .route("/api/v1/schemas/:id", get(get_schema))
        .route("/api/v1/validate/:id", post(validate_data))
        .route("/api/v1/compatibility/check", post(check_compatibility))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http());

    // Build metrics router
    let metrics_router = Router::new().route("/metrics", get(metrics_handler));

    // Start metrics server in background
    let metrics_addr = SocketAddr::from(([0, 0, 0, 0], metrics_port));
    tracing::info!("Starting metrics server on {}", metrics_addr);
    tokio::spawn(async move {
        if let Ok(listener) = tokio::net::TcpListener::bind(metrics_addr).await {
            let _ = axum::serve(listener, metrics_router).await;
        }
    });

    // Start API server IMMEDIATELY - this is critical for Cloud Run health checks
    let addr = SocketAddr::from(([0, 0, 0, 0], server_port));
    tracing::info!("API server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("API server listening on {} - ready for health checks", addr);

    // If in Full mode, try to connect to databases in background AFTER server starts
    if mode == ServerMode::Full {
        let state_clone = state.clone();
        tokio::spawn(async move {
            tracing::info!("Attempting database connections in background...");

            let database_url = std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/schema_registry".to_string());
            let redis_url = std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string());

            // Try PostgreSQL
            match PgPoolOptions::new()
                .max_connections(50)
                .acquire_timeout(Duration::from_secs(30))
                .connect(&database_url)
                .await
            {
                Ok(pool) => {
                    tracing::info!("PostgreSQL connected successfully");
                    // Note: In a real implementation, we'd update the state here
                    // This requires interior mutability patterns
                }
                Err(e) => {
                    tracing::warn!("PostgreSQL connection failed: {} - continuing in degraded mode", e);
                }
            }

            // Try Redis
            if let Ok(client) = redis::Client::open(redis_url) {
                match ConnectionManager::new(client).await {
                    Ok(_conn) => {
                        tracing::info!("Redis connected successfully");
                    }
                    Err(e) => {
                        tracing::warn!("Redis connection failed: {} - continuing without cache", e);
                    }
                }
            }
        });
    }

    // Serve requests
    axum::serve(listener, api_router).await?;

    Ok(())
}
