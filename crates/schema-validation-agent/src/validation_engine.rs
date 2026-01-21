//! Core Validation Engine for the Schema Validation Agent
//!
//! This engine provides deterministic, stateless schema validation.
//!
//! The validation engine MUST:
//! - Validate schema structure and syntax
//! - Detect schema format (JSON Schema, Avro, Protobuf)
//! - Verify type correctness and constraints
//! - Detect deprecated or invalid schema elements
//! - Identify breaking vs non-breaking changes
//! - Assess backward and forward compatibility
//! - Produce deterministic validation results
//! - Track validation metrics
//!
//! The engine MUST NOT:
//! - Modify schemas
//! - Auto-correct definitions
//! - Execute any enforcement logic
//! - Persist data (that's done through ruvector-service)

use chrono::Utc;
use compatibility_checker::formats::FormatCompatibilityChecker;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::types::*;

/// Errors that can occur during validation
#[derive(Debug, Error)]
pub enum ValidationEngineError {
    #[error("Schema parsing error: {0}")]
    ParseError(String),

    #[error("Unsupported schema format")]
    UnsupportedFormat,

    #[error("Schema too large: {size} bytes exceeds limit of {max} bytes")]
    SchemaTooLarge { size: usize, max: usize },

    #[error("Internal validation error: {0}")]
    InternalError(String),

    #[error("Format detection failed: {0}")]
    FormatDetectionError(String),
}

/// Configuration for the validation engine
#[derive(Debug, Clone)]
pub struct ValidationEngineConfig {
    /// Enable strict validation mode
    pub strict_mode: bool,
    /// Maximum schema size in bytes
    pub max_schema_size: usize,
    /// Enable semantic validation checks
    pub enable_semantic_checks: bool,
    /// Enable performance-related checks
    pub enable_performance_checks: bool,
    /// Enable security checks
    pub enable_security_checks: bool,
    /// Maximum allowed nesting depth
    pub max_nesting_depth: usize,
    /// Maximum number of properties allowed
    pub max_properties: usize,
    /// Timeout for validation in milliseconds
    pub validation_timeout_ms: u64,
}

impl Default for ValidationEngineConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,
            max_schema_size: 1_048_576, // 1 MB
            enable_semantic_checks: true,
            enable_performance_checks: true,
            enable_security_checks: true,
            max_nesting_depth: 50,
            max_properties: 500,
            validation_timeout_ms: 5000,
        }
    }
}

/// The main validation engine - stateless and deterministic
pub struct ValidationEngine {
    config: ValidationEngineConfig,
    // Compiled regex patterns for format detection
    json_schema_pattern: Regex,
    avro_pattern: Regex,
    protobuf_pattern: Regex,
}

impl ValidationEngine {
    /// Create a new validation engine with the given configuration
    pub fn new(config: ValidationEngineConfig) -> Self {
        // Compile regex patterns for format detection
        let json_schema_pattern = Regex::new(
            r#"("\$schema"\s*:\s*"[^"]*json-schema[^"]*")|("type"\s*:\s*"(object|array|string|number|integer|boolean|null)")|("\$ref"\s*:)"#
        ).expect("Invalid JSON Schema regex");

        let avro_pattern = Regex::new(
            r#"("type"\s*:\s*"record")|("type"\s*:\s*"enum")|("fields"\s*:\s*\[)"#
        ).expect("Invalid Avro regex");

        let protobuf_pattern = Regex::new(
            r#"^(syntax\s*=\s*"proto[23]";)|(message\s+\w+\s*\{)|(package\s+[\w.]+;)"#
        ).expect("Invalid Protobuf regex");

        Self {
            config,
            json_schema_pattern,
            avro_pattern,
            protobuf_pattern,
        }
    }

    /// Main validation entry point - deterministic, no side effects
    ///
    /// This method performs comprehensive schema validation including:
    /// 1. Format detection
    /// 2. Schema parsing
    /// 3. Structural validation
    /// 4. Type validation
    /// 5. Semantic validation
    /// 6. Security checks
    /// 7. Performance checks
    pub fn validate(&self, input: &ValidationInput) -> Result<ValidationResult, ValidationEngineError> {
        let start = Instant::now();
        let validation_id = Uuid::new_v4();

        info!("Starting validation {}", validation_id);

        // Check schema size
        let schema_size = input.content.len();
        if schema_size > self.config.max_schema_size {
            return Err(ValidationEngineError::SchemaTooLarge {
                size: schema_size,
                max: self.config.max_schema_size,
            });
        }

        // Calculate content hash
        let content_hash = self.calculate_hash(&input.content);

        // Initialize metrics
        let mut metrics = ValidationMetrics {
            schema_size_bytes: schema_size,
            ..Default::default()
        };

        // Step 1: Detect format
        let parse_start = Instant::now();
        let format = input
            .format_hint
            .unwrap_or_else(|| self.detect_format(&input.content).unwrap_or(SchemaFormat::Unknown));
        metrics.parse_duration_ms = parse_start.elapsed().as_millis() as u64;

        if format == SchemaFormat::Unknown {
            return Err(ValidationEngineError::FormatDetectionError(
                "Could not determine schema format".to_string(),
            ));
        }

        debug!("Detected format: {:?}", format);

        // Step 2-7: Format-specific validation
        let (errors, warnings, format_metrics) = match format {
            SchemaFormat::JsonSchema => self.validate_json_schema(&input.content, &input.options)?,
            SchemaFormat::Avro => self.validate_avro_schema(&input.content, &input.options)?,
            SchemaFormat::Protobuf => self.validate_protobuf_schema(&input.content, &input.options)?,
            SchemaFormat::Unknown => {
                return Err(ValidationEngineError::UnsupportedFormat);
            }
        };

        // Merge format-specific metrics
        metrics.structural_duration_ms = format_metrics.structural_duration_ms;
        metrics.semantic_duration_ms = format_metrics.semantic_duration_ms;
        metrics.security_duration_ms = format_metrics.security_duration_ms;
        metrics.complexity_score = format_metrics.complexity_score;
        metrics.nesting_depth = format_metrics.nesting_depth;
        metrics.property_count = format_metrics.property_count;
        metrics.type_count = format_metrics.type_count;

        // Check compatibility if previous schema provided
        let compatibility = if let Some(ref previous) = input.previous_content {
            let compat_mode = input.compatibility_mode.unwrap_or(CompatibilityMode::Backward);
            Some(self.check_compatibility(&input.content, previous, format, compat_mode)?)
        } else {
            None
        };

        // Calculate total duration
        metrics.total_duration_ms = start.elapsed().as_millis() as u64;

        // Determine overall validity
        let is_valid = !errors.iter().any(|e| e.severity == ErrorSeverity::Breaking);

        let result = ValidationResult {
            validation_id,
            is_valid,
            detected_format: format,
            errors,
            warnings,
            compatibility,
            metrics,
            content_hash,
            validated_at: Utc::now(),
        };

        info!(
            "Validation {} completed: valid={}, errors={}, warnings={}, duration={}ms",
            validation_id,
            result.is_valid,
            result.errors.len(),
            result.warnings.len(),
            result.metrics.total_duration_ms
        );

        Ok(result)
    }

