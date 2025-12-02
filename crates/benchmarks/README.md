# Schema Registry Benchmarks

Canonical benchmark infrastructure for measuring performance across all Schema Registry operations.

## Overview

This crate provides a unified benchmarking framework that measures the performance of:

- **Storage Operations**: Schema read, write, and update operations
- **Validation Operations**: Schema validation across JSON Schema, Avro, and Protobuf formats
- **Compatibility Operations**: Backward, forward, full, and transitive compatibility checking

## Architecture

The benchmark infrastructure follows a modular design:

```
benchmarks/
├── src/
│   ├── lib.rs                    # Main library with BenchmarkResult and run_all_benchmarks()
│   ├── adapters/                 # Benchmark target adapters
│   │   ├── mod.rs               # BenchTarget trait and registry
│   │   ├── storage.rs           # Storage operation benchmarks
│   │   ├── validation.rs        # Validation operation benchmarks
│   │   └── compatibility.rs     # Compatibility checking benchmarks
│   ├── markdown.rs              # Markdown report generation utilities
│   └── io.rs                    # I/O utilities for reading/writing results
└── output/                       # Benchmark results output
    ├── summary.md               # Latest summary report
    └── raw/                     # Raw JSON results
        ├── latest.json          # Most recent results
        └── benchmarks_*.json    # Timestamped historical results
```

## Core Types

### BenchmarkResult

The canonical result structure for all benchmarks:

```rust
pub struct BenchmarkResult {
    /// Unique identifier for the benchmark target
    pub target_id: String,

    /// Performance metrics in JSON format for flexibility
    pub metrics: serde_json::Value,

    /// When the benchmark was executed
    pub timestamp: DateTime<Utc>,
}
```

### BenchTarget Trait

All benchmark targets implement this trait:

```rust
#[async_trait]
pub trait BenchTarget: Send + Sync {
    /// Unique identifier for this benchmark target
    fn id(&self) -> &str;

    /// Human-readable description of what this benchmark measures
    fn description(&self) -> &str;

    /// Run the benchmark and return results
    async fn run(&self) -> Result<BenchmarkResult>;
}
```

## Usage

### Running Benchmarks via CLI

```bash
# Run all benchmarks
schema-cli benchmark run

# Run with custom output directory
schema-cli benchmark run --output-dir ./my-benchmarks

# Dry run (don't write results to disk)
schema-cli benchmark run --dry-run

# List available benchmark targets
schema-cli benchmark list
```

### Programmatic Usage

```rust
use schema_registry_benchmarks::{run_all_benchmarks, io, markdown};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Run all benchmarks
    let results = run_all_benchmarks().await;

    // Generate markdown summary
    let summary = markdown::generate_summary(&results);

    // Write results to disk
    io::write_results(&results, &summary)?;

    Ok(())
}
```

## Benchmark Targets

### Storage Operations (`storage_operations`)

Measures performance of schema storage operations:
- **Write**: Time to store a new schema
- **Read**: Time to retrieve an existing schema
- **Update**: Time to update an existing schema

### Validation Operations (`validation_operations`)

Measures validation performance across formats:
- **JSON Schema**: Validation time for JSON Schema documents
- **Avro**: Validation time for Avro schemas
- **Protobuf**: Validation time for Protocol Buffer schemas

### Compatibility Operations (`compatibility_operations`)

Measures compatibility checking performance:
- **Backward**: Backward compatibility check time
- **Forward**: Forward compatibility check time
- **Full**: Full (backward + forward) compatibility check time
- **Transitive**: Transitive compatibility check time (multiple versions)

## Output Format

### Summary Markdown

The `summary.md` file contains:
- Metadata (timestamp, total benchmarks)
- Results table with all benchmark targets
- Detailed metrics for each target

### Raw JSON

JSON files contain the complete `BenchmarkResult` array:

```json
[
  {
    "target_id": "storage_operations",
    "metrics": {
      "iterations": 10,
      "write": {
        "avg_ms": "0.105",
        "min_ms": "0.100",
        "max_ms": "0.115"
      },
      ...
    },
    "timestamp": "2025-12-02T04:57:00.000000Z"
  }
]
```

## Metrics

All benchmarks collect the following statistics across multiple iterations:
- **avg_ms**: Average execution time in milliseconds
- **min_ms**: Minimum execution time in milliseconds
- **max_ms**: Maximum execution time in milliseconds
- **iterations**: Number of iterations performed

## Adding New Benchmarks

To add a new benchmark target:

1. Create a new module in `src/adapters/`
2. Implement the `BenchTarget` trait
3. Register it in `adapters::all_targets()`

Example:

```rust
// src/adapters/my_benchmark.rs
use super::BenchTarget;
use crate::BenchmarkResult;
use async_trait::async_trait;

pub struct MyBenchmark;

impl MyBenchmark {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl BenchTarget for MyBenchmark {
    fn id(&self) -> &str {
        "my_benchmark"
    }

    fn description(&self) -> &str {
        "Description of what this benchmark measures"
    }

    async fn run(&self) -> anyhow::Result<BenchmarkResult> {
        // Implement benchmark logic
        let metrics = serde_json::json!({
            "metric1": 100,
            "metric2": 200
        });

        Ok(BenchmarkResult::new(self.id().to_string(), metrics))
    }
}

// In src/adapters/mod.rs, add to all_targets():
pub fn all_targets() -> Vec<Box<dyn BenchTarget>> {
    vec![
        // ... existing targets
        Box::new(my_benchmark::MyBenchmark::new()),
    ]
}
```

## Testing

Run tests with:

```bash
cargo test --package schema-registry-benchmarks
```

The crate includes comprehensive unit tests for:
- BenchmarkResult creation and serialization
- Markdown generation utilities
- I/O operations
- All benchmark adapters
- Target registry

## Design Principles

1. **100% Backward Compatibility**: Only add new functionality, never modify existing
2. **Canonical Structure**: All benchmarks use the same `BenchmarkResult` format
3. **Extensibility**: Easy to add new benchmark targets via the `BenchTarget` trait
4. **Flexibility**: Metrics stored as JSON for format flexibility
5. **Reliability**: Multiple iterations for statistical accuracy

## Future Enhancements

Potential future additions (all backward-compatible):
- Performance regression detection
- Historical trend analysis
- Configurable iteration counts
- Parallel benchmark execution
- Integration with actual storage/validation implementations
- Performance baselines and alerts
