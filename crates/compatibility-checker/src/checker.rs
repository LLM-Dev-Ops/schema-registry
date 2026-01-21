//! Main compatibility checker implementation

use crate::cache::CompatibilityCache;
use crate::formats::{AvroCompatibilityChecker, FormatCompatibilityChecker, JsonSchemaCompatibilityChecker, ProtobufCompatibilityChecker};
use crate::types::{CompatibilityMode, CompatibilityResult, Schema, SchemaFormat};
use crate::violation::{CompatibilityViolation, ViolationSeverity, ViolationType};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Compatibility checker errors
#[derive(Error, Debug)]
pub enum CompatibilityError {
    #[error("Schema parse error: {0}")]
    ParseError(String),

    #[error("Unsupported schema format: {0:?}")]
    UnsupportedFormat(SchemaFormat),

    #[error("Schema fetch error: {0}")]
    SchemaFetchError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Configuration for compatibility checker
#[derive(Debug, Clone)]
pub struct CompatibilityCheckerConfig {
    /// Enable caching of compatibility results
    pub enable_cache: bool,
    /// Maximum cache size
    pub max_cache_size: u64,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Maximum number of versions to check in transitive mode
    pub max_transitive_versions: usize,
    /// Timeout for compatibility check in milliseconds
    pub check_timeout_ms: u64,
}

impl Default for CompatibilityCheckerConfig {
    fn default() -> Self {
        Self {
            enable_cache: true,
            max_cache_size: 10_000,
            cache_ttl_seconds: 3600, // 1 hour
            max_transitive_versions: 100,
            check_timeout_ms: 25, // p95 target
        }
    }
}

/// Main compatibility checker
pub struct CompatibilityChecker {
    config: CompatibilityCheckerConfig,
    cache: Option<Arc<CompatibilityCache>>,
    json_checker: Arc<JsonSchemaCompatibilityChecker>,
    avro_checker: Arc<AvroCompatibilityChecker>,
    protobuf_checker: Arc<ProtobufCompatibilityChecker>,
}

impl CompatibilityChecker {
    /// Create a new compatibility checker
    pub fn new(config: CompatibilityCheckerConfig) -> Self {
        let cache = if config.enable_cache {
            Some(Arc::new(CompatibilityCache::new(
                config.max_cache_size,
                config.cache_ttl_seconds,
            )))
        } else {
            None
        };

        Self {
            config,
            cache,
            json_checker: Arc::new(JsonSchemaCompatibilityChecker::new()),
            avro_checker: Arc::new(AvroCompatibilityChecker::new()),
            protobuf_checker: Arc::new(ProtobufCompatibilityChecker::new()),
        }
    }

    /// Check compatibility between new and old schema
    ///
    /// This is the main entry point as specified in PSEUDOCODE.md § 1.5
    pub async fn check_compatibility(
        &self,
        new_schema: &Schema,
        old_schema: &Schema,
        mode: CompatibilityMode,
    ) -> Result<CompatibilityResult, CompatibilityError> {
        let start = Instant::now();

        debug!(
            "Checking compatibility: {} vs {} in mode {:?}",
            new_schema.version, old_schema.version, mode
        );

        // Fast path: identical schemas (content hash check)
        if new_schema.content_hash == old_schema.content_hash {
            info!("Schemas are identical (content hash match)");
            return Ok(CompatibilityResult::compatible(
                mode,
                vec![old_schema.version.clone()],
                start.elapsed().as_millis() as u64,
            ));
        }

        // Check cache
        if let Some(ref cache) = self.cache {
            if let Some(cached_result) =
                cache.get(&new_schema.content_hash, &old_schema.content_hash, mode).await
            {
                debug!("Cache hit for compatibility check");
                return Ok(cached_result);
            }
        }

        // Format compatibility check
        if new_schema.format != old_schema.format {
            warn!("Schema format mismatch: {:?} vs {:?}", new_schema.format, old_schema.format);
            let violation = CompatibilityViolation::breaking(
                ViolationType::FormatChanged,
                "schema.format",
                format!(
                    "Schema format changed from {:?} to {:?}",
                    old_schema.format, new_schema.format
                ),
            )
            .with_values(
                Some(serde_json::json!(old_schema.format)),
                Some(serde_json::json!(new_schema.format)),
            );

            let result = CompatibilityResult::incompatible(
                mode,
                vec![violation],
                vec![old_schema.version.clone()],
                start.elapsed().as_millis() as u64,
            );

            // Cache the result
            if let Some(ref cache) = self.cache {
                cache.put(
                    new_schema.content_hash.clone(),
                    old_schema.content_hash.clone(),
                    mode,
                    result.clone(),
                ).await;
            }

            return Ok(result);
        }

        // Delegate to mode-specific checker
        let result = match mode {
            CompatibilityMode::None => {
                // No compatibility checking
                CompatibilityResult::compatible(
                    mode,
                    vec![],
                    start.elapsed().as_millis() as u64,
                )
            }
            CompatibilityMode::Backward => {
                self.check_backward(new_schema, old_schema, start).await?
            }
            CompatibilityMode::Forward => {
                self.check_forward(new_schema, old_schema, start).await?
            }
            CompatibilityMode::Full => self.check_full(new_schema, old_schema, start).await?,
            _ => {
                // Transitive modes handled separately
                return Err(CompatibilityError::InternalError(
                    "Transitive modes should be handled by check_compatibility_transitive".to_string(),
                ));
            }
        };

        // Cache the result
        if let Some(ref cache) = self.cache {
            cache.put(
                new_schema.content_hash.clone(),
                old_schema.content_hash.clone(),
                mode,
                result.clone(),
            ).await;
        }

        Ok(result)
    }

