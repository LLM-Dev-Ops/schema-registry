//! Edge Function HTTP Handler
//!
//! This module provides the HTTP handler for the Google Cloud Edge Function.
//! It is designed to be:
//! - Stateless: No persistent state between invocations
//! - Deterministic: Same inputs produce same outputs
//! - Fast: Optimized for cold start performance
//!
//! The handler supports validation, inspection, and diff endpoints,
//! each producing DecisionEvents that are persisted to ruvector-service.

use agentics_instrumentation::{AgenticsCollector, Artifact};
use crate::decision_event::DecisionEvent;
use crate::ruvector_client::{RuVectorClient, RuVectorError};
use crate::telemetry::TelemetryEmitter;
use crate::types::{
    AgentOutput, CompatibilityResult, CompatibilityViolation, ConstraintsApplied,
    DecisionType, ErrorSeverity, SchemaFormat, ValidationError, ValidationInput,
    ValidationMetrics, ValidationMode, ValidationResult, VersionBounds,
    AGENT_ID, AGENT_VERSION,
};
use crate::validation_engine::{ValidationEngine, ValidationEngineConfig};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    middleware as axum_mw,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request payload for validation endpoint
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    /// Raw schema content to validate
    pub schema_content: String,
    /// Schema namespace (e.g., "com.example.events")
    pub namespace: String,
    /// Schema name (e.g., "UserCreated")
    pub name: String,
    /// Optional version string (semver format)
    #[serde(default)]
    pub version: Option<String>,
    /// Optional format hint ("jsonschema", "avro", "protobuf")
    #[serde(default)]
    pub format: Option<String>,
    /// Validation mode ("strict" or "lenient")
    #[serde(default)]
    pub mode: Option<String>,
    /// Optional target schema/version for compatibility checking
    #[serde(default)]
    pub compatibility_target: Option<String>,
}

/// Response for validation endpoint
#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    /// Whether the overall operation succeeded
    pub success: bool,
    /// Validation result details
    pub validation_result: ValidationResultResponse,
    /// Compatibility result (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility_result: Option<CompatibilityResultResponse>,
    /// Unique event ID for this decision
    pub decision_event_id: String,
    /// Execution reference for distributed tracing
    pub execution_ref: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

/// Validation result in response format
#[derive(Debug, Serialize)]
pub struct ValidationResultResponse {
    /// Whether the schema is valid
    pub is_valid: bool,
    /// Detected or confirmed schema format
    pub format: String,
    /// List of validation errors
    pub errors: Vec<ValidationErrorResponse>,
    /// List of validation warnings
    pub warnings: Vec<ValidationErrorResponse>,
    /// Validation metrics
    pub metrics: ValidationMetricsResponse,
}

/// Validation error in response format
#[derive(Debug, Serialize)]
pub struct ValidationErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub severity: String,
}

/// Validation metrics in response format
#[derive(Debug, Serialize)]
pub struct ValidationMetricsResponse {
    pub rules_checked: u32,
    pub rules_passed: u32,
    pub rules_failed: u32,
    pub duration_ms: u64,
}

/// Compatibility result in response format
#[derive(Debug, Serialize)]
pub struct CompatibilityResultResponse {
    pub is_compatible: bool,
    pub violations: Vec<CompatibilityViolationResponse>,
    pub breaking_changes: Vec<CompatibilityViolationResponse>,
    pub non_breaking_changes: Vec<CompatibilityViolationResponse>,
}

/// Compatibility violation in response format
#[derive(Debug, Serialize)]
pub struct CompatibilityViolationResponse {
    pub violation_type: String,
    pub description: String,
    pub path: String,
    pub is_breaking: bool,
}

/// Request for schema inspection
#[derive(Debug, Deserialize)]
pub struct InspectRequest {
    /// Raw schema content to inspect
    pub schema_content: String,
    /// Optional format hint
    #[serde(default)]
    pub format: Option<String>,
}

/// Response for schema inspection
#[derive(Debug, Serialize)]
pub struct InspectResponse {
    /// Detected schema format
    pub format: String,
    /// Validation result
    pub validation_result: ValidationResultResponse,
    /// Extracted metadata
    pub metadata: serde_json::Value,
}

