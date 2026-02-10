use axum::http::HeaderMap;
use uuid::Uuid;

use crate::error::AgenticsError;
use crate::types::ExecutionContext;

/// HTTP header for the execution ID assigned by the Core.
pub const X_EXECUTION_ID: &str = "X-Execution-ID";

/// HTTP header for the parent span ID (the Core's repo-level span).
pub const X_PARENT_SPAN_ID: &str = "X-Parent-Span-ID";

/// Extract an [`ExecutionContext`] from HTTP request headers.
///
/// Both `X-Execution-ID` and `X-Parent-Span-ID` must be present and contain
/// valid UUIDs. Returns an error if either is missing or malformed.
pub fn extract_execution_context(headers: &HeaderMap) -> Result<ExecutionContext, AgenticsError> {
    let execution_id = headers
        .get(X_EXECUTION_ID)
        .ok_or(AgenticsError::MissingHeader(X_EXECUTION_ID))?
        .to_str()
        .map_err(|_| AgenticsError::InvalidHeader(X_EXECUTION_ID))?;
    let execution_id =
        Uuid::parse_str(execution_id).map_err(|_| AgenticsError::InvalidHeader(X_EXECUTION_ID))?;

    let parent_span_id = headers
        .get(X_PARENT_SPAN_ID)
        .ok_or(AgenticsError::MissingHeader(X_PARENT_SPAN_ID))?
        .to_str()
        .map_err(|_| AgenticsError::InvalidHeader(X_PARENT_SPAN_ID))?;
    let parent_span_id = Uuid::parse_str(parent_span_id)
        .map_err(|_| AgenticsError::InvalidHeader(X_PARENT_SPAN_ID))?;

    Ok(ExecutionContext {
        execution_id,
        parent_span_id,
    })
}
