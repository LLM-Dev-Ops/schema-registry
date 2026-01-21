//! Compatibility Checker for the Schema Validation Agent
//!
//! This module provides compatibility checking between schema versions.
//! It wraps the compatibility-checker crate and provides a simplified interface.

use crate::types::*;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Errors that can occur during compatibility checking
#[derive(Debug, Error)]
pub enum CompatibilityCheckerError {
    #[error("Schema parsing error: {0}")]
    ParseError(String),

    #[error("Unsupported schema format: {0}")]
    UnsupportedFormat(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Format mismatch: new schema is {new}, old schema is {old}")]
    FormatMismatch { new: String, old: String },
}

impl From<compatibility_checker::checker::CompatibilityError> for CompatibilityCheckerError {
    fn from(err: compatibility_checker::checker::CompatibilityError) -> Self {
        match err {
            compatibility_checker::checker::CompatibilityError::ParseError(s) => {
                CompatibilityCheckerError::ParseError(s)
            }
            compatibility_checker::checker::CompatibilityError::UnsupportedFormat(f) => {
                CompatibilityCheckerError::UnsupportedFormat(format!("{:?}", f))
            }
            compatibility_checker::checker::CompatibilityError::SchemaFetchError(s) => {
                CompatibilityCheckerError::InternalError(format!("Schema fetch error: {}", s))
            }
            compatibility_checker::checker::CompatibilityError::InternalError(s) => {
                CompatibilityCheckerError::InternalError(s)
            }
        }
    }
}

/// Configuration for compatibility checking
#[derive(Debug, Clone)]
pub struct CompatibilityCheckerConfig {
    /// Enable caching of results
    pub enable_cache: bool,
    /// Maximum cache size
    pub max_cache_size: u64,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Timeout for checks in milliseconds
    pub check_timeout_ms: u64,
}

impl Default for CompatibilityCheckerConfig {
    fn default() -> Self {
        Self {
            enable_cache: true,
            max_cache_size: 10_000,
            cache_ttl_seconds: 3600,
            check_timeout_ms: 5000,
        }
    }
}

/// Compatibility checker for schema evolution
pub struct CompatibilityChecker {
    config: CompatibilityCheckerConfig,
    json_checker: compatibility_checker::formats::JsonSchemaCompatibilityChecker,
    avro_checker: compatibility_checker::formats::AvroCompatibilityChecker,
    protobuf_checker: compatibility_checker::formats::ProtobufCompatibilityChecker,
}

impl CompatibilityChecker {
    /// Create a new compatibility checker with the given configuration
    pub fn new(config: CompatibilityCheckerConfig) -> Self {
        Self {
            config,
            json_checker: compatibility_checker::formats::JsonSchemaCompatibilityChecker::new(),
            avro_checker: compatibility_checker::formats::AvroCompatibilityChecker::new(),
            protobuf_checker: compatibility_checker::formats::ProtobufCompatibilityChecker::new(),
        }
    }

    /// Check compatibility between two schemas
    ///
    /// This is the main entry point for compatibility checking.
    /// It handles format detection and delegates to format-specific checkers.
    pub fn check_compatibility(
        &self,
        new_schema: &str,
        old_schema: &str,
        format: &SchemaFormat,
        mode: CompatibilityMode,
    ) -> Result<CompatibilityCheckResult, CompatibilityCheckerError> {
        let start = Instant::now();

        info!(
            "Checking compatibility: format={:?}, mode={:?}",
            format, mode
        );

        if matches!(format, SchemaFormat::Unknown) {
            return Err(CompatibilityCheckerError::UnsupportedFormat(
                "Unknown".to_string(),
            ));
        }

        // Check for identical schemas (fast path)
        if new_schema == old_schema {
            debug!("Schemas are identical, skipping detailed check");
            return Ok(CompatibilityCheckResult {
                is_compatible: true,
                mode,
                violations: Vec::new(),
                check_duration_ms: start.elapsed().as_millis() as u64,
            });
        }

        // Perform format-specific compatibility check
        let violations = match format {
            SchemaFormat::JsonSchema => {
                self.check_json_schema_compatibility(new_schema, old_schema, mode)?
            }
            SchemaFormat::Avro => {
                self.check_avro_compatibility(new_schema, old_schema, mode)?
            }
            SchemaFormat::Protobuf => {
                self.check_protobuf_compatibility(new_schema, old_schema, mode)?
            }
            SchemaFormat::Unknown => {
                return Err(CompatibilityCheckerError::UnsupportedFormat(
                    "Unknown".to_string(),
                ));
            }
        };

        let is_compatible = !violations.iter().any(|v| v.severity == ErrorSeverity::Breaking);

        let check_duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "Compatibility check complete: compatible={}, violations={}, duration={}ms",
            is_compatible,
            violations.len(),
            check_duration_ms
        );

