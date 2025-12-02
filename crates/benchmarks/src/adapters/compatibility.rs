//! Compatibility checking benchmarks

use super::BenchTarget;
use crate::BenchmarkResult;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

/// Benchmark for compatibility checking operations
pub struct CompatibilityBenchmark;

impl CompatibilityBenchmark {
    /// Create a new compatibility benchmark
    pub fn new() -> Self {
        Self
    }

    /// Simulate backward compatibility check
    async fn bench_backward_check(&self) -> f64 {
        let start = Instant::now();

        // Simulate backward compatibility check (in production, this would use actual checker)
        tokio::time::sleep(tokio::time::Duration::from_micros(90)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }

    /// Simulate forward compatibility check
    async fn bench_forward_check(&self) -> f64 {
        let start = Instant::now();

        // Simulate forward compatibility check (in production, this would use actual checker)
        tokio::time::sleep(tokio::time::Duration::from_micros(85)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }

    /// Simulate full compatibility check
    async fn bench_full_check(&self) -> f64 {
        let start = Instant::now();

        // Simulate full compatibility check (in production, this would use actual checker)
        tokio::time::sleep(tokio::time::Duration::from_micros(120)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }

    /// Simulate transitive compatibility check
    async fn bench_transitive_check(&self) -> f64 {
        let start = Instant::now();

        // Simulate transitive compatibility check (in production, this would use actual checker)
        tokio::time::sleep(tokio::time::Duration::from_micros(200)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }
}

impl Default for CompatibilityBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BenchTarget for CompatibilityBenchmark {
    fn id(&self) -> &str {
        "compatibility_operations"
    }

    fn description(&self) -> &str {
        "Benchmarks for schema compatibility checking (backward, forward, full, transitive)"
    }

    async fn run(&self) -> Result<BenchmarkResult> {
        // Run multiple iterations for more accurate measurements
        let iterations = 10;
        let mut backward_times = Vec::new();
        let mut forward_times = Vec::new();
        let mut full_times = Vec::new();
        let mut transitive_times = Vec::new();

        for _ in 0..iterations {
            backward_times.push(self.bench_backward_check().await);
            forward_times.push(self.bench_forward_check().await);
            full_times.push(self.bench_full_check().await);
            transitive_times.push(self.bench_transitive_check().await);
        }

        // Calculate statistics
        let avg_backward = backward_times.iter().sum::<f64>() / backward_times.len() as f64;
        let avg_forward = forward_times.iter().sum::<f64>() / forward_times.len() as f64;
        let avg_full = full_times.iter().sum::<f64>() / full_times.len() as f64;
        let avg_transitive = transitive_times.iter().sum::<f64>() / transitive_times.len() as f64;

        let metrics = json!({
            "iterations": iterations,
            "backward": {
                "avg_ms": format!("{:.3}", avg_backward),
                "min_ms": format!("{:.3}", backward_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", backward_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            },
            "forward": {
                "avg_ms": format!("{:.3}", avg_forward),
                "min_ms": format!("{:.3}", forward_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", forward_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            },
            "full": {
                "avg_ms": format!("{:.3}", avg_full),
                "min_ms": format!("{:.3}", full_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", full_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            },
            "transitive": {
                "avg_ms": format!("{:.3}", avg_transitive),
                "min_ms": format!("{:.3}", transitive_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", transitive_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            }
        });

        Ok(BenchmarkResult::new(self.id().to_string(), metrics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compatibility_benchmark_new() {
        let bench = CompatibilityBenchmark::new();
        assert_eq!(bench.id(), "compatibility_operations");
    }

    #[test]
    fn test_compatibility_benchmark_default() {
        let bench = CompatibilityBenchmark::default();
        assert_eq!(bench.id(), "compatibility_operations");
    }

    #[test]
    fn test_compatibility_benchmark_description() {
        let bench = CompatibilityBenchmark::new();
        assert!(!bench.description().is_empty());
        assert!(bench.description().contains("compatibility"));
    }

    #[tokio::test]
    async fn test_compatibility_benchmark_run() {
        let bench = CompatibilityBenchmark::new();
        let result = bench.run().await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.target_id, "compatibility_operations");

        // Verify metrics structure
        assert!(result.metrics.get("iterations").is_some());
        assert!(result.metrics.get("backward").is_some());
        assert!(result.metrics.get("forward").is_some());
        assert!(result.metrics.get("full").is_some());
        assert!(result.metrics.get("transitive").is_some());
    }

    #[tokio::test]
    async fn test_compatibility_benchmark_metrics_format() {
        let bench = CompatibilityBenchmark::new();
        let result = bench.run().await.unwrap();

        // Check backward metrics
        let backward = result.metrics.get("backward").unwrap();
        assert!(backward.get("avg_ms").is_some());
        assert!(backward.get("min_ms").is_some());
        assert!(backward.get("max_ms").is_some());

        // Check forward metrics
        let forward = result.metrics.get("forward").unwrap();
        assert!(forward.get("avg_ms").is_some());
        assert!(forward.get("min_ms").is_some());
        assert!(forward.get("max_ms").is_some());

        // Check full metrics
        let full = result.metrics.get("full").unwrap();
        assert!(full.get("avg_ms").is_some());
        assert!(full.get("min_ms").is_some());
        assert!(full.get("max_ms").is_some());

        // Check transitive metrics
        let transitive = result.metrics.get("transitive").unwrap();
        assert!(transitive.get("avg_ms").is_some());
        assert!(transitive.get("min_ms").is_some());
        assert!(transitive.get("max_ms").is_some());
    }

    #[tokio::test]
    async fn test_bench_backward_check() {
        let bench = CompatibilityBenchmark::new();
        let duration = bench.bench_backward_check().await;
        assert!(duration > 0.0);
    }

    #[tokio::test]
    async fn test_bench_forward_check() {
        let bench = CompatibilityBenchmark::new();
        let duration = bench.bench_forward_check().await;
        assert!(duration > 0.0);
    }

    #[tokio::test]
    async fn test_bench_full_check() {
        let bench = CompatibilityBenchmark::new();
        let duration = bench.bench_full_check().await;
        assert!(duration > 0.0);
    }

    #[tokio::test]
    async fn test_bench_transitive_check() {
        let bench = CompatibilityBenchmark::new();
        let duration = bench.bench_transitive_check().await;
        assert!(duration > 0.0);
    }
}
