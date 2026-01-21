//! Apache Avro compatibility checker
//!
//! Implements compatibility checking for Apache Avro schemas
//! Leverages Avro's built-in schema resolution for compatibility checks

use crate::checker::CompatibilityError;
use crate::formats::FormatCompatibilityChecker;
use crate::violation::{CompatibilityViolation, ViolationType};
use apache_avro::schema::RecordField;
use apache_avro::Schema as AvroSchema;
use std::collections::HashSet;

pub struct AvroCompatibilityChecker;

impl AvroCompatibilityChecker {
    pub fn new() -> Self {
        Self
    }

    /// Parse Avro schema
    fn parse_schema(&self, schema_str: &str) -> Result<AvroSchema, CompatibilityError> {
        AvroSchema::parse_str(schema_str)
            .map_err(|e| CompatibilityError::ParseError(format!("Avro parse error: {}", e)))
    }

    /// Extract fields from Avro record schema
    fn extract_fields<'a>(&self, schema: &'a AvroSchema) -> Option<&'a Vec<RecordField>> {
        match schema {
            AvroSchema::Record(record_schema) => Some(&record_schema.fields),
            _ => None,
        }
    }

    /// Check if a field has a default value
    fn has_default(&self, field: &RecordField) -> bool {
        field.default.is_some()
    }

    /// Check if Avro types are compatible for schema resolution
    fn are_types_compatible(&self, writer_schema: &AvroSchema, reader_schema: &AvroSchema) -> bool {
        // Use Avro's built-in schema resolution rules
        // This is a simplified version - full Avro resolution is complex
        match (writer_schema, reader_schema) {
            // Same types are compatible
            (AvroSchema::String, AvroSchema::String) => true,
            (AvroSchema::Int, AvroSchema::Int) => true,
            (AvroSchema::Long, AvroSchema::Long) => true,
            (AvroSchema::Float, AvroSchema::Float) => true,
            (AvroSchema::Double, AvroSchema::Double) => true,
            (AvroSchema::Boolean, AvroSchema::Boolean) => true,
            (AvroSchema::Bytes, AvroSchema::Bytes) => true,
            (AvroSchema::Null, AvroSchema::Null) => true,

            // Type promotions allowed by Avro spec
            (AvroSchema::Int, AvroSchema::Long) => true,
            (AvroSchema::Int, AvroSchema::Float) => true,
            (AvroSchema::Int, AvroSchema::Double) => true,
            (AvroSchema::Long, AvroSchema::Float) => true,
            (AvroSchema::Long, AvroSchema::Double) => true,
            (AvroSchema::Float, AvroSchema::Double) => true,
            (AvroSchema::String, AvroSchema::Bytes) => true,
            (AvroSchema::Bytes, AvroSchema::String) => true,

            // Arrays are compatible if items are compatible
            (AvroSchema::Array(w), AvroSchema::Array(r)) => {
                self.are_types_compatible(&w, &r)
            }

            // Maps are compatible if values are compatible
            (AvroSchema::Map(w), AvroSchema::Map(r)) => {
                self.are_types_compatible(&w, &r)
            }

            // Unions require special handling
            (AvroSchema::Union(w), AvroSchema::Union(r)) => {
                // Reader union must be a superset of writer union
                w.variants().iter().all(|w_variant| {
                    r.variants()
                        .iter()
                        .any(|r_variant| self.are_types_compatible(w_variant, r_variant))
                })
            }

            // Records require field-by-field comparison
            (AvroSchema::Record(w_record), AvroSchema::Record(r_record)) => {
                // Check if all reader fields can be satisfied by writer fields
                r_record.fields.iter().all(|r_field| {
                    w_record.fields.iter().any(|w_field| {
                        w_field.name == r_field.name
                            && self.are_types_compatible(&w_field.schema, &r_field.schema)
                    }) || r_field.default.is_some()
                })
            }

            _ => false,
        }
    }
}

