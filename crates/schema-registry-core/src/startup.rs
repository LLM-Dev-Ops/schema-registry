//! Server Startup Integration
//!
//! Provides startup utilities for initializing Schema Registry with
//! Config Manager integration. This module demonstrates the Phase 2B
//! runtime "consumes-from" pattern without modifying core logic.

use crate::config_manager_adapter::{
    ConfigConsumer, ConfigManagerAdapter, GlobalConfig, SchemaPolicies, ConfigError,
};
use llm_config_core::Environment;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Startup configuration for Schema Registry
#[derive(Debug, Clone)]
pub struct StartupConfig {
    /// Path to config storage
    pub config_storage_path: PathBuf,

    /// Environment (Development, Staging, Production)
    pub environment: Environment,

    /// Whether to fail if config loading fails
    pub require_config: bool,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            config_storage_path: PathBuf::from("./config"),
            environment: Environment::Development,
            require_config: false,
        }
    }
}

/// Startup context containing loaded configuration and policies
#[derive(Clone)]
pub struct StartupContext {
    /// Global configuration loaded from Config Manager
    pub global_config: GlobalConfig,

    /// Schema validation policies
    pub schema_policies: SchemaPolicies,

    /// Config adapter for runtime refresh
    pub config_adapter: Option<Arc<dyn ConfigConsumer>>,
}

impl Default for StartupContext {
    fn default() -> Self {
        Self {
            global_config: GlobalConfig::default(),
            schema_policies: SchemaPolicies::default(),
            config_adapter: None,
        }
    }
}

impl StartupContext {
    /// Refresh configuration from Config Manager
    pub fn refresh(&self) -> Result<(), ConfigError> {
        if let Some(adapter) = &self.config_adapter {
            adapter.refresh()?;
            info!("Configuration refreshed successfully");
        }
        Ok(())
    }
}

/// Initialize Schema Registry with Config Manager integration
///
/// This function performs Phase 2B runtime integration:
/// 1. Initializes Config Manager adapter
/// 2. Loads global configuration
/// 3. Ingests schema validation policies
/// 4. Prepares optional runtime refresh hooks
///
/// # Arguments
///
/// * `config` - Startup configuration
///
/// # Returns
///
/// A `StartupContext` containing loaded configuration and policies
///
/// # Example
///
/// ```no_run
/// use schema_registry_core::startup::{initialize_with_config_manager, StartupConfig};
/// use llm_config_core::Environment;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = StartupConfig {
///     config_storage_path: "./config".into(),
///     environment: Environment::Production,
///     require_config: false,
/// };
///
/// let context = initialize_with_config_manager(config).await?;
/// println!("Loaded config with max schema size: {}", context.global_config.validation.max_schema_size);
/// # Ok(())
/// # }
/// ```
pub async fn initialize_with_config_manager(
    config: StartupConfig,
) -> Result<StartupContext, ConfigError> {
    info!("Initializing Schema Registry with Config Manager integration");
    info!("Environment: {:?}, Config path: {:?}", config.environment, config.config_storage_path);

    // Create Config Manager adapter
    let adapter = match ConfigManagerAdapter::new(&config.config_storage_path, config.environment) {
        Ok(adapter) => {
            info!("Config Manager adapter initialized successfully");
            adapter
        }
        Err(e) => {
            if config.require_config {
                return Err(e);
            } else {
                warn!("Failed to initialize Config Manager, using defaults: {}", e);
                return Ok(StartupContext::default());
            }
        }
    };

    // Load global configuration
    let global_config = match adapter.load_global_config() {
        Ok(config) => {
            info!("Global configuration loaded from Config Manager");
            config
        }
        Err(e) => {
            if config.require_config {
                return Err(e);
            } else {
                warn!("Failed to load global config, using defaults: {}", e);
                GlobalConfig::default()
            }
        }
    };

    // Load schema validation policies
    let schema_policies = match adapter.load_schema_policies() {
        Ok(policies) => {
            info!("Schema policies loaded from Config Manager");
            policies
        }
        Err(e) => {
            if config.require_config {
                return Err(e);
            } else {
                warn!("Failed to load schema policies, using defaults: {}", e);
                SchemaPolicies::default()
            }
        }
    };

    info!("Schema Registry initialization complete");
    info!("Server will listen on {}:{}", global_config.server.host, global_config.server.port);
    info!("Validation: max_schema_size={} bytes, strict_mode={}",
          global_config.validation.max_schema_size,
          global_config.validation.strict_mode);

    Ok(StartupContext {
        global_config,
        schema_policies,
        config_adapter: Some(Arc::new(adapter)),
    })
}

/// Quick initialization for development
pub async fn initialize_dev() -> Result<StartupContext, ConfigError> {
    initialize_with_config_manager(StartupConfig {
        environment: Environment::Development,
        require_config: false,
        ..Default::default()
    })
    .await
}

/// Quick initialization for production
pub async fn initialize_prod(config_path: PathBuf) -> Result<StartupContext, ConfigError> {
    initialize_with_config_manager(StartupConfig {
        config_storage_path: config_path,
        environment: Environment::Production,
        require_config: true,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_startup_with_defaults() {
        let config = StartupConfig::default();
        assert_eq!(config.environment, Environment::Development);
        assert!(!config.require_config);
    }

    #[tokio::test]
    async fn test_startup_context_default() {
        let context = StartupContext::default();
        assert_eq!(context.global_config.server.port, 8080);
        assert_eq!(context.schema_policies.field_naming.convention, "snake_case");
    }

    #[test]
    fn test_startup_config_builder() {
        let config = StartupConfig {
            config_storage_path: PathBuf::from("/custom/path"),
            environment: Environment::Production,
            require_config: true,
        };

        assert_eq!(config.config_storage_path, PathBuf::from("/custom/path"));
        assert_eq!(config.environment, Environment::Production);
        assert!(config.require_config);
    }
}
