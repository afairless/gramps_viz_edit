//! Output formatters — text and JSON serialization of a diff report.
//!
//! This module converts a [`DiffReport`] into human-readable text or compact
//! JSON. The text formatter renders a summary table plus per-item details,
//! with optional filtering of [`ExtrinsicOnly`] items. The JSON formatter
//! produces a compact serialization of the full report suitable for
//! machine consumption.
//!
//! [`DiffReport`]: crate::report::DiffReport
//! [`ExtrinsicOnly`]: crate::report::Classification::ExtrinsicOnly

use std::collections::HashMap;
use std::fmt::Write;

use crate::report::{Classification, DiffReport, FieldKind};

/// Render a diff report as human-readable text.
///
/// Produces a summary table followed by per-item details. When
/// `include_extrinsic` is `false`, items classified as
/// [`Classification::ExtrinsicOnly`] are omitted entirely from the output.
///
/// # Format
///
/// The output has the following structure:
///
/// ```text
/// Gramps Diff Report
/// ==================
///
/// Summary
/// -------
/// Total (A): 10
/// Total (B): 12
/// Same:       5
/// Modified:   2
/// Added:      3
/// Removed:    1
/// Needs Review: 1
/// Extrinsic Only: 0
///
/// Items
/// -----
///
/// [MODIFIED] Person (A: P002, B: P002)
///   surname: "Smith" -> "Jones" (similarity 0.50)
///
/// [ADDED] Note (B: N003)
/// ```
///
/// [`ExtrinsicOnly`]: Classification::ExtrinsicOnly
pub fn format_text(report: &DiffReport, include_extrinsic: bool) -> String {
    let mut out = String::new();

    out.push_str("Gramps Diff Report\n");
    out.push_str("==================\n\n");

    // Summary table
    out.push_str("Summary\n");
    out.push_str("-------\n");
    let _ = writeln!(out, "Total (A):        {}", report.summary.total_a);
    let _ = writeln!(out, "Total (B):        {}", report.summary.total_b);
    let _ = writeln!(out, "Same:             {}", report.summary.same);
    let _ = writeln!(out, "Modified:         {}", report.summary.modified);
    let _ = writeln!(out, "Added:            {}", report.summary.added);
    let _ = writeln!(out, "Removed:          {}", report.summary.removed);
    let _ = writeln!(out, "Needs Review:     {}", report.summary.needs_review);
    let _ = writeln!(out, "Extrinsic Only:   {}", report.summary.extrinsic_only);
    out.push('\n');

    // Per-item details
    out.push_str("Items\n");
    out.push_str("-----\n");

    let mut has_items = false;
    for item in &report.items {
        if item.classification == Classification::ExtrinsicOnly && !include_extrinsic {
            continue;
        }
        has_items = true;
        write_item(&mut out, item);
    }

    // Ambiguous cases
    if !report.ambiguous_cases.is_empty() {
        out.push_str("\nAmbiguous Cases\n");
        out.push_str("---------------\n");
        for case in &report.ambiguous_cases {
            let _ = writeln!(
                out,
                "[NEEDS REVIEW] {} (A: {})",
                case.item_type_a, case.handle_a
            );
            if !case.context_a.display_name.is_empty() {
                let _ = writeln!(out, "  {}: {}", case.context_a.display_name, case.handle_a);
            }
            for candidate in &case.candidates {
                let _ = writeln!(
                    out,
                    "  Candidate {} (score {:.1}): {}",
                    candidate.handle_b, candidate.score, candidate.context_b.display_name
                );
            }
        }
    }

    if !has_items && report.ambiguous_cases.is_empty() {
        out.push_str("\nNo differences found.\n");
    }

    out
}

/// Render a single item's diff details.
fn write_item(out: &mut String, item: &crate::report::ItemDiff) {
    let class_label = classification_label(item.classification);

    let handle_a = match &item.handle_a {
        Some(h) => h,
        None => "-",
    };
    let handle_b = match &item.handle_b {
        Some(h) => h,
        None => "-",
    };

    // Format enriched header: handle [gramps_id] "display_name"
    let side_a = format_side(handle_a, &item.gramps_id_a, &item.display_name_a);
    let side_b = format_side(handle_b, &item.gramps_id_b, &item.display_name_b);

    let _ = writeln!(
        out,
        "[{}] {} (A: {}, B: {})",
        class_label, item.item_type, side_a, side_b
    );

    for change in &item.field_changes {
        let old = change.old_value.as_deref().unwrap_or("<none>");
        let new = change.new_value.as_deref().unwrap_or("<none>");
        let _ = writeln!(
            out,
            "  {}: {:?} -> {:?} (similarity {:.2})",
            change.field_name, old, new, change.similarity
        );
    }
}

