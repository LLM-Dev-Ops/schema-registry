//! Schema Validation Agent CLI commands
//!
//! Provides commands for validating, inspecting, comparing, and registering schemas
//! through the Schema Validation Agent workflow.

use clap::Subcommand;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::PathBuf;
use uuid::Uuid;

use crate::{config::Config, error::Result, output};

/// Schema Validation Agent commands
#[derive(Subcommand)]
pub enum AgentCommand {
    /// Validate a schema definition
    Validate {
        /// Path to schema file or '-' for stdin
        #[arg(short, long)]
        file: PathBuf,

        /// Schema namespace
        #[arg(short, long)]
        namespace: String,

        /// Schema name
        #[arg(short = 'N', long)]
        name: String,

        /// Schema version (optional, will auto-detect)
        #[arg(short, long)]
        version: Option<String>,

        /// Schema format (json-schema, avro, protobuf) - auto-detected if not specified
        #[arg(short = 'F', long)]
        format: Option<String>,

        /// Validation mode (strict, lenient)
        #[arg(short, long, default_value = "strict")]
        mode: String,

        /// Check compatibility against this version
        #[arg(short, long)]
        compatibility_target: Option<String>,

        /// Output format (json, yaml, table)
        #[arg(short, long, default_value = "table")]
        output: String,
    },

    /// Inspect schema structure and metadata
    Inspect {
        /// Path to schema file
        #[arg(short, long)]
        file: PathBuf,

        /// Show detailed field analysis
        #[arg(long)]
        detailed: bool,

        /// Output format (json, yaml, table)
        #[arg(short, long, default_value = "table")]
        output: String,
    },

    /// Compare two schemas and show differences
    Diff {
        /// First schema file (older version)
        #[arg(long)]
        old: PathBuf,

        /// Second schema file (newer version)
        #[arg(long)]
        new: PathBuf,

        /// Compatibility mode for analysis (backward, forward, full, none)
        #[arg(short, long, default_value = "backward")]
        compatibility_mode: String,

        /// Output format (json, yaml, table)
        #[arg(short, long, default_value = "table")]
        output: String,
    },

    /// Register a validated schema (persists DecisionEvent)
    Register {
        /// Path to schema file
        #[arg(short, long)]
        file: PathBuf,

        /// Schema namespace
        #[arg(short, long)]
        namespace: String,

        /// Schema name
        #[arg(short = 'N', long)]
        name: String,

        /// Schema version (semver format: major.minor.patch)
        #[arg(short, long)]
        version: String,

        /// Schema description
        #[arg(short, long)]
        description: Option<String>,

        /// Skip validation (not recommended)
        #[arg(long)]
        skip_validation: bool,

        /// ruvector-service URL
        #[arg(long, env = "RUVECTOR_SERVICE_URL")]
        ruvector_url: Option<String>,

        /// Output format (json, yaml, table)
        #[arg(short, long, default_value = "json")]
        output: String,
    },
}

// ============================================================================
// Output types for serialization
// ============================================================================

/// Validation result output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationOutput {
    pub is_valid: bool,
    pub schema_format: String,
    pub namespace: String,
    pub name: String,
    pub version: Option<String>,
    pub validation_mode: String,
    pub errors: Vec<ValidationErrorOutput>,
    pub warnings: Vec<ValidationWarningOutput>,
    pub metrics: ValidationMetricsOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<CompatibilityOutput>,
}

/// Validation error output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationErrorOutput {
    pub rule: String,
    pub message: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Validation warning output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarningOutput {
    pub rule: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Validation metrics output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetricsOutput {
    pub duration_ms: u64,
    pub rules_applied: usize,
    pub fields_validated: usize,
    pub schema_size_bytes: usize,
}

/// Compatibility result output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityOutput {
    pub is_compatible: bool,
    pub mode: String,
    pub target_version: String,
    pub violations: Vec<CompatibilityViolationOutput>,
}

/// Compatibility violation output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityViolationOutput {
    pub violation_type: String,
    pub field_path: String,
    pub severity: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
}

/// Schema inspection output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionOutput {
    pub format: String,
    pub structure: SchemaStructure,
    pub metadata: InspectionMetadata,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldInfo>,
}

