//! Schema Validation Agent - Google Cloud Edge Function Entry Point
//!
//! This is the main entry point for the Schema Validation Agent when deployed
//! as a Google Cloud Edge Function. It initializes telemetry, creates the
//! application state, and starts the HTTP server.
//!
//! # Environment Variables
//!
//! - `PORT`: HTTP server port (default: 8080, Cloud Functions sets this)
//! - `RUVECTOR_SERVICE_URL`: URL of the ruvector-service (default: http://localhost:8080)
//! - `RUVECTOR_API_KEY`: Optional API key for ruvector-service authentication
//! - `ENVIRONMENT`: Deployment environment (default: development)
//! - `LOG_LEVEL`: Logging level (default: info)
//! - `OTLP_ENDPOINT`: OpenTelemetry OTLP endpoint for trace export (optional)
//! - `TRACE_SAMPLING_RATE`: Trace sampling rate 0.0-1.0 (default: 0.1)
//! - `JSON_LOGS`: Enable JSON formatted logs (default: true)
//!
//! # Cold Start Optimization
//!
//! The agent is designed for fast cold starts:
//! - Minimal dependency initialization
//! - Lazy telemetry configuration
//! - Connection pooling for ruvector-service
//! - Stateless request handling

use schema_validation_agent::{
    handler::{create_router, AppState},
    telemetry::{TelemetryConfig, TelemetryEmitter},
    validation_engine::ValidationEngineConfig,
};
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};

/// Main entry point for the Edge Function
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize telemetry first for visibility into startup
    let telemetry_config = TelemetryConfig::default();
    if let Err(e) = TelemetryEmitter::init_tracing(telemetry_config.clone()) {
        eprintln!("Warning: Failed to initialize telemetry: {}", e);
        // Continue without telemetry - service should still work
    }

    info!(
        agent_id = %schema_validation_agent::types::AGENT_ID,
        agent_version = %schema_validation_agent::types::AGENT_VERSION,
        "Starting Schema Validation Agent"
    );

    // Load configuration from environment
    let ruvector_url = env::var("RUVECTOR_SERVICE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    let ruvector_api_key = env::var("RUVECTOR_API_KEY").ok();
    let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());

    info!(
        ruvector_url = %ruvector_url,
        environment = %environment,
        "Configuration loaded"
    );

    // Create validation engine configuration
    let validation_config = ValidationEngineConfig {
        strict_mode: env::var("STRICT_MODE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(false),
        max_schema_size: env::var("MAX_SCHEMA_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1_048_576), // 1MB default
        enable_semantic_checks: env::var("ENABLE_SEMANTIC_CHECKS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true),
        enable_performance_checks: env::var("ENABLE_PERFORMANCE_CHECKS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true),
        enable_security_checks: env::var("ENABLE_SECURITY_CHECKS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true),
        max_nesting_depth: env::var("MAX_NESTING_DEPTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50),
        max_properties: env::var("MAX_PROPERTIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(500),
        validation_timeout_ms: env::var("VALIDATION_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5000),
    };

    // Create application state
    let state = Arc::new(AppState::with_config(
        validation_config,
        ruvector_url.clone(),
        ruvector_api_key,
        environment.clone(),
    ));

    // Create router
    let app = create_router(state);

    // Get port from environment (Cloud Functions sets PORT)
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    info!(
        port = %port,
        addr = %addr,
        "Schema Validation Agent listening"
    );

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown handling
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        info!("Shutdown signal received");
    };

    info!("Server ready to accept connections");

    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    // Cleanup
    info!("Shutting down telemetry");
    TelemetryEmitter::shutdown();

    info!("Schema Validation Agent stopped");

    Ok(())
}
