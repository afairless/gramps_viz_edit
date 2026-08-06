//! Visualizer index output format.
//!
//! This module produces the JSON payload consumed by the graph visualizer.
//! The format is a compact index that maps item handles to their diff
//! classification and intrinsic field changes, so the visualizer can color
//! and annotate nodes without re-parsing the full report.
//!
//! # Output structure
//!
//! ```json
//! {
//!   "handle_map": {
//!     "B001": "A001",
//!     "B002": "A002"
//!   },
//!   "entries": {
//!     "A001": { "class": "Same" },
//!     "A002": {
//!       "class": "Modified",
//!       "intrinsic_fields": ["surname"],
//!       "text_scores": { "surname": 0.5 }
//!     },
//!     "B003": { "class": "Added" }
//!   }
//! }
//! ```

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::output::text_scores;
use crate::report::{Classification, DiffReport, FieldKind, ItemDiff};

/// A single entry in the visualizer index, keyed by handle.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct VisualizerEntry {
    /// The diff classification of the item.
    pub class: String,
    /// Names of intrinsic field changes (present for MODIFIED items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intrinsic_fields: Option<Vec<String>>,
    /// Similarity scores for text field changes (present for MODIFIED items
    /// with text changes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_scores: Option<HashMap<String, f64>>,
}

/// The visualizer index — a handle map plus per-handle entries.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct VisualizerIndex {
    /// Maps B-handles → A-handles for all matched items.
    pub handle_map: HashMap<String, String>,
    /// Per-handle entries keyed by the item's primary handle.
    pub entries: BTreeMap<String, VisualizerEntry>,
}

/// Produce the compact JSON visualizer index for a diff report.
///
/// The output contains a `handle_map` (all matched A↔B handle pairs) and a
/// per-handle `entries` map. Each entry includes the item's `class`, plus
/// `intrinsic_fields` and `text_scores` for [`Modified`] items.
///
/// # Entry keys
///
/// Each item is keyed by its primary handle — `handle_a` when present,
/// otherwise `handle_b` (which only happens for [`Added`] items).
///
/// # Intrinsic fields
///
/// `intrinsic_fields` lists the names of field changes that are not handle
/// references (`HandleRef` / `HandleRefList`). Handle-reference changes are
/// considered extrinsic and are excluded, matching the semantics of the
/// cascading module.
///
/// [`Modified`]: Classification::Modified
/// [`Added`]: Classification::Added
pub fn format_visualizer(report: &DiffReport) -> String {
    let index = build_index(report);
    serde_json::to_string(&index).unwrap_or_else(|_| "{}".to_string())
}

/// Build the [`VisualizerIndex`] struct from a report.
fn build_index(report: &DiffReport) -> VisualizerIndex {
    let mut handle_map: HashMap<String, String> = HashMap::new();
    let mut entries: BTreeMap<String, VisualizerEntry> = BTreeMap::new();

    for item in &report.items {
        let primary_key = item.handle_a.clone().or_else(|| item.handle_b.clone());

        let Some(key) = primary_key else {
            // An item with neither handle is malformed; skip it.
            continue;
        };

        // Record matched handle pairs (both handles present).
        if let (Some(handle_a), Some(handle_b)) = (&item.handle_a, &item.handle_b) {
            handle_map.insert(handle_b.clone(), handle_a.clone());
        }

        entries.insert(key, entry_for_item(item));
    }

    VisualizerIndex {
        handle_map,
        entries,
    }
}

/// Build a single [`VisualizerEntry`] for an item.
fn entry_for_item(item: &ItemDiff) -> VisualizerEntry {
    // Intrinsic fields: names of changes that are not handle references.
    let intrinsic_fields: Vec<String> = item
        .field_changes
        .iter()
        .filter(|c| {
            !matches!(
                c.field_kind,
                FieldKind::HandleRef | FieldKind::HandleRefList
            )
        })
        .map(|c| c.field_name.clone())
        .collect();

    let text_changes = text_scores(&item.field_changes);

    let intrinsic_fields = if item.classification == Classification::Modified {
        Some(intrinsic_fields)
    } else {
        None
    };

    let text_scores = if item.classification == Classification::Modified && !text_changes.is_empty()
    {
        Some(text_changes)
    } else {
        None
    };

    VisualizerEntry {
        class: class_string(item.classification),
        intrinsic_fields,
        text_scores,
    }
}