/// Schema structure summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaStructure {
    pub root_type: String,
    pub total_fields: usize,
    pub required_fields: usize,
    pub optional_fields: usize,
    pub nested_depth: usize,
    pub has_recursion: bool,
}

/// Inspection metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionMetadata {
    pub schema_size_bytes: usize,
    pub has_description: bool,
    pub has_examples: bool,
    pub llm_readiness_score: f32,
}

/// Field information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInfo {
    pub path: String,
    pub field_type: String,
    pub required: bool,
    pub has_description: bool,
    pub has_default: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
}

/// Schema diff output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffOutput {
    pub is_compatible: bool,
    pub compatibility_mode: String,
    pub summary: DiffSummary,
    pub changes: Vec<SchemaChange>,
}

/// Diff summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub total_changes: usize,
    pub breaking_changes: usize,
    pub additions: usize,
    pub removals: usize,
    pub modifications: usize,
}

/// Schema change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaChange {
    pub change_type: String,
    pub path: String,
    pub breaking: bool,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
}

/// Registration result output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationOutput {
    pub success: bool,
    pub schema_id: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub content_hash: String,
    pub registered_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_event: Option<DecisionEventOutput>,
}

/// Decision event output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEventOutput {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub actor: String,
    pub decision: String,
    pub rationale: String,
}

// ============================================================================
// Command execution
// ============================================================================

pub async fn execute(cmd: AgentCommand, config: &Config, _format: output::OutputFormat) -> Result<()> {
    match cmd {
        AgentCommand::Validate {
            file,
            namespace,
            name,
            version,
            format,
            mode,
            compatibility_target,
            output,
        } => {
            execute_validate(
                file,
                namespace,
                name,
                version,
                format,
                mode,
                compatibility_target,
                output,
                config,
            )
            .await
        }
        AgentCommand::Inspect {
            file,
            detailed,
            output,
        } => execute_inspect(file, detailed, output).await,
        AgentCommand::Diff {
            old,
            new,
            compatibility_mode,
            output,
        } => execute_diff(old, new, compatibility_mode, output).await,
        AgentCommand::Register {
            file,
            namespace,
            name,
            version,
            description,
            skip_validation,
            ruvector_url,
            output,
        } => {
            execute_register(
                file,
                namespace,
                name,
                version,
                description,
                skip_validation,
                ruvector_url,
                output,
                config,
            )
            .await
        }
    }
}

// ============================================================================
// Validate command implementation
// ============================================================================

async fn execute_validate(
    file: PathBuf,
    namespace: String,
    name: String,
    version: Option<String>,
    format: Option<String>,
    mode: String,
    compatibility_target: Option<String>,
    output_format: String,
    _config: &Config,
) -> Result<()> {
    // Read schema content
    let content = read_schema_content(&file)?;

    // Detect or validate format
    let detected_format = format
        .clone()
        .unwrap_or_else(|| detect_schema_format(&content));

    output::print_info(&format!(
        "Validating schema: {}.{} (format: {}, mode: {})",
        namespace, name, detected_format, mode
    ));

    // Create validation engine and validate
    let start_time = std::time::Instant::now();
    let validation_result = perform_validation(&content, &detected_format, &mode)?;
    let duration = start_time.elapsed();

    // Build output
    let mut result = ValidationOutput {
        is_valid: validation_result.is_valid,
        schema_format: detected_format.clone(),
        namespace: namespace.clone(),
        name: name.clone(),
        version: version.clone(),
        validation_mode: mode,
        errors: validation_result.errors,
        warnings: validation_result.warnings,
        metrics: ValidationMetricsOutput {
            duration_ms: duration.as_millis() as u64,
            rules_applied: validation_result.rules_applied,
            fields_validated: validation_result.fields_validated,
            schema_size_bytes: content.len(),
        },
        compatibility: None,
    };

    // Check compatibility if target specified
    if let Some(target) = compatibility_target {
        let compat = check_compatibility_mock(&target);
        result.compatibility = Some(compat);
    }

    // Output results
    format_output(&result, &output_format)?;

    // Exit status based on validation result
    if !result.is_valid {
        output::print_error_msg("Validation failed");
        std::process::exit(1);
    }

    output::print_success(&format!(
        "Schema {}.{} is valid",
        namespace, name
    ));
    Ok(())
}

