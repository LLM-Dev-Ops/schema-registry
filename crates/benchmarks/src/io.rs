//! I/O utilities for benchmark results

use crate::BenchmarkResult;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Write benchmark results to a JSON file
pub fn write_json(results: &[BenchmarkResult], path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(results)
        .context("Failed to serialize benchmark results")?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    fs::write(path, json)
        .with_context(|| format!("Failed to write benchmark results to {}", path.display()))?;

    Ok(())
}

/// Write benchmark results to a markdown file
pub fn write_markdown(content: &str, path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    fs::write(path, content)
        .with_context(|| format!("Failed to write markdown to {}", path.display()))?;

    Ok(())
}

/// Read benchmark results from a JSON file
pub fn read_json(path: &Path) -> Result<Vec<BenchmarkResult>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read benchmark results from {}", path.display()))?;

    let results: Vec<BenchmarkResult> = serde_json::from_str(&content)
        .context("Failed to parse benchmark results JSON")?;

    Ok(results)
}

/// Get the default output directory path
pub fn default_output_dir() -> PathBuf {
    PathBuf::from("benchmarks/output")
}

/// Get the raw results directory path
pub fn raw_results_dir() -> PathBuf {
    default_output_dir().join("raw")
}

/// Ensure all benchmark output directories exist
pub fn ensure_output_dirs() -> Result<()> {
    let output_dir = default_output_dir();
    let raw_dir = raw_results_dir();

    fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;

    fs::create_dir_all(&raw_dir)
        .with_context(|| format!("Failed to create raw results directory: {}", raw_dir.display()))?;

    Ok(())
}

/// Generate a timestamped filename for raw results
pub fn timestamped_filename(prefix: &str, extension: &str) -> String {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    format!("{}_{}.{}", prefix, timestamp, extension)
}

/// Write benchmark results to both JSON and markdown formats
pub fn write_results(results: &[BenchmarkResult], summary_markdown: &str) -> Result<()> {
    ensure_output_dirs()?;

    // Write summary markdown
    let summary_path = default_output_dir().join("summary.md");
    write_markdown(summary_markdown, &summary_path)?;

    // Write raw JSON results with timestamp
    let json_filename = timestamped_filename("benchmarks", "json");
    let json_path = raw_results_dir().join(&json_filename);
    write_json(results, &json_path)?;

    // Also write latest.json for easy access
    let latest_path = raw_results_dir().join("latest.json");
    write_json(results, &latest_path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_result(id: &str) -> BenchmarkResult {
        BenchmarkResult::new(
            id.to_string(),
            json!({
                "duration_ms": 100
            }),
        )
    }

    #[test]
    fn test_write_and_read_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.json");

        let results = vec![create_test_result("test")];

        // Write
        write_json(&results, &path).unwrap();
        assert!(path.exists());

        // Read
        let read_results = read_json(&path).unwrap();
        assert_eq!(read_results.len(), 1);
        assert_eq!(read_results[0].target_id, "test");
    }

    #[test]
    fn test_write_json_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("subdir").join("test.json");

        let results = vec![create_test_result("test")];

        write_json(&results, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_write_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.md");

        let content = "# Test Markdown\n\nContent here.";

        write_markdown(content, &path).unwrap();
        assert!(path.exists());

        let read_content = fs::read_to_string(&path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_write_markdown_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("subdir").join("test.md");

        write_markdown("test", &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_default_output_dir() {
        let dir = default_output_dir();
        assert_eq!(dir, PathBuf::from("benchmarks/output"));
    }

    #[test]
    fn test_raw_results_dir() {
        let dir = raw_results_dir();
        assert_eq!(dir, PathBuf::from("benchmarks/output/raw"));
    }

    #[test]
    fn test_timestamped_filename() {
        let filename = timestamped_filename("test", "json");
        assert!(filename.starts_with("test_"));
        assert!(filename.ends_with(".json"));
        assert!(filename.contains("_"));
    }

    #[test]
    fn test_timestamped_filename_format() {
        let filename = timestamped_filename("bench", "txt");
        // Should match pattern: bench_YYYYMMDD_HHMMSS.txt
        let parts: Vec<&str> = filename.split('_').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "bench");
    }

    #[test]
    fn test_read_json_nonexistent_file() {
        let path = PathBuf::from("/nonexistent/path/file.json");
        let result = read_json(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_json_invalid_path() {
        let results = vec![create_test_result("test")];
        // Try to write to root (should fail on most systems)
        let path = PathBuf::from("/this/should/not/exist/test.json");
        let result = write_json(&results, &path);
        // May succeed or fail depending on permissions, but shouldn't panic
        let _ = result;
    }

    #[test]
    fn test_multiple_writes_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.json");

        let results1 = vec![create_test_result("test1")];
        write_json(&results1, &path).unwrap();

        let results2 = vec![create_test_result("test2")];
        write_json(&results2, &path).unwrap();

        let read_results = read_json(&path).unwrap();
        assert_eq!(read_results[0].target_id, "test2");
    }
}