    /// Calculate SHA-256 hash of content
    fn calculate_hash(&self, content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Detect schema format from content
    pub fn detect_format(&self, content: &str) -> Result<SchemaFormat, ValidationEngineError> {
        let trimmed = content.trim();

        // Check for Protobuf first (has distinctive syntax)
        if self.protobuf_pattern.is_match(trimmed) {
            return Ok(SchemaFormat::Protobuf);
        }

        // Try parsing as JSON for JSON-based formats
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            // Check for JSON Schema indicators
            if self.json_schema_pattern.is_match(content) {
                // Could be JSON Schema, but need to distinguish from Avro
                if content.contains("\"$schema\"") || content.contains("\"$ref\"") {
                    return Ok(SchemaFormat::JsonSchema);
                }
            }

            // Check for Avro indicators
            if self.avro_pattern.is_match(content) {
                // Avro records have "type": "record" and "fields"
                if (content.contains("\"type\"") && content.contains("\"record\""))
                    || (content.contains("\"type\"") && content.contains("\"enum\""))
                    || content.contains("\"fields\"")
                {
                    // Additional check: Avro uses "namespace" differently
                    if content.contains("\"namespace\"") && content.contains("\"fields\"") {
                        return Ok(SchemaFormat::Avro);
                    }
                }
            }

            // Default to JSON Schema for JSON that has type definitions
            if content.contains("\"type\"") {
                return Ok(SchemaFormat::JsonSchema);
            }
        }

        Err(ValidationEngineError::FormatDetectionError(
            "Unable to detect schema format".to_string(),
        ))
    }

    /// Validate JSON Schema
    fn validate_json_schema(
        &self,
        content: &str,
        options: &ValidationOptions,
    ) -> Result<(Vec<ValidationError>, Vec<ValidationError>, ValidationMetrics), ValidationEngineError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        let structural_start = Instant::now();