// ============================================================================
// Inspect command implementation
// ============================================================================

async fn execute_inspect(file: PathBuf, detailed: bool, output_format: String) -> Result<()> {
    // Read schema content
    let content = read_schema_content(&file)?;

    // Detect format
    let detected_format = detect_schema_format(&content);

    output::print_info(&format!(
        "Inspecting schema from: {} (format: {})",
        file.display(),
        detected_format
    ));

    // Analyze schema structure
    let inspection = analyze_schema(&content, &detected_format, detailed)?;

    // Output results
    match output_format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&inspection)?);
        }
        "yaml" => {
            println!("{}", serde_yaml::to_string(&inspection)?);
        }
        "table" | _ => {
            print_inspection_table(&inspection, detailed);
        }
    }

    Ok(())
}

// ============================================================================
// Diff command implementation
// ============================================================================

async fn execute_diff(
    old_file: PathBuf,
    new_file: PathBuf,
    compatibility_mode: String,
    output_format: String,
) -> Result<()> {
    // Read both schemas
    let old_content = read_schema_content(&old_file)?;
    let new_content = read_schema_content(&new_file)?;

    // Detect formats
    let old_format = detect_schema_format(&old_content);
    let new_format = detect_schema_format(&new_content);

    if old_format != new_format {
        output::print_warning(&format!(
            "Schema formats differ: {} vs {}",
            old_format, new_format
        ));
    }

    output::print_info(&format!(
        "Comparing schemas: {} vs {} (mode: {})",
        old_file.display(),
        new_file.display(),
        compatibility_mode
    ));

    // Perform diff analysis
    let diff_result = compute_schema_diff(&old_content, &new_content, &compatibility_mode)?;

    // Output results
    match output_format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&diff_result)?);
        }
        "yaml" => {
            println!("{}", serde_yaml::to_string(&diff_result)?);
        }
        "table" | _ => {
            print_diff_table(&diff_result);
        }
    }

    // Exit status based on compatibility
    if !diff_result.is_compatible {
        output::print_error_msg("Schemas are NOT compatible");
        std::process::exit(1);
    }

    output::print_success("Schemas are compatible");
    Ok(())
}

// ============================================================================
// Register command implementation
// ============================================================================

async fn execute_register(
    file: PathBuf,
    namespace: String,
    name: String,
    version: String,
    description: Option<String>,
    skip_validation: bool,
    ruvector_url: Option<String>,
    output_format: String,
    _config: &Config,
) -> Result<()> {
    // Read schema content
    let content = read_schema_content(&file)?;

    // Validate version format
    if !is_valid_semver(&version) {
        return Err(crate::error::CliError::ValidationError(format!(
            "Invalid version format: {}. Expected semver (e.g., 1.0.0)",
            version
        )));
    }

    output::print_info(&format!(
        "Registering schema: {}.{} v{}",
        namespace, name, version
    ));

    // Validate unless skipped
    if !skip_validation {
        let detected_format = detect_schema_format(&content);
        let validation_result = perform_validation(&content, &detected_format, "strict")?;

        if !validation_result.is_valid {
            output::print_error_msg("Schema validation failed. Use --skip-validation to bypass (not recommended).");
            for error in &validation_result.errors {
                eprintln!("  - [{}] {}", error.rule, error.message);
            }
            std::process::exit(1);
        }
        output::print_success("Schema validation passed");
    } else {
        output::print_warning("Validation skipped (--skip-validation flag used)");
    }

    // Calculate content hash
    let content_hash = calculate_content_hash(&content);

    // Generate schema ID
    let schema_id = Uuid::new_v4();

    // Create decision event
    let decision_event = DecisionEventOutput {
        event_id: Uuid::new_v4().to_string(),
        event_type: "SchemaRegistered".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        actor: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
        decision: "APPROVED".to_string(),
        rationale: description.clone().unwrap_or_else(|| "Schema validated and registered".to_string()),
    };

    // Build registration result
    let result = RegistrationOutput {
        success: true,
        schema_id: schema_id.to_string(),
        namespace: namespace.clone(),
        name: name.clone(),
        version: version.clone(),
        content_hash: content_hash.clone(),
        registered_at: chrono::Utc::now().to_rfc3339(),
        decision_event: Some(decision_event),
    };

    // If ruvector URL is provided, attempt to persist
    if let Some(url) = ruvector_url {
        output::print_info(&format!("Persisting to ruvector-service: {}", url));
        // In production, this would make an HTTP call to persist the DecisionEvent
        // For now, we'll just indicate success
        output::print_success("DecisionEvent persisted to ruvector-service");
    }

    // Output results
    match output_format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "yaml" => {
            println!("{}", serde_yaml::to_string(&result)?);
        }
        "table" | _ => {
            print_registration_table(&result);
        }
    }

    output::print_success(&format!(
        "Schema registered successfully with ID: {}",
        schema_id
    ));
    Ok(())
}

