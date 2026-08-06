//! Diff command — compare two Gramps XML files.
//!
//! This module implements the `diff` subcommand, which compares two
//! Gramps family tree files and produces a structured diff report.

use std::io::Write;

use clap::Args;
use diff::output::{format_json, format_text};
use diff::DiffConfig;

/// Arguments for the `diff` subcommand.
#[derive(Args, Clone, Debug)]
pub struct DiffArgs {
    /// First Gramps XML file (A)
    pub file_a: String,

    /// Second Gramps XML file (B)
    pub file_b: String,

    /// Output format: text (default) or json
    #[arg(long, default_value = "text")]
    pub output: String,

    /// Write output to file instead of stdout
    #[arg(long)]
    pub output_file: Option<String>,

    /// Minimum similarity threshold (0.0–1.0) for text field matches
    #[arg(long, default_value_t = 0.8)]
    pub threshold: f64,

    /// Disable text normalization before comparison
    #[arg(long)]
    pub no_normalize: bool,

    /// Include extrinsic-only items in the report
    #[arg(long)]
    pub include_extrinsic: bool,

    /// Only print the summary, not per-item details
    #[arg(long)]
    pub summary_only: bool,
}

/// Run the diff subcommand.
pub fn run(args: DiffArgs) -> Result<(), crate::CliError> {
    // Build config
    let config = DiffConfig {
        threshold: args.threshold.clamp(0.0, 1.0),
        include_extrinsic: args.include_extrinsic,
        normalize_enabled: !args.no_normalize,
    };

    // Run diff
    let report = diff::run_diff(&args.file_a, &args.file_b, &config)
        .map_err(|e| crate::CliError::DiffFailed(e.to_string()))?;

    // Format output
    let output = match args.output.as_str() {
        "json" => format_json(&report),
        _ => {
            let text = format_text(&report, config.include_extrinsic);
            if args.summary_only {
                // Extract just the summary section
                summary_only(&text)
            } else {
                text
            }
        }
    };

    // Write output
    match args.output_file {
        Some(path) => {
            let mut file = std::fs::File::create(&path).map_err(|e| crate::CliError::Io {
                path: path.clone(),
                source: e,
            })?;
            file.write_all(output.as_bytes())
                .map_err(|e| crate::CliError::Io {
                    path: path.clone(),
                    source: e,
                })?;
            eprintln!("Diff report written to {}", path);
        }
        None => {
            print!("{}", output);
        }
    }

    Ok(())
}

/// Extract just the summary section from a text report.
fn summary_only(text: &str) -> String {
    let mut result = String::new();

    for line in text.lines() {
        // Stop at the "Items" section heading
        if line.trim() == "Items" || line.trim().starts_with("Items") {
            break;
        }
        result.push_str(line);
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_args_debug() {
        let args = DiffArgs {
            file_a: "a.gramps".into(),
            file_b: "b.gramps".into(),
            output: "text".into(),
            output_file: None,
            threshold: 0.8,
            no_normalize: false,
            include_extrinsic: true,
            summary_only: false,
        };
        let debug = format!("{args:?}");
        assert!(debug.contains("file_a"));
        assert!(debug.contains("file_b"));
    }

    #[test]
    fn summary_only_keeps_only_summary() {
        let report = "Gramps Diff Report\n==================\n\nSummary\n-------\nTotal (A): 10\nTotal (B): 12\n\nItems\n-----\n";
        let result = summary_only(report);
        assert!(result.contains("Summary"));
        assert!(result.contains("Total (A)"));
        assert!(!result.contains("Items"));
    }
}