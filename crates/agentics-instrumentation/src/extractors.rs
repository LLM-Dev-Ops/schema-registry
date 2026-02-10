use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::collector::SpanCollector;

/// Axum extractor that provides access to the agentics [`SpanCollector`].
///
/// This extractor retrieves the `Arc<SpanCollector>` stored in request
/// extensions by the agentics middleware.
///
/// # Example
///
/// ```rust,ignore
/// async fn my_handler(spans: AgenticsCollector) -> impl IntoResponse {
///     let guard = spans.begin_agent("my-agent").await;
///     // ... do work ...
///     guard.close().await;
///     Json(result)
/// }
/// ```
pub struct AgenticsCollector(pub Arc<SpanCollector>);

impl AgenticsCollector {
    /// Get the inner `Arc<SpanCollector>`.
    pub fn inner(&self) -> &Arc<SpanCollector> {
        &self.0
    }
}

impl std::ops::Deref for AgenticsCollector {
    type Target = Arc<SpanCollector>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Rejection type when the AgenticsCollector is not available in request extensions.
pub struct AgenticsCollectorRejection;

impl IntoResponse for AgenticsCollectorRejection {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "AgenticsCollector not available - is agentics middleware applied?",
                "code": "MISSING_AGENTICS_MIDDLEWARE",
            })),
        )
            .into_response()
    }
}

impl<S> FromRequestParts<S> for AgenticsCollector
where
    S: Send + Sync,
{
    type Rejection = AgenticsCollectorRejection;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<Arc<SpanCollector>>()
            .cloned()
            .map(AgenticsCollector)
            .ok_or(AgenticsCollectorRejection);
        std::future::ready(result)
    }
}