/// Request for schema diff
#[derive(Debug, Deserialize)]
pub struct DiffRequest {
    /// First schema content (typically the older version)
    pub schema_a: String,
    /// Second schema content (typically the newer version)
    pub schema_b: String,
    /// Optional format hint
    #[serde(default)]
    pub format: Option<String>,
}

/// Response for schema diff
#[derive(Debug, Serialize)]
pub struct DiffResponse {
    /// Whether schemas are structurally identical
    pub identical: bool,
    /// Changes detected
    pub changes: Vec<DiffChange>,
    /// Compatibility assessment
    pub compatibility: DiffCompatibility,
}

/// A single change in the diff
#[derive(Debug, Serialize)]
pub struct DiffChange {
    /// Type of change
    pub change_type: String,
    /// Path where change occurred
    pub path: String,
    /// Description of the change
    pub description: String,
    /// Old value (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<serde_json::Value>,
    /// New value (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<serde_json::Value>,
}

/// Compatibility assessment from diff
#[derive(Debug, Serialize)]
pub struct DiffCompatibility {
    /// Backward compatible (new can read old data)
    pub backward: bool,
    /// Forward compatible (old can read new data)
    pub forward: bool,
    /// Breaking changes
    pub breaking_changes: Vec<String>,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
    /// Error code
    pub code: String,
    /// Optional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// ============================================================================
// Application State
// ============================================================================

/// Application state for the Edge Function
///
/// This state is shared across all requests within a single instance.
/// It should be considered immutable after initialization.
pub struct AppState {
    /// Schema validation engine
    pub validation_engine: ValidationEngine,
    /// RuVector client for event persistence
    pub ruvector_client: RuVectorClient,
    /// Telemetry emitter
    pub telemetry: TelemetryEmitter,
}

impl AppState {
    /// Create new application state with default configuration
    pub fn new(ruvector_url: String, ruvector_api_key: Option<String>) -> Self {
        Self {
            validation_engine: ValidationEngine::new(ValidationEngineConfig::default()),
            ruvector_client: RuVectorClient::new(ruvector_url, ruvector_api_key),
            telemetry: TelemetryEmitter::default(),
        }
    }

    /// Create application state with custom configuration
    pub fn with_config(
        validation_config: ValidationEngineConfig,
        ruvector_url: String,
        ruvector_api_key: Option<String>,
        environment: String,
    ) -> Self {
        Self {
            validation_engine: ValidationEngine::new(validation_config),
            ruvector_client: RuVectorClient::new(ruvector_url, ruvector_api_key),
            telemetry: TelemetryEmitter::new(AGENT_ID.to_string(), environment),
        }
    }
}

// ============================================================================
// Router Creation
// ============================================================================

/// Create the Edge Function router
///
/// Health, readiness, and metrics routes are NOT instrumented with agentics
/// middleware. Validation, inspection, and diff routes ARE instrumented and
/// require `X-Execution-ID` and `X-Parent-Span-ID` headers.
pub fn create_router(state: Arc<AppState>) -> Router {
    let health_routes = Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
        .route("/metrics", get(metrics_endpoint));

    let instrumented_routes = Router::new()
        .route("/validate", post(validate_schema))
        .route("/inspect", post(inspect_schema))
        .route("/diff", post(diff_schemas))
        .layer(axum_mw::from_fn(agentics_instrumentation::agentics_middleware));

    Router::new()
        .merge(health_routes)
        .merge(instrumented_routes)
        .with_state(state)
}

// ============================================================================
// Health & Readiness Endpoints
// ============================================================================

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "agent_id": AGENT_ID,
        "agent_version": AGENT_VERSION,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Readiness check endpoint
async fn readiness_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Check if RuVector service is reachable
    let ruvector_healthy = match state.ruvector_client.health_check().await {
        Ok(_) => true,
        Err(e) => {
            warn!("RuVector health check failed: {}", e);
            false
        }
    };

    let status = if ruvector_healthy { "ready" } else { "degraded" };

