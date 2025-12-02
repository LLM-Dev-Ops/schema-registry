//! Storage operation benchmarks

use super::BenchTarget;
use crate::BenchmarkResult;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

/// Benchmark for storage operations
pub struct StorageBenchmark;

impl StorageBenchmark {
    /// Create a new storage benchmark
    pub fn new() -> Self {
        Self
    }

    /// Simulate a storage write operation
    async fn bench_write(&self) -> f64 {
        let start = Instant::now();

        // Simulate storage write (in production, this would use actual storage)
        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }

    /// Simulate a storage read operation
    async fn bench_read(&self) -> f64 {
        let start = Instant::now();

        // Simulate storage read (in production, this would use actual storage)
        tokio::time::sleep(tokio::time::Duration::from_micros(50)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }

    /// Simulate a storage update operation
    async fn bench_update(&self) -> f64 {
        let start = Instant::now();

        // Simulate storage update (in production, this would use actual storage)
        tokio::time::sleep(tokio::time::Duration::from_micros(80)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }
}

impl Default for StorageBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BenchTarget for StorageBenchmark {
    fn id(&self) -> &str {
        "storage_operations"
    }

    fn description(&self) -> &str {
        "Benchmarks for schema storage operations (read, write, update)"
    }

    async fn run(&self) -> Result<BenchmarkResult> {
        // Run multiple iterations for more accurate measurements
        let iterations = 10;
        let mut write_times = Vec::new();
        let mut read_times = Vec::new();
        let mut update_times = Vec::new();

        for _ in 0..iterations {
            write_times.push(self.bench_write().await);
            read_times.push(self.bench_read().await);
            update_times.push(self.bench_update().await);
        }

        // Calculate statistics
        let avg_write = write_times.iter().sum::<f64>() / write_times.len() as f64;
        let avg_read = read_times.iter().sum::<f64>() / read_times.len() as f64;
        let avg_update = update_times.iter().sum::<f64>() / update_times.len() as f64;

        let metrics = json!({
            "iterations": iterations,
            "write": {
                "avg_ms": format!("{:.3}", avg_write),
                "min_ms": format!("{:.3}", write_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", write_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            },
            "read": {
                "avg_ms": format!("{:.3}", avg_read),
                "min_ms": format!("{:.3}", read_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", read_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            },
            "update": {
                "avg_ms": format!("{:.3}", avg_update),
                "min_ms": format!("{:.3}", update_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", update_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            }
        });

        Ok(BenchmarkResult::new(self.id().to_string(), metrics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_benchmark_new() {
        let bench = StorageBenchmark::new();
        assert_eq!(bench.id(), "storage_operations");
    }

    #[test]
    fn test_storage_benchmark_default() {
        let bench = StorageBenchmark::default();
        assert_eq!(bench.id(), "storage_operations");
    }

    #[test]
    fn test_storage_benchmark_description() {
        let bench = StorageBenchmark::new();
        assert!(!bench.description().is_empty());
        assert!(bench.description().contains("storage"));
    }

    #[tokio::test]
    async fn test_storage_benchmark_run() {
        let bench = StorageBenchmark::new();
        let result = bench.run().await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.target_id, "storage_operations");

        // Verify metrics structure
        assert!(result.metrics.get("iterations").is_some());
        assert!(result.metrics.get("write").is_some());
        assert!(result.metrics.get("read").is_some());
        assert!(result.metrics.get("update").is_some());
    }

    #[tokio::test]
    async fn test_storage_benchmark_metrics_format() {
        let bench = StorageBenchmark::new();
        let result = bench.run().await.unwrap();

        // Check write metrics
        let write = result.metrics.get("write").unwrap();
        assert!(write.get("avg_ms").is_some());
        assert!(write.get("min_ms").is_some());
        assert!(write.get("max_ms").is_some());

        // Check read metrics
        let read = result.metrics.get("read").unwrap();
        assert!(read.get("avg_ms").is_some());
        assert!(read.get("min_ms").is_some());
        assert!(read.get("max_ms").is_some());

        // Check update metrics
        let update = result.metrics.get("update").unwrap();
        assert!(update.get("avg_ms").is_some());
        assert!(update.get("min_ms").is_some());
        assert!(update.get("max_ms").is_some());
    }

    #[tokio::test]
    async fn test_bench_write() {
        let bench = StorageBenchmark::new();
        let duration = bench.bench_write().await;
        assert!(duration > 0.0);
    }

    #[tokio::test]
    async fn test_bench_read() {
        let bench = StorageBenchmark::new();
        let duration = bench.bench_read().await;
        assert!(duration > 0.0);
    }

    #[tokio::test]
    async fn test_bench_update() {
        let bench = StorageBenchmark::new();
        let duration = bench.bench_update().await;
        assert!(duration > 0.0);
    }
}