impl FormatCompatibilityChecker for AvroCompatibilityChecker {
    /// Check backward compatibility for Avro schemas
    ///
    /// Backward compatibility means: data written with old schema can be read with new schema
    ///
    /// Rules:
    /// 1. Fields in old schema must exist in new schema or have defaults
    /// 2. Field types must be compatible or promotable
    /// 3. New required fields (no default) cannot be added
    fn check_backward(
        &self,
        new_schema: &str,
        old_schema: &str,
    ) -> Result<Vec<CompatibilityViolation>, CompatibilityError> {
        let new = self.parse_schema(new_schema)?;
        let old = self.parse_schema(old_schema)?;

        let mut violations = Vec::new();

        // Check if schemas are compatible using Avro's resolution rules
        // New schema is the reader, old schema is the writer
        if !self.are_types_compatible(&old, &new) {
            violations.push(CompatibilityViolation::breaking(
                ViolationType::TypeChanged,
                "schema",
                "Schemas are not backward compatible according to Avro resolution rules",
            ));
        }

        // Additional field-level checks for records
        if let (Some(new_fields), Some(old_fields)) =
            (self.extract_fields(&new), self.extract_fields(&old))
        {
            let new_field_names: HashSet<_> = new_fields.iter().map(|f| &f.name).collect();
            let old_field_names: HashSet<_> = old_fields.iter().map(|f| &f.name).collect();

            // Check for removed fields
            for old_field in old_fields {
                if !new_field_names.contains(&old_field.name) {
                    if !self.has_default(old_field) {
                        violations.push(CompatibilityViolation::breaking(
                            ViolationType::FieldRemoved,
                            format!("fields.{}", old_field.name),
                            format!("Field '{}' removed without default value", old_field.name),
                        ));
                    }
                }
            }

            // Check for new required fields
            for new_field in new_fields {
                if !old_field_names.contains(&new_field.name) {
                    if !self.has_default(new_field) {
                        violations.push(CompatibilityViolation::breaking(
                            ViolationType::RequiredAdded,
                            format!("fields.{}", new_field.name),
                            format!(
                                "New required field '{}' added without default",
                                new_field.name
                            ),
                        ));
                    }
                }
            }

            // Check for type changes
            for new_field in new_fields {
                if let Some(old_field) = old_fields.iter().find(|f| f.name == new_field.name) {
                    if !self.are_types_compatible(&old_field.schema, &new_field.schema) {
                        violations.push(CompatibilityViolation::breaking(
                            ViolationType::TypeChanged,
                            format!("fields.{}.type", new_field.name),
                            format!(
                                "Type changed incompatibly for field '{}'",
                                new_field.name
                            ),
                        ));
                    }
                }
            }
        }

        Ok(violations)
    }

    /// Check forward compatibility for Avro schemas
    ///
    /// Forward compatibility means: data written with new schema can be read with old schema
    fn check_forward(
        &self,
        new_schema: &str,
        old_schema: &str,
    ) -> Result<Vec<CompatibilityViolation>, CompatibilityError> {
        // Forward: old schema can read new data
        // Check if old schema (reader) can read data written with new schema (writer)
        self.check_backward(old_schema, new_schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_avro_schemas_are_compatible() {
        let checker = AvroCompatibilityChecker::new();
        let schema = r#"{
            "type": "record",
            "name": "Test",
            "fields": [
                {"name": "field1", "type": "string"}
            ]
        }"#;

        let violations = checker.check_backward(schema, schema).unwrap();
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_adding_optional_field_is_compatible() {
        let checker = AvroCompatibilityChecker::new();

        let old_schema = r#"{
            "type": "record",
            "name": "Test",
            "fields": [
                {"name": "field1", "type": "string"}
            ]
        }"#;

        let new_schema = r#"{
            "type": "record",
            "name": "Test",
            "fields": [
                {"name": "field1", "type": "string"},
                {"name": "field2", "type": "string", "default": ""}
            ]
        }"#;

        let violations = checker.check_backward(new_schema, old_schema).unwrap();
        // Should be compatible because new field has default
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_removing_field_without_default_is_breaking() {
        let checker = AvroCompatibilityChecker::new();

        let old_schema = r#"{
            "type": "record",
            "name": "Test",
            "fields": [
                {"name": "field1", "type": "string"},
                {"name": "field2", "type": "string"}
            ]
        }"#;

        let new_schema = r#"{
            "type": "record",
            "name": "Test",
            "fields": [
                {"name": "field1", "type": "string"}
            ]
        }"#;

        let violations = checker.check_backward(new_schema, old_schema).unwrap();
        assert!(violations.len() > 0);
    }

    #[test]
    fn test_int_to_long_promotion_is_compatible() {
        let checker = AvroCompatibilityChecker::new();

        let old_schema = r#"{
            "type": "record",
            "name": "Test",
            "fields": [
                {"name": "field1", "type": "int"}
            ]
        }"#;

        let new_schema = r#"{
            "type": "record",
            "name": "Test",
            "fields": [
                {"name": "field1", "type": "long"}
            ]
        }"#;

        let violations = checker.check_backward(new_schema, old_schema).unwrap();
        // Int to Long is a valid promotion in Avro
        assert_eq!(violations.len(), 0);
    }
}
