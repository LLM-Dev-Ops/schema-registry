//! Infrastructure Signals for Phase 6 Core Infrastructure (Layer 1)
//!
//! This module provides deterministic signal emission for core infrastructure
//! events. These signals are designed for the core infrastructure layer that
//! defines configuration truth, schema truth, and external adapters.
//!
//! # Signal Types
//!
//! - `config_validation_signal` - Emitted when configuration validation occurs
//! - `schema_violation_signal` - Emitted when schema validation finds violations
//! - `integration_health_signal` - Emitted for integration health status
//!
//! # Performance Budgets
//!
//! - MAX_TOKENS: 800
//! - MAX_LATENCY_MS: 1500
//!
//! # Architecture
//!
//! These agents define:
//! - Configuration truth
//! - Schema truth
//! - External adapters
//!
//! They MUST be deterministic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use crate::types::{AGENT_ID, AGENT_VERSION};

// ============================================================================
// Performance Budget Constants
// ============================================================================

/// Maximum tokens allowed for infrastructure operations
pub const MAX_TOKENS: u32 = 800;

/// Maximum latency in milliseconds for infrastructure operations
pub const MAX_LATENCY_MS: u64 = 1500;

/// Agent identifier for infrastructure signals
pub const INFRASTRUCTURE_AGENT_ID: &str = "schema-validation-agent/infrastructure";

// ============================================================================
// Signal Types
// ============================================================================

/// Type of infrastructure signal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureSignalType {
    /// Configuration validation signal
    ConfigValidation,
    /// Schema violation signal
    SchemaViolation,
    /// Integration health signal
    IntegrationHealth,
}

impl std::fmt::Display for InfrastructureSignalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InfrastructureSignalType::ConfigValidation => write!(f, "config_validation_signal"),
            InfrastructureSignalType::SchemaViolation => write!(f, "schema_violation_signal"),
            InfrastructureSignalType::IntegrationHealth => write!(f, "integration_health_signal"),
        }
    }
}

/// Severity level for infrastructure signals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignalSeverity {
    /// Informational signal
    Info,
    /// Warning signal
    Warning,
    /// Error signal
    Error,
    /// Critical signal requiring immediate attention
    Critical,
}

/// Status of an infrastructure signal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalStatus {
    /// Operation passed/succeeded
    Pass,
    /// Operation failed
    Fail,
    /// Operation partially succeeded (with warnings)
    Partial,
    /// Status unknown or pending
    Unknown,
}

// ============================================================================
// Infrastructure Signal
// ============================================================================

/// Infrastructure signal for core Layer 1 events
///
/// This is the primary signal type for configuration validation,
/// schema violations, and integration health events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfrastructureSignal {
    /// Unique signal ID
    pub signal_id: Uuid,
    /// Type of signal
    pub signal_type: InfrastructureSignalType,
    /// Correlation ID for distributed tracing
    pub correlation_id: Uuid,
    /// Timestamp of signal emission
    pub timestamp: DateTime<Utc>,
    /// Agent that emitted the signal
    pub agent_id: String,
    /// Agent version
    pub agent_version: String,
    /// Signal severity
    pub severity: SignalSeverity,
    /// Signal status
    pub status: SignalStatus,
    /// Signal message
    pub message: String,
    /// Signal details (type-specific payload)
    pub details: SignalDetails,
    /// Performance metrics
    pub performance: PerformanceMetrics,
    /// Additional context
    pub context: HashMap<String, serde_json::Value>,
    /// Tags for categorization
    pub tags: Vec<String>,
}