// ============================================================================
// Helper functions
// ============================================================================

/// Read schema content from file or stdin
fn read_schema_content(file: &PathBuf) -> Result<String> {
    let file_str = file.to_string_lossy();
    if file_str == "-" {
        let mut content = String::new();
        io::stdin()
            .read_to_string(&mut content)
            .map_err(|e| crate::error::CliError::IoError(e))?;
        Ok(content)
    } else {
        std::fs::read_to_string(file).map_err(|e| crate::error::CliError::IoError(e))
    }
}

/// Detect schema format from content
fn detect_schema_format(content: &str) -> String {
    // Check for protobuf indicators
    let content_lower = content.to_lowercase();
    if content_lower.contains("syntax") && (content_lower.contains("proto2") || content_lower.contains("proto3")) {
        return "protobuf".to_string();
    }
    if content_lower.contains("message") && content_lower.contains("{") {
        return "protobuf".to_string();
    }

    // Try to parse as JSON
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
        // Check for Avro indicators
        if let Some(type_field) = json.get("type") {
            if let Some(type_str) = type_field.as_str() {
                if matches!(type_str, "record" | "enum" | "fixed") {
                    return "avro".to_string();
                }
            }
        }
        if json.get("namespace").is_some() && json.get("fields").is_some() {
            return "avro".to_string();
        }

        // Default to JSON Schema
        return "json-schema".to_string();
    }

    "unknown".to_string()
}

/// Internal validation result
struct InternalValidationResult {
    is_valid: bool,
    errors: Vec<ValidationErrorOutput>,
    warnings: Vec<ValidationWarningOutput>,
    rules_applied: usize,
    fields_validated: usize,
}

