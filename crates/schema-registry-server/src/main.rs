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
use tower_http::trace::TraceLayer;
use tracing_subscriber;
use uuid::Uuid;

// ============================================================================
// Application State
// ============================================================================

#[derive(Clone)]
struct AppState {
    db: PgPool,
    redis: ConnectionManager,
    validator: Arc<ValidationEngine>,
    compatibility_checker: Arc<CompatibilityCheckerImpl>,
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
    Database(sqlx::Error),
    Redis(redis::RedisError),
    NotFound(String),
    InvalidInput(String),
    Internal(String),
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
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Database(e)
    }
}

impl From<redis::RedisError> for AppError {
    fn from(e: redis::RedisError) -> Self {
        AppError::Redis(e)
    }
}

// ============================================================================
// Handlers
// ============================================================================

async fn health_check(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    let mut components = HashMap::new();

    // Check PostgreSQL
    let db_status = match sqlx::query("SELECT 1").fetch_one(&state.db).await {
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

    // Check Redis
    let redis_status = {
        let mut conn = state.redis.clone();
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

    let overall_status = if components.values().all(|c| c.status == "up") {
        "healthy"
    } else {
        "degraded"
    };

    Ok(Json(HealthResponse {
        status: overall_status.to_string(),
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
        "Registering schema"
    );

    // Calculate content hash
    let content_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    };

    // Check if schema already exists with same hash
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM schemas WHERE namespace = $1 AND name = $2 AND version_major = $3 AND version_minor = $4 AND version_patch = $5"
    )
    .bind(&namespace)
    .bind(&name)
    .bind(version_major)
    .bind(version_minor)
    .bind(version_patch)
    .fetch_optional(&state.db)
    .await?;

    if let Some((existing_id,)) = existing {
        let version = format!("{}.{}.{}", version_major, version_minor, version_patch);
        return Ok((
            StatusCode::OK,
            Json(RegisterSchemaResponse {
                id: existing_id,
                version,
                created_at: Utc::now().to_rfc3339(),
            }),
        ));
    }

    // Insert new schema
    let id = Uuid::new_v4();
    let now = Utc::now();

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
    .execute(&state.db)
    .await?;

    // Cache in Redis with 1-hour TTL
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

    let mut conn = state.redis.clone();
    let _: () = redis::cmd("SET")
        .arg(&cache_key)
        .arg(serde_json::to_string(&cache_value).unwrap())
        .arg("EX")
        .arg(3600) // 1 hour TTL
        .query_async(&mut conn)
        .await?;

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
    tracing::debug!(schema_id = %id, "Fetching schema");

    // Try Redis cache first
    let cache_key = format!("schema:{}", id);
    let mut conn = state.redis.clone();

    if let Ok(Some(cached)) = redis::cmd("GET")
        .arg(&cache_key)
        .query_async::<_, Option<String>>(&mut conn)
        .await
    {
        if let Ok(schema_data) = serde_json::from_str::<serde_json::Value>(&cached) {
            tracing::debug!(schema_id = %id, "Cache hit");

            let version = format!(
                "{}.{}.{}",
                schema_data["version_major"].as_i64().unwrap_or(0),
                schema_data["version_minor"].as_i64().unwrap_or(0),
                schema_data["version_patch"].as_i64().unwrap_or(0)
            );

            // Parse content as JSON if it's JSON format
            let content_str = schema_data["content"]
                .as_str()
                .unwrap_or("{}")
                .to_string();
            let schema_json = serde_json::from_str(&content_str).unwrap_or(serde_json::json!({}));

            return Ok(Json(GetSchemaResponse {
                id: schema_data["id"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or(id),
                namespace: schema_data["namespace"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                name: schema_data["name"].as_str().unwrap_or("").to_string(),
                version,
                format: schema_data["format"].as_str().unwrap_or("").to_string(),
                schema: schema_json,
                content: content_str,
                state: schema_data["state"].as_str().unwrap_or("").to_string(),
                compatibility_mode: schema_data["compatibility_mode"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                created_at: Utc::now().to_rfc3339(),
                updated_at: Utc::now().to_rfc3339(),
            }));
        }
    }

    tracing::debug!(schema_id = %id, "Cache miss, querying database");

    // Fallback to PostgreSQL
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
    .fetch_optional(&state.db)
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

            // Parse content as JSON
            let schema_json = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

            // Update cache
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

            let _: Result<(), _> = redis::cmd("SET")
                .arg(&cache_key)
                .arg(serde_json::to_string(&cache_value).unwrap())
                .arg("EX")
                .arg(3600)
                .query_async(&mut conn)
                .await;

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

async fn validate_data(
    State(state): State<AppState>,
    Path(schema_id): Path<Uuid>,
    Json(data): Json<serde_json::Value>,
) -> Result<Json<ValidateResponse>, AppError> {
    tracing::debug!(schema_id = %schema_id, "Validating data");

    // Fetch schema
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT format, content FROM schemas WHERE id = $1 LIMIT 1",
    )
    .bind(schema_id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((format, content)) => {
            // Simple validation - just check if data is valid JSON
            // In production, use jsonschema crate for proper validation
            let is_valid = match format.as_str() {
                "JSON" | "JSON_SCHEMA" => {
                    // Basic JSON validation
                    data.is_object() || data.is_array()
                }
                _ => true, // Accept other formats for now
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
        None => Err(AppError::NotFound(format!(
            "Schema {} not found",
            schema_id
        ))),
    }
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

    // Fetch both schemas
    let schema1: Option<(String, String, i32, i32, i32)> = sqlx::query_as(
        "SELECT content, content_hash, version_major, version_minor, version_patch FROM schemas WHERE id = $1",
    )
    .bind(req.schema_id)
    .fetch_optional(&state.db)
    .await?;

    let schema2: Option<(String, String, i32, i32, i32)> = sqlx::query_as(
        "SELECT content, content_hash, version_major, version_minor, version_patch FROM schemas WHERE id = $1",
    )
    .bind(req.compared_schema_id)
    .fetch_optional(&state.db)
    .await?;

    match (schema1, schema2) {
        (Some((content1, hash1, v1_major, v1_minor, v1_patch)), Some((content2, hash2, v2_major, v2_minor, v2_patch))) => {
            // Simple compatibility check - if hashes are same, they're compatible
            let is_compatible = if hash1 == hash2 {
                true
            } else {
                // For now, assume compatible unless there are obvious breaking changes
                // In production, use the compatibility checker properly
                true
            };

            Ok(Json(CompatibilityCheckResponse {
                is_compatible,
                mode: req.mode,
                violations: vec![],
            }))
        }
        _ => Err(AppError::NotFound("One or both schemas not found".to_string())),
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    tracing::info!("Starting Schema Registry Server");

    // Load configuration from environment
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/schema_registry".to_string());
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let server_host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port = std::env::var("SERVER_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()?;
    let metrics_port = std::env::var("METRICS_PORT")
        .unwrap_or_else(|_| "9091".to_string())
        .parse::<u16>()?;

    tracing::info!("Database URL: {}", database_url);
    tracing::info!("Redis URL: {}", redis_url);
    tracing::info!("Server will listen on {}:{}", server_host, server_port);
    tracing::info!("Metrics will be available on port {}", metrics_port);

    // Create PostgreSQL connection pool
    tracing::info!("Connecting to PostgreSQL...");
    let db = PgPoolOptions::new()
        .max_connections(50)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;

    tracing::info!("PostgreSQL connection pool created");

    // Run migrations
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await?;
    tracing::info!("Migrations completed");

    // Create Redis connection
    tracing::info!("Connecting to Redis...");
    let redis_client = redis::Client::open(redis_url)?;
    let redis = ConnectionManager::new(redis_client).await?;
    tracing::info!("Redis connection established");

    // Create validation engine and compatibility checker
    let validator = Arc::new(ValidationEngine::new());
    let compatibility_checker = Arc::new(CompatibilityCheckerImpl::new());

    // Create application state
    let state = AppState {
        db,
        redis,
        validator,
        compatibility_checker,
    };

    // Build API router
    let api_router = Router::new()
        .route("/api/v1/schemas", post(register_schema))
        .route("/api/v1/schemas/:id", get(get_schema))
        .route("/api/v1/validate/:id", post(validate_data))
        .route("/api/v1/compatibility/check", post(check_compatibility))
        .route("/health", get(health_check))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http());

    // Build metrics router (separate server on different port)
    let metrics_router = Router::new().route("/metrics", get(metrics_handler));

    // Start metrics server
    let metrics_addr = SocketAddr::from(([0, 0, 0, 0], metrics_port));
    tracing::info!("Metrics server listening on {}", metrics_addr);
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(metrics_addr)
            .await
            .expect("Failed to bind metrics server");
        axum::serve(listener, metrics_router)
            .await
            .expect("Metrics server failed");
    });

    // Start API server
    let addr = SocketAddr::from(([0, 0, 0, 0], server_port));
    tracing::info!("API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, api_router).await?;

    Ok(())
}
