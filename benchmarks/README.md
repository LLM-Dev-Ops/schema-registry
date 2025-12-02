# Schema Registry Benchmarks

This directory contains the benchmark infrastructure and results for the LLM Schema Registry.

## Quick Start

Run benchmarks using the CLI:

```bash
# Run all benchmarks
schema-cli benchmark run

# List available benchmarks
schema-cli benchmark list
```

## Directory Structure

```
benchmarks/
├── output/              # Benchmark results
│   ├── summary.md      # Latest benchmark summary
│   └── raw/            # Raw JSON results
│       ├── latest.json           # Most recent run
│       └── benchmarks_*.json     # Historical results
└── README.md           # This file
```

## Benchmark Implementation

The benchmark implementation is located in `/crates/benchmarks/`. See the [crate README](/crates/benchmarks/README.md) for detailed documentation.

## Available Benchmarks

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

## Output

Each benchmark run produces:

1. **summary.md** - Human-readable markdown report with:
   - Timestamp and metadata
   - Results table
   - Detailed metrics for each target

2. **Raw JSON files** - Machine-readable results:
   - `latest.json` - Most recent results
   - `benchmarks_YYYYMMDD_HHMMSS.json` - Timestamped historical data

## Metrics

Each benchmark captures:
- Average execution time (ms)
- Minimum execution time (ms)
- Maximum execution time (ms)
- Number of iterations

Multiple iterations ensure statistical reliability.

## Integration

Benchmarks are integrated with the CLI via the `benchmark` subcommand:

```bash
schema-cli benchmark --help
```

For programmatic usage, see the [benchmarks crate documentation](/crates/benchmarks/README.md).
