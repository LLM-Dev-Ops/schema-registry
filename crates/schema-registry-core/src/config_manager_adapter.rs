//! Config Manager Adapter
//!
//! Lightweight adapter module for consuming configuration and schema policies
//! from the LLM Config Manager. This module provides a thin, trait-based
//! interface for runtime integration without modifying core logic.
//!
//! # Architecture
//!
//! This adapter follows the "consumes-from" pattern where Schema Registry
//! ingests configuration state from Config Manager as an upstream system.
//!
//! # Integration Points
//!
//! 1. **Startup Configuration**: Load global settings at server initialization
//! 2. **Schema Policies**: Ingest validation rules and policy definitions
//! 3. **Runtime Refresh**: Optional hooks for live configuration updates

use llm_config_core::{ConfigManager, Environment, ConfigValue, Result as ConfigResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, debug};

// ============================================================================
// Configuration Traits
// ============================================================================

/// Trait for consuming configuration from an upstream config source
pub trait ConfigConsumer: Send + Sync {
    /// Load global configuration for the schema registry
    fn load_global_config(&self) -> Result<GlobalConfig, ConfigError>;

    /// Load schema validation policies
    fn load_schema_policies(&self) -> Result<SchemaPolicies, ConfigError>;

    /// Refresh configuration (for runtime updates)
    fn refresh(&self) -> Result<(), ConfigError>;
}

/// Trait for receiving configuration update notifications
pub trait ConfigUpdateListener: Send + Sync {
    /// Called when configuration is updated
    fn on_config_updated(&self, config: &GlobalConfig);

    /// Called when policies are updated
    fn on_policies_updated(&self, policies: &SchemaPolicies);
}

// ============================================================================
// Configuration Types
// ============================================================================

/// Global configuration for Schema Registry consumed from Config Manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Server configuration
    pub server: ServerConfig,

    /// Storage configuration
    pub storage: StorageConfig,

    /// Validation configuration
    pub validation: ValidationConfig,

    /// Security configuration
    pub security: SecurityConfig,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            storage: StorageConfig::default(),
            validation: ValidationConfig::default(),
            security: SecurityConfig::default(),
            metadata: HashMap::new(),
        }
    }
}

/// Server-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host
    pub host: String,

    /// Server port
    pub port: u16,

    /// Maximum request size in bytes
    pub max_request_size: usize,

    /// Request timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_request_size: 10 * 1024 * 1024, // 10MB
            timeout_seconds: 30,
        }
    }
}

/// Storage-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Database connection pool size
    pub pool_size: u32,

    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,

    /// Enable compression
    pub enable_compression: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            pool_size: 10,
            cache_ttl_seconds: 300,
            enable_compression: true,
        }
    }
}

/// Validation-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Maximum schema size in bytes
    pub max_schema_size: usize,

    /// Enable strict validation
    pub strict_mode: bool,

    /// Enable performance validation
    pub performance_checks: bool,

    /// Enable security validation
    pub security_checks: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_schema_size: 1024 * 1024, // 1MB
            strict_mode: false,
            performance_checks: true,
            security_checks: true,
        }
    }
}

/// Security-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable authentication
    pub enable_auth: bool,

    /// Enable TLS
    pub enable_tls: bool,

    /// API rate limit (requests per second)
    pub rate_limit_rps: u32,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_auth: false,
            enable_tls: false,
            rate_limit_rps: 100,
        }
    }
}

/// Schema validation policies consumed from Config Manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaPolicies {
    /// Field naming policies
    pub field_naming: FieldNamingPolicy,

    /// Type restriction policies
    pub type_restrictions: Vec<String>,

    /// Required metadata fields
    pub required_metadata: Vec<String>,

    /// Custom validation rules
    pub custom_rules: Vec<CustomPolicyRule>,
}

impl Default for SchemaPolicies {
    fn default() -> Self {
        Self {
            field_naming: FieldNamingPolicy::default(),
            type_restrictions: Vec::new(),
            required_metadata: Vec::new(),
            custom_rules: Vec::new(),
        }
    }
}

/// Field naming policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldNamingPolicy {
    /// Naming convention: snake_case, camelCase, PascalCase
    pub convention: String,

    /// Enforce convention strictly
    pub enforce: bool,
}

impl Default for FieldNamingPolicy {
    fn default() -> Self {
        Self {
            convention: "snake_case".to_string(),
            enforce: false,
        }
    }
}