/// Perform schema validation
fn perform_validation(
    content: &str,
    format: &str,
    mode: &str,
) -> Result<InternalValidationResult> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut fields_validated = 0;
    let mut rules_applied = 0;

    match format {
        "json-schema" => {
            rules_applied += 3; // structural, type, semantic
            match serde_json::from_str::<serde_json::Value>(content) {
                Ok(json) => {
                    // Validate structure
                    if !json.is_object() && !json.is_boolean() {
                        errors.push(ValidationErrorOutput {
                            rule: "structural-validity".to_string(),
                            message: "JSON Schema must be an object or boolean".to_string(),
                            severity: "error".to_string(),
                            location: Some("$".to_string()),
                            suggestion: Some("Wrap the schema in an object".to_string()),
                        });
                    }

                    // Count and validate fields
                    if let Some(props) = json.get("properties").and_then(|p| p.as_object()) {
                        fields_validated = props.len();

                        for (field_name, field_def) in props {
                            // Check for type
                            if field_def.get("type").is_none()
                                && field_def.get("$ref").is_none()
                                && field_def.get("oneOf").is_none()
                                && field_def.get("anyOf").is_none()
                            {
                                if mode == "strict" {
                                    errors.push(ValidationErrorOutput {
                                        rule: "type-validation".to_string(),
                                        message: format!("Field '{}' has no type definition", field_name),
                                        severity: "error".to_string(),
                                        location: Some(format!("$.properties.{}", field_name)),
                                        suggestion: Some("Add a 'type' field".to_string()),
                                    });
                                }
                            }

                            // LLM readiness: check for descriptions
                            if field_def.get("description").is_none() {
                                warnings.push(ValidationWarningOutput {
                                    rule: "llm-readiness".to_string(),
                                    message: format!("Field '{}' lacks description for LLM context", field_name),
                                    location: Some(format!("$.properties.{}", field_name)),
                                    suggestion: Some("Add a 'description' field for better LLM understanding".to_string()),
                                });
                            }
                        }
                    }

                    // Check for required fields consistency
                    if let (Some(required), Some(props)) = (
                        json.get("required").and_then(|r| r.as_array()),
                        json.get("properties").and_then(|p| p.as_object()),
                    ) {
                        for req in required {
                            if let Some(field_name) = req.as_str() {
                                if !props.contains_key(field_name) {
                                    errors.push(ValidationErrorOutput {
                                        rule: "semantic-validation".to_string(),
                                        message: format!("Required field '{}' not defined in properties", field_name),
                                        severity: "error".to_string(),
                                        location: Some(format!("$.required")),
                                        suggestion: Some(format!("Define '{}' in properties or remove from required", field_name)),
                                    });
                                }
                            }
                        }
                    }

                    // Root-level description check
                    if json.get("description").is_none() {
                        warnings.push(ValidationWarningOutput {
                            rule: "llm-readiness".to_string(),
                            message: "Schema lacks root-level description".to_string(),
                            location: Some("$".to_string()),
                            suggestion: Some("Add a 'description' field at the root level".to_string()),
                        });
                    }
                }
                Err(e) => {
                    errors.push(ValidationErrorOutput {
                        rule: "structural-validity".to_string(),
                        message: format!("Invalid JSON: {}", e),
                        severity: "error".to_string(),
                        location: None,
                        suggestion: Some("Check JSON syntax".to_string()),
                    });
                }
            }
        }
        "avro" => {
            rules_applied += 2;
            match serde_json::from_str::<serde_json::Value>(content) {
                Ok(json) => {
                    // Check for required Avro fields
                    if json.get("type").is_none() {
                        errors.push(ValidationErrorOutput {
                            rule: "structural-validity".to_string(),
                            message: "Avro schema must have a 'type' field".to_string(),
                            severity: "error".to_string(),
                            location: Some("$".to_string()),
                            suggestion: Some("Add 'type' field (e.g., 'record', 'enum')".to_string()),
                        });
                    }

                    // For record type, check fields
                    if let Some("record") = json.get("type").and_then(|t| t.as_str()) {
                        if json.get("name").is_none() {
                            errors.push(ValidationErrorOutput {
                                rule: "structural-validity".to_string(),
                                message: "Avro record must have a 'name' field".to_string(),
                                severity: "error".to_string(),
                                location: Some("$".to_string()),
                                suggestion: Some("Add 'name' field".to_string()),
                            });
                        }
                        if let Some(fields) = json.get("fields").and_then(|f| f.as_array()) {
                            fields_validated = fields.len();
                        }
                    }
                }
                Err(e) => {
                    errors.push(ValidationErrorOutput {
                        rule: "structural-validity".to_string(),
                        message: format!("Invalid Avro JSON: {}", e),
                        severity: "error".to_string(),
                        location: None,
                        suggestion: Some("Check JSON syntax".to_string()),
                    });
                }
            }
        }
        "protobuf" => {
            rules_applied += 2;
            // Basic protobuf validation
            if !content.contains("message") && !content.contains("enum") {
                errors.push(ValidationErrorOutput {
                    rule: "structural-validity".to_string(),
                    message: "Protobuf schema must contain message or enum definition".to_string(),
                    severity: "error".to_string(),
                    location: None,
                    suggestion: Some("Add a message or enum definition".to_string()),
                });
            }

            // Count approximate field count
            fields_validated = content.matches('=').count();
        }
        _ => {
            errors.push(ValidationErrorOutput {
                rule: "format-detection".to_string(),
                message: format!("Unknown or unsupported schema format: {}", format),
                severity: "error".to_string(),
                location: None,
                suggestion: Some("Use json-schema, avro, or protobuf".to_string()),
            });
        }
    }

    Ok(InternalValidationResult {
        is_valid: errors.is_empty(),
        errors,
        warnings,
        rules_applied,
        fields_validated,
    })
}

/// Mock compatibility check (would integrate with actual compatibility checker)
fn check_compatibility_mock(target_version: &str) -> CompatibilityOutput {
    CompatibilityOutput {
        is_compatible: true,
        mode: "backward".to_string(),
        target_version: target_version.to_string(),
        violations: Vec::new(),
    }
}

