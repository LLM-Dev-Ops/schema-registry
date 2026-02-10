pub mod types;
pub mod error;
pub mod context;
pub mod collector;
pub mod extractors;
pub mod response;
pub mod middleware;

pub use types::{AgenticsSpan, Artifact, ExecutionContext, SpanStatus, SpanType};
pub use collector::{AgentSpanGuard, SpanCollector};
pub use context::{extract_execution_context, X_EXECUTION_ID, X_PARENT_SPAN_ID};
pub use middleware::agentics_middleware;
pub use response::{AgenticsResponse, SpanHierarchy};
pub use error::AgenticsError;
pub use extractors::AgenticsCollector;

/// The repository name used in all spans emitted by this crate.
pub const REPO_NAME: &str = "schema-registry";
