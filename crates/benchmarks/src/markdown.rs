//! Markdown generation utilities for benchmark reports

use crate::BenchmarkResult;
use chrono::Utc;

/// Generate a markdown table from benchmark results
pub fn generate_table(results: &[BenchmarkResult]) -> String {
    if results.is_empty() {
        return "No benchmark results available.\n".to_string();
    }

    let mut output = String::new();

    // Header
    output.push_str("| Target ID | Metrics | Timestamp |\n");
    output.push_str("|-----------|---------|----------|\n");

    // Rows
    for result in results {
        let metrics_str = serde_json::to_string(&result.metrics)
            .unwrap_or_else(|_| "{}".to_string());
        let timestamp_str = result.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();

        output.push_str(&format!(
            "| {} | `{}` | {} |\n",
            result.target_id,
            metrics_str.replace('|', "\\|"), // Escape pipe characters
            timestamp_str
        ));
    }

    output
}

/// Generate a full markdown summary report
pub fn generate_summary(results: &[BenchmarkResult]) -> String {
    let mut output = String::new();

    // Title
    output.push_str("# Schema Registry Benchmark Summary\n\n");

    // Metadata
    output.push_str(&format!(
        "**Generated:** {}\n\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));
    output.push_str(&format!("**Total Benchmarks:** {}\n\n", results.len()));

    // Results table
    output.push_str("## Benchmark Results\n\n");
    output.push_str(&generate_table(results));

    // Detailed results
    output.push_str("\n## Detailed Metrics\n\n");
    for result in results {
        output.push_str(&format!("### {}\n\n", result.target_id));
        output.push_str(&format!(
            "**Timestamp:** {}\n\n",
            result.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        output.push_str("**Metrics:**\n\n");
        output.push_str("```json\n");
        output.push_str(&serde_json::to_string_pretty(&result.metrics).unwrap_or_else(|_| "{}".to_string()));
        output.push_str("\n```\n\n");
    }

    output
}

/// Generate a compact summary for quick reference
pub fn generate_compact_summary(results: &[BenchmarkResult]) -> String {
    let mut output = String::new();

    output.push_str("# Benchmark Summary\n\n");
    output.push_str(&format!("Total: {} | ", results.len()));
    output.push_str(&format!("Generated: {}\n\n", Utc::now().format("%Y-%m-%d %H:%M:%S")));

    for result in results {
        output.push_str(&format!("- **{}**: ", result.target_id));
        if let Some(duration) = result.metrics.get("duration_ms") {
            output.push_str(&format!("{}ms", duration));
        } else {
            output.push_str("see details");
        }
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_result(id: &str) -> BenchmarkResult {
        BenchmarkResult::new(
            id.to_string(),
            json!({
                "duration_ms": 100,
                "throughput": 1000
            }),
        )
    }

    #[test]
    fn test_generate_table_empty() {
        let results = vec![];
        let table = generate_table(&results);
        assert_eq!(table, "No benchmark results available.\n");
    }

    #[test]
    fn test_generate_table_single_result() {
        let results = vec![create_test_result("test_bench")];
        let table = generate_table(&results);

        assert!(table.contains("Target ID"));
        assert!(table.contains("Metrics"));
        assert!(table.contains("Timestamp"));
        assert!(table.contains("test_bench"));
    }

    #[test]
    fn test_generate_table_multiple_results() {
        let results = vec![
            create_test_result("bench1"),
            create_test_result("bench2"),
        ];
        let table = generate_table(&results);

        assert!(table.contains("bench1"));
        assert!(table.contains("bench2"));
        assert_eq!(table.matches("| bench").count(), 2);
    }

    #[test]
    fn test_generate_summary_structure() {
        let results = vec![create_test_result("test")];
        let summary = generate_summary(&results);

        assert!(summary.contains("# Schema Registry Benchmark Summary"));
        assert!(summary.contains("**Generated:**"));
        assert!(summary.contains("**Total Benchmarks:** 1"));
        assert!(summary.contains("## Benchmark Results"));
        assert!(summary.contains("## Detailed Metrics"));
    }

    #[test]
    fn test_generate_summary_includes_json() {
        let results = vec![create_test_result("test")];
        let summary = generate_summary(&results);

        assert!(summary.contains("```json"));
        assert!(summary.contains("duration_ms"));
        assert!(summary.contains("throughput"));
    }

    #[test]
    fn test_generate_compact_summary() {
        let results = vec![create_test_result("test")];
        let summary = generate_compact_summary(&results);

        assert!(summary.contains("# Benchmark Summary"));
        assert!(summary.contains("Total: 1"));
        assert!(summary.contains("test"));
        assert!(summary.contains("100ms"));
    }

    #[test]
    fn test_table_escapes_pipe_characters() {
        let result = BenchmarkResult::new(
            "test".to_string(),
            json!({"key": "value|with|pipes"}),
        );
        let table = generate_table(&[result]);

        // Should escape pipes in JSON values
        assert!(table.contains("\\|"));
    }

    #[test]
    fn test_summary_handles_empty_results() {
        let results = vec![];
        let summary = generate_summary(&results);

        assert!(summary.contains("**Total Benchmarks:** 0"));
        assert!(summary.contains("No benchmark results available"));
    }
}