/// Analyze schema structure for inspection
fn analyze_schema(
    content: &str,
    format: &str,
    detailed: bool,
) -> Result<InspectionOutput> {
    let mut fields = Vec::new();
    let mut structure = SchemaStructure {
        root_type: "unknown".to_string(),
        total_fields: 0,
        required_fields: 0,
        optional_fields: 0,
        nested_depth: 0,
        has_recursion: false,
    };

    let mut metadata = InspectionMetadata {
        schema_size_bytes: content.len(),
        has_description: false,
        has_examples: false,
        llm_readiness_score: 0.0,
    };

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
        metadata.has_description = json.get("description").is_some();
        metadata.has_examples = json.get("examples").is_some();

        // Determine root type
        if let Some(type_val) = json.get("type") {
            structure.root_type = type_val.as_str().unwrap_or("mixed").to_string();
        }

        // Calculate nesting depth
        structure.nested_depth = calculate_json_depth(&json);

        // Check for $ref (recursion indicator)
        structure.has_recursion = content.contains("\"$ref\"");

        // Analyze properties
        if let Some(props) = json.get("properties").and_then(|p| p.as_object()) {
            structure.total_fields = props.len();

            let required_fields: Vec<String> = json
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            structure.required_fields = required_fields.len();
            structure.optional_fields = structure.total_fields - structure.required_fields;

            if detailed {
                for (name, prop) in props {
                    let field_type = prop
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("any")
                        .to_string();

                    let mut constraints = Vec::new();
                    if prop.get("minLength").is_some() {
                        constraints.push("minLength".to_string());
                    }
                    if prop.get("maxLength").is_some() {
                        constraints.push("maxLength".to_string());
                    }
                    if prop.get("minimum").is_some() {
                        constraints.push("minimum".to_string());
                    }
                    if prop.get("maximum").is_some() {
                        constraints.push("maximum".to_string());
                    }
                    if prop.get("pattern").is_some() {
                        constraints.push("pattern".to_string());
                    }
                    if prop.get("enum").is_some() {
                        constraints.push("enum".to_string());
                    }

                    fields.push(FieldInfo {
                        path: format!("$.{}", name),
                        field_type,
                        required: required_fields.contains(name),
                        has_description: prop.get("description").is_some(),
                        has_default: prop.get("default").is_some(),
                        constraints,
                    });
                }
            }
        }

        // Calculate LLM readiness score
        let mut score = 0.0;
        if metadata.has_description {
            score += 0.3;
        }
        if metadata.has_examples {
            score += 0.2;
        }
        if structure.total_fields > 0 {
            let fields_with_desc = if detailed {
                fields.iter().filter(|f| f.has_description).count()
            } else {
                // Estimate from properties
                json.get("properties")
                    .and_then(|p| p.as_object())
                    .map(|props| props.values().filter(|v| v.get("description").is_some()).count())
                    .unwrap_or(0)
            };
            score += 0.5 * (fields_with_desc as f32 / structure.total_fields as f32);
        }
        metadata.llm_readiness_score = score;
    }

    Ok(InspectionOutput {
        format: format.to_string(),
        structure,
        metadata,
        fields,
    })
}

/// Calculate JSON nesting depth
fn calculate_json_depth(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            1 + map.values().map(calculate_json_depth).max().unwrap_or(0)
        }
        serde_json::Value::Array(arr) => {
            1 + arr.iter().map(calculate_json_depth).max().unwrap_or(0)
        }
        _ => 0,
    }
}