/// Convert a classification to its display string.
fn class_string(class: Classification) -> String {
    match class {
        Classification::Same => "Same",
        Classification::Modified => "Modified",
        Classification::Added => "Added",
        Classification::Removed => "Removed",
        Classification::NeedsReview => "NeedsReview",
        Classification::ExtrinsicOnly => "ExtrinsicOnly",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        AmbiguousCase, AmbiguousContext, Candidate, DiffSummary, FieldChange, ItemDiff,
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
            gramps_id_a: None,
            gramps_id_b: None,
            display_name_a: None,
            display_name_b: None,
            item_type: item_type.to_string(),
            classification: class,
            field_changes: changes,
            confidence: 1.0,
        }
    }

    /// Build a report with one item of each classification.
    fn six_class_report() -> DiffReport {
        DiffReport {
            summary: DiffSummary::default(),
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
                    vec![
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
                            Some("SRC_A"),
                            Some("SRC_B"),
                            1.0,
                        ),
                    ],
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

    /// Output parses as JSON.
    #[test]
    fn output_parses_as_json() {
        let json = format_visualizer(&six_class_report());
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse JSON");
        assert!(parsed.is_object());
    }

    /// All 6 Classification variants are represented.
    #[test]
    fn all_classifications_represented() {
        let index = build_index(&six_class_report());
        let mut classes: Vec<&str> = index.entries.values().map(|e| e.class.as_str()).collect();
        classes.sort();
        assert_eq!(
            classes,
            vec![
                "Added",
                "ExtrinsicOnly",
                "Modified",
                "NeedsReview",
                "Removed",
                "Same",
            ]
        );
    }

    /// handle_map keys/values match report items.
    #[test]
    fn handle_map_matches_items() {
        let index = build_index(&six_class_report());
        // B001→A001, B002→A002, B005→A005, B006→A006
        assert_eq!(index.handle_map.len(), 4);
        assert_eq!(
            index.handle_map.get("B001").map(String::as_str),
            Some("A001")
        );
        assert_eq!(
            index.handle_map.get("B002").map(String::as_str),
            Some("A002")
        );
        assert_eq!(
            index.handle_map.get("B005").map(String::as_str),
            Some("A005")
        );
        assert_eq!(
            index.handle_map.get("B006").map(String::as_str),
            Some("A006")
        );
        // Added (B003) and Removed (A004) have only one handle → not in map.
        assert!(!index.handle_map.contains_key("B003"));
        assert!(!index.handle_map.contains_key("A004"));
    }

    /// intrinsic_fields appear for MODIFIED items.
    #[test]
    fn intrinsic_fields_for_modified() {
        let index = build_index(&six_class_report());
        let modified = index.entries.get("A002").expect("A002 entry");
        assert_eq!(modified.class, "Modified");
        // Only the text change is intrinsic; the handle-ref change is excluded.
        assert_eq!(
            modified.intrinsic_fields.as_deref(),
            Some(&["surname".to_string()][..])
        );
    }

    /// intrinsic_fields are absent for non-MODIFIED items.
    #[test]
    fn intrinsic_fields_absent_for_non_modified() {
        let index = build_index(&six_class_report());
        let same = index.entries.get("A001").expect("A001 entry");
        assert!(same.intrinsic_fields.is_none());
        assert!(same.text_scores.is_none());
    }

    /// text_scores appear for MODIFIED items with text field changes.
    #[test]
    fn text_scores_for_modified_with_text_changes() {
        let index = build_index(&six_class_report());
        let modified = index.entries.get("A002").expect("A002 entry");
        let scores = modified.text_scores.as_ref().expect("text_scores present");
        assert_eq!(scores.get("surname"), Some(&0.5));
    }

    /// text_scores are absent for MODIFIED items without text changes.
    #[test]
    fn text_scores_absent_without_text_changes() {
        // A Modified item with only handle-ref changes.
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![item(
                Some("A010"),
                Some("B010"),
                "Citation",
                Classification::Modified,
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
        let index = build_index(&report);
        let entry = index.entries.get("A010").expect("A010 entry");
        assert_eq!(entry.intrinsic_fields.as_deref(), Some(&[][..]));
        assert!(entry.text_scores.is_none());
    }

    /// Empty report produces valid JSON.
    #[test]
    fn empty_report_valid_json() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };
        let json = format_visualizer(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse JSON");
        assert_eq!(
            parsed["handle_map"],
            serde_json::Value::Object(Default::default())
        );
        assert_eq!(
            parsed["entries"],
            serde_json::Value::Object(Default::default())
        );
    }

    /// Added items are keyed by their B handle.
    #[test]
    fn added_item_keyed_by_b_handle() {
        let index = build_index(&six_class_report());
        let added = index.entries.get("B003").expect("B003 entry");
        assert_eq!(added.class, "Added");
    }

    /// Removed items are keyed by their A handle.
    #[test]
    fn removed_item_keyed_by_a_handle() {
        let index = build_index(&six_class_report());
        let removed = index.entries.get("A004").expect("A004 entry");
        assert_eq!(removed.class, "Removed");
    }
}
