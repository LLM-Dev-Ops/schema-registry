//! # Schema Registry Core
//!
//! Core types, traits, and business logic for the LLM Schema Registry.
//!
//! This crate provides the foundational data structures and abstractions used
//! throughout the schema registry system, including:
//!
//! - Schema metadata and versioning
//! - State machine definitions
//! - Core traits for storage, validation, and compatibility
//! - Error types
//! - Event system

pub mod error;
pub mod events;
pub mod schema;
pub mod state;
pub mod traits;
pub mod types;
pub mod versioning;

// Config Manager integration adapter (Phase 2B)
pub mod config_manager_adapter;
pub mod startup;
pub mod config_refresh;

// Re-export commonly used types
pub use error::{Error, Result};
pub use schema::{RegisteredSchema, SchemaInput, SchemaMetadata};
pub use state::{SchemaState, StateTransition, SchemaLifecycle};
pub use types::{CompatibilityMode, SerializationFormat};
pub use versioning::SemanticVersion;