    (
        if ruvector_healthy { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE },
        Json(serde_json::json!({
            "status": status,
            "checks": {
                "ruvector": ruvector_healthy,
            },
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
}

/// Metrics endpoint (Prometheus format)
async fn metrics_endpoint(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.telemetry.export_metrics() {
        Ok(metrics) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
            metrics,
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("Content-Type", "text/plain")],
            format!("Error exporting metrics: {}", e),
        ),
    }
}

// ============================================================================
// Validation Endpoint
// ============================================================================

/// Main validation endpoint
#[instrument(skip(state, spans, request), fields(
    namespace = %request.namespace,
    name = %request.name,
    content_size = request.schema_content.len()
))]
async fn validate_schema(
    State(state): State<Arc<AppState>>,
    spans: AgenticsCollector,
    Json(request): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, (StatusCode, Json<ErrorResponse>)> {
    let guard = spans.begin_agent("schema-validation-agent").await;
    let start = Instant::now();

    // 1. Parse format hint
    let format_hint = request.format.as_ref().and_then(|f| match f.to_lowercase().as_str() {
        "jsonschema" | "json-schema" | "json_schema" => Some(SchemaFormat::JsonSchema),
        "avro" => Some(SchemaFormat::Avro),
        "protobuf" | "proto" => Some(SchemaFormat::Protobuf),
        _ => None,
    });

    // 2. Parse validation mode
    let validation_mode = match request.mode.as_deref() {
        Some("lenient") => ValidationMode::Lenient,
        _ => ValidationMode::Strict,
    };

    // 3. Run validation
    let validation_result = state
        .validation_engine
        .validate(&request.schema_content, format_hint)
        .map_err(|e| {
            error!(error = %e, "Validation engine error");
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                    code: "VALIDATION_ERROR".to_string(),
                    details: None,
                }),
            )
        })?;

    // 4. Create validation input for DecisionEvent
    let validation_input = ValidationInput {
        schema_content: request.schema_content.clone(),
        namespace: request.namespace.clone(),
        name: request.name.clone(),
        version: request.version.clone(),
        format: Some(validation_result.detected_format.clone()),
        mode: validation_mode.clone(),
        compatibility_target: request.compatibility_target.clone(),
    };

    // 5. Build internal validation result
    let internal_result = ValidationResult {
        is_valid: validation_result.is_valid,
        errors: validation_result
            .errors
            .iter()
            .map(|e| ValidationError {
                code: e.code.clone(),
                message: e.message.clone(),
                path: e.path.clone(),
                severity: e.severity.clone(),
            })
            .collect(),
        warnings: validation_result
            .warnings
            .iter()
            .map(|e| ValidationError {
                code: e.code.clone(),
                message: e.message.clone(),
                path: e.path.clone(),
                severity: e.severity.clone(),
            })
            .collect(),
        metrics: validation_result.metrics.clone(),
        detected_format: validation_result.detected_format.clone(),
    };

    // 6. Build agent output (compatibility check would go here)
    let agent_output = AgentOutput {
        validation_result: internal_result,
        compatibility_result: None, // TODO: Implement compatibility checking
    };

    // 7. Build constraints applied
    let constraints = ConstraintsApplied {
        schema_scope: format!("{}:{}", request.namespace, request.name),
        version_bounds: request.version.as_ref().map(|v| VersionBounds {
            min_version: None,
            max_version: None,
            target_version: Some(v.clone()),
        }),
        compatibility_mode: request.compatibility_target.as_ref().map(|_| "BACKWARD".to_string()),
        validation_rules: vec![
            "syntax_check".to_string(),
            "structure_validation".to_string(),
            "type_validation".to_string(),
        ],
    };

    // 8. Create DecisionEvent
    let decision_event = DecisionEvent::new(
        &validation_input,
        agent_output.clone(),
        DecisionType::SchemaValidationResult,
        constraints,
    );

    // 9. Emit telemetry
    state.telemetry.emit_validation_span(&decision_event);
    state.telemetry.emit_metrics(&decision_event);
    state.telemetry.emit_structured_log(&decision_event);

    // 10. Persist to ruvector-service (async, non-blocking)
    let persistence_start = Instant::now();
    let persistence_result = state.ruvector_client.persist_event(&decision_event).await;
    let persistence_latency = persistence_start.elapsed().as_millis() as u64;

    match &persistence_result {
        Ok(_) => {
            state.telemetry.record_persistence_result(true, persistence_latency);
            info!(event_id = %decision_event.id, "Event persisted to ruvector-service");
        }
        Err(e) => {
            state.telemetry.record_persistence_result(false, persistence_latency);
            warn!(error = %e, event_id = %decision_event.id, "Failed to persist event to ruvector-service");
            // Continue anyway - validation result is still valid
        }
    }

    // 11. Build response
    let response = ValidateResponse {
        success: validation_result.is_valid,
        validation_result: ValidationResultResponse {
            is_valid: validation_result.is_valid,
            format: format!("{:?}", validation_result.detected_format),
            errors: validation_result
                .errors
                .iter()
                .map(|e| ValidationErrorResponse {
                    code: e.code.clone(),
                    message: e.message.clone(),
                    path: e.path.clone(),
                    severity: format!("{:?}", e.severity),
                })
                .collect(),
            warnings: validation_result
                .warnings
                .iter()
                .map(|e| ValidationErrorResponse {
                    code: e.code.clone(),
                    message: e.message.clone(),
                    path: e.path.clone(),
                    severity: format!("{:?}", e.severity),
                })
                .collect(),
            metrics: ValidationMetricsResponse {
                rules_checked: validation_result.metrics.rules_checked,
                rules_passed: validation_result.metrics.rules_passed,
                rules_failed: validation_result.metrics.rules_failed,
                duration_ms: validation_result.metrics.validation_duration_ms,
            },
        },
        compatibility_result: None,
        decision_event_id: decision_event.id.to_string(),
        execution_ref: decision_event.execution_ref.clone(),
        confidence: decision_event.confidence,
    };

    let total_duration = start.elapsed().as_millis();
    info!(
        duration_ms = %total_duration,
        is_valid = %response.success,
        "Validation request completed"
    );

    guard
        .attach_artifact(Artifact {
            artifact_id: decision_event.id,
            uri: None,
            content_hash: Some(decision_event.content_hash.clone()),
            filename: None,
            artifact_type: "decision_event".to_string(),
        })
        .await;
    guard.close().await;

    Ok(Json(response))
}

