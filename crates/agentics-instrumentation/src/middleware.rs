use std::sync::Arc;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http_body_util::BodyExt;

use crate::collector::SpanCollector;
use crate::context::extract_execution_context;
use crate::response::SpanHierarchy;
use crate::types::SpanStatus;

/// Axum middleware that instruments requests with agentics span collection.
///
/// This middleware:
/// 1. Extracts `ExecutionContext` from `X-Execution-ID` and `X-Parent-Span-ID` headers
/// 2. Creates a `SpanCollector` with a repo-level span
/// 3. Stores the collector in request extensions for handlers to access
/// 4. After the handler completes, enforces span rules and wraps the response
///
/// Health and probe endpoints should NOT use this middleware — apply it only
/// to API route groups that require agentics instrumentation.
pub async fn agentics_middleware(mut request: Request, next: Next) -> Response {
    // 1. Extract execution context from headers
    let context = match extract_execution_context(request.headers()) {
        Ok(ctx) => ctx,
        Err(e) => return e.into_response(),
    };

    // 2. Create collector and store in extensions
    let collector = Arc::new(SpanCollector::new(&context));
    request.extensions_mut().insert(Arc::clone(&collector));

    // 3. Run the handler
    let response = next.run(request).await;
    let status = response.status();

    // 4. Finalize collector
    let (execution_id, mut repo_span, agent_spans) = collector.finalize().await;

    // 5. Enforcement: at least one agent span required for successful responses
    if agent_spans.is_empty() && status.is_success() {
        tracing::error!("Agentics enforcement violation: no agent spans emitted for successful response");
        repo_span.status = SpanStatus::Error;
        repo_span.error = Some("Enforcement violation: no agent spans were emitted".into());

        let envelope = serde_json::json!({
            "execution_id": execution_id,
            "spans": SpanHierarchy {
                repo_span,
                agent_spans: vec![],
            },
            "error": "Execution is INVALID: no agent-level spans were emitted",
            "code": "NO_AGENT_SPANS",
        });

        return (StatusCode::INTERNAL_SERVER_ERROR, Json(envelope)).into_response();
    }

    // 6. On handler failure, mark the repo span as failed
    if !status.is_success() {
        repo_span.status = SpanStatus::Error;
        if repo_span.error.is_none() {
            repo_span.error = Some(format!("Handler returned status {}", status.as_u16()));
        }
    }

    // 7. Wrap the response body in the agentics envelope
    wrap_response(execution_id, response, repo_span, agent_spans).await
}

/// Read the original response body and wrap it in an AgenticsResponse envelope.
async fn wrap_response(
    execution_id: uuid::Uuid,
    response: Response,
    repo_span: crate::types::AgenticsSpan,
    agent_spans: Vec<crate::types::AgenticsSpan>,
) -> Response {
    let status = response.status();
    let original_headers = response.headers().clone();

    // Read the original body using http-body-util
    let body_bytes = match response.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to read response body for agentics wrapping");
            let envelope = serde_json::json!({
                "execution_id": execution_id,
                "spans": SpanHierarchy {
                    repo_span,
                    agent_spans,
                },
                "data": null,
                "error": format!("Failed to read response body: {}", e),
            });
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(envelope)).into_response();
        }
    };

    // Try to parse original body as JSON; fall back to null
    let original_data: serde_json::Value =
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);

    let envelope = serde_json::json!({
        "execution_id": execution_id,
        "spans": SpanHierarchy {
            repo_span,
            agent_spans,
        },
        "data": original_data,
    });

    // Build response with original status code
    let mut new_response = (status, Json(envelope)).into_response();

    // Preserve select original headers (correlation ID, etc.)
    for (key, value) in original_headers.iter() {
        let name = key.as_str();
        // Skip headers that the new JSON body invalidates
        if name == "content-length" || name == "content-type" {
            continue;
        }
        new_response
            .headers_mut()
            .insert(key.clone(), value.clone());
    }

    new_response
}
