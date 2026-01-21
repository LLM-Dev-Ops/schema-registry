//! Schema Validation Agent
//!
//! This crate provides a deterministic, stateless schema validation engine
//! for the LLM Schema Registry.
//!
//! # Architecture
//!
//! The Schema Validation Agent follows strict design principles:
//!
//! ## MUST:
//! - Validate schema structure and syntax
//! - Detect schema format (JSON Schema, Avro, Protobuf)
//! - Verify type correctness and constraints
//! - Detect deprecated or invalid schema elements
//! - Identify breaking vs non-breaking changes
//! - Assess backward and forward compatibility
//! - Produce deterministic validation results
//! - Track validation metrics
//!
//! ## MUST NOT:
//! - Modify schemas
//! - Auto-correct definitions
//! - Execute any enforcement logic
//! - Connect directly to databases (use ruvector-service)
//!
//! # Examples
//!
//! ## Basic validation
//!
//! ```rust
//! use schema_validation_agent::{ValidationEngine, ValidationInput, SchemaFormat};
//!
//! let engine = ValidationEngine::default();
//!
//! let input = ValidationInput::new(r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#)
//!     .with_format(SchemaFormat::JsonSchema);
//!
//! let result = engine.validate(&input).unwrap();
//! assert!(result.is_valid);
//! ```
//!
//! ## Compatibility checking
//!
//! ```rust
//! use schema_validation_agent::{ValidationEngine, ValidationInput, SchemaFormat, CompatibilityMode};
//!
//! let engine = ValidationEngine::default();
//!
//! let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
//! let new_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "number"}}}"#;
//!
//! let input = ValidationInput::new(new_schema)
//!     .with_format(SchemaFormat::JsonSchema)
//!     .with_previous(old_schema)
//!     .with_compatibility_mode(CompatibilityMode::Backward);
//!
//! let result = engine.validate(&input).unwrap();
//! assert!(result.is_compatible());
//! ```
//!
//! ## Persisting decisions (through ruvector-service)
//!
//! ```rust,no_run
//! use schema_validation_agent::{
//!     ValidationEngine, ValidationInput, SchemaFormat,
//!     DecisionEvent, RuVectorClient, RuVectorClientConfig
//! };
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Validate schema
//! let engine = ValidationEngine::default();
//! let input = ValidationInput::new(r#"{"type": "object"}"#)
//!     .with_format(SchemaFormat::JsonSchema);
//! let result = engine.validate(&input)?;
//!
//! // Create decision event
//! let event = DecisionEvent::from_validation(
//!     &result,
//!     Some("my-namespace".to_string()),
//!     Some("my-schema".to_string()),
//!     Some("1.0.0".to_string()),
//! );
//!
//! // Persist through ruvector-service (NOT directly to database!)
//! let client = RuVectorClient::with_url("http://ruvector-service:8080")?;
//! let response = client.persist_decision_event(&event).await?;
//! println!("Persisted: {}", response.event_id);
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]
#![deny(unsafe_code)]

pub mod compatibility_checker;
pub mod decision_event;
pub mod handler;
pub mod ruvector_client;
pub mod telemetry;
pub mod types;
pub mod validation_engine;

// Re-export main types for convenience
pub use compatibility_checker::{CompatibilityChecker, CompatibilityCheckerConfig, CompatibilityCheckerError};
pub use decision_event::{DecisionEvent, DecisionType};
pub use ruvector_client::{RuVectorClient, RuVectorClientConfig, RuVectorError};
pub use types::{
    CompatibilityCheckResult, CompatibilityMode, CompatibilityViolation, ErrorCategory,
    ErrorSeverity, SchemaFormat, ValidationError, ValidationInput, ValidationMetrics,
    ValidationOptions, ValidationResult, ViolationType,
};
pub use validation_engine::{ValidationEngine, ValidationEngineConfig, ValidationEngineError};

// Re-export handler types for Edge Function usage
pub use handler::{create_router, AppState};

// Re-export telemetry for observability
pub use telemetry::{TelemetryConfig, TelemetryEmitter};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::compatibility_checker::{CompatibilityChecker, CompatibilityCheckerConfig};
    pub use crate::decision_event::{DecisionEvent, DecisionType};
    pub use crate::handler::{create_router, AppState};
    pub use crate::ruvector_client::{RuVectorClient, RuVectorClientConfig};
    pub use crate::telemetry::{TelemetryConfig, TelemetryEmitter};
    pub use crate::types::{
        CompatibilityCheckResult, CompatibilityMode, CompatibilityViolation, ErrorCategory,
        ErrorSeverity, SchemaFormat, ValidationError, ValidationInput, ValidationMetrics,
        ValidationOptions, ValidationResult, ViolationType,
    };
    pub use crate::validation_engine::{ValidationEngine, ValidationEngineConfig};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_api_validation() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{"type": "object"}"#)
            .with_format(SchemaFormat::JsonSchema);

        let result = engine.validate(&input).unwrap();
        assert!(result.is_valid);
    }

    #[test]
    fn test_public_api_compatibility() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "number"}}}"#;

        let result = checker
            .check_compatibility(new_schema, old_schema, &SchemaFormat::JsonSchema, CompatibilityMode::Backward)
            .unwrap();

        assert!(result.is_compatible);
    }

    #[test]
    fn test_decision_event_creation() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{"type": "object"}"#)
            .with_format(SchemaFormat::JsonSchema);

        let result = engine.validate(&input).unwrap();
        let event = DecisionEvent::from_validation(&result, None, None, None);

        assert!(!event.event_id.is_nil());
        assert!(event.passed);
    }

    #[test]
    fn test_prelude_imports() {
        use crate::prelude::*;

        let _engine = ValidationEngine::default();
        let _checker = CompatibilityChecker::default();
        let _input = ValidationInput::new("{}");
    }

    #[test]
    fn test_format_detection() {
        let engine = ValidationEngine::default();

        // JSON Schema
        let json_schema = r#"{"type": "object", "properties": {}}"#;
        let input = ValidationInput::new(json_schema);
        let result = engine.validate(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().detected_format, SchemaFormat::JsonSchema);

        // Avro
        let avro_schema = r#"{"type": "record", "name": "Test", "namespace": "com.test", "fields": []}"#;
        let input = ValidationInput::new(avro_schema);
        let result = engine.validate(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().detected_format, SchemaFormat::Avro);

        // Protobuf
        let proto_schema = r#"syntax = "proto3"; message Test {}"#;
        let input = ValidationInput::new(proto_schema);
        let result = engine.validate(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().detected_format, SchemaFormat::Protobuf);
    }

    #[test]
    fn test_validation_metrics() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#)
            .with_format(SchemaFormat::JsonSchema);

        let result = engine.validate(&input).unwrap();

        // Metrics should be populated
        assert!(result.metrics.total_duration_ms >= 0);
        assert!(result.metrics.schema_size_bytes > 0);
        assert!(!result.content_hash.is_empty());
    }

    #[test]
    fn test_error_handling() {
        let engine = ValidationEngine::default();

        // Invalid JSON
        let input = ValidationInput::new("not valid json")
            .with_format(SchemaFormat::JsonSchema);
        let result = engine.validate(&input);
        assert!(result.is_err());

        // Invalid Avro
        let input = ValidationInput::new(r#"{"type": "invalid_type"}"#)
            .with_format(SchemaFormat::Avro);
        let result = engine.validate(&input);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_valid);
    }
}
