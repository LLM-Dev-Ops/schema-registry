//! Benchmark adapters for Schema Registry operations

pub mod storage;
pub mod validation;
pub mod compatibility;

use async_trait::async_trait;
use crate::BenchmarkResult;
use anyhow::Result;

/// Trait for benchmark targets
///
/// Each benchmark target implements this trait to provide a standardized
/// interface for executing benchmarks and collecting results.
#[async_trait]
pub trait BenchTarget: Send + Sync {
    /// Unique identifier for this benchmark target
    fn id(&self) -> &str;

    /// Human-readable description of what this benchmark measures
    fn description(&self) -> &str;

    /// Run the benchmark and return results
    async fn run(&self) -> Result<BenchmarkResult>;
}

/// Get all registered benchmark targets
///
/// This function returns a collection of all available benchmark targets
/// that can be executed.
pub fn all_targets() -> Vec<Box<dyn BenchTarget>> {
    vec![
        Box::new(storage::StorageBenchmark::new()),
        Box::new(validation::ValidationBenchmark::new()),
        Box::new(compatibility::CompatibilityBenchmark::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_targets_returns_targets() {
        let targets = all_targets();
        assert!(!targets.is_empty());
        assert_eq!(targets.len(), 3);
    }

    #[test]
    fn test_all_targets_have_unique_ids() {
        let targets = all_targets();
        let ids: Vec<&str> = targets.iter().map(|t| t.id()).collect();

        // Check for uniqueness
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        sorted_ids.dedup();

        assert_eq!(ids.len(), sorted_ids.len(), "Benchmark target IDs must be unique");
    }

    #[test]
    fn test_all_targets_have_descriptions() {
        let targets = all_targets();

        for target in targets {
            assert!(!target.id().is_empty(), "Target ID should not be empty");
            assert!(!target.description().is_empty(), "Target description should not be empty");
        }
    }
}