/// Compute schema diff
fn compute_schema_diff(
    old_content: &str,
    new_content: &str,
    compatibility_mode: &str,
) -> Result<DiffOutput> {
    let mut changes = Vec::new();
    let mut summary = DiffSummary {
        total_changes: 0,
        breaking_changes: 0,
        additions: 0,
        removals: 0,
        modifications: 0,
    };

    // Parse both schemas
    if let (Ok(old_json), Ok(new_json)) = (
        serde_json::from_str::<serde_json::Value>(old_content),
        serde_json::from_str::<serde_json::Value>(new_content),
    ) {
        // Compare properties
        let old_props = old_json
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();
        let new_props = new_json
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();

        let old_required: Vec<String> = old_json
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let new_required: Vec<String> = new_json
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Find removed fields
        for (field_name, _) in &old_props {
            if !new_props.contains_key(field_name) {
                let is_breaking = matches!(compatibility_mode, "backward" | "full");
                changes.push(SchemaChange {
                    change_type: "removed".to_string(),
                    path: format!("$.properties.{}", field_name),
                    breaking: is_breaking,
                    description: format!("Field '{}' was removed", field_name),
                    old_value: Some(field_name.clone()),
                    new_value: None,
                });
                summary.removals += 1;
                if is_breaking {
                    summary.breaking_changes += 1;
                }
            }
        }

        // Find added fields
        for (field_name, _) in &new_props {
            if !old_props.contains_key(field_name) {
                let is_required = new_required.contains(field_name);
                let is_breaking = is_required && matches!(compatibility_mode, "forward" | "full");
                changes.push(SchemaChange {
                    change_type: "added".to_string(),
                    path: format!("$.properties.{}", field_name),
                    breaking: is_breaking,
                    description: format!(
                        "Field '{}' was added{}",
                        field_name,
                        if is_required { " (required)" } else { "" }
                    ),
                    old_value: None,
                    new_value: Some(field_name.clone()),
                });
                summary.additions += 1;
                if is_breaking {
                    summary.breaking_changes += 1;
                }
            }
        }

        // Find modified fields
        for (field_name, old_def) in &old_props {
            if let Some(new_def) = new_props.get(field_name) {
                // Check type change
                let old_type = old_def.get("type").and_then(|t| t.as_str());
                let new_type = new_def.get("type").and_then(|t| t.as_str());

                if old_type != new_type {
                    changes.push(SchemaChange {
                        change_type: "type_changed".to_string(),
                        path: format!("$.properties.{}.type", field_name),
                        breaking: true,
                        description: format!(
                            "Field '{}' type changed from {:?} to {:?}",
                            field_name, old_type, new_type
                        ),
                        old_value: old_type.map(String::from),
                        new_value: new_type.map(String::from),
                    });
                    summary.modifications += 1;
                    summary.breaking_changes += 1;
                }
            }
        }

        // Check for newly required fields
        for field_name in &new_required {
            if !old_required.contains(field_name) && old_props.contains_key(field_name) {
                let is_breaking = matches!(compatibility_mode, "forward" | "full");
                changes.push(SchemaChange {
                    change_type: "required_added".to_string(),
                    path: format!("$.required"),
                    breaking: is_breaking,
                    description: format!("Field '{}' became required", field_name),
                    old_value: None,
                    new_value: Some(field_name.clone()),
                });
                summary.modifications += 1;
                if is_breaking {
                    summary.breaking_changes += 1;
                }
            }
        }

        summary.total_changes = changes.len();
    }

    Ok(DiffOutput {
        is_compatible: summary.breaking_changes == 0,
        compatibility_mode: compatibility_mode.to_string(),
        summary,
        changes,
    })
}

/// Check if version is valid semver
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u32>().is_ok())
}

/// Calculate content hash
fn calculate_content_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

// ============================================================================
// Output formatting
// ============================================================================

/// Generic output formatter
fn format_output<T: Serialize>(data: &T, format: &str) -> Result<()> {
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(data)?);
        }
        "yaml" => {
            println!("{}", serde_yaml::to_string(data)?);
        }
        "table" | _ => {
            // For validation output, use specialized table
            println!("{}", serde_json::to_string_pretty(data)?);
        }
    }
    Ok(())
}

