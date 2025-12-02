# Schema Registry Benchmark Summary

**Generated:** Initial setup

**Total Benchmarks:** 0

## Benchmark Results

No benchmark results available.

## About

This directory contains benchmark results for Schema Registry operations. Run benchmarks using:

```bash
schema-cli benchmark run
```

Available commands:
- `schema-cli benchmark run` - Execute all benchmarks and generate reports
- `schema-cli benchmark list` - List available benchmark targets

## Output Structure

- `summary.md` - This file, containing the latest benchmark summary
- `raw/` - Directory containing raw JSON benchmark results
  - `latest.json` - Most recent benchmark results
  - `benchmarks_YYYYMMDD_HHMMSS.json` - Timestamped historical results

## Benchmark Targets

The following operations are benchmarked:

### Storage Operations
- Schema write operations
- Schema read operations
- Schema update operations

### Validation Operations
- JSON Schema validation
- Avro schema validation
- Protobuf schema validation

### Compatibility Operations
- Backward compatibility checking
- Forward compatibility checking
- Full compatibility checking
- Transitive compatibility checking

## Metrics

Each benchmark captures:
- Average execution time (ms)
- Minimum execution time (ms)
- Maximum execution time (ms)
- Number of iterations

Results are collected across multiple iterations to ensure statistical reliability.
