//! Benchmark command implementation

use crate::config::Config;
use crate::error::Result;
use crate::output::OutputFormat;
use clap::Subcommand;
use colored::Colorize;
use schema_registry_benchmarks::{io, markdown, run_all_benchmarks};

#[derive(Subcommand)]
pub enum BenchmarkCommand {
    /// Run all benchmarks and generate reports
    Run {
        /// Output directory for benchmark results
        #[arg(short, long, default_value = "benchmarks/output")]
        output_dir: String,

        /// Skip writing to disk (dry run)
        #[arg(long)]
        dry_run: bool,
    },

    /// List available benchmark targets
    List,
}

pub async fn execute(cmd: BenchmarkCommand, _config: &Config, output: OutputFormat) -> Result<()> {
    match cmd {
        BenchmarkCommand::Run { output_dir, dry_run } => {
            run_benchmarks(&output_dir, dry_run, output).await
        }
        BenchmarkCommand::List => list_benchmarks(output).await,
    }
}

async fn run_benchmarks(output_dir: &str, dry_run: bool, output: OutputFormat) -> Result<()> {
    println!("{}", "Running benchmarks...".cyan().bold());
    println!();

    // Run all benchmarks
    let results = run_all_benchmarks().await;

    if results.is_empty() {
        println!("{}", "No benchmarks were executed.".yellow());
        return Ok(());
    }

    println!("{}", format!("Completed {} benchmarks", results.len()).green());
    println!();

    // Display results based on output format
    match output {
        OutputFormat::Table => {
            println!("{}", "Benchmark Results".bold());
            println!("{}", "=".repeat(80));
            for result in &results {
                println!("{}: {}", result.target_id.cyan(), result.timestamp);
                println!("  Metrics: {}", serde_json::to_string_pretty(&result.metrics)?);
                println!();
            }
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&results)?);
        }
    }

    // Write results to disk unless dry run
    if !dry_run {
        // Update output directory paths to use the provided output_dir
        std::env::set_var("BENCHMARK_OUTPUT_DIR", output_dir);

        // Generate markdown summary
        let summary = markdown::generate_summary(&results);

        // Write results
        io::write_results(&results, &summary)?;

        println!();
        println!("{}", "Results written to:".green().bold());
        println!("  Summary: {}/summary.md", output_dir);
        println!("  Raw JSON: {}/raw/latest.json", output_dir);
        println!("  Timestamped: {}/raw/benchmarks_*.json", output_dir);
    } else {
        println!();
        println!("{}", "Dry run - results not written to disk".yellow());
    }

    Ok(())
}

async fn list_benchmarks(output: OutputFormat) -> Result<()> {
    let targets = schema_registry_benchmarks::adapters::all_targets();

    match output {
        OutputFormat::Table => {
            println!("{}", "Available Benchmarks".bold());
            println!("{}", "=".repeat(80));
            println!();

            for target in targets {
                println!("{}", target.id().cyan().bold());
                println!("  {}", target.description());
                println!();
            }

            println!("Total: {} benchmark targets", targets.len());
        }
        OutputFormat::Json => {
            let targets_info: Vec<_> = targets
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id(),
                        "description": t.description()
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&targets_info)?);
        }
        OutputFormat::Yaml => {
            let targets_info: Vec<_> = targets
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id(),
                        "description": t.description()
                    })
                })
                .collect();
            println!("{}", serde_yaml::to_string(&targets_info)?);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_command_variants() {
        // Test that command variants can be constructed
        let _run = BenchmarkCommand::Run {
            output_dir: "test".to_string(),
            dry_run: false,
        };
        let _list = BenchmarkCommand::List;
    }
}