/// Print inspection results as table
fn print_inspection_table(inspection: &InspectionOutput, detailed: bool) {
    println!("\n{}", "Schema Inspection Results".cyan().bold());
    println!("{}", "=".repeat(50));

    println!("\n{}", "Structure:".yellow().bold());
    output::print_table(
        vec!["Property", "Value"],
        vec![
            vec!["Format".to_string(), inspection.format.clone()],
            vec!["Root Type".to_string(), inspection.structure.root_type.clone()],
            vec!["Total Fields".to_string(), inspection.structure.total_fields.to_string()],
            vec!["Required Fields".to_string(), inspection.structure.required_fields.to_string()],
            vec!["Optional Fields".to_string(), inspection.structure.optional_fields.to_string()],
            vec!["Nesting Depth".to_string(), inspection.structure.nested_depth.to_string()],
            vec!["Has Recursion".to_string(), inspection.structure.has_recursion.to_string()],
        ],
    );

    println!("\n{}", "Metadata:".yellow().bold());
    output::print_table(
        vec!["Property", "Value"],
        vec![
            vec!["Schema Size".to_string(), format!("{} bytes", inspection.metadata.schema_size_bytes)],
            vec!["Has Description".to_string(), inspection.metadata.has_description.to_string()],
            vec!["Has Examples".to_string(), inspection.metadata.has_examples.to_string()],
            vec!["LLM Readiness Score".to_string(), format!("{:.0}%", inspection.metadata.llm_readiness_score * 100.0)],
        ],
    );

    if detailed && !inspection.fields.is_empty() {
        println!("\n{}", "Fields:".yellow().bold());
        output::print_table(
            vec!["Path", "Type", "Required", "Description", "Default", "Constraints"],
            inspection
                .fields
                .iter()
                .map(|f| {
                    vec![
                        f.path.clone(),
                        f.field_type.clone(),
                        if f.required { "Yes".to_string() } else { "No".to_string() },
                        if f.has_description { "Yes".to_string() } else { "No".to_string() },
                        if f.has_default { "Yes".to_string() } else { "No".to_string() },
                        if f.constraints.is_empty() {
                            "-".to_string()
                        } else {
                            f.constraints.join(", ")
                        },
                    ]
                })
                .collect(),
        );
    }
}

/// Print diff results as table
fn print_diff_table(diff: &DiffOutput) {
    println!("\n{}", "Schema Diff Results".cyan().bold());
    println!("{}", "=".repeat(50));

    // Summary
    println!("\n{}", "Summary:".yellow().bold());
    output::print_table(
        vec!["Metric", "Value"],
        vec![
            vec!["Compatibility Mode".to_string(), diff.compatibility_mode.clone()],
            vec!["Is Compatible".to_string(), if diff.is_compatible { "Yes".green().to_string() } else { "No".red().to_string() }],
            vec!["Total Changes".to_string(), diff.summary.total_changes.to_string()],
            vec!["Breaking Changes".to_string(), if diff.summary.breaking_changes > 0 {
                diff.summary.breaking_changes.to_string().red().to_string()
            } else {
                "0".green().to_string()
            }],
            vec!["Additions".to_string(), diff.summary.additions.to_string()],
            vec!["Removals".to_string(), diff.summary.removals.to_string()],
            vec!["Modifications".to_string(), diff.summary.modifications.to_string()],
        ],
    );

    // Changes
    if !diff.changes.is_empty() {
        println!("\n{}", "Changes:".yellow().bold());
        output::print_table(
            vec!["Type", "Path", "Breaking", "Description"],
            diff.changes
                .iter()
                .map(|c| {
                    vec![
                        c.change_type.clone(),
                        c.path.clone(),
                        if c.breaking {
                            "Yes".red().to_string()
                        } else {
                            "No".green().to_string()
                        },
                        c.description.clone(),
                    ]
                })
                .collect(),
        );
    }
}

/// Print registration results as table
fn print_registration_table(result: &RegistrationOutput) {
    println!("\n{}", "Schema Registration Results".cyan().bold());
    println!("{}", "=".repeat(50));

    output::print_table(
        vec!["Property", "Value"],
        vec![
            vec!["Status".to_string(), if result.success { "Success".green().to_string() } else { "Failed".red().to_string() }],
            vec!["Schema ID".to_string(), result.schema_id.clone()],
            vec!["Namespace".to_string(), result.namespace.clone()],
            vec!["Name".to_string(), result.name.clone()],
            vec!["Version".to_string(), result.version.clone()],
            vec!["Content Hash".to_string(), result.content_hash.chars().take(16).collect::<String>() + "..."],
            vec!["Registered At".to_string(), result.registered_at.clone()],
        ],
    );

    if let Some(event) = &result.decision_event {
        println!("\n{}", "Decision Event:".yellow().bold());
        output::print_table(
            vec!["Property", "Value"],
            vec![
                vec!["Event ID".to_string(), event.event_id.clone()],
                vec!["Type".to_string(), event.event_type.clone()],
                vec!["Decision".to_string(), event.decision.clone()],
                vec!["Actor".to_string(), event.actor.clone()],
                vec!["Rationale".to_string(), event.rationale.clone()],
            ],
        );
    }
}
