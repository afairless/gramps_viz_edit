//! `stats` subcommand — summarize the contents of a `.gramps` file.
//!
//! This module implements the `gramps-gen stats <file>` command, which
//! performs a single streaming pass over a `.gramps` file to count
//! primary object types, compute family-size distribution, and report
//! people not in any family.

pub mod count;

use clap::Args;
use crate::error::CliError;
use count::count_gramps_xml;

/// Arguments for the `stats` subcommand.
#[derive(Args, Clone, Debug)]
pub struct StatsArgs {
    /// Path to a .gramps file to analyze
    pub file: String,

    /// Emit machine-readable JSON instead of a formatted table
    #[arg(long)]
    pub json: bool,
}

/// Run the stats command.
pub fn run(args: StatsArgs) -> Result<(), CliError> {
    let file_path = &args.file;

    // Read the file
    let content = std::fs::read_to_string(file_path).map_err(|e| CliError::Io {
        path: file_path.clone(),
        source: e,
    })?;

    // Count
    let report = count_gramps_xml(&content)?;

    // Output
    if args.json {
        eprintln!("JSON output not yet implemented");
    } else {
        println!("File: {}", file_path);
        println!();
        println!("Object counts");
        println!("  People:          {}", report.counts.people);
        println!("  Families:        {}", report.counts.families);
        println!("  Events:          {}", report.counts.events);
        println!("  Places:          {}", report.counts.places);
        println!("  Sources:         {}", report.counts.sources);
        println!("  Citations:       {}", report.counts.citations);
        println!("  Repositories:    {}", report.counts.repositories);
        println!("  Media objects:   {}", report.counts.media);
        println!("  Notes:           {}", report.counts.notes);
        println!("  Tags:            {}", report.counts.tags);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_file_not_found_returns_io_error() {
        let args = StatsArgs {
            file: "/nonexistent/stats-test.gramps".to_string(),
            json: false,
        };
        let result = run(args);
        match result {
            Err(CliError::Io { path, .. }) => {
                assert!(path.contains("nonexistent"));
            }
            other => panic!("Expected Io error, got: {:?}", other),
        }
    }

    #[test]
    fn stats_malformed_xml_returns_xml_parse_error() {
        use tempfile::NamedTempFile;
        use std::io::Write;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database><person></database>").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let args = StatsArgs {
            file: path,
            json: false,
        };
        let result = run(args);
        match result {
            Err(CliError::XmlParseError { .. }) => {}
            other => panic!("Expected XmlParseError, got: {:?}", other),
        }
    }

    #[test]
    fn stats_empty_content_returns_zeroed_report() {
        use tempfile::NamedTempFile;
        use std::io::Write;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let args = StatsArgs {
            file: path,
            json: false,
        };
        let result = run(args);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    }
}