// ============================================================================
// Inspection Endpoint
// ============================================================================

/// Schema inspection endpoint (detailed analysis without persistence)
#[instrument(skip(state, spans, request), fields(content_size = request.schema_content.len()))]
async fn inspect_schema(
    State(state): State<Arc<AppState>>,
    spans: AgenticsCollector,
    Json(request): Json<InspectRequest>,
) -> Result<Json<InspectResponse>, (StatusCode, Json<ErrorResponse>)> {
    let guard = spans.begin_agent("schema-inspection-agent").await;

    // Parse format hint
    let format_hint = request.format.as_ref().and_then(|f| match f.to_lowercase().as_str() {
        "jsonschema" | "json-schema" | "json_schema" => Some(SchemaFormat::JsonSchema),
        "avro" => Some(SchemaFormat::Avro),
        "protobuf" | "proto" => Some(SchemaFormat::Protobuf),
        _ => None,
    });

    // Run validation
    let validation_result = state
        .validation_engine
        .validate(&request.schema_content, format_hint)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                    code: "INSPECTION_ERROR".to_string(),
                    details: None,
                }),
            )
        })?;

    // Extract metadata from schema
    let metadata = extract_schema_metadata(&request.schema_content, &validation_result.detected_format);

    guard
        .attach_artifact(Artifact {
            artifact_id: Uuid::new_v4(),
            uri: None,
            content_hash: None,
            filename: None,
            artifact_type: "inspection_report".to_string(),
        })
        .await;
    guard.close().await;

    Ok(Json(InspectResponse {
        format: format!("{:?}", validation_result.detected_format),
        validation_result: ValidationResultResponse {
            is_valid: validation_result.is_valid,
            format: format!("{:?}", validation_result.detected_format),
            errors: validation_result
                .errors
                .iter()
                .map(|e| ValidationErrorResponse {
                    code: e.code.clone(),
                    message: e.message.clone(),
                    path: e.path.clone(),
                    severity: format!("{:?}", e.severity),
                })
                .collect(),
            warnings: validation_result
                .warnings
                .iter()
                .map(|e| ValidationErrorResponse {
                    code: e.code.clone(),
                    message: e.message.clone(),
                    path: e.path.clone(),
                    severity: format!("{:?}", e.severity),
                })
                .collect(),
            metrics: ValidationMetricsResponse {
                rules_checked: validation_result.metrics.rules_checked,
                rules_passed: validation_result.metrics.rules_passed,
                rules_failed: validation_result.metrics.rules_failed,
                duration_ms: validation_result.metrics.validation_duration_ms,
            },
        },
        metadata,
    }))
}