impl InfrastructureSignal {
    /// Create a new configuration validation signal
    pub fn config_validation(
        status: SignalStatus,
        message: impl Into<String>,
        config_details: ConfigValidationDetails,
    ) -> Self {
        Self {
            signal_id: Uuid::new_v4(),
            signal_type: InfrastructureSignalType::ConfigValidation,
            correlation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            agent_id: INFRASTRUCTURE_AGENT_ID.to_string(),
            agent_version: AGENT_VERSION.to_string(),
            severity: match status {
                SignalStatus::Pass => SignalSeverity::Info,
                SignalStatus::Fail => SignalSeverity::Error,
                SignalStatus::Partial => SignalSeverity::Warning,
                SignalStatus::Unknown => SignalSeverity::Warning,
            },
            status,
            message: message.into(),
            details: SignalDetails::ConfigValidation(config_details),
            performance: PerformanceMetrics::default(),
            context: HashMap::new(),
            tags: vec!["infrastructure".to_string(), "config".to_string()],
        }
    }

    /// Create a new schema violation signal
    pub fn schema_violation(
        message: impl Into<String>,
        violation_details: SchemaViolationDetails,
    ) -> Self {
        let severity = match violation_details.violation_count {
            0 => SignalSeverity::Info,
            1..=5 => SignalSeverity::Warning,
            _ => SignalSeverity::Error,
        };

        Self {
            signal_id: Uuid::new_v4(),
            signal_type: InfrastructureSignalType::SchemaViolation,
            correlation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            agent_id: INFRASTRUCTURE_AGENT_ID.to_string(),
            agent_version: AGENT_VERSION.to_string(),
            severity,
            status: if violation_details.violation_count == 0 {
                SignalStatus::Pass
            } else {
                SignalStatus::Fail
            },
            message: message.into(),
            details: SignalDetails::SchemaViolation(violation_details),
            performance: PerformanceMetrics::default(),
            context: HashMap::new(),
            tags: vec!["infrastructure".to_string(), "schema".to_string()],
        }
    }

    /// Create a new integration health signal
    pub fn integration_health(
        status: SignalStatus,
        message: impl Into<String>,
        health_details: IntegrationHealthDetails,
    ) -> Self {
        Self {
            signal_id: Uuid::new_v4(),
            signal_type: InfrastructureSignalType::IntegrationHealth,
            correlation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            agent_id: INFRASTRUCTURE_AGENT_ID.to_string(),
            agent_version: AGENT_VERSION.to_string(),
            severity: match status {
                SignalStatus::Pass => SignalSeverity::Info,
                SignalStatus::Fail => SignalSeverity::Critical,
                SignalStatus::Partial => SignalSeverity::Warning,
                SignalStatus::Unknown => SignalSeverity::Warning,
            },
            status,
            message: message.into(),
            details: SignalDetails::IntegrationHealth(health_details),
            performance: PerformanceMetrics::default(),
            context: HashMap::new(),
            tags: vec!["infrastructure".to_string(), "health".to_string()],
        }
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.correlation_id = id;
        self
    }

    /// Add context
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set performance metrics
    pub fn with_performance(mut self, metrics: PerformanceMetrics) -> Self {
        self.performance = metrics;
        self
    }

    /// Check if signal meets performance budget
    pub fn meets_performance_budget(&self) -> bool {
        self.performance.latency_ms <= MAX_LATENCY_MS
            && self.performance.token_count <= MAX_TOKENS
    }
}

// ============================================================================
// Signal Details
// ============================================================================

/// Details specific to each signal type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalDetails {
    /// Configuration validation details
    ConfigValidation(ConfigValidationDetails),
    /// Schema violation details
    SchemaViolation(SchemaViolationDetails),
    /// Integration health details
    IntegrationHealth(IntegrationHealthDetails),
}

/// Details for configuration validation signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValidationDetails {
    /// Configuration namespace/scope
    pub config_scope: String,
    /// Configuration keys validated
    pub validated_keys: Vec<String>,
    /// Invalid configuration keys
    pub invalid_keys: Vec<ConfigValidationError>,
    /// Whether configuration is valid
    pub is_valid: bool,
    /// Configuration source
    pub source: String,
    /// Validation rules applied
    pub rules_applied: Vec<String>,
}

/// Configuration validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValidationError {
    /// Configuration key
    pub key: String,
    /// Error message
    pub message: String,
    /// Expected type or value
    pub expected: Option<String>,
    /// Actual type or value
    pub actual: Option<String>,
}

