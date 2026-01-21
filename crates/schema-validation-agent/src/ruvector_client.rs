//! RuVector Service Client for Persistence
//!
//! CRITICAL: Schema Registry NEVER connects directly to databases.
//! All persistence goes through ruvector-service via this client.
//!
//! The Schema Validation Agent MUST NOT:
//! - Execute SQL queries
//! - Connect to databases directly
//! - Store data anywhere except through this client
//!
//! All data persistence happens through DecisionEvent submission to ruvector-service.

use crate::decision_event::DecisionEvent;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Errors that can occur with the RuVector client
#[derive(Debug, Error)]
pub enum RuVectorError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Persistence failed: {0}")]
    PersistenceError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Schema not found: {namespace}/{name}:{version}")]
    SchemaNotFound {
        namespace: String,
        name: String,
        version: String,
    },

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Rate limited: retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Configuration for the RuVector client
#[derive(Debug, Clone)]
pub struct RuVectorClientConfig {
    /// Base URL of the ruvector-service
    pub base_url: String,
    /// API key for authentication (optional)
    pub api_key: Option<String>,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Number of retries on failure
    pub max_retries: u32,
    /// Initial retry delay in milliseconds
    pub retry_delay_ms: u64,
    /// Maximum retry delay in milliseconds
    pub max_retry_delay_ms: u64,
    /// Enable request logging
    pub enable_logging: bool,
}

impl Default for RuVectorClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            api_key: None,
            timeout_ms: 30_000,
            max_retries: 3,
            retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
            enable_logging: true,
        }
    }
}

/// Response from persisting a decision event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistResponse {
    /// Whether the persistence was successful
    pub success: bool,
    /// Event ID that was persisted
    pub event_id: String,
    /// Timestamp of persistence
    pub persisted_at: String,
    /// Any warnings
    pub warnings: Vec<String>,
}

/// Schema version response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVersionResponse {
    /// Schema content
    pub content: String,
    /// Schema version
    pub version: String,
    /// Content hash
    pub content_hash: String,
    /// Created timestamp
    pub created_at: String,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Service version
    pub version: String,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Component statuses
    pub components: std::collections::HashMap<String, String>,
}

/// Client for communicating with ruvector-service
///
/// This is the ONLY way the validation agent should persist data.
/// CRITICAL: This agent MUST NOT execute SQL or connect to databases directly.
pub struct RuVectorClient {
    client: Client,
    config: RuVectorClientConfig,
}

impl RuVectorClient {
    /// Create a new RuVector client
    pub fn new(config: RuVectorClientConfig) -> Result<Self, RuVectorError> {
        let timeout = Duration::from_millis(config.timeout_ms);

        let client = Client::builder()
            .timeout(timeout)
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| RuVectorError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    /// Create a client with default configuration
    pub fn with_url(base_url: impl Into<String>) -> Result<Self, RuVectorError> {
        let config = RuVectorClientConfig {
            base_url: base_url.into(),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Persist a DecisionEvent - ONLY way to store data
    ///
    /// This agent MUST NOT execute SQL or connect to databases directly.
    /// All data persistence goes through ruvector-service.
    pub async fn persist_decision_event(&self, event: &DecisionEvent) -> Result<PersistResponse, RuVectorError> {
        let url = format!("{}/api/v1/decision-events", self.config.base_url);

        if self.config.enable_logging {
            info!(
                "Persisting decision event {} of type {:?}",
                event.event_id, event.decision_type
            );
        }

        let mut retry_count = 0;
        let mut delay = self.config.retry_delay_ms;

        loop {
            let mut request = self.client.post(&url).json(event);

            // Add authentication if configured
            if let Some(ref api_key) = self.config.api_key {
                request = request.header("Authorization", format!("Bearer {}", api_key));
            }

            // Add tracing headers
            request = request
                .header("X-Correlation-Id", event.correlation_id.to_string())
                .header("X-Agent-Id", &event.agent_id);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        let persist_response: PersistResponse = response
                            .json()
                            .await
                            .map_err(|e| RuVectorError::InvalidResponse(e.to_string()))?;

                        if self.config.enable_logging {
                            info!(
                                "Successfully persisted decision event {}",
                                event.event_id
                            );
                        }

                        return Ok(persist_response);
                    }

                    // Handle specific error codes
                    match status.as_u16() {
                        401 | 403 => {
                            return Err(RuVectorError::AuthError(
                                response.text().await.unwrap_or_default(),
                            ));
                        }
                        429 => {
                            // Rate limited - check retry-after header
                            let retry_after = response
                                .headers()
                                .get("retry-after")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(60);

                            if retry_count >= self.config.max_retries {
                                return Err(RuVectorError::RateLimited {
                                    retry_after_seconds: retry_after,
                                });
                            }

                            warn!("Rate limited, waiting {}s before retry", retry_after);
                            tokio::time::sleep(Duration::from_secs(retry_after)).await;
                        }
                        503 => {
                            if retry_count >= self.config.max_retries {
                                return Err(RuVectorError::ServiceUnavailable(
                                    response.text().await.unwrap_or_default(),
                                ));
                            }
                            // Fall through to retry logic
                        }
                        _ => {
                            let error_body = response.text().await.unwrap_or_default();
                            if retry_count >= self.config.max_retries {
                                return Err(RuVectorError::PersistenceError(format!(
                                    "HTTP {}: {}",
                                    status, error_body
                                )));
                            }
                        }
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        if retry_count >= self.config.max_retries {
                            return Err(RuVectorError::Timeout(format!(
                                "Request timed out after {}ms",
                                self.config.timeout_ms
                            )));
                        }
                    } else if retry_count >= self.config.max_retries {
                        return Err(RuVectorError::NetworkError(e));
                    }

                    error!("Request failed: {}, retrying...", e);
                }
            }

            // Exponential backoff
            retry_count += 1;
            debug!(
                "Retry {} of {}, waiting {}ms",
                retry_count, self.config.max_retries, delay
            );
            tokio::time::sleep(Duration::from_millis(delay)).await;
            delay = (delay * 2).min(self.config.max_retry_delay_ms);
        }
    }