/// Extract metadata from schema based on format
fn extract_schema_metadata(content: &str, format: &SchemaFormat) -> serde_json::Value {
    match format {
        SchemaFormat::JsonSchema => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
                serde_json::json!({
                    "schema_uri": json.get("$schema"),
                    "title": json.get("title"),
                    "description": json.get("description"),
                    "type": json.get("type"),
                    "properties_count": json.get("properties").and_then(|p| p.as_object()).map(|o| o.len()),
                    "required_fields": json.get("required"),
                    "has_definitions": json.get("definitions").is_some() || json.get("$defs").is_some(),
                })
            } else {
                serde_json::json!({})
            }
        }
        SchemaFormat::Avro => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
                serde_json::json!({
                    "type": json.get("type"),
                    "name": json.get("name"),
                    "namespace": json.get("namespace"),
                    "doc": json.get("doc"),
                    "fields_count": json.get("fields").and_then(|f| f.as_array()).map(|a| a.len()),
                    "aliases": json.get("aliases"),
                })
            } else {
                serde_json::json!({})
            }
        }
        SchemaFormat::Protobuf => {
            // Extract basic protobuf metadata using regex
            let syntax_re = regex::Regex::new(r#"syntax\s*=\s*["']([^"']+)["']"#).unwrap();
            let package_re = regex::Regex::new(r"package\s+([\w\.]+)").unwrap();
            let message_re = regex::Regex::new(r"message\s+(\w+)").unwrap();

            let syntax = syntax_re.captures(content).map(|c| c[1].to_string());
            let package = package_re.captures(content).map(|c| c[1].to_string());
            let messages: Vec<String> = message_re
                .captures_iter(content)
                .map(|c| c[1].to_string())
                .collect();

            serde_json::json!({
                "syntax": syntax,
                "package": package,
                "messages": messages,
                "message_count": messages.len(),
            })
        }
        SchemaFormat::Unknown => {
            serde_json::json!({
                "format": "unknown",
                "content_length": content.len(),
            })
        }
    }
}

// ============================================================================
// Diff Endpoint
// ============================================================================

/// Schema diff endpoint (compare two schemas)
#[instrument(skip(state, spans, request), fields(
    schema_a_size = request.schema_a.len(),
    schema_b_size = request.schema_b.len()
))]
async fn diff_schemas(
    State(state): State<Arc<AppState>>,
    spans: AgenticsCollector,
    Json(request): Json<DiffRequest>,
) -> Result<Json<DiffResponse>, (StatusCode, Json<ErrorResponse>)> {
    let guard = spans.begin_agent("schema-diff-agent").await;
    // Parse format hint
    let format_hint = request.format.as_ref().and_then(|f| match f.to_lowercase().as_str() {
        "jsonschema" | "json-schema" | "json_schema" => Some(SchemaFormat::JsonSchema),
        "avro" => Some(SchemaFormat::Avro),
        "protobuf" | "proto" => Some(SchemaFormat::Protobuf),
        _ => None,
    });

    // Validate both schemas
    let result_a = state
        .validation_engine
        .validate(&request.schema_a, format_hint.clone())
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Schema A validation error: {}", e),
                    code: "SCHEMA_A_INVALID".to_string(),
                    details: None,
                }),
            )
        })?;

    let result_b = state
        .validation_engine
        .validate(&request.schema_b, format_hint)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Schema B validation error: {}", e),
                    code: "SCHEMA_B_INVALID".to_string(),
                    details: None,
                }),
            )
        })?;

    // Check if formats match
    if result_a.detected_format != result_b.detected_format {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "Schema format mismatch: {:?} vs {:?}",
                    result_a.detected_format, result_b.detected_format
                ),
                code: "FORMAT_MISMATCH".to_string(),
                details: None,
            }),
        ));
    }

    // Compute diff based on format
    let (changes, compatibility) = compute_schema_diff(
        &request.schema_a,
        &request.schema_b,
        &result_a.detected_format,
    );

    let identical = changes.is_empty();

    guard
        .attach_artifact(Artifact {
            artifact_id: Uuid::new_v4(),
            uri: None,
            content_hash: None,
            filename: None,
            artifact_type: "diff_report".to_string(),
        })
        .await;
    guard.close().await;

    Ok(Json(DiffResponse {
        identical,
        changes,
        compatibility,
    }))
}