        // Parse JSON
        let schema: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| ValidationEngineError::ParseError(format!("JSON parse error: {}", e)))?;

        // Structural validation
        self.validate_json_schema_structure(&schema, "", &mut errors, &mut warnings, options, &mut metrics)?;

        metrics.structural_duration_ms = structural_start.elapsed().as_millis() as u64;

        // Semantic validation
        if options.enable_semantic_checks {
            let semantic_start = Instant::now();
            self.validate_json_schema_semantics(&schema, "", &mut errors, &mut warnings)?;
            metrics.semantic_duration_ms = semantic_start.elapsed().as_millis() as u64;
        }

        // Security checks
        if options.enable_security_checks {
            let security_start = Instant::now();
            self.check_json_schema_security(&schema, "", &mut errors, &mut warnings)?;
            metrics.security_duration_ms = security_start.elapsed().as_millis() as u64;
        }

        // Performance checks
        if options.enable_performance_checks {
            self.check_json_schema_performance(&schema, &mut warnings, &metrics, options)?;
        }

        // Calculate complexity
        metrics.complexity_score = self.calculate_json_complexity(&schema);

        Ok((errors, warnings, metrics))
    }

    /// Validate JSON Schema structure
    fn validate_json_schema_structure(
        &self,
        schema: &serde_json::Value,
        path: &str,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
        options: &ValidationOptions,
        metrics: &mut ValidationMetrics,
    ) -> Result<(), ValidationEngineError> {
        match schema {
            serde_json::Value::Object(obj) => {
                metrics.type_count += 1;

                // Check for required type field in strict mode
                if options.strict_mode && !obj.contains_key("type") && path.is_empty() {
                    warnings.push(
                        ValidationError::warning("W001", "Root schema should have a 'type' field")
                            .with_path(path)
                            .with_category(ErrorCategory::BestPractice),
                    );
                }

                // Validate type field
                if let Some(type_val) = obj.get("type") {
                    self.validate_json_type(type_val, path, errors)?;
                }

                // Validate properties
                if let Some(props) = obj.get("properties") {
                    if let serde_json::Value::Object(prop_map) = props {
                        metrics.property_count += prop_map.len();

                        // Check property count
                        if prop_map.len() > options.max_properties {
                            errors.push(
                                ValidationError::breaking(
                                    "E002",
                                    format!(
                                        "Too many properties: {} exceeds limit of {}",
                                        prop_map.len(),
                                        options.max_properties
                                    ),
                                )
                                .with_path(format!("{}.properties", path))
                                .with_category(ErrorCategory::Performance),
                            );
                        }

                        for (name, prop_schema) in prop_map {
                            let prop_path = if path.is_empty() {
                                format!("properties.{}", name)
                            } else {
                                format!("{}.properties.{}", path, name)
                            };

                            // Recursively validate nested schemas
                            let current_depth = path.matches('.').count();
                            if current_depth < options.max_nesting_depth {
                                metrics.nesting_depth = metrics.nesting_depth.max(current_depth + 1);
                                self.validate_json_schema_structure(
                                    prop_schema,
                                    &prop_path,
                                    errors,
                                    warnings,
                                    options,
                                    metrics,
                                )?;
                            } else {
                                errors.push(
                                    ValidationError::breaking(
                                        "E003",
                                        format!(
                                            "Nesting depth {} exceeds maximum {}",
                                            current_depth, options.max_nesting_depth
                                        ),
                                    )
                                    .with_path(&prop_path)
                                    .with_category(ErrorCategory::Performance),
                                );
                            }
                        }
                    }
                }

                // Validate items (for arrays)
                if let Some(items) = obj.get("items") {
                    let items_path = if path.is_empty() {
                        "items".to_string()
                    } else {
                        format!("{}.items", path)
                    };
                    self.validate_json_schema_structure(items, &items_path, errors, warnings, options, metrics)?;
                }

                // Check for deprecated patterns
                if obj.contains_key("id") && !obj.contains_key("$id") {
                    warnings.push(
                        ValidationError::warning(
                            "W002",
                            "Use '$id' instead of 'id' (deprecated in JSON Schema draft-06+)",
                        )
                        .with_path(path)
                        .with_category(ErrorCategory::Deprecation),
                    );
                }

                // Validate additionalProperties
                if let Some(additional) = obj.get("additionalProperties") {
                    if !additional.is_boolean() && !additional.is_object() {
                        errors.push(
                            ValidationError::breaking(
                                "E004",
                                "additionalProperties must be boolean or schema object",
                            )
                            .with_path(format!("{}.additionalProperties", path))
                            .with_category(ErrorCategory::Type),
                        );
                    }
                }

                // Validate required field
                if let Some(required) = obj.get("required") {
                    if let serde_json::Value::Array(req_arr) = required {
                        for (i, req_item) in req_arr.iter().enumerate() {
                            if !req_item.is_string() {
                                errors.push(
                                    ValidationError::breaking(
                                        "E005",
                                        "Required array must contain only strings",
                                    )
                                    .with_path(format!("{}.required[{}]", path, i))
                                    .with_category(ErrorCategory::Type),
                                );
                            }
                        }

                        // Check for duplicates in required
                        let req_set: HashSet<_> = req_arr.iter().filter_map(|v| v.as_str()).collect();
                        if req_set.len() != req_arr.len() {
                            warnings.push(
                                ValidationError::warning("W003", "Required array contains duplicates")
                                    .with_path(format!("{}.required", path))
                                    .with_category(ErrorCategory::BestPractice),
                            );
                        }
                    } else {
                        errors.push(
                            ValidationError::breaking("E006", "Required must be an array")
                                .with_path(format!("{}.required", path))
                                .with_category(ErrorCategory::Type),
                        );
                    }
                }

                // Validate enum
                if let Some(enum_val) = obj.get("enum") {
                    if let serde_json::Value::Array(enum_arr) = enum_val {
                        if enum_arr.is_empty() {
                            errors.push(
                                ValidationError::breaking("E007", "Enum array must not be empty")
                                    .with_path(format!("{}.enum", path))
                                    .with_category(ErrorCategory::Semantic),
                            );
                        }
                    } else {
                        errors.push(
                            ValidationError::breaking("E008", "Enum must be an array")
                                .with_path(format!("{}.enum", path))
                                .with_category(ErrorCategory::Type),
                        );
                    }
                }

                // Validate numeric constraints
                self.validate_numeric_constraints(obj, path, errors)?;
            }
            serde_json::Value::Bool(_) => {
                // Boolean schemas are valid in JSON Schema
            }
            _ => {
                if options.strict_mode {
                    errors.push(
                        ValidationError::breaking("E009", "Schema must be an object or boolean")
                            .with_path(path)
                            .with_category(ErrorCategory::Structural),
                    );
                }
            }
        }

        Ok(())
    }

    /// Validate JSON type field
    fn validate_json_type(
        &self,
        type_val: &serde_json::Value,
        path: &str,
        errors: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        let valid_types = ["object", "array", "string", "number", "integer", "boolean", "null"];

        match type_val {
            serde_json::Value::String(t) => {
                if !valid_types.contains(&t.as_str()) {
                    errors.push(
                        ValidationError::breaking("E010", format!("Invalid type: '{}'", t))
                            .with_path(format!("{}.type", path))
                            .with_category(ErrorCategory::Type)
                            .with_suggestion(format!("Valid types are: {}", valid_types.join(", "))),
                    );
                }
            }
            serde_json::Value::Array(types) => {
                // Type can be an array for union types
                for (i, t) in types.iter().enumerate() {
                    if let serde_json::Value::String(type_str) = t {
                        if !valid_types.contains(&type_str.as_str()) {
                            errors.push(
                                ValidationError::breaking("E010", format!("Invalid type in union: '{}'", type_str))
                                    .with_path(format!("{}.type[{}]", path, i))
                                    .with_category(ErrorCategory::Type),
                            );
                        }
                    } else {
                        errors.push(
                            ValidationError::breaking("E011", "Type union must contain only strings")
                                .with_path(format!("{}.type[{}]", path, i))
                                .with_category(ErrorCategory::Type),
                        );
                    }
                }
            }
            _ => {
                errors.push(
                    ValidationError::breaking("E012", "Type must be a string or array of strings")
                        .with_path(format!("{}.type", path))
                        .with_category(ErrorCategory::Type),
                );
            }
        }

        Ok(())
    }

    /// Validate numeric constraints
    fn validate_numeric_constraints(
        &self,
        obj: &serde_json::Map<String, serde_json::Value>,
        path: &str,
        errors: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        // Check minimum/maximum
        if let (Some(min), Some(max)) = (obj.get("minimum"), obj.get("maximum")) {
            if let (Some(min_val), Some(max_val)) = (min.as_f64(), max.as_f64()) {
                if min_val > max_val {
                    errors.push(
                        ValidationError::breaking(
                            "E013",
                            format!("minimum ({}) cannot be greater than maximum ({})", min_val, max_val),
                        )
                        .with_path(path)
                        .with_category(ErrorCategory::Semantic),
                    );
                }
            }
        }

        // Check minLength/maxLength
        if let (Some(min), Some(max)) = (obj.get("minLength"), obj.get("maxLength")) {
            if let (Some(min_val), Some(max_val)) = (min.as_u64(), max.as_u64()) {
                if min_val > max_val {
                    errors.push(
                        ValidationError::breaking(
                            "E014",
                            format!("minLength ({}) cannot be greater than maxLength ({})", min_val, max_val),
                        )
                        .with_path(path)
                        .with_category(ErrorCategory::Semantic),
                    );
                }
            }
        }

        // Check minItems/maxItems
        if let (Some(min), Some(max)) = (obj.get("minItems"), obj.get("maxItems")) {
            if let (Some(min_val), Some(max_val)) = (min.as_u64(), max.as_u64()) {
                if min_val > max_val {
                    errors.push(
                        ValidationError::breaking(
                            "E015",
                            format!("minItems ({}) cannot be greater than maxItems ({})", min_val, max_val),
                        )
                        .with_path(path)
                        .with_category(ErrorCategory::Semantic),
                    );
                }
            }
        }

        Ok(())
    }

    /// Semantic validation for JSON Schema
    fn validate_json_schema_semantics(
        &self,
        schema: &serde_json::Value,
        path: &str,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        if let serde_json::Value::Object(obj) = schema {
            // Check for conflicting constraints
            if obj.contains_key("minimum") && obj.contains_key("exclusiveMinimum") {
                if let (Some(min), Some(exc_min)) = (
                    obj.get("minimum").and_then(|v| v.as_f64()),
                    obj.get("exclusiveMinimum").and_then(|v| v.as_f64()),
                ) {
                    if min >= exc_min {
                        warnings.push(
                            ValidationError::warning(
                                "W004",
                                "Both minimum and exclusiveMinimum specified - consider using just one",
                            )
                            .with_path(path)
                            .with_category(ErrorCategory::BestPractice),
                        );
                    }
                }
            }

            // Check required fields exist in properties
            if let (Some(required), Some(props)) = (obj.get("required"), obj.get("properties")) {
                if let (serde_json::Value::Array(req_arr), serde_json::Value::Object(prop_map)) = (required, props) {
                    for req_item in req_arr {
                        if let serde_json::Value::String(req_name) = req_item {
                            if !prop_map.contains_key(req_name) {
                                warnings.push(
                                    ValidationError::warning(
                                        "W005",
                                        format!("Required field '{}' not defined in properties", req_name),
                                    )
                                    .with_path(format!("{}.required", path))
                                    .with_category(ErrorCategory::Semantic),
                                );
                            }
                        }
                    }
                }
            }

            // Recursively check nested schemas
            if let Some(props) = obj.get("properties") {
                if let serde_json::Value::Object(prop_map) = props {
                    for (name, prop_schema) in prop_map {
                        let prop_path = if path.is_empty() {
                            format!("properties.{}", name)
                        } else {
                            format!("{}.properties.{}", path, name)
                        };
                        self.validate_json_schema_semantics(prop_schema, &prop_path, errors, warnings)?;
                    }
                }
            }

            if let Some(items) = obj.get("items") {
                let items_path = if path.is_empty() {
                    "items".to_string()
                } else {
                    format!("{}.items", path)
                };
                self.validate_json_schema_semantics(items, &items_path, errors, warnings)?;
            }
        }

        Ok(())
    }

    /// Security checks for JSON Schema
    fn check_json_schema_security(
        &self,
        schema: &serde_json::Value,
        path: &str,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        if let serde_json::Value::Object(obj) = schema {
            // Check for potentially dangerous patterns in regex
            if let Some(pattern) = obj.get("pattern") {
                if let serde_json::Value::String(p) = pattern {
                    // Check for ReDoS patterns (catastrophic backtracking)
                    if self.is_redos_vulnerable(p) {
                        errors.push(
                            ValidationError::breaking(
                                "S001",
                                "Potentially vulnerable regex pattern (ReDoS risk)",
                            )
                            .with_path(format!("{}.pattern", path))
                            .with_category(ErrorCategory::Security)
                            .with_suggestion("Simplify the regex pattern or use atomic groups"),
                        );
                    }
                }
            }

            // Check for external $ref URLs (potential SSRF)
            if let Some(ref_val) = obj.get("$ref") {
                if let serde_json::Value::String(ref_str) = ref_val {
                    if ref_str.starts_with("http://") || ref_str.starts_with("https://") {
                        warnings.push(
                            ValidationError::warning(
                                "S002",
                                "External URL reference detected - ensure trusted source",
                            )
                            .with_path(format!("{}.$ref", path))
                            .with_category(ErrorCategory::Security),
                        );
                    }
                }
            }

            // Recursively check nested schemas
            if let Some(props) = obj.get("properties") {
                if let serde_json::Value::Object(prop_map) = props {
                    for (name, prop_schema) in prop_map {
                        let prop_path = if path.is_empty() {
                            format!("properties.{}", name)
                        } else {
                            format!("{}.properties.{}", path, name)
                        };
                        self.check_json_schema_security(prop_schema, &prop_path, errors, warnings)?;
                    }
                }
            }

            if let Some(items) = obj.get("items") {
                let items_path = if path.is_empty() {
                    "items".to_string()
                } else {
                    format!("{}.items", path)
                };
                self.check_json_schema_security(items, &items_path, errors, warnings)?;
            }
        }

        Ok(())
    }

    /// Check for ReDoS vulnerable patterns
    fn is_redos_vulnerable(&self, pattern: &str) -> bool {
        // Simple heuristics for ReDoS detection
        // Look for patterns like (.*)+, (.+)+, (a+)+, nested quantifiers
        let dangerous_patterns = [
            r"\(\.\*\)\+",
            r"\(\.\+\)\+",
            r"\([^)]+\+\)\+",
            r"\(\[.*\]\+\)\+",
            r"\(\.\*\)\*",
        ];

        for dangerous in dangerous_patterns {
            if let Ok(re) = Regex::new(dangerous) {
                if re.is_match(pattern) {
                    return true;
                }
            }
        }

        // Check for excessive quantifiers
        let quantifier_count = pattern.matches('+').count() + pattern.matches('*').count();
        if quantifier_count > 5 {
            return true;
        }

        false
    }

    /// Performance checks for JSON Schema
    fn check_json_schema_performance(
        &self,
        _schema: &serde_json::Value,
        warnings: &mut Vec<ValidationError>,
        metrics: &ValidationMetrics,
        options: &ValidationOptions,
    ) -> Result<(), ValidationEngineError> {
        // Check complexity score
        if metrics.complexity_score > 80 {
            warnings.push(
                ValidationError::warning("P001", "High schema complexity may impact validation performance")
                    .with_category(ErrorCategory::Performance)
                    .with_suggestion("Consider breaking the schema into smaller referenced schemas"),
            );
        }

        // Check nesting depth
        if metrics.nesting_depth > options.max_nesting_depth / 2 {
            warnings.push(
                ValidationError::warning(
                    "P002",
                    format!(
                        "Deep nesting ({} levels) may impact performance",
                        metrics.nesting_depth
                    ),
                )
                .with_category(ErrorCategory::Performance),
            );
        }

        // Check property count
        if metrics.property_count > options.max_properties / 2 {
            warnings.push(
                ValidationError::warning(
                    "P003",
                    format!(
                        "Large number of properties ({}) may impact performance",
                        metrics.property_count
                    ),
                )
                .with_category(ErrorCategory::Performance),
            );
        }

        Ok(())
    }

    /// Calculate JSON Schema complexity score (0-100)
    fn calculate_json_complexity(&self, schema: &serde_json::Value) -> u8 {
        let mut score = 0u32;

        fn count_complexity(value: &serde_json::Value) -> u32 {
            match value {
                serde_json::Value::Object(obj) => {
                    let mut complexity = obj.len() as u32;
                    for v in obj.values() {
                        complexity += count_complexity(v);
                    }
                    // Add weight for special keywords
                    if obj.contains_key("oneOf") || obj.contains_key("anyOf") || obj.contains_key("allOf") {
                        complexity += 5;
                    }
                    if obj.contains_key("$ref") {
                        complexity += 2;
                    }
                    if obj.contains_key("pattern") {
                        complexity += 3;
                    }
                    complexity
                }
                serde_json::Value::Array(arr) => {
                    let mut complexity = arr.len() as u32;
                    for v in arr {
                        complexity += count_complexity(v);
                    }
                    complexity
                }
                _ => 1,
            }
        }

        score += count_complexity(schema);

        // Normalize to 0-100
        (score.min(100) as u8)
    }

    /// Validate Avro schema
    fn validate_avro_schema(
        &self,
        content: &str,
        options: &ValidationOptions,
    ) -> Result<(Vec<ValidationError>, Vec<ValidationError>, ValidationMetrics), ValidationEngineError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        let structural_start = Instant::now();

        // Try to parse as Avro schema using apache-avro
        match apache_avro::Schema::parse_str(content) {
            Ok(schema) => {
                // Schema parsed successfully, do additional validation
                self.validate_avro_structure(&schema, "", &mut errors, &mut warnings, options, &mut metrics)?;
            }
            Err(e) => {
                errors.push(
                    ValidationError::breaking("E100", format!("Avro schema parse error: {}", e))
                        .with_category(ErrorCategory::Structural),
                );
            }
        }

        metrics.structural_duration_ms = structural_start.elapsed().as_millis() as u64;

        // Semantic validation
        if options.enable_semantic_checks && errors.is_empty() {
            let semantic_start = Instant::now();
            // Parse JSON to do semantic checks
            if let Ok(json_schema) = serde_json::from_str::<serde_json::Value>(content) {
                self.validate_avro_semantics(&json_schema, "", &mut warnings)?;
            }
            metrics.semantic_duration_ms = semantic_start.elapsed().as_millis() as u64;
        }

        // Security checks
        if options.enable_security_checks {
            let security_start = Instant::now();
            if let Ok(json_schema) = serde_json::from_str::<serde_json::Value>(content) {
                self.check_avro_security(&json_schema, &mut warnings)?;
            }
            metrics.security_duration_ms = security_start.elapsed().as_millis() as u64;
        }

        // Calculate complexity
        if let Ok(json_schema) = serde_json::from_str::<serde_json::Value>(content) {
            metrics.complexity_score = self.calculate_json_complexity(&json_schema);
        }

        Ok((errors, warnings, metrics))
    }

    /// Validate Avro schema structure
    fn validate_avro_structure(
        &self,
        schema: &apache_avro::Schema,
        _path: &str,
        _errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
        _options: &ValidationOptions,
        metrics: &mut ValidationMetrics,
    ) -> Result<(), ValidationEngineError> {
        match schema {
            apache_avro::Schema::Record(record) => {
                metrics.type_count += 1;
                metrics.property_count += record.fields.len();

                // Check for best practices
                if record.doc.is_none() {
                    warnings.push(
                        ValidationError::warning("W100", "Record schema should have documentation")
                            .with_path(&record.name.fullname(None))
                            .with_category(ErrorCategory::BestPractice),
                    );
                }

                // Count nesting
                for field in &record.fields {
                    self.count_avro_depth(&field.schema, 1, metrics);
                }
            }
            apache_avro::Schema::Enum(enum_schema) => {
                metrics.type_count += 1;
                if enum_schema.symbols.is_empty() {
                    warnings.push(
                        ValidationError::warning("W101", "Enum should have at least one symbol")
                            .with_category(ErrorCategory::Semantic),
                    );
                }
            }
            apache_avro::Schema::Array(item_schema) => {
                self.count_avro_depth(item_schema, 1, metrics);
            }
            apache_avro::Schema::Map(value_schema) => {
                self.count_avro_depth(value_schema, 1, metrics);
            }
            apache_avro::Schema::Union(union) => {
                for variant in union.variants() {
                    self.count_avro_depth(variant, 1, metrics);
                }
            }
            _ => {
                metrics.type_count += 1;
            }
        }

        Ok(())
    }

    /// Count Avro schema nesting depth
    fn count_avro_depth(&self, schema: &apache_avro::Schema, depth: usize, metrics: &mut ValidationMetrics) {
        metrics.nesting_depth = metrics.nesting_depth.max(depth);

        match schema {
            apache_avro::Schema::Record(record) => {
                metrics.type_count += 1;
                for field in &record.fields {
                    self.count_avro_depth(&field.schema, depth + 1, metrics);
                }
            }
            apache_avro::Schema::Array(item_schema) => {
                self.count_avro_depth(item_schema, depth + 1, metrics);
            }
            apache_avro::Schema::Map(value_schema) => {
                self.count_avro_depth(value_schema, depth + 1, metrics);
            }
            apache_avro::Schema::Union(union) => {
                for variant in union.variants() {
                    self.count_avro_depth(variant, depth + 1, metrics);
                }
            }
            _ => {}
        }
    }

    /// Semantic validation for Avro
    fn validate_avro_semantics(
        &self,
        schema: &serde_json::Value,
        path: &str,
        warnings: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        if let serde_json::Value::Object(obj) = schema {
            // Check for default values matching field type
            if let (Some(field_type), Some(_default)) = (obj.get("type"), obj.get("default")) {
                // This is a simplified check - full validation would need type matching
                if field_type.as_str() == Some("null") {
                    warnings.push(
                        ValidationError::warning("W102", "Null type with default value may be redundant")
                            .with_path(path)
                            .with_category(ErrorCategory::BestPractice),
                    );
                }
            }

            // Check fields array
            if let Some(fields) = obj.get("fields") {
                if let serde_json::Value::Array(field_arr) = fields {
                    for (i, field) in field_arr.iter().enumerate() {
                        let field_path = format!("{}.fields[{}]", path, i);
                        self.validate_avro_semantics(field, &field_path, warnings)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Security checks for Avro schema
    fn check_avro_security(
        &self,
        schema: &serde_json::Value,
        warnings: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        if let serde_json::Value::Object(obj) = schema {
            // Check for very long strings in names (potential DoS)
            if let Some(name) = obj.get("name") {
                if let serde_json::Value::String(n) = name {
                    if n.len() > 256 {
                        warnings.push(
                            ValidationError::warning("S100", "Very long name may impact performance")
                                .with_category(ErrorCategory::Security),
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate Protobuf schema
    fn validate_protobuf_schema(
        &self,
        content: &str,
        options: &ValidationOptions,
    ) -> Result<(Vec<ValidationError>, Vec<ValidationError>, ValidationMetrics), ValidationEngineError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        let structural_start = Instant::now();

        // Parse and validate protobuf structure
        self.validate_protobuf_structure(content, &mut errors, &mut warnings, &mut metrics, options)?;

        metrics.structural_duration_ms = structural_start.elapsed().as_millis() as u64;

        // Semantic validation
        if options.enable_semantic_checks {
            let semantic_start = Instant::now();
            self.validate_protobuf_semantics(content, &mut warnings)?;
            metrics.semantic_duration_ms = semantic_start.elapsed().as_millis() as u64;
        }

        // Security checks
        if options.enable_security_checks {
            let security_start = Instant::now();
            self.check_protobuf_security(content, &mut warnings)?;
            metrics.security_duration_ms = security_start.elapsed().as_millis() as u64;
        }

        Ok((errors, warnings, metrics))
    }

    /// Validate Protobuf structure
    fn validate_protobuf_structure(
        &self,
        content: &str,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
        metrics: &mut ValidationMetrics,
        _options: &ValidationOptions,
    ) -> Result<(), ValidationEngineError> {
        let mut has_syntax = false;
        let mut field_numbers = HashSet::new();
        let mut in_message = false;
        let mut message_depth = 0;

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let line_number = line_num + 1;

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // Check for syntax declaration
            if trimmed.starts_with("syntax") {
                has_syntax = true;
                if !trimmed.contains("proto3") && !trimmed.contains("proto2") {
                    errors.push(
                        ValidationError::breaking("E200", "Invalid syntax version")
                            .with_location(line_number, 0)
                            .with_category(ErrorCategory::Structural),
                    );
                }
            }

            // Track message blocks
            if trimmed.starts_with("message ") {
                in_message = true;
                message_depth += 1;
                metrics.type_count += 1;
                metrics.nesting_depth = metrics.nesting_depth.max(message_depth);

                // Check message naming convention
                let name_part = trimmed.strip_prefix("message ").unwrap_or("").trim();
                let name = name_part.split_whitespace().next().unwrap_or("");
                if !name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    warnings.push(
                        ValidationError::warning("W200", "Message names should be PascalCase")
                            .with_location(line_number, 0)
                            .with_category(ErrorCategory::BestPractice),
                    );
                }
            }

            // Track closing braces
            if trimmed == "}" {
                if in_message {
                    message_depth -= 1;
                    if message_depth == 0 {
                        in_message = false;
                        field_numbers.clear();
                    }
                }
            }

            // Parse field definitions
            if in_message && trimmed.contains('=') && !trimmed.starts_with("option") {
                metrics.property_count += 1;

                // Extract field number
                if let Some(eq_pos) = trimmed.find('=') {
                    let after_eq = &trimmed[eq_pos + 1..];
                    let field_num_str = after_eq
                        .trim()
                        .split(|c: char| !c.is_ascii_digit())
                        .next()
                        .unwrap_or("");

                    if let Ok(field_num) = field_num_str.parse::<u32>() {
                        // Check for reserved field numbers
                        if (19000..=19999).contains(&field_num) {
                            errors.push(
                                ValidationError::breaking(
                                    "E201",
                                    format!("Field number {} is in reserved range 19000-19999", field_num),
                                )
                                .with_location(line_number, eq_pos + 1)
                                .with_category(ErrorCategory::Structural),
                            );
                        }

                        // Check for duplicate field numbers
                        if !field_numbers.insert(field_num) {
                            errors.push(
                                ValidationError::breaking("E202", format!("Duplicate field number: {}", field_num))
                                    .with_location(line_number, eq_pos + 1)
                                    .with_category(ErrorCategory::Structural),
                            );
                        }

                        // Check field number limits
                        if field_num > 536870911 {
                            errors.push(
                                ValidationError::breaking(
                                    "E203",
                                    format!("Field number {} exceeds maximum (536870911)", field_num),
                                )
                                .with_location(line_number, eq_pos + 1)
                                .with_category(ErrorCategory::Structural),
                            );
                        }
                    }
                }
            }
        }

        // Check for missing syntax declaration
        if !has_syntax {
            warnings.push(
                ValidationError::warning(
                    "W201",
                    "Missing syntax declaration - defaulting to proto2",
                )
                .with_category(ErrorCategory::BestPractice)
                .with_suggestion("Add 'syntax = \"proto3\";' at the beginning"),
            );
        }

        // Calculate complexity
        metrics.complexity_score = ((metrics.type_count * 10 + metrics.property_count * 2 + metrics.nesting_depth * 5)
            .min(100)) as u8;

        Ok(())
    }

    /// Semantic validation for Protobuf
    fn validate_protobuf_semantics(
        &self,
        content: &str,
        warnings: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        // Check for deprecated features
        if content.contains("required ") {
            warnings.push(
                ValidationError::warning(
                    "W202",
                    "'required' fields are deprecated in proto3",
                )
                .with_category(ErrorCategory::Deprecation)
                .with_suggestion("Use optional or remove the label in proto3"),
            );
        }

        // Check for group syntax (deprecated)
        if content.contains("group ") {
            warnings.push(
                ValidationError::warning("W203", "'group' syntax is deprecated")
                    .with_category(ErrorCategory::Deprecation)
                    .with_suggestion("Use nested messages instead"),
            );
        }

        Ok(())
    }

    /// Security checks for Protobuf
    fn check_protobuf_security(
        &self,
        content: &str,
        warnings: &mut Vec<ValidationError>,
    ) -> Result<(), ValidationEngineError> {
        // Check for import statements (potential supply chain risk)
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") {
                warnings.push(
                    ValidationError::warning(
                        "S200",
                        "Import statement detected - ensure imported proto files are from trusted sources",
                    )
                    .with_location(line_num + 1, 0)
                    .with_category(ErrorCategory::Security),
                );
            }
        }

        Ok(())
    }

    /// Check compatibility between schemas
    fn check_compatibility(
        &self,
        new_content: &str,
        old_content: &str,
        format: SchemaFormat,
        mode: CompatibilityMode,
    ) -> Result<CompatibilityCheckResult, ValidationEngineError> {
        let start = Instant::now();
        let mut violations = Vec::new();

        // Use the compatibility-checker crate for detailed checks
        let compat_mode = mode.into();

        match format {
            SchemaFormat::JsonSchema => {
                let checker = compatibility_checker::formats::JsonSchemaCompatibilityChecker::new();

                let compat_violations = match mode {
                    CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
                        checker.check_backward(new_content, old_content)
                    }
                    CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
                        checker.check_forward(new_content, old_content)
                    }
                    CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
                        let mut v = checker.check_backward(new_content, old_content)?;
                        v.extend(checker.check_forward(new_content, old_content)?);
                        Ok(v)
                    }
                    CompatibilityMode::None => Ok(Vec::new()),
                }
                .map_err(|e| ValidationEngineError::InternalError(e.to_string()))?;

                violations = compat_violations
                    .into_iter()
                    .map(|v| CompatibilityViolation {
                        violation_type: convert_violation_type(&v.violation_type),
                        field_path: v.field_path,
                        old_value: v.old_value,
                        new_value: v.new_value,
                        severity: convert_severity(&v.severity),
                        description: v.description,
                    })
                    .collect();
            }
            SchemaFormat::Avro => {
                let checker = compatibility_checker::formats::AvroCompatibilityChecker::new();

                let compat_violations = match mode {
                    CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
                        checker.check_backward(new_content, old_content)
                    }
                    CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
                        checker.check_forward(new_content, old_content)
                    }
                    CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
                        let mut v = checker.check_backward(new_content, old_content)?;
                        v.extend(checker.check_forward(new_content, old_content)?);
                        Ok(v)
                    }
                    CompatibilityMode::None => Ok(Vec::new()),
                }
                .map_err(|e| ValidationEngineError::InternalError(e.to_string()))?;

                violations = compat_violations
                    .into_iter()
                    .map(|v| CompatibilityViolation {
                        violation_type: convert_violation_type(&v.violation_type),
                        field_path: v.field_path,
                        old_value: v.old_value,
                        new_value: v.new_value,
                        severity: convert_severity(&v.severity),
                        description: v.description,
                    })
                    .collect();
            }
            SchemaFormat::Protobuf => {
                let checker = compatibility_checker::formats::ProtobufCompatibilityChecker::new();

                let compat_violations = match mode {
                    CompatibilityMode::Backward | CompatibilityMode::BackwardTransitive => {
                        checker.check_backward(new_content, old_content)
                    }
                    CompatibilityMode::Forward | CompatibilityMode::ForwardTransitive => {
                        checker.check_forward(new_content, old_content)
                    }
                    CompatibilityMode::Full | CompatibilityMode::FullTransitive => {
                        let mut v = checker.check_backward(new_content, old_content)?;
                        v.extend(checker.check_forward(new_content, old_content)?);
                        Ok(v)
                    }
                    CompatibilityMode::None => Ok(Vec::new()),
                }
                .map_err(|e| ValidationEngineError::InternalError(e.to_string()))?;

                violations = compat_violations
                    .into_iter()
                    .map(|v| CompatibilityViolation {
                        violation_type: convert_violation_type(&v.violation_type),
                        field_path: v.field_path,
                        old_value: v.old_value,
                        new_value: v.new_value,
                        severity: convert_severity(&v.severity),
                        description: v.description,
                    })
                    .collect();
            }
            SchemaFormat::Unknown => {
                return Err(ValidationEngineError::UnsupportedFormat);
            }
        }

        let is_compatible = !violations.iter().any(|v| v.severity == ErrorSeverity::Breaking);

        Ok(CompatibilityCheckResult {
            is_compatible,
            mode,
            violations,
            check_duration_ms: start.elapsed().as_millis() as u64,
        })
    }
}