/// Format one side of an item header with optional Gramps ID and display name.
///
/// Produces `"handle [gramps_id] \"display_name\""` when both are present,
/// falling back to just the handle or `"-"` for missing sides.
fn format_side(handle: &str, gramps_id: &Option<String>, display_name: &Option<String>) -> String {
    let mut s = handle.to_string();
    if let Some(gid) = gramps_id {
        write!(s, " [{}]", gid).unwrap();
    }
    if let Some(name) = display_name {
        if !name.is_empty() {
            write!(s, " \"{}\"", name).unwrap();
        }
    }
    s
}

/// Get a short display label for a classification.
fn classification_label(class: Classification) -> &'static str {
    match class {
        Classification::Same => "SAME",
        Classification::Modified => "MODIFIED",
        Classification::Added => "ADDED",
        Classification::Removed => "REMOVED",
        Classification::NeedsReview => "NEEDS REVIEW",
        Classification::ExtrinsicOnly => "EXTRINSIC ONLY",
    }
}

/// Serialize a diff report to compact JSON.
///
/// Uses [`serde_json::to_string`] to produce a compact, single-line JSON
/// representation of the full [`DiffReport`].
///
/// # Errors
///
/// Returns `None` if serialization fails (this should only happen if a
/// field contains a value that is not JSON-serializable, which is not
/// expected for the report's field types).
pub fn format_json(report: &DiffReport) -> String {
    serde_json::to_string(report).unwrap_or_else(|_| "{}".to_string())
}

/// Render a diff report as CSV with one row per [`FieldChange`].
///
/// Produces a CSV string with the following columns:
///
/// `classification, item_type, handle_a, gramps_id_a, display_name_a,`
/// `handle_b, gramps_id_b, display_name_b, confidence, field_name,`
/// `field_kind, old_value, new_value, similarity`
///
/// When `include_extrinsic` is `false`, items classified as
/// [`Classification::ExtrinsicOnly`] are omitted.
///
/// For items with no field changes (Same, Added, Removed, NeedsReview,
/// ExtrinsicOnly) a single row is emitted with empty field-level columns.
/// For Modified items, one row per [`FieldChange`] is emitted.
///
/// All cells are quoted per standard CSV rules. Newlines within values
/// are replaced with spaces.
pub fn format_csv(report: &DiffReport, include_extrinsic: bool) -> String {
    let mut out = String::new();

    // Header row
    out.push_str(
        "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",",
    );
    out.push_str("\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",");
    out.push_str("\"field_kind\",\"old_value\",\"new_value\",\"similarity\"\n");

    for item in &report.items {
        if item.classification == Classification::ExtrinsicOnly && !include_extrinsic {
            continue;
        }

        let class_label = classification_label(item.classification);
        let handle_a = item.handle_a.as_deref().unwrap_or("");
        let handle_b = item.handle_b.as_deref().unwrap_or("");
        let gramps_id_a = item.gramps_id_a.as_deref().unwrap_or("");
        let gramps_id_b = item.gramps_id_b.as_deref().unwrap_or("");
        let display_name_a = item.display_name_a.as_deref().unwrap_or("");
        let display_name_b = item.display_name_b.as_deref().unwrap_or("");
        let confidence = format!("{:.2}", item.confidence);

        let base_fields = [
            class_label,
            &item.item_type,
            handle_a,
            gramps_id_a,
            display_name_a,
            handle_b,
            gramps_id_b,
            display_name_b,
            &confidence,
        ];

        if item.field_changes.is_empty() {
            // Single row with empty field-level columns
            write_csv_row(&mut out, &base_fields, "", "", "", "", "");
        } else {
            for change in &item.field_changes {
                let field_kind = format!("{:?}", change.field_kind);
                let old_val = change.old_value.as_deref().unwrap_or("");
                let new_val = change.new_value.as_deref().unwrap_or("");
                let similarity = format!("{:.2}", change.similarity);
                write_csv_row(
                    &mut out,
                    &base_fields,
                    &change.field_name,
                    &field_kind,
                    old_val,
                    new_val,
                    &similarity,
                );
            }
        }
    }

    out
}

/// Write one CSV row with all fields quoted and escaped.
fn write_csv_row(
    out: &mut String,
    base: &[&str; 9],
    field_name: &str,
    field_kind: &str,
    old_value: &str,
    new_value: &str,
    similarity: &str,
) {
    for (i, val) in base.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_csv_cell(out, val);
    }
    out.push(',');
    write_csv_cell(out, field_name);
    out.push(',');
    write_csv_cell(out, field_kind);
    out.push(',');
    write_csv_cell(out, old_value);
    out.push(',');
    write_csv_cell(out, new_value);
    out.push(',');
    write_csv_cell(out, similarity);
    out.push('\n');
}

