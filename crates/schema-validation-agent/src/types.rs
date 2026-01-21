//! Core types for the Schema Validation Agent
//!
//! Defines validation inputs, outputs, and related data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent identification constant
pub const AGENT_ID: &str = "schema-validation-agent";

/// Agent semantic version
pub const AGENT_VERSION: &str = "1.0.0";

/// Schema format detection result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchemaFormat {
    /// JSON Schema format
    JsonSchema,
    /// Apache Avro format
    Avro,
    /// Protocol Buffers format
    Protobuf,
    /// Unknown format
    Unknown,
}

impl std::fmt::Display for SchemaFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaFormat::JsonSchema => write!(f, "JSON_SCHEMA"),
            SchemaFormat::Avro => write!(f, "AVRO"),
            SchemaFormat::Protobuf => write!(f, "PROTOBUF"),
            SchemaFormat::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

impl From<schema_registry_core::types::SerializationFormat> for SchemaFormat {
    fn from(format: schema_registry_core::types::SerializationFormat) -> Self {
        match format {
            schema_registry_core::types::SerializationFormat::JsonSchema => SchemaFormat::JsonSchema,
            schema_registry_core::types::SerializationFormat::Avro => SchemaFormat::Avro,
            schema_registry_core::types::SerializationFormat::Protobuf => SchemaFormat::Protobuf,
        }
    }
}

impl From<SchemaFormat> for Option<schema_registry_core::types::SerializationFormat> {
    fn from(format: SchemaFormat) -> Self {
        match format {
            SchemaFormat::JsonSchema => Some(schema_registry_core::types::SerializationFormat::JsonSchema),
            SchemaFormat::Avro => Some(schema_registry_core::types::SerializationFormat::Avro),
            SchemaFormat::Protobuf => Some(schema_registry_core::types::SerializationFormat::Protobuf),
            SchemaFormat::Unknown => None,
        }
    }
}

/// Input for schema validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationInput {
    /// Raw schema content to validate
    pub content: String,
    /// Optional format hint (if not provided, will be auto-detected)
    pub format_hint: Option<SchemaFormat>,
    /// Schema namespace for context
    pub namespace: Option<String>,
    /// Schema name for context
    pub name: Option<String>,
    /// Version being validated (for compatibility checks)
    pub version: Option<String>,
    /// Previous schema content (for compatibility checks)
    pub previous_content: Option<String>,
    /// Compatibility mode to check against
    pub compatibility_mode: Option<CompatibilityMode>,
    /// Additional validation options
    pub options: ValidationOptions,
}

impl Default for ValidationInput {
    fn default() -> Self {
        Self {
            content: String::new(),
            format_hint: None,
            namespace: None,
            name: None,
            version: None,
            previous_content: None,
            compatibility_mode: None,
            options: ValidationOptions::default(),
        }
    }
}

impl ValidationInput {
    /// Create a new validation input with content
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Default::default()
        }
    }

    /// Set the format hint
    pub fn with_format(mut self, format: SchemaFormat) -> Self {
        self.format_hint = Some(format);
        self
    }

    /// Set previous content for compatibility checking
    pub fn with_previous(mut self, previous: impl Into<String>) -> Self {
        self.previous_content = Some(previous.into());
        self
    }

    /// Set compatibility mode
    pub fn with_compatibility_mode(mut self, mode: CompatibilityMode) -> Self {
        self.compatibility_mode = Some(mode);
        self
    }
}

/// Options for validation behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationOptions {
    /// Enable strict validation mode (stricter checks)
    pub strict_mode: bool,
    /// Enable semantic validation (beyond syntax)
    pub enable_semantic_checks: bool,
    /// Enable performance-related checks (complexity limits)
    pub enable_performance_checks: bool,
    /// Enable security checks (detect malicious patterns)
    pub enable_security_checks: bool,
    /// Maximum allowed schema size in bytes
    pub max_schema_size: usize,
    /// Maximum allowed nesting depth
    pub max_nesting_depth: usize,
    /// Maximum number of properties/fields allowed
    pub max_properties: usize,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            strict_mode: false,
            enable_semantic_checks: true,
            enable_performance_checks: true,
            enable_security_checks: true,
            max_schema_size: 1_048_576, // 1 MB
            max_nesting_depth: 50,
            max_properties: 500,
        }
    }
}

