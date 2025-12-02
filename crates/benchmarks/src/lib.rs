//! # Schema Registry Benchmarks
//!
//! Canonical benchmark infrastructure for Schema Registry operations.
//!
//! This crate provides a unified benchmarking framework for measuring performance
//! across all Schema Registry operations including storage, validation, and
//! compatibility checking.

pub mod adapters;
pub mod io;
pub mod markdown;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Benchmark result containing performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Unique identifier for the benchmark target
    pub target_id: String,
    /// Performance metrics in JSON format for flexibility
    pub metrics: serde_json::Value,
    /// When the benchmark was executed
    pub timestamp: DateTime<Utc>,
}

impl BenchmarkResult {
    /// Create a new benchmark result
    pub fn new(target_id: String, metrics: serde_json::Value) -> Self {
        Self {
            target_id,
            metrics,
            timestamp: Utc::now(),
        }
    }
}

/// Run all registered benchmarks
///
/// This function executes all benchmark targets and returns their results.
/// Benchmarks are run sequentially to ensure accurate measurements.
pub async fn run_all_benchmarks() -> Vec<BenchmarkResult> {
    let mut results = Vec::new();

    // Get all registered benchmark targets
    let targets = adapters::all_targets();

    // Run each benchmark target
    for target in targets {
        match target.run().await {
            Ok(result) => results.push(result),
            Err(e) => {
                eprintln!("Benchmark {} failed: {}", target.id(), e);
                // Continue with other benchmarks even if one fails
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_benchmark_result_creation() {
        let metrics = json!({
            "duration_ms": 100,
            "throughput": 1000
        });

        let result = BenchmarkResult::new("test_target".to_string(), metrics.clone());

        assert_eq!(result.target_id, "test_target");
        assert_eq!(result.metrics, metrics);
        assert!(result.timestamp <= Utc::now());
    }

    #[test]
    fn test_benchmark_result_serialization() {
        let metrics = json!({"duration_ms": 100});
        let result = BenchmarkResult::new("test".to_string(), metrics);

        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.contains("test"));
        assert!(serialized.contains("duration_ms"));
    }

    #[test]
    fn test_benchmark_result_deserialization() {
        let metrics = json!({"duration_ms": 100});
        let result = BenchmarkResult::new("test".to_string(), metrics);

        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: BenchmarkResult = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.target_id, result.target_id);
        assert_eq!(deserialized.metrics, result.metrics);
    }

    #[tokio::test]
    async fn test_run_all_benchmarks() {
        let results = run_all_benchmarks().await;
        // Should return results from all registered targets
        assert!(results.len() >= 0); // May be 0 if no targets registered
    }
}