/// Details for schema violation signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaViolationDetails {
    /// Schema namespace
    pub namespace: String,
    /// Schema name
    pub name: String,
    /// Schema version (if applicable)
    pub version: Option<String>,
    /// Schema format
    pub format: String,
    /// Number of violations
    pub violation_count: u32,
    /// Violation list
    pub violations: Vec<SchemaViolationEntry>,
    /// Breaking violations count
    pub breaking_count: u32,
    /// Warning violations count
    pub warning_count: u32,
    /// Schema content hash
    pub content_hash: String,
}

/// Individual schema violation entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaViolationEntry {
    /// Violation code
    pub code: String,
    /// Violation message
    pub message: String,
    /// Field path where violation occurred
    pub path: Option<String>,
    /// Whether this is a breaking violation
    pub is_breaking: bool,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Details for integration health signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationHealthDetails {
    /// Integration name (e.g., "ruvector")
    pub integration_name: String,
    /// Integration endpoint
    pub endpoint: String,
    /// Health check result
    pub is_healthy: bool,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Component health statuses
    pub components: HashMap<String, ComponentHealth>,
    /// Last successful check timestamp
    pub last_success: Option<DateTime<Utc>>,
    /// Consecutive failures count
    pub consecutive_failures: u32,
    /// Error message if unhealthy
    pub error_message: Option<String>,
}

/// Health status of an individual component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Whether component is healthy
    pub is_healthy: bool,
    /// Status message
    pub status: String,
    /// Additional details
    pub details: Option<String>,
}

// ============================================================================
// Performance Metrics
// ============================================================================

/// Performance metrics for infrastructure signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Operation latency in milliseconds
    pub latency_ms: u64,
    /// Token count (for LLM-based operations)
    pub token_count: u32,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// Whether performance budget was exceeded
    pub budget_exceeded: bool,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            latency_ms: 0,
            token_count: 0,
            memory_bytes: 0,
            budget_exceeded: false,
        }
    }
}

impl PerformanceMetrics {
    /// Create metrics with latency
    pub fn with_latency(latency_ms: u64) -> Self {
        let budget_exceeded = latency_ms > MAX_LATENCY_MS;
        Self {
            latency_ms,
            budget_exceeded,
            ..Default::default()
        }
    }

    /// Create metrics with latency and token count
    pub fn new(latency_ms: u64, token_count: u32) -> Self {
        let budget_exceeded = latency_ms > MAX_LATENCY_MS || token_count > MAX_TOKENS;
        Self {
            latency_ms,
            token_count,
            budget_exceeded,
            ..Default::default()
        }
    }

    /// Check if within performance budget
    pub fn within_budget(&self) -> bool {
        self.latency_ms <= MAX_LATENCY_MS && self.token_count <= MAX_TOKENS
    }
}

// ============================================================================
// Signal Emitter
// ============================================================================

/// Emitter for infrastructure signals
///
/// Provides methods for emitting deterministic signals for core infrastructure
/// events with performance tracking.
pub struct InfrastructureSignalEmitter {
    /// Environment (development, staging, production)
    environment: String,
    /// Default correlation ID
    default_correlation_id: Option<Uuid>,
}

impl InfrastructureSignalEmitter {
    /// Create a new signal emitter
    pub fn new(environment: impl Into<String>) -> Self {
        Self {
            environment: environment.into(),
            default_correlation_id: None,
        }
    }

