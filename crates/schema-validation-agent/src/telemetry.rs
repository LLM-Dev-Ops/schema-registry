//! LLM-Observatory Compatible Telemetry
//!
//! This module provides telemetry emission for the Schema Validation Agent,
//! compatible with LLM-Observatory's ingestion requirements. It includes:
//!
//! - OpenTelemetry tracing with OTLP export
//! - Prometheus metrics for monitoring
//! - Structured JSON logging for LLM-Observatory
//!
//! The telemetry is designed for serverless/edge function environments with
//! minimal overhead and fast cold starts.

use crate::decision_event::DecisionEvent;
use crate::types::{AGENT_ID, AGENT_VERSION};
use opentelemetry::{
    global,
    trace::{TraceContextExt, Tracer},
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    trace::{RandomIdGenerator, Sampler},
    Resource,
};
use prometheus::{
    register_counter_vec, register_histogram_vec, CounterVec, Encoder, HistogramVec,
    TextEncoder,
};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{error, info, instrument, warn, Span};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

/// Global metrics registry
static METRICS: OnceLock<TelemetryMetrics> = OnceLock::new();

/// Prometheus metrics for the validation agent
pub struct TelemetryMetrics {
    /// Validation latency histogram
    pub validation_latency: HistogramVec,
    /// Validation success counter
    pub validation_success: CounterVec,
    /// Validation failure counter
    pub validation_failure: CounterVec,
    /// Validation confidence histogram
    pub validation_confidence: HistogramVec,
    /// Schema format counter
    pub schema_format: CounterVec,
    /// Compatibility check counter
    pub compatibility_checks: CounterVec,
    /// Breaking changes detected counter
    pub breaking_changes: CounterVec,
    /// RuVector persistence counter
    pub ruvector_persistence: CounterVec,
    /// RuVector persistence latency
    pub ruvector_latency: HistogramVec,
}

impl TelemetryMetrics {
    /// Create and register all metrics
    fn new() -> Result<Self, prometheus::Error> {
        Ok(Self {
            validation_latency: register_histogram_vec!(
                "schema_validation_agent_validation_latency_seconds",
                "Schema validation latency in seconds",
                &["format", "mode"],
                vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
            )?,
            validation_success: register_counter_vec!(
                "schema_validation_agent_validation_success_total",
                "Total successful validations",
                &["format"]
            )?,
            validation_failure: register_counter_vec!(
                "schema_validation_agent_validation_failure_total",
                "Total failed validations",
                &["format", "error_type"]
            )?,
            validation_confidence: register_histogram_vec!(
                "schema_validation_agent_validation_confidence",
                "Validation confidence score distribution",
                &["format"],
                vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95, 1.0]
            )?,
            schema_format: register_counter_vec!(
                "schema_validation_agent_schema_format_total",
                "Schema format usage counter",
                &["format"]
            )?,
            compatibility_checks: register_counter_vec!(
                "schema_validation_agent_compatibility_checks_total",
                "Total compatibility checks",
                &["mode", "result"]
            )?,
            breaking_changes: register_counter_vec!(
                "schema_validation_agent_breaking_changes_total",
                "Breaking changes detected",
                &["violation_type"]
            )?,
            ruvector_persistence: register_counter_vec!(
                "schema_validation_agent_ruvector_persistence_total",
                "RuVector event persistence counter",
                &["result"]
            )?,
            ruvector_latency: register_histogram_vec!(
                "schema_validation_agent_ruvector_latency_seconds",
                "RuVector persistence latency in seconds",
                &[],
                vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
            )?,
        })
    }

    /// Get or initialize global metrics
    pub fn global() -> &'static TelemetryMetrics {
        METRICS.get_or_init(|| {
            TelemetryMetrics::new().expect("Failed to initialize metrics")
        })
    }
}

/// Telemetry configuration
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name for tracing
    pub service_name: String,
    /// Environment (development, staging, production)
    pub environment: String,
    /// OTLP endpoint for trace export
    pub otlp_endpoint: Option<String>,
    /// Sampling rate (0.0 to 1.0)
    pub sampling_rate: f64,
    /// Enable JSON logging
    pub json_logs: bool,
    /// Log level
    pub log_level: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: AGENT_ID.to_string(),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            otlp_endpoint: std::env::var("OTLP_ENDPOINT").ok(),
            sampling_rate: std::env::var("TRACE_SAMPLING_RATE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.1),
            json_logs: std::env::var("JSON_LOGS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            log_level: std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        }
    }
}

/// Telemetry emitter for the Schema Validation Agent
///
/// Handles emission of:
/// - OpenTelemetry spans for distributed tracing
/// - Prometheus metrics for monitoring
/// - Structured logs for LLM-Observatory
pub struct TelemetryEmitter {
    service_name: String,
    environment: String,
    metrics: &'static TelemetryMetrics,
}