    /// Check backward compatibility
    /// New schema can read old data
    async fn check_backward(
        &self,
        new_schema: &Schema,
        old_schema: &Schema,
        start: Instant,
    ) -> Result<CompatibilityResult, CompatibilityError> {
        let violations = match new_schema.format {
            SchemaFormat::JsonSchema => {
                self.json_checker
                    .check_backward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Avro => {
                self.avro_checker
                    .check_backward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Protobuf => {
                self.protobuf_checker
                    .check_backward(&new_schema.content, &old_schema.content)?
            }
        };

        let is_compatible = violations
            .iter()
            .all(|v| v.severity != ViolationSeverity::Breaking);

        Ok(CompatibilityResult {
            is_compatible,
            mode: CompatibilityMode::Backward,
            violations,
            checked_versions: vec![old_schema.version.clone()],
            check_duration_ms: start.elapsed().as_millis() as u64,
            metadata: Default::default(),
        })
    }

    /// Check forward compatibility
    /// Old schema can read new data
    async fn check_forward(
        &self,
        new_schema: &Schema,
        old_schema: &Schema,
        start: Instant,
    ) -> Result<CompatibilityResult, CompatibilityError> {
        // Forward compatibility is the inverse of backward
        // Check if old schema can read data written with new schema
        let violations = match new_schema.format {
            SchemaFormat::JsonSchema => {
                self.json_checker
                    .check_forward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Avro => {
                self.avro_checker
                    .check_forward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Protobuf => {
                self.protobuf_checker
                    .check_forward(&new_schema.content, &old_schema.content)?
            }
        };

        let is_compatible = violations
            .iter()
            .all(|v| v.severity != ViolationSeverity::Breaking);

        Ok(CompatibilityResult {
            is_compatible,
            mode: CompatibilityMode::Forward,
            violations,
            checked_versions: vec![old_schema.version.clone()],
            check_duration_ms: start.elapsed().as_millis() as u64,
            metadata: Default::default(),
        })
    }

    /// Check full compatibility (both backward and forward)
    async fn check_full(
        &self,
        new_schema: &Schema,
        old_schema: &Schema,
        start: Instant,
    ) -> Result<CompatibilityResult, CompatibilityError> {
        let backward_violations = match new_schema.format {
            SchemaFormat::JsonSchema => {
                self.json_checker
                    .check_backward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Avro => {
                self.avro_checker
                    .check_backward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Protobuf => {
                self.protobuf_checker
                    .check_backward(&new_schema.content, &old_schema.content)?
            }
        };

        let forward_violations = match new_schema.format {
            SchemaFormat::JsonSchema => {
                self.json_checker
                    .check_forward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Avro => {
                self.avro_checker
                    .check_forward(&new_schema.content, &old_schema.content)?
            }
            SchemaFormat::Protobuf => {
                self.protobuf_checker
                    .check_forward(&new_schema.content, &old_schema.content)?
            }
        };

        let mut all_violations = backward_violations;
        all_violations.extend(forward_violations);

        let is_compatible = all_violations
            .iter()
            .all(|v| v.severity != ViolationSeverity::Breaking);

        Ok(CompatibilityResult {
            is_compatible,
            mode: CompatibilityMode::Full,
            violations: all_violations,
            checked_versions: vec![old_schema.version.clone()],
            check_duration_ms: start.elapsed().as_millis() as u64,
            metadata: Default::default(),
        })
    }

    /// Check compatibility in transitive mode
    ///
    /// Checks compatibility against all versions, not just the latest
    pub async fn check_compatibility_transitive<F>(
        &self,
        new_schema: &Schema,
        mode: CompatibilityMode,
        fetch_versions: F,
    ) -> Result<CompatibilityResult, CompatibilityError>
    where
        F: Fn(&str, &str) -> Result<Vec<Schema>, String>,
    {
        let start = Instant::now();

        if !mode.is_transitive() {
            return Err(CompatibilityError::InternalError(
                "Non-transitive mode passed to check_compatibility_transitive".to_string(),
            ));
        }

        // Fetch all versions to check
        let versions = fetch_versions(&new_schema.namespace, &new_schema.name)
            .map_err(|e| CompatibilityError::SchemaFetchError(e))?;

        if versions.is_empty() {
            // First version, automatically compatible
            return Ok(CompatibilityResult::compatible(
                mode,
                vec![],
                start.elapsed().as_millis() as u64,
            ));
        }

        // Limit to max transitive versions (most recent)
        let versions_to_check: Vec<_> = versions
            .into_iter()
            .take(self.config.max_transitive_versions)
            .collect();

        let mut all_violations = Vec::new();
        let mut checked_versions = Vec::new();

        // Check against each version
        for old_version in &versions_to_check {
            checked_versions.push(old_version.version.clone());

            // Use base mode for each individual check
            let base_mode = mode.base_mode();
            let result = self
                .check_compatibility(new_schema, old_version, base_mode)
                .await?;

            all_violations.extend(result.violations);
        }

        let is_compatible = all_violations
            .iter()
            .all(|v| v.severity != ViolationSeverity::Breaking);

        Ok(CompatibilityResult {
            is_compatible,
            mode,
            violations: all_violations,
            checked_versions,
            check_duration_ms: start.elapsed().as_millis() as u64,
            metadata: Default::default(),
        })
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> Option<(u64, u64, f64)> {
        self.cache
            .as_ref()
            .map(|cache| cache.stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_identical_schemas_are_compatible() {
        let checker = CompatibilityChecker::new(CompatibilityCheckerConfig::default());

        let schema1 = create_test_schema("test", "1.0.0");
        let schema2 = schema1.clone();

        let result = checker
            .check_compatibility(&schema1, &schema2, CompatibilityMode::Backward)
            .await
            .unwrap();

        assert!(result.is_compatible);
        assert_eq!(result.violations.len(), 0);
    }

    #[tokio::test]
    async fn test_format_mismatch_is_incompatible() {
        let checker = CompatibilityChecker::new(CompatibilityCheckerConfig::default());

        let mut schema1 = create_test_schema("test", "1.0.0");
        let mut schema2 = schema1.clone();

        schema1.format = SchemaFormat::JsonSchema;
        schema2.format = SchemaFormat::Avro;

        let result = checker
            .check_compatibility(&schema1, &schema2, CompatibilityMode::Backward)
            .await
            .unwrap();

        assert!(!result.is_compatible);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].violation_type, ViolationType::FormatChanged);
    }

    fn create_test_schema(name: &str, version: &str) -> Schema {
        use chrono::Utc;
        use uuid::Uuid;

        let content = r#"{"type": "object", "properties": {"field1": {"type": "string"}}}"#;
        Schema {
            id: Uuid::new_v4(),
            name: name.to_string(),
            namespace: "test".to_string(),
            version: crate::types::SemanticVersion::parse(version).unwrap(),
            format: SchemaFormat::JsonSchema,
            content: content.to_string(),
            content_hash: Schema::calculate_hash(content),
            description: "Test schema".to_string(),
            compatibility_mode: CompatibilityMode::Backward,
            created_at: Utc::now(),
            metadata: Default::default(),
        }
    }
}
