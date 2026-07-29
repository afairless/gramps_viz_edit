//! Generate command — wire generation, validation, and serialization.
//!
//! This module implements the `generate` subcommand for the gramps-gen CLI.
//! It follows the five-stage pipeline:
//!
//! Generate → Validate → [Adversarial Transform] → Validate → Serialize

use clap::Args;

/// Arguments for the `generate` subcommand.
#[derive(Args, Clone, Debug)]
pub struct GenerateArgs {
    /// Number of persons to generate
    #[arg(short = 'n', long, default_value = "200")]
    pub count: usize,

    /// Number of generations
    #[arg(short = 'd', long, default_value = "3")]
    pub depth: usize,

    /// Output .gramps file
    #[arg(short = 'o', long, default_value = "output.gramps")]
    pub output: String,

    /// RNG seed for reproducible generation
    #[arg(long)]
    pub seed: Option<u64>,

    /// Promote plausibility warnings to errors
    #[arg(long)]
    pub strict: bool,

    /// Comma-separated adversarial strategies, or "all"
    #[arg(long)]
    pub adversarial: Option<String>,

    /// How often to report generation progress
    #[arg(long, default_value = "100")]
    pub progress_interval: usize,

    /// YAML scenario file (overrides other options)
    #[arg(short = 'c', long)]
    pub config: Option<String>,

    /// Generate Place nodes
    #[arg(long)]
    pub with_places: bool,

    /// Generate Citation and Source nodes
    #[arg(long)]
    pub with_citations: bool,

    /// Generate Note nodes
    #[arg(long)]
    pub with_notes: bool,

    /// Generate Media objects
    #[arg(long)]
    pub with_media: bool,

    /// Generate Tag nodes
    #[arg(long)]
    pub with_tags: bool,
}

/// Run the generate command (stub — will be wired in Step 6).
pub fn run(args: GenerateArgs) -> Result<(), crate::error::CliError> {
    eprintln!("Generate command stub: count={}, depth={}, output={}",
        args.count, args.depth, args.output);
    Ok(())
}