impl TelemetryEmitter {
    /// Create a new telemetry emitter
    pub fn new(service_name: String, environment: String) -> Self {
        Self {
            service_name,
            environment,
            metrics: TelemetryMetrics::global(),
        }
    }

    /// Initialize OpenTelemetry tracing
    ///
    /// This should be called once at application startup.
    pub fn init_tracing(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
        // Create env filter
        let env_filter = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&config.log_level))
            .unwrap_or_else(|_| EnvFilter::new("info"));

        // Build subscriber based on configuration
        if let Some(ref otlp_endpoint) = config.otlp_endpoint {
            // Full tracing with OTLP export
            let tracer = init_otlp_tracer(&config, otlp_endpoint)?;
            let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

            if config.json_logs {
                let subscriber = Registry::default()
                    .with(env_filter)
                    .with(telemetry_layer)
                    .with(fmt::layer().json());
                tracing::subscriber::set_global_default(subscriber)?;
            } else {
                let subscriber = Registry::default()
                    .with(env_filter)
                    .with(telemetry_layer)
                    .with(fmt::layer());
                tracing::subscriber::set_global_default(subscriber)?;
            }
        } else {
            // Local tracing without OTLP
            if config.json_logs {
                let subscriber = Registry::default()
                    .with(env_filter)
                    .with(fmt::layer().json());
                tracing::subscriber::set_global_default(subscriber)?;
            } else {
                let subscriber = Registry::default()
                    .with(env_filter)
                    .with(fmt::layer());
                tracing::subscriber::set_global_default(subscriber)?;
            }
        }

        info!(
            service = %config.service_name,
            environment = %config.environment,
            sampling_rate = %config.sampling_rate,
            "Telemetry initialized"
        );

        Ok(())
    }

    /// Emit a span for a validation operation
    #[instrument(skip(self, event), fields(
        agent_id = %event.agent_id,
        decision_type = ?event.decision_type,
        confidence = %event.confidence,
        passed = %event.passed
    ))]
    pub fn emit_validation_span(&self, event: &DecisionEvent) {
        let format = format!("{:?}", event.format);

        info!(
            event_id = %event.event_id,
            content_hash = %event.content_hash,
            passed = %event.passed,
            duration_ms = %event.metrics.total_duration_ms,
            format = %format,
            "Schema validation completed"
        );
    }

    /// Emit Prometheus metrics for a DecisionEvent
    pub fn emit_metrics(&self, event: &DecisionEvent) {
        let format = format!("{:?}", event.format).to_lowercase();
        let mode = "strict"; // Could be extracted from event metadata

        // Validation latency
        let duration_secs = event.metrics.total_duration_ms as f64 / 1000.0;
        self.metrics
            .validation_latency
            .with_label_values(&[&format, mode])
            .observe(duration_secs);

        // Validation success/failure
        if event.passed {
            self.metrics
                .validation_success
                .with_label_values(&[&format])
                .inc();
        } else {
            let error_type = if !event.errors.is_empty() {
                &event.errors[0].code
            } else {
                "unknown"
            };
            self.metrics
                .validation_failure
                .with_label_values(&[&format, error_type])
                .inc();
        }

        // Confidence distribution
        self.metrics
            .validation_confidence
            .with_label_values(&[&format])
            .observe(event.confidence);

        // Schema format counter
        self.metrics
            .schema_format
            .with_label_values(&[&format])
            .inc();

        // Compatibility check metrics (if applicable)
        if !event.compatibility_violations.is_empty() {
            let result = "incompatible";
            let compat_mode = "unknown";

            self.metrics
                .compatibility_checks
                .with_label_values(&[compat_mode, result])
                .inc();

            // Breaking changes
            for violation in &event.compatibility_violations {
                self.metrics
                    .breaking_changes
                    .with_label_values(&[&violation.violation_type])
                    .inc();
            }
        }
    }

    /// Emit a structured log for LLM-Observatory ingestion
    pub fn emit_structured_log(&self, event: &DecisionEvent) {
        // Create a structured log entry compatible with LLM-Observatory
        let log_entry = LlmObservatoryLogEntry {
            timestamp: event.timestamp.to_rfc3339(),
            service: self.service_name.clone(),
            environment: self.environment.clone(),
            agent_id: event.agent_id.clone(),
            decision_type: format!("{:?}", event.decision_type),
            event_id: event.event_id.to_string(),
            content_hash: event.content_hash.clone(),
            confidence: event.confidence,
            passed: event.passed,
            has_compatibility_issues: !event.compatibility_violations.is_empty(),
            breaking_change_count: event.compatibility_violations.len(),
            validation_duration_ms: event.metrics.total_duration_ms,
            error_count: event.errors.len(),
            warning_count: event.warnings.len(),
            schema_format: format!("{:?}", event.format),
            namespace: event.namespace.clone(),
            name: event.name.clone(),
        };

        // Emit as JSON log line for LLM-Observatory ingestion
        if let Ok(json) = serde_json::to_string(&log_entry) {
            info!(
                target: "llm_observatory",
                event_type = "decision_event",
                "{}",
                json
            );
        }
    }

    /// Record RuVector persistence metrics
    pub fn record_persistence_result(&self, success: bool, latency_ms: u64) {
        let result = if success { "success" } else { "failure" };
        self.metrics
            .ruvector_persistence
            .with_label_values(&[result])
            .inc();

        self.metrics
            .ruvector_latency
            .with_label_values(&[])
            .observe(latency_ms as f64 / 1000.0);
    }

    /// Export metrics in Prometheus format
    pub fn export_metrics(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        encoder.encode_to_string(&metric_families)
    }

    /// Shutdown telemetry gracefully
    pub fn shutdown() {
        info!("Shutting down telemetry");
        global::shutdown_tracer_provider();
    }
}

