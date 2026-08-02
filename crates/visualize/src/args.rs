//! Command-line argument parsing for the `gramps-gen-visualize` binary.
//!
//! This module provides a pure function `parse_cli_args` that extracts
//! the file path, `--no-impute` flag, and `--generation-gap` value from
//! command-line arguments. It is testable without involving Tauri.

/// Parsed CLI arguments for the visualize binary.
#[derive(Debug, Clone, PartialEq)]
pub struct CliArgs {
    /// Path to the `.gramps` file, if provided as a positional argument.
    pub path: Option<String>,
    /// Whether `--no-impute` was passed (default: `false`).
    pub no_impute: bool,
    /// Generation gap in years (default: `25`, parsed from `--generation-gap`).
    pub generation_gap: u32,
}

/// Parse command-line arguments into a `CliArgs` value.
///
/// The binary name (`argv[0]`) is skipped via `std::env::args().skip(1)`.
/// A single positional (non-flag) argument is accepted as the file path.
/// Any additional positional arguments or unknown flags are silently ignored.
///
/// # Argument ordering
///
/// All orderings of the positional path, `--no-impute`, and
/// `--generation-gap <N>` are accepted. For example:
///
/// - `file.gramps`
/// - `file.gramps --no-impute --generation-gap 50`
/// - `--no-impute file.gramps --generation-gap 50`
///
/// # Errors
///
/// If `--generation-gap` is followed by a non-numeric value, the parse
/// fails silently and the default (25) is used.
pub fn parse_cli_args(args: &[String]) -> CliArgs {
    let mut iter = args.iter();
    let mut path: Option<String> = None;
    let mut no_impute = false;
    let mut generation_gap = 25u32;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--no-impute" => {
                no_impute = true;
            }
            "--generation-gap" => {
                if let Some(val) = iter.next() {
                    generation_gap = val.parse().unwrap_or(25);
                }
            }
            other if !other.starts_with("--") && path.is_none() => {
                // First non-flag token is the file path.
                path = Some(other.to_string());
            }
            _ => {
                // Unknown flag or extra positional — silently ignore.
            }
        }
    }

    CliArgs {
        path,
        no_impute,
        generation_gap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positional_only() {
        let args: Vec<String> = vec!["/path/to/file.gramps".into()];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/path/to/file.gramps".into()),
                no_impute: false,
                generation_gap: 25,
            }
        );
    }

    #[test]
    fn positional_with_no_impute() {
        let args: Vec<String> = vec!["/path/to/file.gramps".into(), "--no-impute".into()];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/path/to/file.gramps".into()),
                no_impute: true,
                generation_gap: 25,
            }
        );
    }

    #[test]
    fn positional_with_generation_gap() {
        let args: Vec<String> = vec![
            "/path/to/file.gramps".into(),
            "--generation-gap".into(),
            "50".into(),
        ];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/path/to/file.gramps".into()),
                no_impute: false,
                generation_gap: 50,
            }
        );
    }

    #[test]
    fn empty_args() {
        let args: Vec<String> = vec![];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: None,
                no_impute: false,
                generation_gap: 25,
            }
        );
    }

    #[test]
    fn flags_before_path() {
        let args: Vec<String> = vec![
            "--no-impute".into(),
            "/path/to/file.gramps".into(),
        ];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/path/to/file.gramps".into()),
                no_impute: true,
                generation_gap: 25,
            }
        );
    }

    #[test]
    fn invalid_generation_gap_uses_default() {
        let args: Vec<String> = vec![
            "--generation-gap".into(),
            "abc".into(),
            "/p.gramps".into(),
        ];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/p.gramps".into()),
                no_impute: false,
                generation_gap: 25,
            }
        );
    }

    #[test]
    fn all_flags_and_path() {
        let args: Vec<String> = vec![
            "/path/to/file.gramps".into(),
            "--no-impute".into(),
            "--generation-gap".into(),
            "30".into(),
        ];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/path/to/file.gramps".into()),
                no_impute: true,
                generation_gap: 30,
            }
        );
    }

    #[test]
    fn no_impute_flag_only_no_path() {
        let args: Vec<String> = vec!["--no-impute".into()];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: None,
                no_impute: true,
                generation_gap: 25,
            }
        );
    }

    #[test]
    fn unknown_flag_is_ignored() {
        let args: Vec<String> = vec![
            "/path/to/file.gramps".into(),
            "--unknown-flag".into(),
        ];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/path/to/file.gramps".into()),
                no_impute: false,
                generation_gap: 25,
            }
        );
    }

    #[test]
    fn extra_positional_is_ignored() {
        let args: Vec<String> = vec![
            "/path/to/file.gramps".into(),
            "extra_arg".into(),
        ];
        let result = parse_cli_args(&args);
        assert_eq!(
            result,
            CliArgs {
                path: Some("/path/to/file.gramps".into()),
                no_impute: false,
                generation_gap: 25,
            }
        );
    }
}