    /// Set default correlation ID for all signals
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.default_correlation_id = Some(id);
        self
    }

    /// Emit a configuration validation signal
    #[instrument(skip(self, config_details), fields(
        signal_type = "config_validation_signal",
        status = ?status
    ))]
    pub fn emit_config_validation(
        &self,
        status: SignalStatus,
        message: impl Into<String>,
        config_details: ConfigValidationDetails,
        latency_ms: u64,
    ) -> InfrastructureSignal {
        let msg = message.into();
        let mut signal = InfrastructureSignal::config_validation(status, &msg, config_details)
            .with_performance(PerformanceMetrics::with_latency(latency_ms))
            .with_context("environment", serde_json::json!(self.environment));

        if let Some(correlation_id) = self.default_correlation_id {
            signal = signal.with_correlation_id(correlation_id);
        }

        // Log the signal emission
        match status {
            SignalStatus::Pass => info!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                latency_ms = %latency_ms,
                "config_validation_signal emitted (PASS)"
            ),
            SignalStatus::Fail => error!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                message = %msg,
                latency_ms = %latency_ms,
                "config_validation_signal emitted (FAIL)"
            ),
            _ => warn!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                latency_ms = %latency_ms,
                "config_validation_signal emitted"
            ),
        }

        if !signal.meets_performance_budget() {
            warn!(
                signal_id = %signal.signal_id,
                latency_ms = %latency_ms,
                max_latency_ms = %MAX_LATENCY_MS,
                "Performance budget exceeded for config_validation_signal"
            );
        }

        signal
    }

    /// Emit a schema violation signal
    #[instrument(skip(self, violation_details), fields(
        signal_type = "schema_violation_signal",
        namespace = %violation_details.namespace,
        name = %violation_details.name
    ))]
    pub fn emit_schema_violation(
        &self,
        message: impl Into<String>,
        violation_details: SchemaViolationDetails,
        latency_ms: u64,
    ) -> InfrastructureSignal {
        let msg = message.into();
        let violation_count = violation_details.violation_count;
        let breaking_count = violation_details.breaking_count;

        let mut signal = InfrastructureSignal::schema_violation(&msg, violation_details)
            .with_performance(PerformanceMetrics::with_latency(latency_ms))
            .with_context("environment", serde_json::json!(self.environment));

        if let Some(correlation_id) = self.default_correlation_id {
            signal = signal.with_correlation_id(correlation_id);
        }

        // Log the signal emission
        if violation_count == 0 {
            info!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                latency_ms = %latency_ms,
                "schema_violation_signal emitted (no violations)"
            );
        } else if breaking_count > 0 {
            error!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                violation_count = %violation_count,
                breaking_count = %breaking_count,
                message = %msg,
                latency_ms = %latency_ms,
                "schema_violation_signal emitted (breaking violations)"
            );
        } else {
            warn!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                violation_count = %violation_count,
                message = %msg,
                latency_ms = %latency_ms,
                "schema_violation_signal emitted (warnings)"
            );
        }

        if !signal.meets_performance_budget() {
            warn!(
                signal_id = %signal.signal_id,
                latency_ms = %latency_ms,
                max_latency_ms = %MAX_LATENCY_MS,
                "Performance budget exceeded for schema_violation_signal"
            );
        }

        signal
    }

    /// Emit an integration health signal
    #[instrument(skip(self, health_details), fields(
        signal_type = "integration_health_signal",
        integration = %health_details.integration_name
    ))]
    pub fn emit_integration_health(
        &self,
        status: SignalStatus,
        message: impl Into<String>,
        health_details: IntegrationHealthDetails,
    ) -> InfrastructureSignal {
        let msg = message.into();
        let latency_ms = health_details.response_time_ms;

        let mut signal = InfrastructureSignal::integration_health(status, &msg, health_details)
            .with_performance(PerformanceMetrics::with_latency(latency_ms))
            .with_context("environment", serde_json::json!(self.environment));

        if let Some(correlation_id) = self.default_correlation_id {
            signal = signal.with_correlation_id(correlation_id);
        }

        // Log the signal emission
        match status {
            SignalStatus::Pass => info!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                latency_ms = %latency_ms,
                "integration_health_signal emitted (healthy)"
            ),
            SignalStatus::Fail => error!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                message = %msg,
                latency_ms = %latency_ms,
                "integration_health_signal emitted (unhealthy)"
            ),
            SignalStatus::Partial => warn!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                latency_ms = %latency_ms,
                "integration_health_signal emitted (degraded)"
            ),
            SignalStatus::Unknown => warn!(
                signal_id = %signal.signal_id,
                signal_type = %signal.signal_type,
                latency_ms = %latency_ms,
                "integration_health_signal emitted (unknown status)"
            ),
        }

        if !signal.meets_performance_budget() {
            warn!(
                signal_id = %signal.signal_id,
                latency_ms = %latency_ms,
                max_latency_ms = %MAX_LATENCY_MS,
                "Performance budget exceeded for integration_health_signal"
            );
        }

        signal
    }
}

