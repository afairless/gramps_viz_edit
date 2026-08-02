//! `stats` subcommand — summarize the contents of a `.gramps` file.
//!
//! This module implements the `gramps-gen stats <file>` command, which
//! performs a single streaming pass over a `.gramps` file to count
//! primary object types, compute family-size distribution, and report
//! people not in any family.

pub mod count;

use crate::error::CliError;
use clap::Args;
use count::{count_gramps_xml, StatsReport};

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
    let mut report = count_gramps_xml(&content)?;
    report.file = file_path.clone();

    // Output
    print!("{}", render(&report, args.json));
    Ok(())
}

/// Render a `StatsReport` as text or JSON.
fn render(report: &StatsReport, json: bool) -> String {
    if json {
        serde_json::to_string_pretty(report).unwrap()
    } else {
        format_text_report(report)
    }
}

/// Format a `StatsReport` as human-readable text.
fn format_text_report(report: &StatsReport) -> String {
    let mut out = String::new();

    out.push_str(&format!("File: {}\n\n", report.file));

    // Object counts
    out.push_str("Object counts\n");
    let counts: [(&str, usize); 10] = [
        ("People", report.counts.people),
        ("Families", report.counts.families),
        ("Events", report.counts.events),
        ("Places", report.counts.places),
        ("Sources", report.counts.sources),
        ("Citations", report.counts.citations),
        ("Repositories", report.counts.repositories),
        ("Media objects", report.counts.media),
        ("Notes", report.counts.notes),
        ("Tags", report.counts.tags),
    ];
    for (label, n) in &counts {
        out.push_str(&format!("  {:<14}{:>5}\n", format!("{}:", label), n));
    }

    // Family size distribution
    if !report.family_size_distribution.is_empty() {
        out.push_str("\nFamily size distribution\n");
        for (size, families) in &report.family_size_distribution {
            let people = size * families;
            if *size == 0 {
                let fam_word = if *families == 1 { "family" } else { "families" };
                out.push_str(&format!(
                    "  size {:>2}: {} empty {}\n",
                    size, families, fam_word
                ));
            } else {
                let fam_word = if *families == 1 { "family" } else { "families" };
                let people_word = if people == 1 { "person" } else { "people" };
                out.push_str(&format!(
                    "  size {:>2}: {} {} ({} {})\n",
                    size, families, fam_word, people, people_word
                ));
            }
        }
    }

    // People not in family / dangling refs
    out.push_str(&format!(
        "\nPeople not in any family: {}\n",
        report.people_not_in_family
    ));
    out.push_str(&format!(
        "Dangling family refs:     {}\n",
        report.dangling_refs
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::commands::stats::count::PrimaryTypeCounts;

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
        use std::io::Write;
        use tempfile::NamedTempFile;

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
        use std::io::Write;
        use tempfile::NamedTempFile;

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

    #[test]
    fn format_text_report_expected_output() {
        let report = StatsReport {
            file: "family.gramps".to_string(),
            counts: PrimaryTypeCounts {
                people: 42,
                families: 10,
                events: 57,
                places: 12,
                sources: 3,
                citations: 9,
                repositories: 1,
                media: 4,
                notes: 15,
                tags: 2,
            },
            family_size_distribution: BTreeMap::from([(1, 1), (2, 2), (3, 5), (4, 2)]),
            family_generation_table: BTreeMap::new(),
            people_not_in_family: 8,
            dangling_refs: 0,
            warnings: vec![],
        };

        let text = format_text_report(&report);

        // Check key parts of the output
        assert!(text.contains("File: family.gramps"));
        assert!(text.contains("Object counts"));
        // Check that the People line has the right format
        assert!(text
            .lines()
            .any(|l| l.starts_with("  People:") && l.ends_with("42")));
        assert!(text
            .lines()
            .any(|l| l.starts_with("  Families:") && l.ends_with("10")));
        assert!(text
            .lines()
            .any(|l| l.starts_with("  Media objects:") && l.ends_with("4")));
        assert!(text.contains("Family size distribution"));
        assert!(text.contains("size  1: 1 family (1 person)"));
        assert!(text.contains("size  3: 5 families (15 people)"));
        assert!(text.contains("size  4: 2 families (8 people)"));
        assert!(text.contains("People not in any family: 8"));
        assert!(text.contains("Dangling family refs:     0"));
    }

    #[test]
    fn format_text_report_zero_size_histogram() {
        let mut dist = BTreeMap::new();
        dist.insert(0, 2);
        let report = StatsReport {
            file: "test.gramps".to_string(),
            counts: PrimaryTypeCounts::default(),
            family_size_distribution: dist,
            family_generation_table: BTreeMap::new(),
            people_not_in_family: 5,
            dangling_refs: 1,
            warnings: vec![],
        };

        let text = format_text_report(&report);
        assert!(text.contains("size  0: 2 empty families"));
        assert!(text.contains("People not in any family: 5"));
        assert!(text.contains("Dangling family refs:     1"));
    }

    #[test]
    fn render_json_round_trip() {
        let report = StatsReport {
            file: "test.gramps".to_string(),
            counts: PrimaryTypeCounts {
                people: 10,
                families: 3,
                events: 5,
                places: 2,
                sources: 1,
                citations: 4,
                repositories: 0,
                media: 2,
                notes: 3,
                tags: 1,
            },
            family_size_distribution: BTreeMap::from([(0, 1), (2, 1), (4, 1)]),
            family_generation_table: BTreeMap::new(),
            people_not_in_family: 3,
            dangling_refs: 0,
            warnings: vec![],
        };

        let json = serde_json::to_string(&report).unwrap();
        let back: StatsReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
    }

    #[test]
    fn render_json_includes_expected_fields() {
        let report = StatsReport {
            file: "test.gramps".to_string(),
            counts: PrimaryTypeCounts {
                people: 5,
                families: 1,
                ..PrimaryTypeCounts::default()
            },
            family_size_distribution: BTreeMap::from([(2, 1)]),
            family_generation_table: BTreeMap::new(),
            people_not_in_family: 2,
            dangling_refs: 0,
            warnings: vec!["a warning".to_string()],
        };

        let json = render(&report, true);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["file"], "test.gramps");
        assert_eq!(parsed["counts"]["people"], 5);
        assert_eq!(parsed["counts"]["families"], 1);
        assert_eq!(parsed["family_size_distribution"]["2"], 1);
        assert_eq!(parsed["people_not_in_family"], 2);
        assert_eq!(parsed["dangling_refs"], 0);
        assert!(parsed["warnings"].is_array());
    }
}
