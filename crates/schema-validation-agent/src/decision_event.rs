//! Decision Event for persistence through ruvector-service
//!
//! The Schema Validation Agent NEVER connects directly to databases.
//! All persistence goes through ruvector-service via DecisionEvents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::types::{CompatibilityMode, ErrorSeverity, SchemaFormat, ValidationError, ValidationMetrics, ValidationResult};

/// Decision type for validation outcomes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionType {
    /// Schema was validated and accepted
    SchemaAccepted,
    /// Schema was validated and rejected
    SchemaRejected,
    /// Schema passed compatibility check
    CompatibilityPassed,
    /// Schema failed compatibility check
    CompatibilityFailed,
    /// Security issue detected
    SecurityAlert,
    /// Performance warning issued
    PerformanceWarning,
}

/// Decision event to be persisted via ruvector-service
///
/// This is the ONLY way the validation agent stores data.
/// CRITICAL: The agent MUST NOT execute SQL or connect to databases directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEvent {
    /// Unique event ID
    pub event_id: Uuid,
    /// Type of decision made
    pub decision_type: DecisionType,
    /// Correlation ID for tracing
    pub correlation_id: Uuid,
    /// Timestamp of the decision
    pub timestamp: DateTime<Utc>,
    /// Agent that made the decision
    pub agent_id: String,
    /// Schema namespace
    pub namespace: Option<String>,
    /// Schema name
    pub name: Option<String>,
    /// Schema version
    pub version: Option<String>,
    /// Schema format
    pub format: SchemaFormat,
    /// Content hash of the schema
    pub content_hash: String,
    /// Overall decision (true = pass, false = fail)
    pub passed: bool,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Validation errors (if any)
    pub errors: Vec<SerializedError>,
    /// Validation warnings (if any)
    pub warnings: Vec<SerializedError>,
    /// Compatibility violations (if any)
    pub compatibility_violations: Vec<SerializedViolation>,
    /// Metrics from validation
    pub metrics: SerializedMetrics,
    /// Additional context
    pub context: HashMap<String, serde_json::Value>,
    /// Tags for categorization
    pub tags: Vec<String>,
}

impl DecisionEvent {
    /// Create a new decision event from a validation result
    pub fn from_validation(
        result: &ValidationResult,
        namespace: Option<String>,
        name: Option<String>,
        version: Option<String>,
    ) -> Self {
        let decision_type = if result.is_valid && result.is_compatible() {
            DecisionType::SchemaAccepted
        } else if result.errors.iter().any(|e| e.category == crate::types::ErrorCategory::Security) {
            DecisionType::SecurityAlert
        } else if let Some(compat) = &result.compatibility {
            if compat.is_compatible {
                DecisionType::CompatibilityPassed
            } else {
                DecisionType::CompatibilityFailed
            }
        } else {
            DecisionType::SchemaRejected
        };

        let confidence = calculate_confidence(result);

        Self {
            event_id: Uuid::new_v4(),
            decision_type,
            correlation_id: result.validation_id,
            timestamp: result.validated_at,
            agent_id: "schema-validation-agent".to_string(),
            namespace,
            name,
            version,
            format: result.detected_format,
            content_hash: result.content_hash.clone(),
            passed: result.is_valid && result.is_compatible(),
            confidence,
            errors: result.errors.iter().map(SerializedError::from).collect(),
            warnings: result.warnings.iter().map(SerializedError::from).collect(),
            compatibility_violations: result
                .compatibility
                .as_ref()
                .map(|c| c.violations.iter().map(SerializedViolation::from).collect())
                .unwrap_or_default(),
            metrics: SerializedMetrics::from(&result.metrics),
            context: HashMap::new(),
            tags: Vec::new(),
        }
    }

    /// Add context to the event
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set the correlation ID
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.correlation_id = id;
        self
    }
}

/// Serialized error for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedError {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub category: String,
    pub field_path: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub suggestion: Option<String>,
}

impl From<&ValidationError> for SerializedError {
    fn from(error: &ValidationError) -> Self {
        Self {
            code: error.code.clone(),
            message: error.message.clone(),
            severity: format!("{:?}", error.severity),
            category: format!("{:?}", error.category),
            field_path: error.field_path.clone(),
            line: error.line,
            column: error.column,
            suggestion: error.suggestion.clone(),
        }
    }
}

/// Serialized compatibility violation for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedViolation {
    pub violation_type: String,
    pub field_path: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub severity: String,
    pub description: String,
}

