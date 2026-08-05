//! `stats` subcommand — summarize the contents of a Gramps XML file.
//!
//! This module implements the `gramps-gen stats <file>` command, which
//! performs a single streaming pass over a Gramps XML file to count
//! primary object types, compute family-size distribution, report
//! people not in any family, and tabulate the family-size ×
//! generation-span contingency table.

pub mod count;

use crate::error::CliError;
use clap::Args;
use gramps_reader::{count_gramps_xml, FamilyGroupGenerationTable, StatsReport};

/// Arguments for the `stats` subcommand.
#[derive(Args, Clone, Debug)]
pub struct StatsArgs {
    /// Path to a .gramps or .xml file to analyze
    pub file: String,

    /// Emit machine-readable JSON instead of a formatted table
    #[arg(long)]
    pub json: bool,

    /// Use ASCII characters instead of Unicode box-drawing in text output
    #[arg(long)]
    pub no_unicode: bool,
}

/// Run the stats command.
pub fn run(args: StatsArgs) -> Result<(), CliError> {
    let file_path = &args.file;

    // Read the file
    let content = gramps_reader::read_gramps_file(file_path)?;

    // Count
    let mut report = count_gramps_xml(&content)?;
    report.file = file_path.clone();

    // Output
    print!("{}", render(&report, args.json, args.no_unicode));
    Ok(())
}

/// Render a `StatsReport` as text or JSON.
fn render(report: &StatsReport, json: bool, no_unicode: bool) -> String {
    if json {
        serde_json::to_string_pretty(report).unwrap()
    } else {
        format_text_report(report, no_unicode)
    }
}

/// Format a `StatsReport` as human-readable text.
fn format_text_report(report: &StatsReport, no_unicode: bool) -> String {
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

    // Family group distribution
    if !report.family_group_distribution.is_empty() {
        out.push_str("\nFamily group distribution\n");
        for (size, groups) in &report.family_group_distribution {
            let people = size * groups;
            let group_word = if *groups == 1 { "group" } else { "groups" };
            let people_word = if people == 1 { "person" } else { "people" };
            out.push_str(&format!(
                "  size {:>2}: {} {} ({} {})\n",
                size, groups, group_word, people, people_word
            ));
        }
    }

    // Family group size × generation table
    out.push('\n');
    if no_unicode {
        out.push_str(&render_family_group_table_ascii(
            &report.family_group_generation_table,
        ));
    } else {
        out.push_str(&render_family_group_table(
            &report.family_group_generation_table,
        ));
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

    // Warnings (cycle detection, etc.)
    if !report.warnings.is_empty() {
        out.push_str("\nWarnings:\n");
        for warning in &report.warnings {
            out.push_str(&format!("  - {}\n", warning));
        }
    }

    out
}

/// Render the family-group-size × generation-span contingency table using
/// Unicode box-drawing characters.
fn render_family_group_table(table: &FamilyGroupGenerationTable) -> String {
    render_family_group_table_inner(table, true)
}

/// Render the family-group-size × generation-span contingency table using
/// pure ASCII characters.
fn render_family_group_table_ascii(table: &FamilyGroupGenerationTable) -> String {
    render_family_group_table_inner(table, false)
}

/// Shared table renderer. `unicode` selects box-drawing characters;
/// ASCII mode falls back to `|`, `-`, `+`.
fn render_family_group_table_inner(table: &FamilyGroupGenerationTable, unicode: bool) -> String {
    let (vline, hline, cross, title) = if unicode {
        ("│", "─", "┼", "Family group size × generation table")
    } else {
        ("|", "-", "+", "Family group size x generation table")
    };

    if table.is_empty() {
        return format!("{}\n  (no data)\n", title);
    }

    // Numeric keys, sorted ascending.
    let mut sizes: Vec<usize> = table.keys().filter_map(|k| k.parse().ok()).collect();
    sizes.sort_unstable();
    let mut spans: Vec<usize> = table
        .values()
        .flat_map(|row| row.keys())
        .filter_map(|k| k.parse().ok())
        .collect();
    spans.sort_unstable();
    spans.dedup();

    // Row labels: "# people" header on the first row, sizes below.
    let mut labels: Vec<String> = Vec::with_capacity(sizes.len());
    for (i, size) in sizes.iter().enumerate() {
        if i == 0 {
            labels.push(format!("# people {}", size));
        } else {
            labels.push(size.to_string());
        }
    }

    // Column labels: span columns plus the "total" marginal.
    let mut col_labels: Vec<String> = spans.iter().map(|s| s.to_string()).collect();
    col_labels.push("total".to_string());

    // Column and row marginals.
    let col_totals: Vec<usize> = spans
        .iter()
        .map(|span| {
            table
                .values()
                .filter_map(|row| row.get(&span.to_string()))
                .sum()
        })
        .collect();
    let row_totals: Vec<usize> = sizes
        .iter()
        .map(|size| {
            table
                .get(&size.to_string())
                .and_then(|row| row.get("total"))
                .copied()
                .unwrap_or(0)
        })
        .collect();
    let grand_total: usize = row_totals.iter().sum();

    // Column widths fit header, cells, and column totals.
    let mut col_widths: Vec<usize> = Vec::with_capacity(col_labels.len());
    for (j, span) in spans.iter().enumerate() {
        let mut w = col_labels[j].len().max(col_totals[j].to_string().len());
        for size in &sizes {
            if let Some(row) = table.get(&size.to_string()) {
                if let Some(c) = row.get(&span.to_string()) {
                    w = w.max(c.to_string().len());
                }
            }
        }
        col_widths.push(w);
    }
    let total_col_w = "total".len().max(grand_total.to_string().len()).max(
        row_totals
            .iter()
            .map(|t| t.to_string().len())
            .max()
            .unwrap_or(0),
    );
    col_widths.push(total_col_w);

    // Left label column width.
    let mut label_w = "# generations".len();
    for label in &labels {
        label_w = label_w.max(label.len());
    }
    label_w = label_w.max("total".len());

    let right_w: usize = col_widths.iter().sum::<usize>() + 2 * (col_widths.len() - 1);
    let separator = format!(
        "  {}{}{}\n",
        hline.repeat(label_w + 1),
        cross,
        hline.repeat(right_w),
    );

    let mut out = String::new();
    out.push_str(title);
    out.push('\n');

    // Header row
    out.push_str("  ");
    out.push_str(&format!("{:<w$}", "# generations", w = label_w));
    out.push(' ');
    out.push_str(vline);
    for (j, label) in col_labels.iter().enumerate() {
        if j > 0 {
            out.push_str("  ");
        }
        out.push_str(&format!("{:>w$}", label, w = col_widths[j]));
    }
    out.push('\n');
    out.push_str(&separator);

    // Data rows
    for (i, size) in sizes.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&format!("{:<w$}", labels[i], w = label_w));
        out.push(' ');
        out.push_str(vline);
        for (j, span) in spans.iter().enumerate() {
            if j > 0 {
                out.push_str("  ");
            }
            let val = table
                .get(&size.to_string())
                .and_then(|row| row.get(&span.to_string()))
                .copied()
                .unwrap_or(0);
            out.push_str(&format!("{:>w$}", val, w = col_widths[j]));
        }
        out.push_str("  ");
        out.push_str(&format!("{:>w$}", row_totals[i], w = total_col_w));
        out.push('\n');
    }

    out.push_str(&separator);

    // Total row
    out.push_str("  ");
    out.push_str(&format!("{:<w$}", "total", w = label_w));
    out.push(' ');
    out.push_str(vline);
    for (j, _span) in spans.iter().enumerate() {
        if j > 0 {
            out.push_str("  ");
        }
        out.push_str(&format!("{:>w$}", col_totals[j], w = col_widths[j]));
    }
    out.push_str("  ");
    out.push_str(&format!("{:>w$}", grand_total, w = total_col_w));
    out.push('\n');

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
            no_unicode: false,
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
            no_unicode: false,
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
            no_unicode: false,
        };
        let result = run(args);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    }

    #[test]
    fn stats_gzip_compressed_file_returns_success() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // A minimal valid Gramps XML document.
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><name><first>Test</first><surname>Person</surname></name></person>
  </people>
