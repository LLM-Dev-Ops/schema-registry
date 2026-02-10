use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::AgenticsSpan;

/// The span hierarchy emitted by a repo execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanHierarchy {
    pub repo_span: AgenticsSpan,
    pub agent_spans: Vec<AgenticsSpan>,
}

/// Response envelope that wraps all instrumented endpoint responses.
///
/// The `data` field contains the original handler response. The `spans` field
/// contains the full execution span hierarchy for this repo invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticsResponse<T: Serialize> {
    pub execution_id: Uuid,
    pub spans: SpanHierarchy,
    pub data: T,
}