impl Default for InfrastructureSignalEmitter {
    fn default() -> Self {
        Self::new(
            std::env::var("PLATFORM_ENV").unwrap_or_else(|_| "development".to_string()),
        )
    }
}

// ============================================================================
// Performance Budget Guard
// ============================================================================

/// Guard for enforcing performance budgets
///
/// Tracks operation start time and provides methods to check if
/// the operation is within performance budget.
pub struct PerformanceBudgetGuard {
    start: Instant,
    token_count: u32,
    max_latency_ms: u64,
    max_tokens: u32,
}

impl PerformanceBudgetGuard {
    /// Create a new performance budget guard with default limits
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            token_count: 0,
            max_latency_ms: MAX_LATENCY_MS,
            max_tokens: MAX_TOKENS,
        }
    }

    /// Create with custom limits
    pub fn with_limits(max_latency_ms: u64, max_tokens: u32) -> Self {
        Self {
            start: Instant::now(),
            token_count: 0,
            max_latency_ms,
            max_tokens,
        }
    }

    /// Add tokens to the count
    pub fn add_tokens(&mut self, count: u32) {
        self.token_count += count;
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// Check if within latency budget
    pub fn within_latency_budget(&self) -> bool {
        self.elapsed_ms() <= self.max_latency_ms
    }

    /// Check if within token budget
    pub fn within_token_budget(&self) -> bool {
        self.token_count <= self.max_tokens
    }

    /// Check if within full performance budget
    pub fn within_budget(&self) -> bool {
        self.within_latency_budget() && self.within_token_budget()
    }

    /// Get remaining latency budget in milliseconds
    pub fn remaining_latency_ms(&self) -> i64 {
        self.max_latency_ms as i64 - self.elapsed_ms() as i64
    }

    /// Get remaining token budget
    pub fn remaining_tokens(&self) -> i32 {
        self.max_tokens as i32 - self.token_count as i32
    }

    /// Finalize and get performance metrics
    pub fn finalize(self) -> PerformanceMetrics {
        PerformanceMetrics::new(self.elapsed_ms(), self.token_count)
    }
}

impl Default for PerformanceBudgetGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation_signal() {
        let details = ConfigValidationDetails {
            config_scope: "schema-registry".to_string(),
            validated_keys: vec!["server.port".to_string(), "server.host".to_string()],
            invalid_keys: vec![],
            is_valid: true,
            source: "config-manager".to_string(),
            rules_applied: vec!["type_check".to_string()],
        };

        let signal = InfrastructureSignal::config_validation(
            SignalStatus::Pass,
            "Configuration validated successfully",
            details,
        );