</database>"#;

        // Gzip-compress the XML.
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(xml.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&compressed).unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let args = StatsArgs {
            file: path,
            json: false,
            no_unicode: false,
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
            family_group_generation_table: BTreeMap::new(),
            family_group_distribution: BTreeMap::new(),
            people_not_in_family: 8,
            dangling_refs: 0,
            warnings: vec![],
        };

        let text = format_text_report(&report, false);

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
            family_group_generation_table: BTreeMap::new(),
            family_group_distribution: BTreeMap::new(),
            people_not_in_family: 5,
            dangling_refs: 1,
            warnings: vec![],
        };

        let text = format_text_report(&report, false);
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
            family_group_generation_table: BTreeMap::new(),
            family_group_distribution: BTreeMap::new(),
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
            family_group_generation_table: BTreeMap::new(),
            family_group_distribution: BTreeMap::new(),
            people_not_in_family: 2,
            dangling_refs: 0,
            warnings: vec!["a warning".to_string()],
        };

        let json = render(&report, true, false);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["file"], "test.gramps");
        assert_eq!(parsed["counts"]["people"], 5);
        assert_eq!(parsed["counts"]["families"], 1);
        assert_eq!(parsed["family_size_distribution"]["2"], 1);
        assert_eq!(parsed["people_not_in_family"], 2);
        assert_eq!(parsed["dangling_refs"], 0);
        assert!(parsed["warnings"].is_array());
        assert!(
            parsed["family_group_generation_table"].is_object(),
            "Should have family_group_generation_table"
        );
        assert!(
            parsed["family_group_distribution"].is_object(),
            "Should have family_group_distribution"
        );
    }

    #[test]
    fn render_family_group_table_empty() {
        let table: FamilyGroupGenerationTable = BTreeMap::new();
        let text = render_family_group_table(&table);
        assert!(text.contains("Family group size × generation table"));
        assert!(text.contains("(no data)"));
    }

    #[test]
    fn render_family_group_table_single_row() {
        let table: FamilyGroupGenerationTable = BTreeMap::from([(
            "2".to_string(),
            BTreeMap::from([("1".to_string(), 3), ("total".to_string(), 3)]),
        )]);
        let text = render_family_group_table(&table);
        assert!(text.contains("Family group size × generation table"));
        assert!(text.contains("# generations"));
        assert!(text.contains("# people"));
        assert!(text.contains("total"));
        assert!(text.lines().any(|l| l.contains("│") && l.contains("3")));
    }

    #[test]
    fn render_family_group_table_multi_row() {
        let mut table: FamilyGroupGenerationTable = BTreeMap::new();
        table.insert(
            "1".to_string(),
            BTreeMap::from([
                ("1".to_string(), 5),
                ("2".to_string(), 0),
                ("3".to_string(), 0),
                ("total".to_string(), 5),
            ]),
        );
        table.insert(
            "2".to_string(),
            BTreeMap::from([
                ("1".to_string(), 4),
                ("2".to_string(), 2),
                ("3".to_string(), 0),
                ("total".to_string(), 6),
            ]),
        );
        table.insert(
            "3".to_string(),
            BTreeMap::from([
                ("1".to_string(), 1),
                ("2".to_string(), 8),
                ("3".to_string(), 2),
                ("total".to_string(), 11),
            ]),
        );

        let text = render_family_group_table(&table);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains("Family group size × generation table"));
        assert!(lines[1].contains("# generations"));
        // Total row: 10, 10, 2, 22
        assert!(
            lines.last().unwrap().contains("10") && lines.last().unwrap().contains("22"),
            "Total row wrong: {}",
            lines.last().unwrap()
        );
        // Row totals appear
        assert!(text.contains("5"));
        assert!(text.contains("11"));
    }

    #[test]
    fn render_family_group_table_unicode_ascii() {
        let mut table: FamilyGroupGenerationTable = BTreeMap::new();
        table.insert(
            "2".to_string(),
            BTreeMap::from([("1".to_string(), 3), ("total".to_string(), 3)]),
        );

        let unicode_text = render_family_group_table(&table);
        let ascii_text = render_family_group_table_ascii(&table);

        assert!(unicode_text.contains('│'));
        assert!(!ascii_text.contains('│'));
        assert!(ascii_text.contains('|'));
        assert!(ascii_text.contains('+'));
        // Both contain the same data
        assert!(ascii_text.contains("3"));
        assert!(unicode_text.contains("3"));
    }

    #[test]
    fn format_text_report_contains_generation_table_section() {
        let mut table: FamilyGroupGenerationTable = BTreeMap::new();
        table.insert(
            "3".to_string(),
            BTreeMap::from([("2".to_string(), 1), ("total".to_string(), 1)]),
        );
        let report = StatsReport {
            file: "family.gramps".to_string(),
            counts: PrimaryTypeCounts {
                families: 1,
                ..PrimaryTypeCounts::default()
            },
            family_size_distribution: BTreeMap::from([(3, 1)]),
            family_group_generation_table: table,
            family_group_distribution: BTreeMap::from([(3, 1)]),
            people_not_in_family: 0,
            dangling_refs: 0,
            warnings: vec![],
        };

        let text = format_text_report(&report, false);
        assert!(text.contains("Family size distribution"));
        assert!(text.contains("Family group size × generation table"));
        assert!(text.contains("Family group distribution"));
        assert!(text.contains("# generations"));
        // Sections appear in order: distribution, generation table, then dangling refs
        let dist_pos = text.find("Family size distribution").unwrap();
        let fg_dist_pos = text.find("Family group distribution").unwrap();
        let table_pos = text.find("Family group size × generation table").unwrap();
        let people_pos = text.find("People not in any family").unwrap();
        assert!(dist_pos < fg_dist_pos);
        assert!(fg_dist_pos < table_pos);
        assert!(table_pos < people_pos);
    }

    #[test]
    fn format_text_report_warnings() {
        let report = StatsReport {
            file: "test.gramps".to_string(),
            counts: PrimaryTypeCounts::default(),
            family_size_distribution: BTreeMap::new(),
            family_group_generation_table: BTreeMap::new(),
            family_group_distribution: BTreeMap::new(),
            people_not_in_family: 0,
            dangling_refs: 0,
            warnings: vec![
                "WARNING: detected 1 cycle(s) in the family graph; generation layering may be approximate"
                    .to_string(),
            ],
        };

        let text = format_text_report(&report, false);
        assert!(text.contains("Warnings:"));
        assert!(text.contains("WARNING: detected 1 cycle(s)"));
        assert!(text.contains("  - WARNING: detected 1 cycle(s)"));
    }
}
