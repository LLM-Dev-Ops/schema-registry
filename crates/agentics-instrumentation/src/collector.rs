use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::types::{AgenticsSpan, Artifact, ExecutionContext, SpanStatus, SpanType};
use crate::REPO_NAME;

/// Per-request, thread-safe, append-only collector that accumulates agentics
/// execution spans during request processing.
///
/// Stored in Axum request extensions as `Arc<SpanCollector>` and retrieved
/// by handlers via the [`AgenticsCollector`](crate::extractors::AgenticsCollector) extractor.
pub struct SpanCollector {
    repo_span: RwLock<AgenticsSpan>,
    agent_spans: RwLock<Vec<AgenticsSpan>>,
    execution_id: Uuid,
}

impl SpanCollector {
    /// Create a new collector. Immediately opens a repo-level span whose
    /// `parent_span_id` comes from the caller's execution context.
    pub fn new(ctx: &ExecutionContext) -> Self {
        let repo_span = AgenticsSpan {
            span_id: Uuid::new_v4(),
            parent_span_id: ctx.parent_span_id,
            span_type: SpanType::Repo,
            repo_name: REPO_NAME.to_string(),
            agent_name: None,
            start_time: Utc::now(),
            end_time: None,
            status: SpanStatus::Ok,
            artifacts: Vec::new(),
            error: None,
        };

        Self {
            repo_span: RwLock::new(repo_span),
            agent_spans: RwLock::new(Vec::new()),
            execution_id: ctx.execution_id,
        }
    }

    /// Begin a new agent-level span. The returned [`AgentSpanGuard`] must be
    /// closed (explicitly or on drop) to record the span's end time.
    pub async fn begin_agent(self: &Arc<Self>, agent_name: &str) -> AgentSpanGuard {
        let repo_span = self.repo_span.read().await;
        let span = AgenticsSpan {
            span_id: Uuid::new_v4(),
            parent_span_id: repo_span.span_id,
            span_type: SpanType::Agent,
            repo_name: REPO_NAME.to_string(),
            agent_name: Some(agent_name.to_string()),
            start_time: Utc::now(),
            end_time: None,
            status: SpanStatus::Ok,
            artifacts: Vec::new(),
            error: None,
        };
        let span_id = span.span_id;
        drop(repo_span);

        self.agent_spans.write().await.push(span);

        tracing::debug!(
            agent_name = agent_name,
            span_id = %span_id,
            "Agentics: agent span started"
        );

        AgentSpanGuard {
            collector: Arc::clone(self),
            span_id,
            closed: false,
        }
    }

    /// Attach an artifact to a specific agent span by its span_id.
    pub async fn attach_artifact(&self, agent_span_id: Uuid, artifact: Artifact) {
        let mut spans = self.agent_spans.write().await;
        if let Some(span) = spans.iter_mut().find(|s| s.span_id == agent_span_id) {
            span.artifacts.push(artifact);
        }
    }

    /// Mark a specific agent span as errored.
    pub async fn set_agent_error(&self, agent_span_id: Uuid, msg: String) {
        let mut spans = self.agent_spans.write().await;
        if let Some(span) = spans.iter_mut().find(|s| s.span_id == agent_span_id) {
            span.status = SpanStatus::Error;
            span.error = Some(msg);
        }
    }

    /// Close a specific agent span by recording its end time.
    pub async fn close_agent(&self, agent_span_id: Uuid) {
        let mut spans = self.agent_spans.write().await;
        if let Some(span) = spans.iter_mut().find(|s| s.span_id == agent_span_id) {
            if span.end_time.is_none() {
                span.end_time = Some(Utc::now());
            }
        }
    }

    /// Return the number of agent spans emitted so far.
    pub async fn agent_span_count(&self) -> usize {
        self.agent_spans.read().await.len()
    }

    /// Finalize the collector: close the repo span and return the full hierarchy.
    ///
    /// Any agent spans that were not explicitly closed get their end_time
    /// backfilled and status set to error.
    pub async fn finalize(&self) -> (Uuid, AgenticsSpan, Vec<AgenticsSpan>) {
        let mut repo_span = self.repo_span.write().await;
        repo_span.end_time = Some(Utc::now());

        let mut agent_spans = self.agent_spans.write().await;

        // Backfill any unclosed agent spans
        let now = Utc::now();
        for span in agent_spans.iter_mut() {
            if span.end_time.is_none() {
                span.end_time = Some(now);
                if span.status == SpanStatus::Ok {
                    span.status = SpanStatus::Error;
                    span.error =
                        Some("Span was not explicitly closed (possible panic or early return)".into());
                }
            }
        }

        (self.execution_id, repo_span.clone(), agent_spans.clone())
    }
}

/// RAII guard for an agent-level span. Closing it records the span's end time.
///
/// If dropped without calling [`close`](AgentSpanGuard::close), the span will
/// be backfilled during [`SpanCollector::finalize`] with an error status.
pub struct AgentSpanGuard {
    collector: Arc<SpanCollector>,
    span_id: Uuid,
    closed: bool,
}

impl AgentSpanGuard {
    /// The UUID of the agent span this guard manages.
    pub fn span_id(&self) -> Uuid {
        self.span_id
    }

    /// Attach an artifact to this agent's span.
    pub async fn attach_artifact(&self, artifact: Artifact) {
        self.collector
            .attach_artifact(self.span_id, artifact)
            .await;
    }

    /// Mark this agent's span as errored with a message.
    pub async fn set_error(&self, msg: impl Into<String>) {
        self.collector
            .set_agent_error(self.span_id, msg.into())
            .await;
    }

    /// Explicitly close this agent span, recording its end time.
    pub async fn close(mut self) {
        self.collector.close_agent(self.span_id).await;
        self.closed = true;

        tracing::debug!(
            span_id = %self.span_id,
            "Agentics: agent span closed"
        );
    }
}

impl Drop for AgentSpanGuard {
    fn drop(&mut self) {
        if !self.closed {
            tracing::warn!(
                span_id = %self.span_id,
                "Agentics: agent span guard dropped without explicit close"
            );
            // The span will be backfilled during finalize().
            // We can't do async work in Drop, so finalize() handles the cleanup.
        }
    }
}
