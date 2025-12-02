# Benchmark Quick Reference

## Commands

```bash
# Run all benchmarks
schema-cli benchmark run

# Run with custom output directory
schema-cli benchmark run --output-dir ./custom-output

# Dry run (no files written)
schema-cli benchmark run --dry-run

# Output in different formats
schema-cli benchmark run -o json    # JSON format
schema-cli benchmark run -o yaml    # YAML format
schema-cli benchmark run -o table   # Table format (default)

# List available benchmarks
schema-cli benchmark list
schema-cli benchmark list -o json
```

## File Locations

### Source Code
- **Crate:** `/crates/benchmarks/`
- **Main:** `/crates/benchmarks/src/lib.rs`
- **Adapters:** `/crates/benchmarks/src/adapters/`
- **CLI:** `/crates/schema-registry-cli/src/commands/benchmark.rs`

### Output Files
- **Summary:** `/benchmarks/output/summary.md`
- **Latest:** `/benchmarks/output/raw/latest.json`
- **Historical:** `/benchmarks/output/raw/benchmarks_YYYYMMDD_HHMMSS.json`

## Benchmark Targets

| Target ID | Description | Operations |
|-----------|-------------|------------|
| `storage_operations` | Storage benchmarks | write, read, update |
| `validation_operations` | Validation benchmarks | json_schema, avro, protobuf |
| `compatibility_operations` | Compatibility benchmarks | backward, forward, full, transitive |

## Metrics Format

Each benchmark produces:

```json
{
  "target_id": "storage_operations",
  "metrics": {
    "iterations": 10,
    "write": {
      "avg_ms": "0.105",
      "min_ms": "0.100",
      "max_ms": "0.115"
    }
  },
  "timestamp": "2025-12-02T04:57:00Z"
}
```

## Adding New Benchmarks

1. Create new adapter in `/crates/benchmarks/src/adapters/`
2. Implement `BenchTarget` trait:
   ```rust
   #[async_trait]
   impl BenchTarget for MyBenchmark {
       fn id(&self) -> &str { "my_benchmark" }
       fn description(&self) -> &str { "..." }
       async fn run(&self) -> Result<BenchmarkResult> { ... }
   }
   ```
3. Register in `adapters::all_targets()`
4. Add tests

## Programmatic Usage

```rust
use schema_registry_benchmarks::{run_all_benchmarks, io, markdown};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Run benchmarks
    let results = run_all_benchmarks().await;

    // Generate report
    let summary = markdown::generate_summary(&results);

    // Write results
    io::write_results(&results, &summary)?;

    Ok(())
}
```

## Documentation

- **Crate README:** `/crates/benchmarks/README.md`
- **Benchmarks README:** `/benchmarks/README.md`
- **Implementation Report:** `/BENCHMARK_IMPLEMENTATION_REPORT.md`
- **This file:** `/benchmarks/QUICK_REFERENCE.md`
