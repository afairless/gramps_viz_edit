//! CLI binary for the Gramps data generator.
//!
//! This crate provides the `gramps-gen` command-line interface with
//! subcommands for generating, validating, and extracting schemas.

use clap::Parser;
use clap::Subcommand;
use cli::commands::extract_schema;
use cli::commands::generate::GenerateArgs;
use cli::commands::schema::SchemaCommand;
use cli::commands::stats::StatsArgs;
use cli::commands::validate::ValidateArgs;
use cli::commands::visualize::VisualizeArgs;
use cli::error::CliError;

/// Generate valid Gramps family tree datasets
#[derive(Parser)]
#[command(name = "gramps-gen", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a random family tree dataset
    Generate(GenerateArgs),
    /// Summarize the contents of a .gramps file
    Stats(StatsArgs),
    /// Validate a .gramps file
    Validate(ValidateArgs),
    /// Open a .gramps file in the family-group visualization app
    Visualize(VisualizeArgs),
    /// Extract the schema from a Gramps installation
    ExtractSchema(ExtractSchemaArgs),
    /// List and download Gramps schemas
    #[command(subcommand)]
    Schema(SchemaCommand),
}

/// Arguments for the `extract-schema` command.
pub type ExtractSchemaArgs = cli::commands::extract_schema::ExtractSchemaArgs;

fn main() -> Result<(), CliError> {
    env_logger::init();
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => cli::commands::generate::run(args)?,
        Command::Stats(args) => cli::commands::stats::run(args)?,
        Command::Validate(args) => cli::commands::validate::run(args)?,
        Command::Visualize(args) => cli::commands::visualize::run(args)?,
        Command::ExtractSchema(args) => extract_schema::run(args)?,
        Command::Schema(args) => cli::commands::schema::run(args)?,
    }
    Ok(())
}