/// Compatibility mode for schema evolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompatibilityMode {
    /// New schema can read old data
    Backward,
    /// Old schema can read new data
    Forward,
    /// Both backward and forward compatible
    Full,
    /// No compatibility required
    None,
    /// Backward compatible with all previous versions
    BackwardTransitive,
    /// Forward compatible with all previous versions
    ForwardTransitive,
    /// Full compatible with all previous versions
    FullTransitive,
}

impl From<schema_registry_core::types::CompatibilityMode> for CompatibilityMode {
    fn from(mode: schema_registry_core::types::CompatibilityMode) -> Self {
        match mode {
            schema_registry_core::types::CompatibilityMode::Backward => CompatibilityMode::Backward,
            schema_registry_core::types::CompatibilityMode::Forward => CompatibilityMode::Forward,
            schema_registry_core::types::CompatibilityMode::Full => CompatibilityMode::Full,
            schema_registry_core::types::CompatibilityMode::None => CompatibilityMode::None,
            schema_registry_core::types::CompatibilityMode::BackwardTransitive => CompatibilityMode::BackwardTransitive,
            schema_registry_core::types::CompatibilityMode::ForwardTransitive => CompatibilityMode::ForwardTransitive,
            schema_registry_core::types::CompatibilityMode::FullTransitive => CompatibilityMode::FullTransitive,
        }
    }
}

impl From<CompatibilityMode> for compatibility_checker::types::CompatibilityMode {
    fn from(mode: CompatibilityMode) -> Self {
        match mode {
            CompatibilityMode::Backward => compatibility_checker::types::CompatibilityMode::Backward,
            CompatibilityMode::Forward => compatibility_checker::types::CompatibilityMode::Forward,
            CompatibilityMode::Full => compatibility_checker::types::CompatibilityMode::Full,
            CompatibilityMode::None => compatibility_checker::types::CompatibilityMode::None,
            CompatibilityMode::BackwardTransitive => compatibility_checker::types::CompatibilityMode::BackwardTransitive,
            CompatibilityMode::ForwardTransitive => compatibility_checker::types::CompatibilityMode::ForwardTransitive,
            CompatibilityMode::FullTransitive => compatibility_checker::types::CompatibilityMode::FullTransitive,
        }
    }
}

impl CompatibilityMode {
    /// Check if this mode requires transitive checking
    pub fn is_transitive(&self) -> bool {
        matches!(
            self,
            CompatibilityMode::BackwardTransitive
                | CompatibilityMode::ForwardTransitive
                | CompatibilityMode::FullTransitive
        )
    }
}

/// Result of schema validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Unique ID for this validation
    pub validation_id: Uuid,
    /// Whether the schema is valid
    pub is_valid: bool,
    /// Detected schema format
    pub detected_format: SchemaFormat,
    /// List of validation errors
    pub errors: Vec<ValidationError>,
    /// List of validation warnings
    pub warnings: Vec<ValidationError>,
    /// Compatibility check result (if applicable)
    pub compatibility: Option<CompatibilityCheckResult>,
    /// Validation metrics
    pub metrics: ValidationMetrics,
    /// Content hash of the validated schema
    pub content_hash: String,
    /// Timestamp of validation
    pub validated_at: DateTime<Utc>,
}

impl ValidationResult {
    /// Check if there are any breaking errors
    pub fn has_breaking_errors(&self) -> bool {
        self.errors.iter().any(|e| e.severity == ErrorSeverity::Breaking)
    }

    /// Get only breaking errors
    pub fn breaking_errors(&self) -> Vec<&ValidationError> {
        self.errors
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Breaking)
            .collect()
    }

    /// Check if compatible (considering compatibility result if present)
    pub fn is_compatible(&self) -> bool {
        self.compatibility.as_ref().map(|c| c.is_compatible).unwrap_or(true)
    }
}

/// Compatibility check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityCheckResult {
    /// Whether schemas are compatible
    pub is_compatible: bool,
    /// Compatibility mode that was checked
    pub mode: CompatibilityMode,
    /// List of compatibility violations
    pub violations: Vec<CompatibilityViolation>,
    /// Check duration in milliseconds
    pub check_duration_ms: u64,
}

/// Compatibility violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityViolation {
    /// Type of violation
    pub violation_type: ViolationType,
    /// Field path where violation occurred
    pub field_path: String,
    /// Old value (JSON representation)
    pub old_value: Option<serde_json::Value>,
    /// New value (JSON representation)
    pub new_value: Option<serde_json::Value>,
    /// Severity of the violation
    pub severity: ErrorSeverity,
    /// Human-readable description
    pub description: String,
}