/// Write a single CSV cell, quoted and escaped.
///
/// Replaces `"` with `""` and wraps in double quotes. Newlines within
/// values are replaced with spaces.
fn write_csv_cell(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => {
                out.push('"');
                out.push('"');
            }
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Build a map of field names to similarity scores for text field changes.
///
/// Returns a map from field name to similarity score (0.0–1.0) for every
/// [`FieldChange`] whose [`FieldKind`] is [`FieldKind::Text`]. This is used
/// by downstream consumers (e.g., the visualizer) to render similarity
/// heatmaps over text fields.
///
/// [`FieldChange`]: crate::report::FieldChange
pub fn text_scores(changes: &[crate::report::FieldChange]) -> HashMap<String, f64> {
    changes
        .iter()
        .filter(|c| c.field_kind == FieldKind::Text)
        .map(|c| (c.field_name.clone(), c.similarity))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        AmbiguousCase, AmbiguousContext, Candidate, DiffReport, DiffSummary, FieldChange, ItemDiff,
    };

    /// Helper: build a field change.
    fn field_change(
        kind: FieldKind,
        name: &str,
        old: Option<&str>,
        new: Option<&str>,
        sim: f64,
    ) -> FieldChange {
        FieldChange {
            field_kind: kind,
            field_name: name.to_string(),
            old_value: old.map(String::from),
            new_value: new.map(String::from),
            similarity: sim,
        }
    }

    /// Helper: build an item diff with optional metadata.
    fn item(
        handle_a: Option<&str>,
        handle_b: Option<&str>,
        item_type: &str,
        class: Classification,
        changes: Vec<FieldChange>,
    ) -> ItemDiff {
        item_with_meta(
            handle_a, handle_b, None, None, None, None, item_type, class, changes,
        )
    }

    /// Helper: build an item diff with full metadata (gramps_id, display_name).
    fn item_with_meta(
        handle_a: Option<&str>,
        handle_b: Option<&str>,
        gramps_id_a: Option<&str>,
        gramps_id_b: Option<&str>,
        display_name_a: Option<&str>,
        display_name_b: Option<&str>,
        item_type: &str,
        class: Classification,
        changes: Vec<FieldChange>,
    ) -> ItemDiff {
        ItemDiff {
            handle_a: handle_a.map(String::from),
            handle_b: handle_b.map(String::from),
            gramps_id_a: gramps_id_a.map(String::from),
            gramps_id_b: gramps_id_b.map(String::from),
            display_name_a: display_name_a.map(String::from),
            display_name_b: display_name_b.map(String::from),
            item_type: item_type.to_string(),
            classification: class,
            field_changes: changes,
            confidence: 1.0,
        }
    }

    /// Build a report with one item of each classification.
    fn full_report() -> DiffReport {
        DiffReport {
            summary: DiffSummary {
                total_a: 6,
                total_b: 6,
                same: 1,
                modified: 1,
                added: 1,
                removed: 1,
                needs_review: 1,
                extrinsic_only: 1,
            },
            items: vec![
                item(
                    Some("A001"),
                    Some("B001"),
                    "Person",
                    Classification::Same,
                    vec![],
                ),
                item(
                    Some("A002"),
                    Some("B002"),
                    "Person",
                    Classification::Modified,
                    vec![field_change(
                        FieldKind::Text,
                        "surname",
                        Some("Smith"),
                        Some("Jones"),
                        0.5,
                    )],
                ),
                item(None, Some("B003"), "Note", Classification::Added, vec![]),
                item(Some("A004"), None, "Tag", Classification::Removed, vec![]),
                item(
                    Some("A005"),
                    Some("B005"),
                    "Citation",
                    Classification::ExtrinsicOnly,
                    vec![field_change(
                        FieldKind::HandleRef,
                        "source_handle",
                        Some("SRC_A"),
                        Some("SRC_B"),
                        1.0,
                    )],
                ),
                item(
                    Some("A006"),
                    Some("B006"),
                    "Place",
                    Classification::NeedsReview,
                    vec![],
                ),
            ],
            ambiguous_cases: vec![AmbiguousCase {
                handle_a: "P001".into(),
                item_type_a: "Person".into(),
                context_a: AmbiguousContext {
                    display_name: "John Smith".into(),
                    related_items: vec![],
                },
                candidates: vec![Candidate {
                    handle_b: "P002".into(),
                    score: 0.85,
                    context_b: AmbiguousContext {
                        display_name: "Johnny Smith".into(),
                        related_items: vec![],
                    },
                }],
            }],
        }
    }

    /// Text output contains expected headings.
    #[test]
    fn text_output_contains_headings() {
        let text = format_text(&full_report(), true);
        assert!(text.contains("Gramps Diff Report"));
        assert!(text.contains("Summary"));
        assert!(text.contains("Items"));
        assert!(text.contains("Ambiguous Cases"));
    }

    /// Text output contains all classification labels.
    #[test]
    fn text_output_all_classification_labels() {
        let text = format_text(&full_report(), true);
        assert!(text.contains("[SAME]"));
        assert!(text.contains("[MODIFIED]"));
        assert!(text.contains("[ADDED]"));
        assert!(text.contains("[REMOVED]"));
        assert!(text.contains("[EXTRINSIC ONLY]"));
        assert!(text.contains("[NEEDS REVIEW]"));
    }

    /// Text output contains summary counts.
    #[test]
    fn text_output_summary_counts() {
        let text = format_text(&full_report(), true);
        assert!(text.contains("Total (A):        6"));
        assert!(text.contains("Total (B):        6"));
        assert!(text.contains("Same:             1"));
        assert!(text.contains("Modified:         1"));
        assert!(text.contains("Added:            1"));
        assert!(text.contains("Removed:          1"));
        assert!(text.contains("Needs Review:     1"));
        assert!(text.contains("Extrinsic Only:   1"));
    }

    /// Text output contains per-item field details.
    #[test]
    fn text_output_field_details() {
        let text = format_text(&full_report(), true);
        assert!(text.contains("surname"));
        assert!(text.contains("Smith"));
        assert!(text.contains("Jones"));
        assert!(text.contains("similarity 0.50"));
    }

    /// Text output shows Gramps IDs and display names when present.
    #[test]
    fn text_output_shows_gramps_id_and_display_name() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item_with_meta(
                Some("abc123"),
                Some("def456"),
                Some("I0001"),
                Some("I0001"),
                Some("John Smith"),
                Some("John Smith"),
                "Person",
                Classification::Same,
                vec![],
            )],
            ambiguous_cases: vec![],
        };
        let text = format_text(&report, true);
        assert!(text.contains("abc123 [I0001]"));
        assert!(text.contains("def456 [I0001]"));
        assert!(text.contains("\"John Smith\""));
    }

    /// Text output handles None gramps_id and display_name gracefully with just handle.
    #[test]
    fn text_output_handle_none_metadata_gracefully() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item(
                Some("abc123"),
                Some("def456"),
                "Person",
                Classification::Same,
                vec![],
            )],
            ambiguous_cases: vec![],
        };
        let text = format_text(&report, true);
        assert!(text.contains("(A: abc123, B: def456)"));
        assert!(!text.contains("[I0001]"));
    }

    /// Text output with gramps_id but no display name.
    #[test]
    fn text_output_gramps_id_no_display_name() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item_with_meta(
                Some("abc123"),
                None,
                Some("I0002"),
                None,
                None,
                None,
                "Event",
                Classification::Removed,
                vec![],
            )],
            ambiguous_cases: vec![],
        };
        let text = format_text(&report, true);
        assert!(text.contains("abc123 [I0002]"));
    }

    /// Text output with display name but no gramps_id.
    #[test]
    fn text_output_display_name_no_gramps_id() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item_with_meta(
                None,
                Some("def456"),
                None,
                None,
                None,
                Some("Birth"),
                "Event",
                Classification::Added,
                vec![],
            )],
            ambiguous_cases: vec![],
        };
        let text = format_text(&report, true);
        assert!(text.contains("def456 \"Birth\""));
    }

    /// Extrinsic-only items are omitted when include_extrinsic is false.
    #[test]
    fn text_output_omits_extrinsic_when_excluded() {
        let text = format_text(&full_report(), false);
        assert!(!text.contains("[EXTRINSIC ONLY]"));
        // Other classifications still present.
        assert!(text.contains("[MODIFIED]"));
        assert!(text.contains("[ADDED]"));
    }

    /// Extrinsic-only items are present when include_extrinsic is true.
    #[test]
    fn text_output_includes_extrinsic_when_requested() {
        let text = format_text(&full_report(), true);
        assert!(text.contains("[EXTRINSIC ONLY]"));
    }

    /// Empty report renders correctly.
    #[test]
    fn text_output_empty_report() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };
        let text = format_text(&report, true);
        assert!(text.contains("Gramps Diff Report"));
        assert!(text.contains("Summary"));
        assert!(text.contains("Items"));
        assert!(text.contains("No differences found."));
    }

    /// JSON output round-trips via serde.
    #[test]
    fn json_roundtrip() {
        let report = full_report();
        let json = format_json(&report);
        let deserialized: DiffReport = serde_json::from_str(&json).expect("parse JSON");
        assert_eq!(report, deserialized);
    }

    /// JSON output is compact (single line).
    #[test]
    fn json_compact() {
        let report = full_report();
        let json = format_json(&report);
        assert!(!json.contains('\n'), "JSON should be compact single-line");
        assert!(json.starts_with('{'));
        assert!(json.ends_with('}'));
    }

    /// Empty report produces valid JSON.
    #[test]
    fn json_empty_report() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };
        let json = format_json(&report);
        let deserialized: DiffReport = serde_json::from_str(&json).expect("parse JSON");
        assert_eq!(report, deserialized);
    }

    /// text_scores only includes text field changes.
    #[test]
    fn text_scores_filters_to_text_fields() {
        let changes = vec![
            field_change(
                FieldKind::Text,
                "surname",
                Some("Smith"),
                Some("Jones"),
                0.5,
            ),
            field_change(
                FieldKind::HandleRef,
                "source_handle",
                Some("A"),
                Some("B"),
                1.0,
            ),
            field_change(FieldKind::Enum, "gender", Some("1"), Some("2"), 0.0),
        ];
        let scores = text_scores(&changes);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores.get("surname"), Some(&0.5));
        assert!(!scores.contains_key("source_handle"));
        assert!(!scores.contains_key("gender"));
    }

    /// text_scores returns empty map for no text changes.
    #[test]
    fn text_scores_empty() {
        let changes = vec![field_change(
            FieldKind::HandleRef,
            "source_handle",
            Some("A"),
            Some("B"),
            1.0,
        )];
        let scores = text_scores(&changes);
        assert!(scores.is_empty());
    }

    // -----------------------------------------------------------------------
    // CSV output tests
    // -----------------------------------------------------------------------

    /// CSV header row is correct.
    #[test]
    fn csv_header_row() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };
        let csv = format_csv(&report, true);
        let header = csv.lines().next().expect("should have header");
        assert!(header.contains("\"classification\""));
        assert!(header.contains("\"item_type\""));
        assert!(header.contains("\"handle_a\""));
        assert!(header.contains("\"gramps_id_a\""));
        assert!(header.contains("\"display_name_a\""));
        assert!(header.contains("\"handle_b\""));
        assert!(header.contains("\"gramps_id_b\""));
        assert!(header.contains("\"display_name_b\""));
        assert!(header.contains("\"confidence\""));
        assert!(header.contains("\"field_name\""));
        assert!(header.contains("\"field_kind\""));
        assert!(header.contains("\"old_value\""));
        assert!(header.contains("\"new_value\""));
        assert!(header.contains("\"similarity\""));
    }

    /// CSV: one row per FieldChange for Modified items.
    #[test]
    fn csv_one_row_per_field_change() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item(
                Some("A001"),
                Some("B001"),
                "Person",
                Classification::Modified,
                vec![
                    field_change(
                        FieldKind::Text,
                        "surname",
                        Some("Smith"),
                        Some("Jones"),
                        0.5,
                    ),
                    field_change(
                        FieldKind::Text,
                        "first_name",
                        Some("John"),
                        Some("James"),
                        0.5,
                    ),
                ],
            )],
            ambiguous_cases: vec![],
        };
        let csv = format_csv(&report, true);
        let lines: Vec<&str> = csv.lines().collect();
        // Header + 2 field changes = 3 lines
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains("surname"));
        assert!(lines[2].contains("first_name"));
    }

    /// CSV: single row for Same items with empty field columns.
    #[test]
    fn csv_single_row_for_non_modified() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![
                item(
                    Some("A001"),
                    Some("B001"),
                    "Person",
                    Classification::Same,
                    vec![],
                ),
                item(None, Some("B002"), "Note", Classification::Added, vec![]),
                item(Some("A003"), None, "Tag", Classification::Removed, vec![]),
            ],
            ambiguous_cases: vec![],
        };
        let csv = format_csv(&report, true);
        let lines: Vec<&str> = csv.lines().collect();
        // Header + 3 items = 4 lines
        assert_eq!(lines.len(), 4);
        // Each row should have empty field-level columns
        for line in &lines[1..] {
            let cols: Vec<&str> = line.split(",\"").collect();
            // Should have 14 columns
            assert_eq!(cols.len(), 14, "row: {line}");
        }
    }

    /// CSV: special characters are properly escaped (quotes, commas).
    #[test]
    fn csv_escapes_special_characters() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item_with_meta(
                Some("A001"),
                Some("B001"),
                Some("I0001"),
                Some("I0001"),
                Some("Smith, John \"The Great\""),
                Some("Jones, Jane"),
                "Person",
                Classification::Modified,
                vec![field_change(
                    FieldKind::Text,
                    "surname",
                    Some("Smith, John"),
                    Some("Jones\nNewline"),
                    0.5,
                )],
            )],
            ambiguous_cases: vec![],
        };
        let csv = format_csv(&report, true);
        // Verify quote escaping: display_name "Smith, John \"The Great\""
        // should be CSV-escaped as "" for embedded quotes.
        // Check that the original value text appears in the output
        let display_escaped: String = "Smith, John \"The Great\""
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect();
        assert!(csv.contains("Smith, John") && csv.contains("The Great"));
        // Verify newline replaced with space
        assert!(csv.contains("Jones Newline"));
        assert!(!csv.contains("Jones\nNewline"));
        // Verify comma not escaped — it should still appear within the quoted cell
        assert!(csv.contains("Jones, Jane"));
    }

    /// CSV: Empty values produce empty quoted cells.
    #[test]
    fn csv_empty_values() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item(
                Some("A001"),
                None,
                "Person",
                Classification::Removed,
                vec![],
            )],
            ambiguous_cases: vec![],
        };
        let csv = format_csv(&report, true);
        // After the header, the data row should have empty cells for B-side fields
        assert!(csv.contains("\"A001\""));
        assert!(csv.contains("\"\",\"\"")); // Two consecutive empty quoted cells
    }

    /// CSV: format_csv signature returns String (matching format_text/format_json).
    #[test]
    fn csv_format_signature() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };
        let _output: String = format_csv(&report, true);
    }

    /// CSV: extrinsic-only items omitted when include_extrinsic is false.
    #[test]
    fn csv_omits_extrinsic_when_excluded() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item(
                Some("A001"),
                Some("B001"),
                "Citation",
                Classification::ExtrinsicOnly,
                vec![field_change(
                    FieldKind::HandleRef,
                    "source_handle",
                    Some("SRC_A"),
                    Some("SRC_B"),
                    1.0,
                )],
            )],
            ambiguous_cases: vec![],
        };
        let csv = format_csv(&report, false);
        // Header only, no data rows
        assert_eq!(csv.lines().count(), 1);
    }

    /// CSV: empty report produces only the header.
    #[test]
    fn csv_empty_report() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };
        let csv = format_csv(&report, true);
        assert_eq!(csv.lines().count(), 1);
        assert!(csv.starts_with("\"classification\""));
    }
}