/// Custom policy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPolicyRule {
    /// Rule name
    pub name: String,

    /// Rule description
    pub description: String,

    /// Pattern to match (regex)
    pub pattern: Option<String>,

    /// Whether this rule is mandatory
    pub mandatory: bool,
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during config consumption
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Config Manager error: {0}")]
    ConfigManager(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Configuration not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ============================================================================
// Config Manager Adapter Implementation
// ============================================================================

/// Adapter for consuming configuration from LLM Config Manager
pub struct ConfigManagerAdapter {
    manager: Arc<ConfigManager>,
    environment: Environment,
    namespace: String,
}

impl ConfigManagerAdapter {
    /// Create a new adapter with the specified storage path
    ///
    /// # Arguments
    ///
    /// * `storage_path` - Path to the config storage directory
    /// * `environment` - The environment to use (Development, Production, etc.)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use schema_registry_core::config_manager_adapter::ConfigManagerAdapter;
    /// use llm_config_core::Environment;
    ///
    /// let adapter = ConfigManagerAdapter::new("./config", Environment::Development)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(storage_path: impl AsRef<Path>, environment: Environment) -> Result<Self, ConfigError> {
        let manager = ConfigManager::new(storage_path)
            .map_err(|e| ConfigError::ConfigManager(format!("{:?}", e)))?;

        info!("Initialized Config Manager adapter with environment: {:?}", environment);

        Ok(Self {
            manager: Arc::new(manager),
            environment,
            namespace: "schema-registry".to_string(),
        })
    }

    /// Create adapter with custom namespace
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Get the underlying config manager (for advanced usage)
    pub fn manager(&self) -> &Arc<ConfigManager> {
        &self.manager
    }

    /// Helper to get a config value from Config Manager
    fn get_config_value(&self, key: &str) -> ConfigResult<Option<ConfigValue>> {
        match self.manager.get_with_overrides(&self.namespace, key, self.environment.clone())? {
            Some(value) => Ok(Some(value)),
            None => {
                debug!("Config key '{}' not found, using default", key);
                Ok(None)
            }
        }
    }

    /// Parse config value as a specific type
    fn parse_value<T: for<'de> Deserialize<'de>>(&self, value: &ConfigValue) -> Result<T, ConfigError> {
        // ConfigValue should be serializable to JSON
        let json = serde_json::to_value(value)
            .map_err(|e| ConfigError::Serialization(e))?;

        serde_json::from_value(json)
            .map_err(|e| ConfigError::Serialization(e))
    }
}

impl ConfigConsumer for ConfigManagerAdapter {
    fn load_global_config(&self) -> Result<GlobalConfig, ConfigError> {
        info!("Loading global configuration from Config Manager");

        let mut config = GlobalConfig::default();

        // Attempt to load server config
        if let Ok(Some(value)) = self.get_config_value("server") {
            if let Ok(server_config) = self.parse_value::<ServerConfig>(&value) {
                config.server = server_config;
                debug!("Loaded server configuration from Config Manager");
            }
        }

        // Attempt to load storage config
        if let Ok(Some(value)) = self.get_config_value("storage") {
            if let Ok(storage_config) = self.parse_value::<StorageConfig>(&value) {
                config.storage = storage_config;
                debug!("Loaded storage configuration from Config Manager");
            }
        }

        // Attempt to load validation config
        if let Ok(Some(value)) = self.get_config_value("validation") {
            if let Ok(validation_config) = self.parse_value::<ValidationConfig>(&value) {
                config.validation = validation_config;
                debug!("Loaded validation configuration from Config Manager");
            }
        }

        // Attempt to load security config
        if let Ok(Some(value)) = self.get_config_value("security") {
            if let Ok(security_config) = self.parse_value::<SecurityConfig>(&value) {
                config.security = security_config;
                debug!("Loaded security configuration from Config Manager");
            }
        }

        info!("Global configuration loaded successfully");
        Ok(config)
    }

    fn load_schema_policies(&self) -> Result<SchemaPolicies, ConfigError> {
        info!("Loading schema policies from Config Manager");

        let mut policies = SchemaPolicies::default();

        // Attempt to load schema policies
        if let Ok(Some(value)) = self.get_config_value("policies/schema") {
            if let Ok(schema_policies) = self.parse_value::<SchemaPolicies>(&value) {
                policies = schema_policies;
                debug!("Loaded schema policies from Config Manager");
            }
        }

        // Load individual policy components if available
        if let Ok(Some(value)) = self.get_config_value("policies/field-naming") {
            if let Ok(field_naming) = self.parse_value::<FieldNamingPolicy>(&value) {
                policies.field_naming = field_naming;
                debug!("Loaded field naming policy from Config Manager");
            }
        }

        info!("Schema policies loaded successfully");
        Ok(policies)
    }

    fn refresh(&self) -> Result<(), ConfigError> {
        info!("Refreshing configuration from Config Manager");

        // In a production system, this would:
        // 1. Check for version changes in Config Manager
        // 2. Reload modified configurations
        // 3. Notify listeners of changes
        // 4. Apply new policies without restart

        // For now, we simply log the refresh attempt
        // The Config Manager supports version tracking and rollback
        // which enables safe runtime updates

        debug!("Configuration refresh completed");
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a default adapter for development environment
pub fn create_dev_adapter(storage_path: impl AsRef<Path>) -> Result<Arc<dyn ConfigConsumer>, ConfigError> {
    let adapter = ConfigManagerAdapter::new(storage_path, Environment::Development)?;
    Ok(Arc::new(adapter))
}

/// Create a default adapter for production environment
pub fn create_prod_adapter(storage_path: impl AsRef<Path>) -> Result<Arc<dyn ConfigConsumer>, ConfigError> {
    let adapter = ConfigManagerAdapter::new(storage_path, Environment::Production)?;
    Ok(Arc::new(adapter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configs() {
        let global = GlobalConfig::default();
        assert_eq!(global.server.port, 8080);
        assert_eq!(global.storage.pool_size, 10);
        assert!(global.validation.performance_checks);

        let policies = SchemaPolicies::default();
        assert_eq!(policies.field_naming.convention, "snake_case");
    }

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert_eq!(config.max_request_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_validation_config_defaults() {
        let config = ValidationConfig::default();
        assert_eq!(config.max_schema_size, 1024 * 1024);
        assert!(config.performance_checks);
        assert!(config.security_checks);
    }
}
