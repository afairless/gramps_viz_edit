//! Extract-schema command — extract the Gramps schema from a local installation.
//!
//! This module implements the `extract-schema` subcommand, which runs the
//! Python schema extractor against a local Gramps source checkout.

use clap::Args;

/// Arguments for the `extract-schema` subcommand.
#[derive(Args)]
pub struct ExtractSchemaArgs {
    /// Path to a local Gramps source repository
    pub path: String,
}

/// Run the extract-schema command (stub).
pub fn run(args: ExtractSchemaArgs) -> Result<(), crate::CliError> {
    eprintln!("Extract-schema command stub: path={}", args.path);
    Ok(())
}