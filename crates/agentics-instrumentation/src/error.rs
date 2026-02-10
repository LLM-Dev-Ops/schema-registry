use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// Errors that can occur during agentics instrumentation.
#[derive(Debug, thiserror::Error)]
pub enum AgenticsError {
    #[error("Missing required header: {0}")]
    MissingHeader(&'static str),

    #[error("Invalid header value: {0}")]
    InvalidHeader(&'static str),

    #[error("Enforcement violation: no agent spans were emitted")]
    NoAgentSpans,

    #[error("Span collector was already finalized")]
    AlreadyFinalized,
}

impl IntoResponse for AgenticsError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AgenticsError::MissingHeader(_) => (StatusCode::BAD_REQUEST, "MISSING_EXECUTION_CONTEXT"),
            AgenticsError::InvalidHeader(_) => (StatusCode::BAD_REQUEST, "INVALID_EXECUTION_CONTEXT"),
            AgenticsError::NoAgentSpans => {
                (StatusCode::INTERNAL_SERVER_ERROR, "NO_AGENT_SPANS")
            }
            AgenticsError::AlreadyFinalized => {
                (StatusCode::INTERNAL_SERVER_ERROR, "ALREADY_FINALIZED")
            }
        };

        let body = serde_json::json!({
            "error": self.to_string(),
            "code": code,
        });

        (status, Json(body)).into_response()
    }
}
