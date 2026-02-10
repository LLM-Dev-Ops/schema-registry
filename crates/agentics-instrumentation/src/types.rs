use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type of span in the agentics execution hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanType {
    Repo,
    Agent,
}

/// Whether the span completed successfully or with an error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
}

/// An agentics execution span. This is a custom, JSON-serializable structure
/// for the Agentics execution system — not an OpenTelemetry span.
///
/// The hierarchy is always: Core → Repo → Agent(s).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticsSpan {
    pub span_id: Uuid,
    pub parent_span_id: Uuid,
    pub span_type: SpanType,
    pub repo_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub start_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
    pub status: SpanStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// An artifact produced by an agent, attached to the agent's span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub artifact_type: String,
}

/// Execution context propagated from the calling Core system via HTTP headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub execution_id: Uuid,
    pub parent_span_id: Uuid,
}
