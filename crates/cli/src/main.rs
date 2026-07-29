//! CLI binary for the Gramps data generator.
//!
//! This crate provides the `gramps-gen` command-line interface with
//! subcommands for generating, validating, and extracting schemas.

pub mod commands;
pub mod error;
pub mod progress;
pub mod scenario;

use clap::Parser;
use clap::Subcommand;
use commands::generate::GenerateArgs;
use commands::validate::ValidateArgs;

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
    /// Validate a .gramps file
    Validate(ValidateArgs),
    /// Extract the schema from a Gramps installation
    ExtractSchema(ExtractSchemaArgs),
}

/// Arguments for the `extract-schema` command.
#[derive(clap::Args)]
pub struct ExtractSchemaArgs {
    /// Path to a local Gramps source repository
    path: String,
}

fn main() -> Result<(), error::CliError> {
    env_logger::init();
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => commands::generate::run(args)?,
        Command::Validate(args) => commands::validate::run(args)?,
        Command::ExtractSchema(args) => commands::extract_schema::run(args)?,
    }
    Ok(())
}