    /// Retrieve previous schema version for compatibility check
    ///
    /// This is used to get the previous schema content for compatibility validation.
    pub async fn get_schema_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<String>, RuVectorError> {
        let url = format!(
            "{}/api/v1/schemas/{}/{}/versions/{}",
            self.config.base_url, namespace, name, version
        );

        if self.config.enable_logging {
            debug!(
                "Fetching schema version: {}/{} v{}",
                namespace, name, version
            );
        }

        let mut request = self.client.get(&url);

        if let Some(ref api_key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request.send().await?;

        match response.status().as_u16() {
            200 => {
                let schema_response: SchemaVersionResponse = response
                    .json()
                    .await
                    .map_err(|e| RuVectorError::InvalidResponse(e.to_string()))?;

                Ok(Some(schema_response.content))
            }
            404 => Ok(None),
            401 | 403 => Err(RuVectorError::AuthError(
                response.text().await.unwrap_or_default(),
            )),
            _ => Err(RuVectorError::PersistenceError(format!(
                "Failed to fetch schema: HTTP {}",
                response.status()
            ))),
        }
    }

    /// Get the latest schema version
    pub async fn get_latest_schema(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<Option<SchemaVersionResponse>, RuVectorError> {
        let url = format!(
            "{}/api/v1/schemas/{}/{}/latest",
            self.config.base_url, namespace, name
        );

        if self.config.enable_logging {
            debug!("Fetching latest schema: {}/{}", namespace, name);
        }

        let mut request = self.client.get(&url);

        if let Some(ref api_key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request.send().await?;

        match response.status().as_u16() {
            200 => {
                let schema_response: SchemaVersionResponse = response
                    .json()
                    .await
                    .map_err(|e| RuVectorError::InvalidResponse(e.to_string()))?;

                Ok(Some(schema_response))
            }
            404 => Ok(None),
            401 | 403 => Err(RuVectorError::AuthError(
                response.text().await.unwrap_or_default(),
            )),
            _ => Err(RuVectorError::PersistenceError(format!(
                "Failed to fetch latest schema: HTTP {}",
                response.status()
            ))),
        }
    }

    /// List all versions of a schema
    pub async fn list_schema_versions(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<Vec<String>, RuVectorError> {
        let url = format!(
            "{}/api/v1/schemas/{}/{}/versions",
            self.config.base_url, namespace, name
        );

        let mut request = self.client.get(&url);

        if let Some(ref api_key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request.send().await?;

        match response.status().as_u16() {
            200 => {
                let versions: Vec<String> = response
                    .json()
                    .await
                    .map_err(|e| RuVectorError::InvalidResponse(e.to_string()))?;

                Ok(versions)
            }
            404 => Ok(Vec::new()),
            401 | 403 => Err(RuVectorError::AuthError(
                response.text().await.unwrap_or_default(),
            )),
            _ => Err(RuVectorError::PersistenceError(format!(
                "Failed to list versions: HTTP {}",
                response.status()
            ))),
        }
    }

    /// Check health of ruvector-service
    pub async fn health_check(&self) -> Result<HealthResponse, RuVectorError> {
        let url = format!("{}/health", self.config.base_url);

        let response = self.client.get(&url).send().await?;

        if response.status().is_success() {
            let health: HealthResponse = response
                .json()
                .await
                .map_err(|e| RuVectorError::InvalidResponse(e.to_string()))?;

            Ok(health)
        } else {
            Err(RuVectorError::ServiceUnavailable(format!(
                "Health check failed: HTTP {}",
                response.status()
            )))
        }
    }

    /// Batch persist multiple decision events
    pub async fn persist_batch(&self, events: &[DecisionEvent]) -> Result<Vec<PersistResponse>, RuVectorError> {
        let url = format!("{}/api/v1/decision-events/batch", self.config.base_url);

        if self.config.enable_logging {
            info!("Persisting batch of {} decision events", events.len());
        }

        let mut request = self.client.post(&url).json(events);

        if let Some(ref api_key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request.send().await?;

        if response.status().is_success() {
            let results: Vec<PersistResponse> = response
                .json()
                .await
                .map_err(|e| RuVectorError::InvalidResponse(e.to_string()))?;

            Ok(results)
        } else {
            let error_body = response.text().await.unwrap_or_default();
            Err(RuVectorError::PersistenceError(format!(
                "Batch persist failed: {}",
                error_body
            )))
        }
    }
}

impl Default for RuVectorClient {
    fn default() -> Self {
        Self::new(RuVectorClientConfig::default()).expect("Failed to create default client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SchemaFormat, ValidationMetrics, ValidationResult};
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_event() -> DecisionEvent {
        let result = ValidationResult {
            validation_id: Uuid::new_v4(),
            is_valid: true,
            detected_format: SchemaFormat::JsonSchema,
            errors: Vec::new(),
            warnings: Vec::new(),
            compatibility: None,
            metrics: ValidationMetrics::default(),
            content_hash: "test_hash".to_string(),
            validated_at: Utc::now(),
        };

        DecisionEvent::from_validation(
            &result,
            Some("test".to_string()),
            Some("schema".to_string()),
            Some("1.0.0".to_string()),
        )
    }

    #[test]
    fn test_client_config_default() {
        let config = RuVectorClientConfig::default();
        assert_eq!(config.base_url, "http://localhost:8080");
        assert_eq!(config.timeout_ms, 30_000);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_client_creation() {
        let client = RuVectorClient::with_url("http://localhost:8080");
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_with_api_key() {
        let config = RuVectorClientConfig {
            base_url: "http://localhost:8080".to_string(),
            api_key: Some("test-api-key".to_string()),
            ..Default::default()
        };

        let client = RuVectorClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_decision_event_creation() {
        let event = create_test_event();

        assert!(!event.event_id.is_nil());
        assert_eq!(event.agent_id, "schema-validation-agent");
        assert_eq!(event.namespace, Some("test".to_string()));
        assert_eq!(event.name, Some("schema".to_string()));
        assert!(event.passed);
    }

    #[test]
    fn test_persist_response_deserialization() {
        let json = r#"{
            "success": true,
            "event_id": "123e4567-e89b-12d3-a456-426614174000",
            "persisted_at": "2024-01-01T00:00:00Z",
            "warnings": []
        }"#;

        let response: PersistResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert!(response.warnings.is_empty());
    }

    #[test]
    fn test_health_response_deserialization() {
        let json = r#"{
            "status": "healthy",
            "version": "1.0.0",
            "uptime_seconds": 3600,
            "components": {
                "database": "healthy",
                "cache": "healthy"
            }
        }"#;

        let response: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "healthy");
        assert_eq!(response.version, "1.0.0");
        assert_eq!(response.uptime_seconds, 3600);
        assert!(response.components.contains_key("database"));
    }

    #[test]
    fn test_schema_version_response_deserialization() {
        let json = r#"{
            "content": "{\"type\": \"object\"}",
            "version": "1.0.0",
            "content_hash": "abc123",
            "created_at": "2024-01-01T00:00:00Z"
        }"#;

        let response: SchemaVersionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.version, "1.0.0");
        assert_eq!(response.content_hash, "abc123");
    }

    // Note: Integration tests with actual ruvector-service would be in a separate test module
    // using wiremock or similar to mock the HTTP responses
}
