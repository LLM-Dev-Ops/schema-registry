//! Validation operation benchmarks

use super::BenchTarget;
use crate::BenchmarkResult;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

/// Benchmark for validation operations
pub struct ValidationBenchmark;

impl ValidationBenchmark {
    /// Create a new validation benchmark
    pub fn new() -> Self {
        Self
    }

    /// Simulate JSON schema validation
    async fn bench_json_validation(&self) -> f64 {
        let start = Instant::now();

        // Simulate JSON schema validation (in production, this would use actual validator)
        tokio::time::sleep(tokio::time::Duration::from_micros(75)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }

    /// Simulate Avro schema validation
    async fn bench_avro_validation(&self) -> f64 {
        let start = Instant::now();

        // Simulate Avro schema validation (in production, this would use actual validator)
        tokio::time::sleep(tokio::time::Duration::from_micros(60)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }

    /// Simulate Protobuf schema validation
    async fn bench_protobuf_validation(&self) -> f64 {
        let start = Instant::now();

        // Simulate Protobuf schema validation (in production, this would use actual validator)
        tokio::time::sleep(tokio::time::Duration::from_micros(70)).await;

        start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
    }
}

impl Default for ValidationBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BenchTarget for ValidationBenchmark {
    fn id(&self) -> &str {
        "validation_operations"
    }

    fn description(&self) -> &str {
        "Benchmarks for schema validation across different formats (JSON, Avro, Protobuf)"
    }

    async fn run(&self) -> Result<BenchmarkResult> {
        // Run multiple iterations for more accurate measurements
        let iterations = 10;
        let mut json_times = Vec::new();
        let mut avro_times = Vec::new();
        let mut protobuf_times = Vec::new();

        for _ in 0..iterations {
            json_times.push(self.bench_json_validation().await);
            avro_times.push(self.bench_avro_validation().await);
            protobuf_times.push(self.bench_protobuf_validation().await);
        }

        // Calculate statistics
        let avg_json = json_times.iter().sum::<f64>() / json_times.len() as f64;
        let avg_avro = avro_times.iter().sum::<f64>() / avro_times.len() as f64;
        let avg_protobuf = protobuf_times.iter().sum::<f64>() / protobuf_times.len() as f64;

        let metrics = json!({
            "iterations": iterations,
            "json_schema": {
                "avg_ms": format!("{:.3}", avg_json),
                "min_ms": format!("{:.3}", json_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", json_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            },
            "avro": {
                "avg_ms": format!("{:.3}", avg_avro),
                "min_ms": format!("{:.3}", avro_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", avro_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            },
            "protobuf": {
                "avg_ms": format!("{:.3}", avg_protobuf),
                "min_ms": format!("{:.3}", protobuf_times.iter().cloned().fold(f64::INFINITY, f64::min)),
                "max_ms": format!("{:.3}", protobuf_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
            }
        });

        Ok(BenchmarkResult::new(self.id().to_string(), metrics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_benchmark_new() {
        let bench = ValidationBenchmark::new();
        assert_eq!(bench.id(), "validation_operations");
    }

    #[test]
    fn test_validation_benchmark_default() {
        let bench = ValidationBenchmark::default();
        assert_eq!(bench.id(), "validation_operations");
    }

    #[test]
    fn test_validation_benchmark_description() {
        let bench = ValidationBenchmark::new();
        assert!(!bench.description().is_empty());
        assert!(bench.description().contains("validation"));
    }

    #[tokio::test]
    async fn test_validation_benchmark_run() {
        let bench = ValidationBenchmark::new();
        let result = bench.run().await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.target_id, "validation_operations");

        // Verify metrics structure
        assert!(result.metrics.get("iterations").is_some());
        assert!(result.metrics.get("json_schema").is_some());
        assert!(result.metrics.get("avro").is_some());
        assert!(result.metrics.get("protobuf").is_some());
    }

    #[tokio::test]
    async fn test_validation_benchmark_metrics_format() {
        let bench = ValidationBenchmark::new();
        let result = bench.run().await.unwrap();

        // Check JSON schema metrics
        let json = result.metrics.get("json_schema").unwrap();
        assert!(json.get("avg_ms").is_some());
        assert!(json.get("min_ms").is_some());
        assert!(json.get("max_ms").is_some());

        // Check Avro metrics
        let avro = result.metrics.get("avro").unwrap();
        assert!(avro.get("avg_ms").is_some());
        assert!(avro.get("min_ms").is_some());
        assert!(avro.get("max_ms").is_some());

        // Check Protobuf metrics
        let protobuf = result.metrics.get("protobuf").unwrap();
        assert!(protobuf.get("avg_ms").is_some());
        assert!(protobuf.get("min_ms").is_some());
        assert!(protobuf.get("max_ms").is_some());
    }

    #[tokio::test]
    async fn test_bench_json_validation() {
        let bench = ValidationBenchmark::new();
        let duration = bench.bench_json_validation().await;
        assert!(duration > 0.0);
    }

    #[tokio::test]
    async fn test_bench_avro_validation() {
        let bench = ValidationBenchmark::new();
        let duration = bench.bench_avro_validation().await;
        assert!(duration > 0.0);
    }

    #[tokio::test]
    async fn test_bench_protobuf_validation() {
        let bench = ValidationBenchmark::new();
        let duration = bench.bench_protobuf_validation().await;
        assert!(duration > 0.0);
    }
}
