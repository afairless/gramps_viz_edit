//! Validate command — validate a .gramps file's structure.
//!
//! This module implements the `validate` subcommand, which checks
//! that a .gramps file is well-formed XML and has the expected
//! document structure.

use clap::Args;

/// Arguments for the `validate` subcommand.
#[derive(Args, Clone, Debug)]
pub struct ValidateArgs {
    /// Path to a .gramps file to validate
    pub file: String,

    /// Promote plausibility warnings to errors
    #[arg(long)]
    pub strict: bool,
}

/// Run the validate command (stub).
pub fn run(args: ValidateArgs) -> Result<(), crate::error::CliError> {
    eprintln!("Validate command stub: file={}, strict={}", args.file, args.strict);
    Ok(())
}