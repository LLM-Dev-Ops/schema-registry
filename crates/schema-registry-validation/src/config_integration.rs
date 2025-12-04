//! Config Manager Integration for Validation Engine
//!
//! This module provides policy-driven validation by consuming schema policies
//! from the Config Manager adapter. It extends the validation engine with
//! custom rules based on organizational policies.

use crate::types::{ValidationError, Severity, SchemaFormat};
use crate::engine::ValidationRule;
use schema_registry_core::config_manager_adapter::{
    SchemaPolicies, FieldNamingPolicy,
};
use anyhow::Result;
use regex::Regex;
use tracing::{debug, info};

/// Policy-based validation rule that consumes policies from Config Manager
pub struct PolicyBasedValidationRule {
    policies: SchemaPolicies,
}

impl PolicyBasedValidationRule {
    /// Create a new policy-based validation rule
    pub fn new(policies: SchemaPolicies) -> Self {
        info!("Initializing policy-based validation with {} custom rules", policies.custom_rules.len());
        Self { policies }
    }

    /// Update policies (for runtime refresh)
    pub fn update_policies(&mut self, policies: SchemaPolicies) {
        info!("Updating validation policies with {} custom rules", policies.custom_rules.len());
        self.policies = policies;
    }

    /// Validate field naming conventions
    fn validate_field_naming(&self, schema: &str, format: SchemaFormat) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if !self.policies.field_naming.enforce {
            return errors;
        }

        debug!("Validating field naming convention: {}", self.policies.field_naming.convention);

        // For JSON schemas, check field names
        if format == SchemaFormat::JsonSchema {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(schema) {
                self.check_json_field_names(&json, &self.policies.field_naming, &mut errors, "$");
            }
        }

        errors
    }

    /// Recursively check JSON field names
    fn check_json_field_names(
        &self,
        value: &serde_json::Value,
        policy: &FieldNamingPolicy,
        errors: &mut Vec<ValidationError>,
        path: &str,
    ) {
        if let Some(obj) = value.as_object() {
            for (key, val) in obj {
                let field_path = format!("{}.{}", path, key);

                // Check if field name follows the convention
                if !self.matches_naming_convention(key, &policy.convention) {
                    errors.push(
                        ValidationError::new(
                            "field-naming-policy",
                            format!(
                                "Field '{}' does not follow {} naming convention",
                                key, policy.convention
                            ),
                        )
                        .with_location(field_path.clone())
                        .with_suggestion(format!(
                            "Rename field to follow {} convention",
                            policy.convention
                        )),
                    );
                }

                // Recurse into nested objects
                self.check_json_field_names(val, policy, errors, &field_path);
            }
        } else if let Some(arr) = value.as_array() {
            for (idx, item) in arr.iter().enumerate() {
                let array_path = format!("{}[{}]", path, idx);
                self.check_json_field_names(item, policy, errors, &array_path);
            }
        }
    }

    /// Check if a field name matches the naming convention
    fn matches_naming_convention(&self, field_name: &str, convention: &str) -> bool {
        match convention {
            "snake_case" => {
                // snake_case: lowercase with underscores
                field_name.chars().all(|c| c.is_lowercase() || c.is_numeric() || c == '_')
            }
            "camelCase" => {
                // camelCase: starts lowercase, no underscores or hyphens
                !field_name.is_empty()
                    && field_name.chars().next().unwrap().is_lowercase()
                    && !field_name.contains('_')
                    && !field_name.contains('-')
            }
            "PascalCase" => {
                // PascalCase: starts uppercase, no underscores or hyphens
                !field_name.is_empty()
                    && field_name.chars().next().unwrap().is_uppercase()
                    && !field_name.contains('_')
                    && !field_name.contains('-')
            }
            _ => true, // Unknown convention, allow all
        }
    }

    /// Apply custom policy rules
    fn apply_custom_rules(&self, schema: &str) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for rule in &self.policies.custom_rules {
            debug!("Applying custom policy rule: {}", rule.name);

            if let Some(pattern_str) = &rule.pattern {
                if let Ok(regex) = Regex::new(pattern_str) {
                    if !regex.is_match(schema) && rule.mandatory {
                        errors.push(
                            ValidationError::new(
                                format!("custom-policy-{}", rule.name),
                                format!("Schema violates policy: {}", rule.description),
                            )
                            .with_suggestion("Review schema against policy requirements"),
                        );
                    }
                }
            }
        }

        errors
    }
}

impl ValidationRule for PolicyBasedValidationRule {
    fn name(&self) -> &str {
        "config-manager-policy"
    }

    fn severity(&self) -> Severity {
        Severity::Warning
    }

    fn validate(&self, schema: &str, format: SchemaFormat) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Validate field naming
        errors.extend(self.validate_field_naming(schema, format));

        // Apply custom rules
        errors.extend(self.apply_custom_rules(schema));

        Ok(errors)
    }
}

/// Extension trait for ValidationEngine to support Config Manager policies
pub trait ValidationEngineExt {
    /// Configure validation engine with policies from Config Manager
    fn with_config_manager_policies(&mut self, policies: SchemaPolicies) -> &mut Self;
}

// Note: The actual implementation would extend the ValidationEngine in the engine module
// This is a demonstration of how policies would be integrated

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_validation() {
        let rule = PolicyBasedValidationRule::new(SchemaPolicies::default());
        assert!(rule.matches_naming_convention("user_name", "snake_case"));
        assert!(rule.matches_naming_convention("user_id_123", "snake_case"));
        assert!(!rule.matches_naming_convention("userName", "snake_case"));
        assert!(!rule.matches_naming_convention("UserName", "snake_case"));
    }

    #[test]
    fn test_camel_case_validation() {
        let rule = PolicyBasedValidationRule::new(SchemaPolicies::default());
        assert!(rule.matches_naming_convention("userName", "camelCase"));
        assert!(rule.matches_naming_convention("userId123", "camelCase"));
        assert!(!rule.matches_naming_convention("UserName", "camelCase"));
        assert!(!rule.matches_naming_convention("user_name", "camelCase"));
    }

    #[test]
    fn test_pascal_case_validation() {
        let rule = PolicyBasedValidationRule::new(SchemaPolicies::default());
        assert!(rule.matches_naming_convention("UserName", "PascalCase"));
        assert!(rule.matches_naming_convention("UserId123", "PascalCase"));
        assert!(!rule.matches_naming_convention("userName", "PascalCase"));
        assert!(!rule.matches_naming_convention("user_name", "PascalCase"));
    }

    #[test]
    fn test_policy_rule_creation() {
        let policies = SchemaPolicies::default();
        let rule = PolicyBasedValidationRule::new(policies);
        assert_eq!(rule.name(), "config-manager-policy");
        assert_eq!(rule.severity(), Severity::Warning);
    }
}