/// Compute differences between two schemas
fn compute_schema_diff(
    schema_a: &str,
    schema_b: &str,
    format: &SchemaFormat,
) -> (Vec<DiffChange>, DiffCompatibility) {
    let mut changes = Vec::new();
    let mut breaking_changes = Vec::new();

    match format {
        SchemaFormat::JsonSchema => {
            if let (Ok(json_a), Ok(json_b)) = (
                serde_json::from_str::<serde_json::Value>(schema_a),
                serde_json::from_str::<serde_json::Value>(schema_b),
            ) {
                // Compare properties
                if let (Some(props_a), Some(props_b)) = (
                    json_a.get("properties").and_then(|p| p.as_object()),
                    json_b.get("properties").and_then(|p| p.as_object()),
                ) {
                    // Check for removed properties
                    for key in props_a.keys() {
                        if !props_b.contains_key(key) {
                            changes.push(DiffChange {
                                change_type: "PROPERTY_REMOVED".to_string(),
                                path: format!("$.properties.{}", key),
                                description: format!("Property '{}' was removed", key),
                                old_value: props_a.get(key).cloned(),
                                new_value: None,
                            });
                            breaking_changes.push(format!("Property '{}' removed", key));
                        }
                    }

                    // Check for added properties
                    for key in props_b.keys() {
                        if !props_a.contains_key(key) {
                            changes.push(DiffChange {
                                change_type: "PROPERTY_ADDED".to_string(),
                                path: format!("$.properties.{}", key),
                                description: format!("Property '{}' was added", key),
                                old_value: None,
                                new_value: props_b.get(key).cloned(),
                            });
                        }
                    }

                    // Check for modified properties
                    for key in props_a.keys() {
                        if let (Some(val_a), Some(val_b)) = (props_a.get(key), props_b.get(key)) {
                            if val_a != val_b {
                                changes.push(DiffChange {
                                    change_type: "PROPERTY_MODIFIED".to_string(),
                                    path: format!("$.properties.{}", key),
                                    description: format!("Property '{}' was modified", key),
                                    old_value: Some(val_a.clone()),
                                    new_value: Some(val_b.clone()),
                                });
                            }
                        }
                    }
                }

                // Check required array
                if let (Some(req_a), Some(req_b)) = (
                    json_a.get("required").and_then(|r| r.as_array()),
                    json_b.get("required").and_then(|r| r.as_array()),
                ) {
                    let set_a: std::collections::HashSet<_> = req_a.iter().collect();
                    let set_b: std::collections::HashSet<_> = req_b.iter().collect();

                    for item in set_b.difference(&set_a) {
                        changes.push(DiffChange {
                            change_type: "REQUIRED_ADDED".to_string(),
                            path: "$.required".to_string(),
                            description: format!("Field {:?} is now required", item),
                            old_value: None,
                            new_value: Some((*item).clone()),
                        });
                        breaking_changes.push(format!("New required field: {:?}", item));
                    }
                }
            }
        }
        SchemaFormat::Avro => {
            // Simplified Avro diff
            if let (Ok(json_a), Ok(json_b)) = (
                serde_json::from_str::<serde_json::Value>(schema_a),
                serde_json::from_str::<serde_json::Value>(schema_b),
            ) {
                if let (Some(fields_a), Some(fields_b)) = (
                    json_a.get("fields").and_then(|f| f.as_array()),
                    json_b.get("fields").and_then(|f| f.as_array()),
                ) {
                    let names_a: std::collections::HashSet<_> = fields_a
                        .iter()
                        .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
                        .collect();
                    let names_b: std::collections::HashSet<_> = fields_b
                        .iter()
                        .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
                        .collect();

                    for name in names_a.difference(&names_b) {
                        changes.push(DiffChange {
                            change_type: "FIELD_REMOVED".to_string(),
                            path: format!("$.fields[name={}]", name),
                            description: format!("Field '{}' was removed", name),
                            old_value: None,
                            new_value: None,
                        });
                        breaking_changes.push(format!("Field '{}' removed", name));
                    }

                    for name in names_b.difference(&names_a) {
                        changes.push(DiffChange {
                            change_type: "FIELD_ADDED".to_string(),
                            path: format!("$.fields[name={}]", name),
                            description: format!("Field '{}' was added", name),
                            old_value: None,
                            new_value: None,
                        });
                    }
                }
            }
        }
        SchemaFormat::Protobuf => {
            // Simplified Protobuf diff using regex
            let message_re = regex::Regex::new(r"message\s+(\w+)").unwrap();
            let messages_a: std::collections::HashSet<_> = message_re
                .captures_iter(schema_a)
                .map(|c| c[1].to_string())
                .collect();
            let messages_b: std::collections::HashSet<_> = message_re
                .captures_iter(schema_b)
                .map(|c| c[1].to_string())
                .collect();

            for name in messages_a.difference(&messages_b) {
                changes.push(DiffChange {
                    change_type: "MESSAGE_REMOVED".to_string(),
                    path: format!("message {}", name),
                    description: format!("Message '{}' was removed", name),
                    old_value: None,
                    new_value: None,
                });
                breaking_changes.push(format!("Message '{}' removed", name));
            }

            for name in messages_b.difference(&messages_a) {
                changes.push(DiffChange {
                    change_type: "MESSAGE_ADDED".to_string(),
                    path: format!("message {}", name),
                    description: format!("Message '{}' was added", name),
                    old_value: None,
                    new_value: None,
                });
            }
        }
        SchemaFormat::Unknown => {
            // For unknown format, do basic text comparison
            if schema_a != schema_b {
                changes.push(DiffChange {
                    change_type: "CONTENT_CHANGED".to_string(),
                    path: "$".to_string(),
                    description: "Schema content changed (unknown format)".to_string(),
                    old_value: None,
                    new_value: None,
                });
            }
        }
    }

    let compatibility = DiffCompatibility {
        backward: breaking_changes.is_empty(),
        forward: true, // Simplified: additions are forward compatible
        breaking_changes,
    };

    (changes, compatibility)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_request_deserialization() {
        let json = r#"{
            "schema_content": "{}",
            "namespace": "test",
            "name": "TestSchema",
            "version": "1.0.0",
            "format": "jsonschema"
        }"#;

        let request: ValidateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.namespace, "test");
        assert_eq!(request.name, "TestSchema");
        assert_eq!(request.format, Some("jsonschema".to_string()));
    }

    #[test]
    fn test_validate_response_serialization() {
        let response = ValidateResponse {
            success: true,
            validation_result: ValidationResultResponse {
                is_valid: true,
                format: "JsonSchema".to_string(),
                errors: vec![],
                warnings: vec![],
                metrics: ValidationMetricsResponse {
                    rules_checked: 5,
                    rules_passed: 5,
                    rules_failed: 0,
                    duration_ms: 10,
                },
            },
            compatibility_result: None,
            decision_event_id: "test-id".to_string(),
            execution_ref: "exec-123".to_string(),
            confidence: 1.0,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("validation_result"));
    }

    #[test]
    fn test_error_response_serialization() {
        let error = ErrorResponse {
            error: "Test error".to_string(),
            code: "TEST_ERROR".to_string(),
            details: Some(serde_json::json!({"field": "value"})),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Test error"));
        assert!(json.contains("TEST_ERROR"));
    }

    #[test]
    fn test_extract_json_schema_metadata() {
        let content = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "Test Schema",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        }"#;

        let metadata = extract_schema_metadata(content, &SchemaFormat::JsonSchema);
        assert!(metadata.get("title").is_some());
        assert_eq!(metadata.get("properties_count"), Some(&serde_json::json!(1)));
    }

    #[test]
    fn test_compute_json_schema_diff() {
        let schema_a = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let schema_b = r#"{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "integer"}}}"#;

        let (changes, compatibility) = compute_schema_diff(schema_a, schema_b, &SchemaFormat::JsonSchema);

        assert!(!changes.is_empty());
        assert!(changes.iter().any(|c| c.change_type == "PROPERTY_ADDED"));
        assert!(compatibility.backward);
    }
}
