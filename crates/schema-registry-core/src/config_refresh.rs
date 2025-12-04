//! Runtime Configuration Refresh
//!
//! Provides optional hooks for live configuration updates from Config Manager
//! without requiring server restart. This enables dynamic policy updates and
//! configuration changes in production environments.

use crate::config_manager_adapter::{
    ConfigConsumer, ConfigUpdateListener, GlobalConfig, SchemaPolicies, ConfigError,
};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time;
use tracing::{info, warn, error};

/// Configuration refresh strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStrategy {
    /// Manual refresh only (via API call)
    Manual,

    /// Periodic refresh at fixed intervals
    Periodic(Duration),

    /// Event-driven refresh (watches for changes)
    EventDriven,
}

/// Configuration refresh manager
///
/// Manages runtime configuration updates from Config Manager, providing
/// hooks for automatic refresh and notification to listeners.
pub struct ConfigRefreshManager {
    /// Config adapter
    adapter: Arc<dyn ConfigConsumer>,

    /// Current global configuration
    global_config: Arc<RwLock<GlobalConfig>>,

    /// Current schema policies
    schema_policies: Arc<RwLock<SchemaPolicies>>,

    /// Registered listeners
    listeners: Arc<RwLock<Vec<Arc<dyn ConfigUpdateListener>>>>,

    /// Refresh strategy
    strategy: RefreshStrategy,
}

impl ConfigRefreshManager {
    /// Create a new refresh manager
    pub fn new(
        adapter: Arc<dyn ConfigConsumer>,
        initial_config: GlobalConfig,
        initial_policies: SchemaPolicies,
        strategy: RefreshStrategy,
    ) -> Self {
        Self {
            adapter,
            global_config: Arc::new(RwLock::new(initial_config)),
            schema_policies: Arc::new(RwLock::new(initial_policies)),
            listeners: Arc::new(RwLock::new(Vec::new())),
            strategy,
        }
    }

    /// Register a configuration update listener
    pub fn register_listener(&self, listener: Arc<dyn ConfigUpdateListener>) {
        let mut listeners = self.listeners.write().unwrap();
        listeners.push(listener);
        info!("Registered config update listener ({} total)", listeners.len());
    }

    /// Get current global configuration
    pub fn get_global_config(&self) -> GlobalConfig {
        self.global_config.read().unwrap().clone()
    }

    /// Get current schema policies
    pub fn get_schema_policies(&self) -> SchemaPolicies {
        self.schema_policies.read().unwrap().clone()
    }

    /// Manually trigger a configuration refresh
    pub async fn refresh(&self) -> Result<(), ConfigError> {
        info!("Triggering manual configuration refresh");

        // Refresh via adapter
        self.adapter.refresh()?;

        // Reload configuration
        let new_config = self.adapter.load_global_config()?;
        let new_policies = self.adapter.load_schema_policies()?;

        // Update internal state
        {
            let mut config = self.global_config.write().unwrap();
            *config = new_config.clone();
        }

        {
            let mut policies = self.schema_policies.write().unwrap();
            *policies = new_policies.clone();
        }

        // Notify listeners
        self.notify_listeners(&new_config, &new_policies).await;

        info!("Configuration refresh completed successfully");
        Ok(())
    }

    /// Notify all registered listeners of config updates
    async fn notify_listeners(&self, config: &GlobalConfig, policies: &SchemaPolicies) {
        let listeners = self.listeners.read().unwrap().clone();

        info!("Notifying {} listeners of config update", listeners.len());

        for listener in listeners {
            listener.on_config_updated(config);
            listener.on_policies_updated(policies);
        }
    }

    /// Start background refresh task (for periodic strategy)
    pub async fn start_background_refresh(self: Arc<Self>) {
        match self.strategy {
            RefreshStrategy::Manual => {
                info!("Manual refresh strategy - no background task needed");
            }
            RefreshStrategy::Periodic(interval) => {
                info!("Starting periodic refresh task with interval: {:?}", interval);
                tokio::spawn(async move {
                    let mut ticker = time::interval(interval);

                    loop {
                        ticker.tick().await;

                        match self.refresh().await {
                            Ok(()) => {
                                info!("Periodic configuration refresh succeeded");
                            }
                            Err(e) => {
                                error!("Periodic configuration refresh failed: {}", e);
                            }
                        }
                    }
                });
            }
            RefreshStrategy::EventDriven => {
                info!("Event-driven refresh strategy - watching for config changes");
                // In a production system, this would set up file watchers or
                // subscribe to Config Manager change events
                warn!("Event-driven refresh not fully implemented yet");
            }
        }
    }
}

/// Example listener that logs configuration changes
pub struct LoggingConfigListener;

impl ConfigUpdateListener for LoggingConfigListener {
    fn on_config_updated(&self, config: &GlobalConfig) {
        info!("Configuration updated: server={}:{}, max_schema_size={} bytes",
              config.server.host,
              config.server.port,
              config.validation.max_schema_size);
    }

    fn on_policies_updated(&self, policies: &SchemaPolicies) {
        info!("Policies updated: {} custom rules, field_naming={}",
              policies.custom_rules.len(),
              policies.field_naming.convention);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_manager_adapter::ConfigManagerAdapter;
    use llm_config_core::Environment;

    #[test]
    fn test_refresh_strategy() {
        let manual = RefreshStrategy::Manual;
        let periodic = RefreshStrategy::Periodic(Duration::from_secs(60));

        assert_eq!(manual, RefreshStrategy::Manual);
        assert!(matches!(periodic, RefreshStrategy::Periodic(_)));
    }

    #[test]
    fn test_logging_listener() {
        let listener = LoggingConfigListener;
        let config = GlobalConfig::default();
        let policies = SchemaPolicies::default();

        // Should not panic
        listener.on_config_updated(&config);
        listener.on_policies_updated(&policies);
    }
}