        assert_eq!(signal.signal_type, InfrastructureSignalType::ConfigValidation);
        assert_eq!(signal.status, SignalStatus::Pass);
        assert_eq!(signal.severity, SignalSeverity::Info);
        assert!(!signal.signal_id.is_nil());
    }

    #[test]
    fn test_schema_violation_signal() {
        let details = SchemaViolationDetails {
            namespace: "test".to_string(),
            name: "TestSchema".to_string(),
            version: Some("1.0.0".to_string()),
            format: "JSON_SCHEMA".to_string(),
            violation_count: 2,
            violations: vec![
                SchemaViolationEntry {
                    code: "E001".to_string(),
                    message: "Missing type".to_string(),
                    path: Some("$.properties.name".to_string()),
                    is_breaking: true,
                    suggestion: Some("Add type annotation".to_string()),
                },
            ],
            breaking_count: 1,
            warning_count: 1,
            content_hash: "abc123".to_string(),
        };

        let signal = InfrastructureSignal::schema_violation(
            "Schema validation found violations",
            details,
        );

        assert_eq!(signal.signal_type, InfrastructureSignalType::SchemaViolation);
        assert_eq!(signal.status, SignalStatus::Fail);
        assert_eq!(signal.severity, SignalSeverity::Warning);
    }

    #[test]
    fn test_integration_health_signal() {
        let mut components = HashMap::new();
        components.insert("database".to_string(), ComponentHealth {
            name: "database".to_string(),
            is_healthy: true,
            status: "connected".to_string(),
            details: None,
        });

        let details = IntegrationHealthDetails {
            integration_name: "ruvector".to_string(),
            endpoint: "http://ruvector:8080".to_string(),
            is_healthy: true,
            response_time_ms: 50,
            components,
            last_success: Some(Utc::now()),
            consecutive_failures: 0,
            error_message: None,
        };

        let signal = InfrastructureSignal::integration_health(
            SignalStatus::Pass,
            "RuVector integration healthy",
            details,
        );

        assert_eq!(signal.signal_type, InfrastructureSignalType::IntegrationHealth);
        assert_eq!(signal.status, SignalStatus::Pass);
        assert_eq!(signal.severity, SignalSeverity::Info);
    }

    #[test]
    fn test_performance_budget_constants() {
        assert_eq!(MAX_TOKENS, 800);
        assert_eq!(MAX_LATENCY_MS, 1500);
    }

    #[test]
    fn test_performance_metrics() {
        // Within budget
        let metrics = PerformanceMetrics::new(1000, 500);
        assert!(metrics.within_budget());
        assert!(!metrics.budget_exceeded);

        // Latency exceeded
        let metrics = PerformanceMetrics::new(2000, 500);
        assert!(!metrics.within_budget());
        assert!(metrics.budget_exceeded);

        // Tokens exceeded
        let metrics = PerformanceMetrics::new(1000, 1000);
        assert!(!metrics.within_budget());
        assert!(metrics.budget_exceeded);
    }

    #[test]
    fn test_performance_budget_guard() {
        let mut guard = PerformanceBudgetGuard::new();
        guard.add_tokens(100);

        assert!(guard.within_latency_budget()); // Just started
        assert!(guard.within_token_budget());
        assert!(guard.within_budget());

        guard.add_tokens(800); // Now at 900 tokens
        assert!(!guard.within_token_budget());
        assert!(!guard.within_budget());
    }

    #[test]
    fn test_signal_type_display() {
        assert_eq!(
            InfrastructureSignalType::ConfigValidation.to_string(),
            "config_validation_signal"
        );
        assert_eq!(
            InfrastructureSignalType::SchemaViolation.to_string(),
            "schema_violation_signal"
        );
        assert_eq!(
            InfrastructureSignalType::IntegrationHealth.to_string(),
            "integration_health_signal"
        );
    }

    #[test]
    fn test_signal_with_context() {
        let details = ConfigValidationDetails {
            config_scope: "test".to_string(),
            validated_keys: vec![],
            invalid_keys: vec![],
            is_valid: true,
            source: "test".to_string(),
            rules_applied: vec![],
        };

        let signal = InfrastructureSignal::config_validation(
            SignalStatus::Pass,
            "Test",
            details,
        )
        .with_context("key", serde_json::json!("value"))
        .with_tag("test-tag")
        .with_correlation_id(Uuid::new_v4());

        assert!(signal.context.contains_key("key"));
        assert!(signal.tags.contains(&"test-tag".to_string()));
    }

    #[test]
    fn test_signal_emitter() {
        let emitter = InfrastructureSignalEmitter::new("test");

        let details = ConfigValidationDetails {
            config_scope: "test".to_string(),
            validated_keys: vec!["key1".to_string()],
            invalid_keys: vec![],
            is_valid: true,
            source: "test".to_string(),
            rules_applied: vec!["rule1".to_string()],
        };

        let signal = emitter.emit_config_validation(
            SignalStatus::Pass,
            "Config validated",
            details,
            100,
        );

        assert_eq!(signal.signal_type, InfrastructureSignalType::ConfigValidation);
        assert!(signal.meets_performance_budget());
    }
}