/// Structured log entry for LLM-Observatory
#[derive(Debug, Serialize)]
struct LlmObservatoryLogEntry {
    timestamp: String,
    service: String,
    environment: String,
    agent_id: String,
    decision_type: String,
    event_id: String,
    content_hash: String,
    confidence: f64,
    passed: bool,
    has_compatibility_issues: bool,
    breaking_change_count: usize,
    validation_duration_ms: u64,
    error_count: usize,
    warning_count: usize,
    schema_format: String,
    namespace: Option<String>,
    name: Option<String>,
}

/// Initialize OTLP tracer
fn init_otlp_tracer(
    config: &TelemetryConfig,
    endpoint: &str,
) -> Result<opentelemetry_sdk::trace::Tracer, Box<dyn std::error::Error>> {
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint)
        .with_timeout(Duration::from_secs(3));

    let trace_config = opentelemetry_sdk::trace::config()
        .with_sampler(Sampler::ParentBased(Box::new(
            Sampler::TraceIdRatioBased(config.sampling_rate),
        )))
        .with_id_generator(RandomIdGenerator::default())
        .with_max_events_per_span(64)
        .with_max_attributes_per_span(32)
        .with_resource(Resource::new(vec![
            KeyValue::new("service.name", config.service_name.clone()),
            KeyValue::new("service.version", AGENT_VERSION),
            KeyValue::new("deployment.environment", config.environment.clone()),
            KeyValue::new("telemetry.sdk.name", "opentelemetry"),
            KeyValue::new("telemetry.sdk.language", "rust"),
        ]));

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(trace_config)
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    Ok(tracer)
}

impl Default for TelemetryEmitter {
    fn default() -> Self {
        Self::new(
            AGENT_ID.to_string(),
            std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_defaults() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, AGENT_ID);
        assert!(config.sampling_rate >= 0.0 && config.sampling_rate <= 1.0);
    }

    #[test]
    fn test_telemetry_emitter_creation() {
        let emitter = TelemetryEmitter::new(
            "test-service".to_string(),
            "test".to_string(),
        );
        assert_eq!(emitter.service_name, "test-service");
        assert_eq!(emitter.environment, "test");
    }

    #[test]
    fn test_metrics_initialization() {
        let metrics = TelemetryMetrics::global();
        // Should not panic
        metrics.validation_success.with_label_values(&["jsonschema"]).inc();
    }

    #[test]
    fn test_llm_observatory_log_serialization() {
        let entry = LlmObservatoryLogEntry {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            service: "test".to_string(),
            environment: "test".to_string(),
            agent_id: AGENT_ID.to_string(),
            agent_version: AGENT_VERSION.to_string(),
            decision_type: "SchemaValidationResult".to_string(),
            event_id: "test-id".to_string(),
            execution_ref: "exec-123".to_string(),
            inputs_hash: "abc123".to_string(),
            confidence: 0.95,
            is_valid: true,
            is_compatible: Some(true),
            breaking_change_count: 0,
            validation_duration_ms: 100,
            rules_checked: 10,
            rules_passed: 10,
            rules_failed: 0,
            error_count: 0,
            warning_count: 0,
            schema_format: "JsonSchema".to_string(),
            schema_scope: "test:TestSchema".to_string(),
            compatibility_mode: Some("BACKWARD".to_string()),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("schema-validation-agent"));
        assert!(json.contains("test:TestSchema"));
    }
}