/// Type of compatibility violation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ViolationType {
    /// Field was removed
    FieldRemoved,
    /// Field type changed incompatibly
    TypeChanged,
    /// New required field added without default
    RequiredAdded,
    /// Constraint was tightened
    ConstraintTightened,
    /// Enum value removed
    EnumValueRemoved,
    /// Schema format changed
    FormatChanged,
    /// Field became required
    FieldMadeRequired,
    /// Array items type changed
    ArrayItemsChanged,
    /// Map value type changed
    MapValueChanged,
    /// Union types incompatible
    UnionTypesIncompatible,
    /// Namespace changed
    NamespaceChanged,
    /// Name changed
    NameChanged,
    /// Default value removed
    DefaultRemoved,
    /// Custom violation
    Custom(String),
}

/// Validation error or warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Error code for programmatic handling
    pub code: String,
    /// Error message
    pub message: String,
    /// Severity level
    pub severity: ErrorSeverity,
    /// Category of the error
    pub category: ErrorCategory,
    /// Field path where error occurred (if applicable)
    pub field_path: Option<String>,
    /// Line number in schema (if applicable)
    pub line: Option<usize>,
    /// Column number in schema (if applicable)
    pub column: Option<usize>,
    /// Suggestion for fixing the error
    pub suggestion: Option<String>,
}

impl ValidationError {
    /// Create a new breaking error
    pub fn breaking(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: ErrorSeverity::Breaking,
            category: ErrorCategory::Structural,
            field_path: None,
            line: None,
            column: None,
            suggestion: None,
        }
    }

    /// Create a new warning
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: ErrorSeverity::Warning,
            category: ErrorCategory::BestPractice,
            field_path: None,
            line: None,
            column: None,
            suggestion: None,
        }
    }

    /// Create a new info-level notice
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: ErrorSeverity::Info,
            category: ErrorCategory::BestPractice,
            field_path: None,
            line: None,
            column: None,
            suggestion: None,
        }
    }

    /// Set the field path
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.field_path = Some(path.into());
        self
    }

    /// Set the category
    pub fn with_category(mut self, category: ErrorCategory) -> Self {
        self.category = category;
        self
    }

    /// Set a suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Set location information
    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }
}

/// Severity level of validation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorSeverity {
    /// Breaking error - schema cannot be used
    Breaking,
    /// Warning - schema can be used but has issues
    Warning,
    /// Info - informational notice
    Info,
}

/// Category of validation error
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCategory {
    /// Structural/syntax error
    Structural,
    /// Type error
    Type,
    /// Semantic error (valid syntax but invalid meaning)
    Semantic,
    /// Security-related error
    Security,
    /// Performance-related warning
    Performance,
    /// Deprecation notice
    Deprecation,
    /// Best practice violation
    BestPractice,
    /// Compatibility issue
    Compatibility,
}

/// Validation metrics for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetrics {
    /// Total validation duration in milliseconds
    pub total_duration_ms: u64,
    /// Parse duration in milliseconds
    pub parse_duration_ms: u64,
    /// Structural validation duration in milliseconds
    pub structural_duration_ms: u64,
    /// Semantic validation duration in milliseconds
    pub semantic_duration_ms: u64,
    /// Security check duration in milliseconds
    pub security_duration_ms: u64,
    /// Schema size in bytes
    pub schema_size_bytes: usize,
    /// Schema complexity score (0-100)
    pub complexity_score: u8,
    /// Nesting depth
    pub nesting_depth: usize,
    /// Number of properties/fields
    pub property_count: usize,
    /// Number of types defined
    pub type_count: usize,
}

impl Default for ValidationMetrics {
    fn default() -> Self {
        Self {
            total_duration_ms: 0,
            parse_duration_ms: 0,
            structural_duration_ms: 0,
            semantic_duration_ms: 0,
            security_duration_ms: 0,
            schema_size_bytes: 0,
            complexity_score: 0,
            nesting_depth: 0,
            property_count: 0,
            type_count: 0,
        }
    }
}

/// Agent output combining validation and compatibility results
///
/// This is the primary output structure for the DecisionEvent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    /// The schema validation result
    pub validation_result: ValidationResult,
    /// Optional compatibility check result
    pub compatibility_result: Option<CompatibilityCheckResult>,
}

impl AgentOutput {
    /// Create a new AgentOutput with only validation result
    pub fn validation_only(result: ValidationResult) -> Self {
        Self {
            validation_result: result,
            compatibility_result: None,
        }
    }