impl From<&crate::types::CompatibilityViolation> for SerializedViolation {
    fn from(v: &crate::types::CompatibilityViolation) -> Self {
        Self {
            violation_type: format!("{:?}", v.violation_type),
            field_path: v.field_path.clone(),
            old_value: v.old_value.clone(),
            new_value: v.new_value.clone(),
            severity: format!("{:?}", v.severity),
            description: v.description.clone(),
        }
    }
}

/// Serialized metrics for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedMetrics {
    pub total_duration_ms: u64,
    pub parse_duration_ms: u64,
    pub structural_duration_ms: u64,
    pub semantic_duration_ms: u64,
    pub security_duration_ms: u64,
    pub schema_size_bytes: usize,
    pub complexity_score: u8,
    pub nesting_depth: usize,
    pub property_count: usize,
    pub type_count: usize,
}

impl From<&ValidationMetrics> for SerializedMetrics {
    fn from(m: &ValidationMetrics) -> Self {
        Self {
            total_duration_ms: m.total_duration_ms,
            parse_duration_ms: m.parse_duration_ms,
            structural_duration_ms: m.structural_duration_ms,
            semantic_duration_ms: m.semantic_duration_ms,
            security_duration_ms: m.security_duration_ms,
            schema_size_bytes: m.schema_size_bytes,
            complexity_score: m.complexity_score,
            nesting_depth: m.nesting_depth,
            property_count: m.property_count,
            type_count: m.type_count,
        }
    }
}

/// Calculate confidence score based on validation result
fn calculate_confidence(result: &ValidationResult) -> f64 {
    let mut score = 1.0;

    // Reduce confidence for each error
    for error in &result.errors {
        match error.severity {
            ErrorSeverity::Breaking => score -= 0.3,
            ErrorSeverity::Warning => score -= 0.1,
            ErrorSeverity::Info => score -= 0.02,
        }
    }

    // Reduce confidence for compatibility issues
    if let Some(compat) = &result.compatibility {
        if !compat.is_compatible {
            score -= 0.2;
        }
        for violation in &compat.violations {
            match violation.severity {
                ErrorSeverity::Breaking => score -= 0.15,
                ErrorSeverity::Warning => score -= 0.05,
                ErrorSeverity::Info => score -= 0.01,
            }
        }
    }

    // Ensure score stays within bounds
    score.max(0.0).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompatibilityCheckResult, ValidationMetrics};

    fn create_test_result(is_valid: bool, errors: Vec<ValidationError>) -> ValidationResult {
        ValidationResult {
            validation_id: Uuid::new_v4(),
            is_valid,
            detected_format: SchemaFormat::JsonSchema,
            errors,
            warnings: Vec::new(),
            compatibility: None,
            metrics: ValidationMetrics::default(),
            content_hash: "test_hash".to_string(),
            validated_at: Utc::now(),
        }
    }

    #[test]
    fn test_decision_event_from_valid_result() {
        let result = create_test_result(true, Vec::new());
        let event = DecisionEvent::from_validation(&result, Some("test".into()), Some("schema".into()), Some("1.0.0".into()));

        assert_eq!(event.decision_type, DecisionType::SchemaAccepted);
        assert!(event.passed);
        assert_eq!(event.confidence, 1.0);
    }

    #[test]
    fn test_decision_event_from_invalid_result() {
        let errors = vec![ValidationError::breaking("E001", "Invalid syntax")];
        let result = create_test_result(false, errors);
        let event = DecisionEvent::from_validation(&result, None, None, None);

        assert_eq!(event.decision_type, DecisionType::SchemaRejected);
        assert!(!event.passed);
        assert!(event.confidence < 1.0);
    }

    #[test]
    fn test_decision_event_context() {
        let result = create_test_result(true, Vec::new());
        let event = DecisionEvent::from_validation(&result, None, None, None)
            .with_context("source", serde_json::json!("api"))
            .with_tag("automated")
            .with_correlation_id(Uuid::new_v4());

        assert!(event.context.contains_key("source"));
        assert!(event.tags.contains(&"automated".to_string()));
    }

    #[test]
    fn test_confidence_calculation() {
        // Perfect validation
        let result = create_test_result(true, Vec::new());
        let event = DecisionEvent::from_validation(&result, None, None, None);
        assert_eq!(event.confidence, 1.0);

        // With breaking error
        let errors = vec![
            ValidationError::breaking("E001", "Error 1"),
            ValidationError::warning("W001", "Warning 1"),
        ];
        let result = create_test_result(false, errors);
        let event = DecisionEvent::from_validation(&result, None, None, None);
        assert!(event.confidence < 1.0);
        assert!(event.confidence > 0.0);
    }
}