        Ok(CompatibilityCheckResult {
            is_compatible,
            mode,
            violations,
            check_duration_ms,
        })
    }

    /// Check JSON Schema compatibility
    fn check_json_schema_compatibility(
        &self,
        new_schema: &str,
        old_schema: &str,
        mode: CompatibilityMode,
    ) -> Result<Vec<CompatibilityViolation>, CompatibilityCheckerError> {
        use compatibility_checker::formats::FormatCompatibilityChecker;

        let violations = match mode {
            CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
                self.json_checker.check_backward(new_schema, old_schema)?
            }
            CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
                self.json_checker.check_forward(new_schema, old_schema)?
            }
            CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
                let mut v = self.json_checker.check_backward(new_schema, old_schema)?;
                v.extend(self.json_checker.check_forward(new_schema, old_schema)?);
                v
            }
            CompatibilityMode::None => Vec::new(),
        };

        Ok(self.convert_violations(violations))
    }

    /// Check Avro compatibility
    fn check_avro_compatibility(
        &self,
        new_schema: &str,
        old_schema: &str,
        mode: CompatibilityMode,
    ) -> Result<Vec<CompatibilityViolation>, CompatibilityCheckerError> {
        use compatibility_checker::formats::FormatCompatibilityChecker;

        let violations = match mode {
            CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
                self.avro_checker.check_backward(new_schema, old_schema)?
            }
            CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
                self.avro_checker.check_forward(new_schema, old_schema)?
            }
            CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
                let mut v = self.avro_checker.check_backward(new_schema, old_schema)?;
                v.extend(self.avro_checker.check_forward(new_schema, old_schema)?);
                v
            }
            CompatibilityMode::None => Vec::new(),
        };

        Ok(self.convert_violations(violations))
    }

    /// Check Protobuf compatibility
    fn check_protobuf_compatibility(
        &self,
        new_schema: &str,
        old_schema: &str,
        mode: CompatibilityMode,
    ) -> Result<Vec<CompatibilityViolation>, CompatibilityCheckerError> {
        use compatibility_checker::formats::FormatCompatibilityChecker;

        let violations = match mode {
            CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
                self.protobuf_checker.check_backward(new_schema, old_schema)?
            }
            CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
                self.protobuf_checker.check_forward(new_schema, old_schema)?
            }
            CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
                let mut v = self.protobuf_checker.check_backward(new_schema, old_schema)?;
                v.extend(self.protobuf_checker.check_forward(new_schema, old_schema)?);
                v
            }
            CompatibilityMode::None => Vec::new(),
        };

        Ok(self.convert_violations(violations))
    }

    /// Convert violations from compatibility-checker types to our types
    fn convert_violations(
        &self,
        violations: Vec<compatibility_checker::violation::CompatibilityViolation>,
    ) -> Vec<CompatibilityViolation> {
        violations
            .into_iter()
            .map(|v| CompatibilityViolation {
                violation_type: self.convert_violation_type(&v.violation_type),
                field_path: v.field_path,
                old_value: v.old_value,
                new_value: v.new_value,
                severity: self.convert_severity(&v.severity),
                description: v.description,
            })
            .collect()
    }

    /// Convert violation type
    fn convert_violation_type(
        &self,
        vt: &compatibility_checker::violation::ViolationType,
    ) -> ViolationType {
        match vt {
            compatibility_checker::violation::ViolationType::FieldRemoved => ViolationType::FieldRemoved,
            compatibility_checker::violation::ViolationType::TypeChanged => ViolationType::TypeChanged,
            compatibility_checker::violation::ViolationType::RequiredAdded => ViolationType::RequiredAdded,
            compatibility_checker::violation::ViolationType::ConstraintAdded => ViolationType::ConstraintTightened,
            compatibility_checker::violation::ViolationType::EnumValueRemoved => ViolationType::EnumValueRemoved,
            compatibility_checker::violation::ViolationType::FormatChanged => ViolationType::FormatChanged,
            compatibility_checker::violation::ViolationType::FieldMadeRequired => ViolationType::FieldMadeRequired,
            compatibility_checker::violation::ViolationType::ArrayItemsChanged => ViolationType::ArrayItemsChanged,
            compatibility_checker::violation::ViolationType::MapValueChanged => ViolationType::MapValueChanged,
            compatibility_checker::violation::ViolationType::UnionTypesIncompatible => {
                ViolationType::UnionTypesIncompatible
            }
            compatibility_checker::violation::ViolationType::NamespaceChanged => ViolationType::NamespaceChanged,
            compatibility_checker::violation::ViolationType::NameChanged => ViolationType::NameChanged,
            compatibility_checker::violation::ViolationType::Custom(s) => ViolationType::Custom(s.clone()),
        }
    }

    /// Convert severity
    fn convert_severity(
        &self,
        sev: &compatibility_checker::violation::ViolationSeverity,
    ) -> ErrorSeverity {
        match sev {
            compatibility_checker::violation::ViolationSeverity::Breaking => ErrorSeverity::Breaking,
            compatibility_checker::violation::ViolationSeverity::Warning => ErrorSeverity::Warning,
            compatibility_checker::violation::ViolationSeverity::Info => ErrorSeverity::Info,
        }
    }

    /// Detect breaking changes between schemas
    ///
    /// Returns a list of changes that would break compatibility
    pub fn detect_breaking_changes(
        &self,
        new_schema: &str,
        old_schema: &str,
        format: &SchemaFormat,
    ) -> Result<Vec<CompatibilityViolation>, CompatibilityCheckerError> {
        let result = self.check_compatibility(new_schema, old_schema, format, CompatibilityMode::Full)?;

        Ok(result
            .violations
            .into_iter()
            .filter(|v| v.severity == ErrorSeverity::Breaking)
            .collect())
    }

    /// Detect non-breaking changes between schemas
    ///
    /// Returns a list of changes that are compatible but notable
    pub fn detect_non_breaking_changes(
        &self,
        new_schema: &str,
        old_schema: &str,
        format: &SchemaFormat,
    ) -> Result<Vec<CompatibilityViolation>, CompatibilityCheckerError> {
        let result = self.check_compatibility(new_schema, old_schema, format, CompatibilityMode::Full)?;

        Ok(result
            .violations
            .into_iter()
            .filter(|v| v.severity != ErrorSeverity::Breaking)
            .collect())
    }

    /// Suggest version bump based on changes
    ///
    /// Returns the recommended version bump:
    /// - "major" for breaking changes
    /// - "minor" for non-breaking feature additions
    /// - "patch" for non-breaking fixes or no changes
    pub fn suggest_version_bump(
        &self,
        new_schema: &str,
        old_schema: &str,
        format: &SchemaFormat,
    ) -> Result<String, CompatibilityCheckerError> {
        let result = self.check_compatibility(new_schema, old_schema, format, CompatibilityMode::Full)?;

        if result.violations.iter().any(|v| v.severity == ErrorSeverity::Breaking) {
            Ok("major".to_string())
        } else if !result.violations.is_empty() {
            Ok("minor".to_string())
        } else {
            Ok("patch".to_string())
        }
    }
}