    /// Create a new AgentOutput with both validation and compatibility results
    pub fn with_compatibility(
        validation: ValidationResult,
        compatibility: CompatibilityCheckResult,
    ) -> Self {
        Self {
            validation_result: validation,
            compatibility_result: Some(compatibility),
        }
    }
}

/// Constraints applied during validation
///
/// Documents the scope and rules applied during a validation operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintsApplied {
    /// Schema scope identifier (e.g., "namespace:name")
    pub schema_scope: String,
    /// Optional version bounds for compatibility checking
    pub version_bounds: Option<VersionBounds>,
    /// Compatibility mode used (e.g., "BACKWARD", "FORWARD", "FULL")
    pub compatibility_mode: Option<String>,
    /// List of validation rule identifiers that were applied
    pub validation_rules: Vec<String>,
}

impl Default for ConstraintsApplied {
    fn default() -> Self {
        Self {
            schema_scope: String::new(),
            version_bounds: None,
            compatibility_mode: None,
            validation_rules: Vec::new(),
        }
    }
}

impl ConstraintsApplied {
    /// Create constraints for a specific schema scope
    pub fn new(namespace: &str, name: &str) -> Self {
        Self {
            schema_scope: format!("{}:{}", namespace, name),
            ..Default::default()
        }
    }

    /// Set version bounds
    pub fn with_version_bounds(mut self, bounds: VersionBounds) -> Self {
        self.version_bounds = Some(bounds);
        self
    }

    /// Set compatibility mode
    pub fn with_compatibility_mode(mut self, mode: CompatibilityMode) -> Self {
        self.compatibility_mode = Some(format!("{:?}", mode).to_uppercase());
        self
    }

    /// Add validation rules
    pub fn with_rules(mut self, rules: Vec<String>) -> Self {
        self.validation_rules = rules;
        self
    }
}

/// Version bounds for compatibility checking
///
/// Specifies the version range to consider during compatibility analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionBounds {
    /// Minimum version to consider (inclusive)
    pub min_version: Option<String>,
    /// Maximum version to consider (inclusive)
    pub max_version: Option<String>,
    /// Specific target version to compare against
    pub target_version: Option<String>,
}

impl Default for VersionBounds {
    fn default() -> Self {
        Self {
            min_version: None,
            max_version: None,
            target_version: None,
        }
    }
}

impl VersionBounds {
    /// Create version bounds with a target version
    pub fn target(version: impl Into<String>) -> Self {
        Self {
            target_version: Some(version.into()),
            ..Default::default()
        }
    }

    /// Create version bounds with a range
    pub fn range(min: Option<String>, max: Option<String>) -> Self {
        Self {
            min_version: min,
            max_version: max,
            target_version: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_input_builder() {
        let input = ValidationInput::new(r#"{"type": "object"}"#)
            .with_format(SchemaFormat::JsonSchema)
            .with_compatibility_mode(CompatibilityMode::Backward);

        assert_eq!(input.format_hint, Some(SchemaFormat::JsonSchema));
        assert_eq!(input.compatibility_mode, Some(CompatibilityMode::Backward));
    }

    #[test]
    fn test_validation_error_builder() {
        let error = ValidationError::breaking("E001", "Invalid syntax")
            .with_path("properties.name")
            .with_category(ErrorCategory::Structural)
            .with_suggestion("Add a type annotation")
            .with_location(10, 5);

        assert_eq!(error.code, "E001");
        assert_eq!(error.severity, ErrorSeverity::Breaking);
        assert_eq!(error.field_path, Some("properties.name".to_string()));
        assert_eq!(error.line, Some(10));
        assert_eq!(error.column, Some(5));
    }

    #[test]
    fn test_compatibility_mode_transitive() {
        assert!(!CompatibilityMode::Backward.is_transitive());
        assert!(!CompatibilityMode::Forward.is_transitive());
        assert!(!CompatibilityMode::Full.is_transitive());
        assert!(!CompatibilityMode::None.is_transitive());
        assert!(CompatibilityMode::BackwardTransitive.is_transitive());
        assert!(CompatibilityMode::ForwardTransitive.is_transitive());
        assert!(CompatibilityMode::FullTransitive.is_transitive());
    }

    #[test]
    fn test_schema_format_display() {
        assert_eq!(SchemaFormat::JsonSchema.to_string(), "JSON_SCHEMA");
        assert_eq!(SchemaFormat::Avro.to_string(), "AVRO");
        assert_eq!(SchemaFormat::Protobuf.to_string(), "PROTOBUF");
        assert_eq!(SchemaFormat::Unknown.to_string(), "UNKNOWN");
    }
}
