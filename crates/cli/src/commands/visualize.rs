//! `visualize` subcommand — open a Gramps XML file in the Tauri
//! visualization app.
//!
//! Tauri apps must own `main()`, so this command locates the sibling
//! `gramps-gen-visualize` binary (installed alongside `gramps-gen` in the
//! same `bin/` directory) and spawns it with the canonical file path and
//! flags as arguments.

use std::path::{Path, PathBuf};

use clap::Args;

use crate::error::CliError;

/// Arguments for the `visualize` subcommand.
#[derive(Args, Clone, Debug)]
pub struct VisualizeArgs {
    /// Path to a .gramps or .xml file to visualize
    pub file: String,

    /// Skip date imputation; undated nodes get neutral color
    #[arg(long)]
    pub no_impute: bool,

    /// Years per generation for date imputation (validated: 1-100)
    #[arg(long, default_value_t = 25)]
    pub generation_gap: u32,
}

/// Valid range for `--generation-gap` (years per generation).
pub const GENERATION_GAP_MIN: u32 = 1;
pub const GENERATION_GAP_MAX: u32 = 100;

/// Run the visualize command.
pub fn run(args: VisualizeArgs) -> Result<(), CliError> {
    let canonical = canonicalize_gramps_path(&args.file)?;
    validate_generation_gap(args.generation_gap)?;

    let sibling = sibling_binary_path().ok_or_else(|| {
        CliError::ConfigError(
            "cannot locate the current executable to find the gramps-gen-visualize sibling binary"
                .to_string(),
        )
    })?;
    if !sibling.is_file() {
        return Err(CliError::ConfigError(format!(
            "gramps-gen-visualize binary not found at {}: build with `cargo build -p visualize --features visualize` and install both binaries to the same directory",
            sibling.display()
        )));
    }

    // Spawn the Tauri binary with the canonical path and flags.
    let mut cmd = std::process::Command::new(&sibling);
    cmd.arg(&canonical);
    if args.no_impute {
        cmd.arg("--no-impute");
    }
    cmd.arg("--generation-gap")
        .arg(args.generation_gap.to_string());

    let status = cmd.status().map_err(|e| CliError::Io {
        path: sibling.display().to_string(),
        source: e,
    })?;
    if !status.success() {
        return Err(CliError::ConfigError(format!(
            "gramps-gen-visualize exited with status {:?}",
            status.code()
        )));
    }
    Ok(())
}

