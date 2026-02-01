# Phase 2B: Config Manager Runtime Integration

This document describes the runtime "consumes-from" integration between Schema Registry and Config Manager, implemented in Phase 2B.

## Overview

Schema Registry now consumes configuration state, schema policies, and optional live-update signals from Config Manager as an upstream system. This integration is implemented as a lightweight, non-breaking adapter layer that preserves Schema Registry's core functionality.

## Architecture

The integration consists of three main components:

### 1. Config Manager Adapter (`config_manager_adapter.rs`)

The adapter module provides trait-based interfaces for consuming configuration:

```rust
use schema_registry_core::config_manager_adapter::{
    ConfigManagerAdapter, ConfigConsumer, GlobalConfig, SchemaPolicies
};
use llm_config_core::Environment;

// Initialize adapter
let adapter = ConfigManagerAdapter::new("./config", Environment::Production)?;

// Load global configuration
let global_config: GlobalConfig = adapter.load_global_config()?;

// Load schema validation policies
let policies: SchemaPolicies = adapter.load_schema_policies()?;
```

### 2. Startup Integration (`startup.rs`)

Server initialization with Config Manager integration:

```rust
use schema_registry_core::startup::{initialize_with_config_manager, StartupConfig};
use llm_config_core::Environment;

// Initialize at server startup
let config = StartupConfig {
    config_storage_path: "./config".into(),
    environment: Environment::Production,
    require_config: false, // Graceful fallback to defaults
};

let context = initialize_with_config_manager(config).await?;

// Access loaded configuration
println!("Server: {}:{}",
    context.global_config.server.host,
    context.global_config.server.port
);
```

### 3. Policy-Based Validation (`config_integration.rs`)

Validation engine consumes schema policies:

```rust
use schema_registry_validation::config_integration::PolicyBasedValidationRule;
use schema_registry_core::config_manager_adapter::SchemaPolicies;

// Create policy-based validation rule
let policies = adapter.load_schema_policies()?;
let policy_rule = PolicyBasedValidationRule::new(policies);

// Add to validation engine
validation_engine.add_rule(Arc::new(policy_rule));
```

### 4. Runtime Refresh (`config_refresh.rs`)

Optional live configuration updates:

```rust
use schema_registry_core::config_refresh::{
    ConfigRefreshManager, RefreshStrategy, LoggingConfigListener
};
use std::time::Duration;

// Create refresh manager with periodic strategy
let refresh_manager = Arc::new(ConfigRefreshManager::new(
    adapter,
    global_config,
    schema_policies,
    RefreshStrategy::Periodic(Duration::from_secs(300)) // 5 minutes
));

// Register listeners for config changes
refresh_manager.register_listener(Arc::new(LoggingConfigListener));

// Start background refresh task
refresh_manager.clone().start_background_refresh().await;

// Manual refresh
refresh_manager.refresh().await?;
```

## Configuration Surfaces Consumed

### From Config Manager

The integration consumes the following configuration surfaces from Config Manager:

1. **Global Configuration** (`schema-registry/server`)
   - Server host and port
   - Request size limits and timeouts
   - Storage pool configuration
   - Cache settings

2. **Validation Configuration** (`schema-registry/validation`)
   - Maximum schema size limits
   - Strict mode enforcement
   - Performance check toggles
   - Security validation flags

3. **Security Configuration** (`schema-registry/security`)
   - Authentication settings
   - TLS configuration
   - Rate limiting policies

4. **Schema Policies** (`schema-registry/policies/schema`)
   - Field naming conventions (snake_case, camelCase, PascalCase)
   - Type restrictions
   - Required metadata fields
   - Custom validation rules

## Integration Points

### At Server Startup

```rust
// In main.rs or server initialization
use schema_registry_core::startup::initialize_with_config_manager;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize with Config Manager
    let context = initialize_with_config_manager(StartupConfig {
        config_storage_path: "./config".into(),
        environment: Environment::Production,
        require_config: false,
    }).await?;

    // Use configuration
    let server_addr = format!("{}:{}",
        context.global_config.server.host,
        context.global_config.server.port
    );

    // Start server with loaded config...
}
```

### In Validation Engine

```rust
// Extend validation with policies
use schema_registry_validation::config_integration::PolicyBasedValidationRule;

let policies = startup_context.schema_policies;
let policy_rule = PolicyBasedValidationRule::new(policies);

// Add to existing validation engine
validation_engine.add_rule(Arc::new(policy_rule));
```

## Configuration Example

Store configuration in Config Manager:

```bash
# Server configuration
llm-config set schema-registry server '{
  "host": "0.0.0.0",
  "port": 8080,
  "max_request_size": 10485760,
  "timeout_seconds": 30
}' --env production

# Validation configuration
llm-config set schema-registry validation '{
  "max_schema_size": 1048576,
  "strict_mode": true,
  "performance_checks": true,
  "security_checks": true
}' --env production

# Schema policies
llm-config set schema-registry policies/schema '{
  "field_naming": {
    "convention": "snake_case",
    "enforce": true
  },
  "type_restrictions": [],
  "required_metadata": ["description", "owner"],
  "custom_rules": []
}' --env production
```

## Benefits

1. **Centralized Configuration**: All Schema Registry settings managed through Config Manager
2. **Environment-Specific**: Different configs for dev, staging, production
3. **Policy-Driven Validation**: Organizational schema policies enforced automatically
4. **Runtime Updates**: Live configuration refresh without restart (optional)
5. **Non-Breaking**: Graceful fallback to defaults if Config Manager unavailable
6. **No Core Changes**: Preserves existing Schema Registry API and behavior

## Backward Compatibility

- All integration is **optional** and **non-breaking**
- If Config Manager is unavailable, Schema Registry uses sensible defaults
- Existing code continues to work without modifications
- No changes to public APIs or core logic

## Testing

Run tests to verify integration:

```bash
# Test core adapter
cargo test -p schema-registry-core --lib config_manager_adapter

# Test validation integration
cargo test -p schema-registry-validation --lib config_integration

# Test startup integration
cargo test -p schema-registry-core --lib startup

# Test refresh hooks
cargo test -p schema-registry-core --lib config_refresh
```

## Next Steps

This Phase 2B integration prepares Schema Registry to:

1. **Be consumed by downstream repositories**: Validation Service, Schema Evolution Service
2. **Provide validated schemas** with enforced organizational policies
3. **Support dynamic policy updates** in production environments
4. **Enable centralized governance** across the LLM platform

The Schema Registry is now fully Phase 2B-aligned and ready to serve as a foundational provider in the LLM Dev Ops ecosystem.
