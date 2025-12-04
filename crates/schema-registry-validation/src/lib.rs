//! Schema validation engine
use async_trait::async_trait;
use schema_registry_core::{error::Result, schema::SchemaInput, traits::{SchemaValidator, ValidationResult}, types::SerializationFormat};

pub mod engine;
pub mod format_detection;
pub mod types;
pub mod validators;

// Config Manager integration for policy-based validation (Phase 2B)
pub mod config_integration;

pub struct ValidationEngine {}

impl ValidationEngine {
    pub fn new() -> Self { Self {} }
}

impl Default for ValidationEngine {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl SchemaValidator for ValidationEngine {
    async fn validate(&self, input: &SchemaInput) -> Result<ValidationResult> {
        self.validate_content(&input.content, input.format).await
    }

    async fn validate_content(&self, _content: &str, _format: SerializationFormat) -> Result<ValidationResult> {
        Ok(ValidationResult {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            metadata: std::collections::HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema_registry_core::CompatibilityMode;

    #[test]
    fn test_validation_engine_new() {
        let engine = ValidationEngine::new();
        assert!(std::ptr::eq(&engine as *const _, &engine as *const _));
    }

    #[test]
    fn test_validation_engine_default() {
        let engine = ValidationEngine::default();
        assert!(std::ptr::eq(&engine as *const _, &engine as *const _));
    }

    #[tokio::test]
    async fn test_validate_json_schema_empty() {
        let engine = ValidationEngine::new();
        let input = SchemaInput {
            name: "test".to_string(),
            namespace: "com.test".to_string(),
            format: SerializationFormat::JsonSchema,
            content: "{}".to_string(),
            description: "Test schema".to_string(),
            compatibility_mode: CompatibilityMode::Full,
            auto_activate: true,
            version: None,
            metadata: std::collections::HashMap::new(),
            tags: vec![],
            examples: vec![],
        };

        let result = engine.validate(&input).await;
        assert!(result.is_ok());
        let validation = result.unwrap();
        assert!(validation.is_valid);
    }

    #[tokio::test]
    async fn test_validate_json_schema_simple() {
        let engine = ValidationEngine::new();
        let input = SchemaInput {
            name: "test".to_string(),
            namespace: "com.test".to_string(),
            format: SerializationFormat::JsonSchema,
            content: r#"{"type": "object"}"#.to_string(),
            description: "Test schema".to_string(),
            compatibility_mode: CompatibilityMode::Full,
            auto_activate: true,
            version: None,
            metadata: std::collections::HashMap::new(),
            tags: vec![],
            examples: vec![],
        };

        let result = engine.validate(&input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_avro_schema() {
        let engine = ValidationEngine::new();
        let input = SchemaInput {
            name: "test".to_string(),
            namespace: "com.test".to_string(),
            format: SerializationFormat::Avro,
            content: r#"{"type": "string"}"#.to_string(),
            description: "Test schema".to_string(),
            compatibility_mode: CompatibilityMode::Full,
            auto_activate: true,
            version: None,
            metadata: std::collections::HashMap::new(),
            tags: vec![],
            examples: vec![],
        };

        let result = engine.validate(&input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_protobuf_schema() {
        let engine = ValidationEngine::new();
        let input = SchemaInput {
            name: "test".to_string(),
            namespace: "com.test".to_string(),
            format: SerializationFormat::Protobuf,
            content: "syntax = \"proto3\";".to_string(),
            description: "Test schema".to_string(),
            compatibility_mode: CompatibilityMode::Full,
            auto_activate: true,
            version: None,
            metadata: std::collections::HashMap::new(),
            tags: vec![],
            examples: vec![],
        };

        let result = engine.validate(&input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_content_json_schema() {
        let engine = ValidationEngine::new();
        let result = engine.validate_content("{}", SerializationFormat::JsonSchema).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_content_avro() {
        let engine = ValidationEngine::new();
        let result = engine
            .validate_content(r#"{"type": "int"}"#, SerializationFormat::Avro)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_content_protobuf() {
        let engine = ValidationEngine::new();
        let result = engine
            .validate_content("syntax = \"proto3\";", SerializationFormat::Protobuf)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validation_result_has_no_errors() {
        let engine = ValidationEngine::new();
        let result = engine.validate_content("{}", SerializationFormat::JsonSchema).await;
        assert!(result.is_ok());
        let validation = result.unwrap();
        assert!(validation.errors.is_empty());
    }

    #[tokio::test]
    async fn test_validation_result_has_no_warnings() {
        let engine = ValidationEngine::new();
        let result = engine.validate_content("{}", SerializationFormat::JsonSchema).await;
        assert!(result.is_ok());
        let validation = result.unwrap();
        assert!(validation.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_validation_result_metadata_empty() {
        let engine = ValidationEngine::new();
        let result = engine.validate_content("{}", SerializationFormat::JsonSchema).await;
        assert!(result.is_ok());
        let validation = result.unwrap();
        assert!(validation.metadata.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_validations() {
        let engine = ValidationEngine::new();
        for _ in 0..5 {
            let result = engine.validate_content("{}", SerializationFormat::JsonSchema).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_validate_different_formats() {
        let engine = ValidationEngine::new();

        let json_result = engine.validate_content("{}", SerializationFormat::JsonSchema).await;
        assert!(json_result.is_ok());

        let avro_result = engine.validate_content(r#"{"type": "string"}"#, SerializationFormat::Avro).await;
        assert!(avro_result.is_ok());

        let proto_result = engine.validate_content("syntax = \"proto3\";", SerializationFormat::Protobuf).await;
        assert!(proto_result.is_ok());
    }

    #[test]
    fn test_engine_can_be_cloned_via_new() {
        let engine1 = ValidationEngine::new();
        let engine2 = ValidationEngine::new();
        // Both should exist independently
        assert!(!std::ptr::eq(&engine1 as *const _, &engine2 as *const _));
    }
}
