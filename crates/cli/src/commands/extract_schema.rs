//! Extract-schema command — extract the Gramps schema from a local installation.
//!
//! This module implements the `extract-schema` subcommand, which runs the
//! Python schema extractor against a local Gramps source checkout.


/// Arguments for the `extract-schema` subcommand (re-exported from main).
pub use crate::ExtractSchemaArgs;

/// Run the extract-schema command (stub).
pub fn run(args: ExtractSchemaArgs) -> Result<(), crate::error::CliError> {
    eprintln!("Extract-schema command stub: path={}", args.path);
    Ok(())
}