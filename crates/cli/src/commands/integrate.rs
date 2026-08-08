//! Integrate command — merge diff CSV results with visualizer selections.
//!
//! This module implements the `integrate diff-viz` subcommand, which takes
//! a diff CSV file and a visualizer selections JSON file, matches Person
//! rows by handle, and produces a combined output (CSV or JSON).

use std::io::Write;

use clap::Args;
use clap::Subcommand;
use integrate::output::{format_csv, format_json};
use integrate::IntegrateReport;

/// Arguments for the `integrate` subcommand.
#[derive(Args, Clone, Debug)]
pub struct IntegrateArgs {
    #[command(subcommand)]
    pub mode: IntegrateMode,
}

/// Integration mode selector.
#[derive(Subcommand, Clone, Debug)]
pub enum IntegrateMode {
    /// Merge diff CSV with visualizer selection JSON
    DiffViz(DiffVizArgs),
}

/// Arguments for the `diff-viz` integration mode.
#[derive(Args, Clone, Debug)]
pub struct DiffVizArgs {
    /// Path to the diff CSV file
    #[arg(long)]
    pub diff: String,

    /// Path to the visualizer selections JSON file
    #[arg(long)]
    pub selections: String,

    /// Output file path (stdout if not specified)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Output format: csv (default) or json
    #[arg(long, default_value = "csv")]
    pub format: String,
}

/// Run the integrate subcommand.
pub fn run(args: IntegrateArgs) -> Result<(), crate::CliError> {
    match args.mode {
        IntegrateMode::DiffViz(dv_args) => run_diff_viz(dv_args),
    }
}

/// Run the diff-viz integration mode.
fn run_diff_viz(args: DiffVizArgs) -> Result<(), crate::CliError> {
    let report: IntegrateReport = integrate::integrate_diff_viz(&args.diff, &args.selections)?;

    let output = match args.format.as_str() {
        "json" => format_json(&report.rows, &args.diff, &args.selections),
        _ => format_csv(&report.rows),
    };

    match args.output {
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
            eprintln!("Integrated report written to {}", path);
        }
        None => {
            print!("{}", output);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_viz_args_debug() {
        let args = DiffVizArgs {
            diff: "diff.csv".into(),
            selections: "selections.json".into(),
            output: None,
            format: "csv".into(),
        };
        let debug = format!("{args:?}");
        assert!(debug.contains("diff.csv"));
        assert!(debug.contains("selections.json"));
    }

    #[test]
    fn integrate_args_debug() {
        let args = IntegrateArgs {
            mode: IntegrateMode::DiffViz(DiffVizArgs {
                diff: "d.csv".into(),
                selections: "s.json".into(),
                output: Some("out.csv".into()),
                format: "csv".into(),
            }),
        };
        let debug = format!("{args:?}");
        assert!(debug.contains("d.csv"));
    }
}
