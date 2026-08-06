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

    let _ = writeln!(
        out,
        "[{}] {} (A: {}, B: {})",
        class_label, item.item_type, handle_a, handle_b
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

    /// Helper: build an item diff.
    fn item(
        handle_a: Option<&str>,
        handle_b: Option<&str>,
        item_type: &str,
        class: Classification,
        changes: Vec<FieldChange>,
    ) -> ItemDiff {
        ItemDiff {
            handle_a: handle_a.map(String::from),
            handle_b: handle_b.map(String::from),
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
}