/// Canonicalize `file` and verify it exists, is readable, and has a
/// `.gramps` or `.xml` extension.
fn canonicalize_gramps_path(file: &str) -> Result<PathBuf, CliError> {
    let path = Path::new(file);
    let canonical = path.canonicalize().map_err(|e| CliError::Io {
        path: file.to_string(),
        source: e,
    })?;
    let ext = canonical.extension().and_then(|e| e.to_str());
    if ext != Some("gramps") && ext != Some("xml") {
        return Err(CliError::ConfigError(format!(
            "not a .gramps or .xml file: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Validate that `gap` is within `GENERATION_GAP_MIN..=GENERATION_GAP_MAX`.
fn validate_generation_gap(gap: u32) -> Result<(), CliError> {
    if !(GENERATION_GAP_MIN..=GENERATION_GAP_MAX).contains(&gap) {
        return Err(CliError::ConfigError(format!(
            "--generation-gap must be in range {GENERATION_GAP_MIN}-{GENERATION_GAP_MAX}, got {gap}"
        )));
    }
    Ok(())
}

/// Path to the `gramps-gen-visualize` sibling binary, alongside the
/// currently running executable (Windows appends `.exe`).
///
/// Override with `GRAMPS_GEN_VISUALIZE_BIN` environment variable (useful
/// for testing when the sibling binary may not be present).
fn sibling_binary_path() -> Option<PathBuf> {
    if let Some(overridden) = std::env::var_os("GRAMPS_GEN_VISUALIZE_BIN") {
        return Some(PathBuf::from(overridden));
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    #[cfg(windows)]
    let name = "gramps-gen-visualize.exe";
    #[cfg(not(windows))]
    let name = "gramps-gen-visualize";
    Some(dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_missing_file_returns_io_error() {
        let result = canonicalize_gramps_path("/nonexistent/visualize-test.gramps");
        match result {
            Err(CliError::Io { path, .. }) => {
                assert!(path.contains("nonexistent"));
            }
            other => panic!("Expected Io error, got: {:?}", other),
        }
    }

    #[test]
    fn canonicalize_non_gramps_extension_returns_config_error() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database/>").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let result = canonicalize_gramps_path(&path);
        match result {
            Err(CliError::ConfigError(msg)) => {
                assert!(msg.contains(".gramps or .xml"), "got: {}", msg);
            }
            other => panic!("Expected ConfigError, got: {:?}", other),
        }
    }

    #[test]
    fn canonicalize_xml_file_accepted() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database/>").unwrap();
        let xml_path = tmp.path().with_extension("xml");
        std::fs::rename(tmp.path(), &xml_path).unwrap();
        let canonical = canonicalize_gramps_path(xml_path.to_str().unwrap()).unwrap();
        assert_eq!(canonical.extension().unwrap(), "xml");
        assert!(canonical.is_file());
    }

    #[test]
    fn canonicalize_valid_gramps_file_ok() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database/>").unwrap();
        let _path = tmp.path().to_str().unwrap().to_string();

        // Rename to a .gramps extension.
        let gramps_path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &gramps_path).unwrap();
        let canonical = canonicalize_gramps_path(gramps_path.to_str().unwrap()).unwrap();
        assert_eq!(canonical.extension().unwrap(), "gramps");
        assert!(canonical.is_file());
    }

    #[test]
    fn validate_gap_in_range_ok() {
        assert!(validate_generation_gap(1).is_ok());
        assert!(validate_generation_gap(25).is_ok());
        assert!(validate_generation_gap(100).is_ok());
    }

    #[test]
    fn validate_gap_out_of_range_returns_config_error() {
        for gap in [0u32, 101u32, 5000u32] {
            let result = validate_generation_gap(gap);
            match result {
                Err(CliError::ConfigError(msg)) => {
                    assert!(msg.contains("1-100"), "got: {}", msg);
                    assert!(msg.contains(&gap.to_string()), "got: {}", msg);
                }
                other => panic!("Expected ConfigError for gap {gap}, got: {:?}", other),
            }
        }
    }

    #[test]
    fn run_valid_file_without_sibling_binary_returns_error() {
        // A real .gramps file with valid gap; the test harness binary has no
        // `gramps-gen-visualize` sibling, so the lookup must fail gracefully.
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database/>").unwrap();
        let gramps_path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &gramps_path).unwrap();

        let args = VisualizeArgs {
            file: gramps_path.to_str().unwrap().to_string(),
            no_impute: false,
            generation_gap: 25,
        };
        let result = run(args);
        match result {
            Err(CliError::ConfigError(msg)) => {
                assert!(
                    msg.contains("binary not found"),
                    "Expected sibling-binary-not-found error, got: {}",
                    msg
                );
            }
            other => panic!("Expected ConfigError, got: {:?}", other),
        }
    }

    #[test]
    fn run_invalid_gap_reports_config_error() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database/>").unwrap();
        let gramps_path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &gramps_path).unwrap();

        let args = VisualizeArgs {
            file: gramps_path.to_str().unwrap().to_string(),
            no_impute: false,
            generation_gap: 200,
        };
        let result = run(args);
        match result {
            Err(CliError::ConfigError(msg)) => {
                assert!(msg.contains("1-100"), "got: {}", msg);
            }
            other => panic!("Expected ConfigError, got: {:?}", other),
        }
    }
}