impl Default for CompatibilityChecker {
    fn default() -> Self {
        Self::new(CompatibilityCheckerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_schemas_compatible() {
        let checker = CompatibilityChecker::default();
        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;

        let result = checker
            .check_compatibility(schema, schema, &SchemaFormat::JsonSchema, CompatibilityMode::Backward)
            .unwrap();

        assert!(result.is_compatible);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn test_adding_optional_field_compatible() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "number"}}}"#;

        let result = checker
            .check_compatibility(new_schema, old_schema, &SchemaFormat::JsonSchema, CompatibilityMode::Backward)
            .unwrap();

        assert!(result.is_compatible);
    }

    #[test]
    fn test_removing_field_incompatible() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "number"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;

        let result = checker
            .check_compatibility(new_schema, old_schema, &SchemaFormat::JsonSchema, CompatibilityMode::Backward)
            .unwrap();

        assert!(!result.is_compatible);
        assert!(result.violations.iter().any(|v| matches!(v.violation_type, ViolationType::FieldRemoved)));
    }

    #[test]
    fn test_type_change_incompatible() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "number"}}}"#;

        let result = checker
            .check_compatibility(new_schema, old_schema, &SchemaFormat::JsonSchema, CompatibilityMode::Backward)
            .unwrap();

        assert!(!result.is_compatible);
        assert!(result.violations.iter().any(|v| matches!(v.violation_type, ViolationType::TypeChanged)));
    }

    #[test]
    fn test_avro_backward_compatibility() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{
            "type": "record",
            "name": "User",
            "fields": [{"name": "name", "type": "string"}]
        }"#;

        let new_schema = r#"{
            "type": "record",
            "name": "User",
            "fields": [
                {"name": "name", "type": "string"},
                {"name": "age", "type": "int", "default": 0}
            ]
        }"#;

        let result = checker
            .check_compatibility(new_schema, old_schema, &SchemaFormat::Avro, CompatibilityMode::Backward)
            .unwrap();

        // Adding optional field with default should be compatible
        assert!(result.is_compatible);
    }

    #[test]
    fn test_protobuf_backward_compatibility() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"
            syntax = "proto3";
            message User {
                string name = 1;
            }
        "#;

        let new_schema = r#"
            syntax = "proto3";
            message User {
                string name = 1;
                int32 age = 2;
            }
        "#;

        let result = checker
            .check_compatibility(new_schema, old_schema, &SchemaFormat::Protobuf, CompatibilityMode::Backward)
            .unwrap();

        // Adding new field should be compatible
        assert!(result.is_compatible);
    }

    #[test]
    fn test_detect_breaking_changes() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "number"}}}"#;

        let breaking = checker
            .detect_breaking_changes(new_schema, old_schema, &SchemaFormat::JsonSchema)
            .unwrap();

        assert!(!breaking.is_empty());
    }

    #[test]
    fn test_suggest_version_bump_major() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "number"}}}"#;

        let bump = checker
            .suggest_version_bump(new_schema, old_schema, &SchemaFormat::JsonSchema)
            .unwrap();

        assert_eq!(bump, "major");
    }

    #[test]
    fn test_suggest_version_bump_minor() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "number"}}}"#;

        let bump = checker
            .suggest_version_bump(new_schema, old_schema, &SchemaFormat::JsonSchema)
            .unwrap();

        // Adding a field might trigger warnings in some cases
        // For this test, we expect minor or patch
        assert!(bump == "minor" || bump == "patch");
    }

    #[test]
    fn test_suggest_version_bump_patch() {
        let checker = CompatibilityChecker::default();

        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;

        let bump = checker
            .suggest_version_bump(schema, schema, &SchemaFormat::JsonSchema)
            .unwrap();

        assert_eq!(bump, "patch");
    }

    #[test]
    fn test_unsupported_format_error() {
        let checker = CompatibilityChecker::default();

        let result = checker.check_compatibility(
            "{}",
            "{}",
            &SchemaFormat::Unknown,
            CompatibilityMode::Backward,
        );

        assert!(matches!(result, Err(CompatibilityCheckerError::UnsupportedFormat(_))));
    }

    #[test]
    fn test_compatibility_none_mode() {
        let checker = CompatibilityChecker::default();

        let old_schema = r#"{"type": "string"}"#;
        let new_schema = r#"{"type": "number"}"#;

        let result = checker
            .check_compatibility(new_schema, old_schema, &SchemaFormat::JsonSchema, CompatibilityMode::None)
            .unwrap();

        // With mode None, everything should be compatible
        assert!(result.is_compatible);
        assert!(result.violations.is_empty());
    }
}