/// Convert violation type from compatibility-checker to our type
fn convert_violation_type(vt: &compatibility_checker::violation::ViolationType) -> ViolationType {
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
        compatibility_checker::violation::ViolationType::UnionTypesIncompatible => ViolationType::UnionTypesIncompatible,
        compatibility_checker::violation::ViolationType::NamespaceChanged => ViolationType::NamespaceChanged,
        compatibility_checker::violation::ViolationType::NameChanged => ViolationType::NameChanged,
        compatibility_checker::violation::ViolationType::Custom(s) => ViolationType::Custom(s.clone()),
    }
}

/// Convert severity from compatibility-checker to our type
fn convert_severity(sev: &compatibility_checker::violation::ViolationSeverity) -> ErrorSeverity {
    match sev {
        compatibility_checker::violation::ViolationSeverity::Breaking => ErrorSeverity::Breaking,
        compatibility_checker::violation::ViolationSeverity::Warning => ErrorSeverity::Warning,
        compatibility_checker::violation::ViolationSeverity::Info => ErrorSeverity::Info,
    }
}

impl Default for ValidationEngine {
    fn default() -> Self {
        Self::new(ValidationEngineConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_json_schema() {
        let engine = ValidationEngine::default();
        let content = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let format = engine.detect_format(content).unwrap();
        assert_eq!(format, SchemaFormat::JsonSchema);
    }

    #[test]
    fn test_detect_json_schema_with_ref() {
        let engine = ValidationEngine::default();
        let content = r#"{"$schema": "http://json-schema.org/draft-07/schema#", "type": "object"}"#;
        let format = engine.detect_format(content).unwrap();
        assert_eq!(format, SchemaFormat::JsonSchema);
    }

    #[test]
    fn test_detect_avro() {
        let engine = ValidationEngine::default();
        let content = r#"{
            "type": "record",
            "name": "User",
            "namespace": "com.example",
            "fields": [{"name": "id", "type": "long"}]
        }"#;
        let format = engine.detect_format(content).unwrap();
        assert_eq!(format, SchemaFormat::Avro);
    }

    #[test]
    fn test_detect_protobuf() {
        let engine = ValidationEngine::default();
        let content = r#"
            syntax = "proto3";
            message User {
                string name = 1;
            }
        "#;
        let format = engine.detect_format(content).unwrap();
        assert_eq!(format, SchemaFormat::Protobuf);
    }

    #[test]
    fn test_validate_json_schema_simple() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#)
            .with_format(SchemaFormat::JsonSchema);

        let result = engine.validate(&input).unwrap();
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_json_schema_invalid_type() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{"type": "invalid"}"#)
            .with_format(SchemaFormat::JsonSchema);

        let result = engine.validate(&input).unwrap();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_json_schema_min_max_constraint() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{"type": "number", "minimum": 10, "maximum": 5}"#)
            .with_format(SchemaFormat::JsonSchema);

        let result = engine.validate(&input).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.code == "E013"));
    }

    #[test]
    fn test_validate_avro_schema() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{
            "type": "record",
            "name": "User",
            "fields": [
                {"name": "id", "type": "long"},
                {"name": "name", "type": "string"}
            ]
        }"#)
            .with_format(SchemaFormat::Avro);

        let result = engine.validate(&input).unwrap();
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_avro_invalid() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{"type": "invalid_type"}"#)
            .with_format(SchemaFormat::Avro);

        let result = engine.validate(&input).unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validate_protobuf() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"
            syntax = "proto3";
            
            message User {
                string name = 1;
                int32 id = 2;
            }
        "#)
            .with_format(SchemaFormat::Protobuf);

        let result = engine.validate(&input).unwrap();
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_protobuf_duplicate_field_number() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"
            syntax = "proto3";
            
            message User {
                string name = 1;
                int32 id = 1;
            }
        "#)
            .with_format(SchemaFormat::Protobuf);

        let result = engine.validate(&input).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.code == "E202"));
    }

    #[test]
    fn test_schema_too_large() {
        let config = ValidationEngineConfig {
            max_schema_size: 100,
            ..Default::default()
        };
        let engine = ValidationEngine::new(config);

        let large_content = "x".repeat(200);
        let input = ValidationInput::new(large_content);

        let result = engine.validate(&input);
        assert!(matches!(result, Err(ValidationEngineError::SchemaTooLarge { .. })));
    }

    #[test]
    fn test_content_hash_deterministic() {
        let engine = ValidationEngine::default();
        let content = r#"{"type": "object"}"#;

        let hash1 = engine.calculate_hash(content);
        let hash2 = engine.calculate_hash(content);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compatibility_check() {
        let engine = ValidationEngine::default();

        let old_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        let new_schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "number"}}}"#;

        let input = ValidationInput::new(new_schema)
            .with_format(SchemaFormat::JsonSchema)
            .with_previous(old_schema)
            .with_compatibility_mode(CompatibilityMode::Backward);

        let result = engine.validate(&input).unwrap();
        assert!(result.is_valid);
        assert!(result.compatibility.as_ref().unwrap().is_compatible);
    }

    #[test]
    fn test_redos_detection() {
        let engine = ValidationEngine::default();

        // Pattern with nested quantifiers (potential ReDoS)
        assert!(engine.is_redos_vulnerable("(a+)+"));

        // Safe pattern
        assert!(!engine.is_redos_vulnerable("^[a-z]+$"));
    }

    #[test]
    fn test_metrics_populated() {
        let engine = ValidationEngine::default();
        let input = ValidationInput::new(r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"},
                "address": {
                    "type": "object",
                    "properties": {
                        "street": {"type": "string"},
                        "city": {"type": "string"}
                    }
                }
            }
        }"#)
            .with_format(SchemaFormat::JsonSchema);

        let result = engine.validate(&input).unwrap();

        assert!(result.metrics.total_duration_ms > 0);
        assert!(result.metrics.schema_size_bytes > 0);
        assert!(result.metrics.property_count > 0);
        assert!(result.metrics.nesting_depth > 0);
    }
}
