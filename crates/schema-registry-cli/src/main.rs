//! LLM Schema Registry CLI
//!
//! A comprehensive command-line interface for managing schemas, lineage tracking,
//! analytics, migrations, and administrative operations.

mod commands;
mod config;
mod error;
mod output;

use clap::{Parser, Subcommand};
use commands::{admin, analytics, benchmark, lineage, migration, schema};
use error::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser)]
#[command(name = "schema-cli")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "LLM Schema Registry CLI - Manage schemas, lineage, analytics, and more")]
#[command(long_about = "A comprehensive CLI for the LLM Schema Registry platform.\n\nProvides commands for schema management, lineage tracking, analytics, migrations, and administrative operations.")]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true, env = "SCHEMA_REGISTRY_CONFIG")]
    config: Option<String>,

    /// Registry URL
    #[arg(short = 'u', long, global = true, env = "SCHEMA_REGISTRY_URL")]
    url: Option<String>,

    /// Output format
    #[arg(short = 'o', long, global = true, value_enum, default_value = "table")]
    output: output::OutputFormat,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Enable quiet mode (errors only)
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Schema management commands
    #[command(subcommand)]
    Schema(schema::SchemaCommand),

    /// Lineage tracking commands
    #[command(subcommand)]
    Lineage(lineage::LineageCommand),

    /// Analytics commands
    #[command(subcommand)]
    Analytics(analytics::AnalyticsCommand),

    /// Migration commands
    #[command(subcommand)]
    Migration(migration::MigrationCommand),

    /// Administrative commands
    #[command(subcommand)]
    Admin(admin::AdminCommand),

    /// Benchmark commands
    #[command(subcommand)]
    Benchmark(benchmark::BenchmarkCommand),

    /// Initialize configuration
    Init {
        /// Registry URL
        #[arg(short, long)]
        url: String,

        /// Force overwrite existing config
        #[arg(short, long)]
        force: bool,
    },

    /// Show configuration
    Config,

    /// Validate configuration
    Validate,
}

#[tokio::main]
async fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose, cli.quiet);

    // Run the command
    if let Err(e) = run(cli).await {
        error::print_error(&e);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    // Load configuration
    let config = config::load_config(cli.config.as_deref())?;

    // Override config with CLI args
    let mut config = config;
    if let Some(url) = cli.url {
        config.registry_url = url;
    }

    match cli.command {
        Commands::Schema(cmd) => schema::execute(cmd, &config, cli.output).await,
        Commands::Lineage(cmd) => lineage::execute(cmd, &config, cli.output).await,
        Commands::Analytics(cmd) => analytics::execute(cmd, &config, cli.output).await,
        Commands::Migration(cmd) => migration::execute(cmd, &config, cli.output).await,
        Commands::Admin(cmd) => admin::execute(cmd, &config, cli.output).await,
        Commands::Benchmark(cmd) => benchmark::execute(cmd, &config, cli.output).await,
        Commands::Init { url, force } => {
            config::init_config(&url, force)?;
            println!("✓ Configuration initialized successfully");
            println!("  Registry URL: {}", url);
            println!("  Config file: {}", config::config_path()?.display());
            Ok(())
        }
        Commands::Config => {
            println!("{}", serde_yaml::to_string(&config)?);
            Ok(())
        }
        Commands::Validate => {
            println!("✓ Configuration is valid");
            println!("  Registry URL: {}", config.registry_url);
            Ok(())
        }
    }
}

fn init_logging(verbose: bool, quiet: bool) {
    let filter = if quiet {
        EnvFilter::new("error")
    } else if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false).without_time())
        .init();
}
