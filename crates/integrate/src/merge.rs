//! Handle-based diff-viz merging logic.
//!
//! This module matches diff CSV rows (filtered to `item_type == "Person"`)
//! against visualizer selections by handle, producing combined [`MergedRow`]s.

use serde::{Deserialize, Serialize};

use crate::csv_reader::DiffRow;
use crate::json_reader::Selection;

/// The provenance of a merged row — which data sources contributed to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    /// Person appears in both diff and selections — full merge.
    Matched,
    /// Person appears only in the diff CSV — viz fields are empty.
    DiffOnly,
    /// Person appears only in the visualizer selections — diff fields are empty.
    VizOnly,
}

/// A combined row from merging a diff CSV row with a visualizer selection.
///
/// Contains all original diff fields plus the matching side and visualizer
/// selection data. The field names are designed to match the CSV header
/// columns for direct round-trip deserialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergedRow {
    // --- Diff fields (from DiffRow) ---
    pub classification: String,
    pub item_type: String,
    pub handle_a: Option<String>,
    pub gramps_id_a: Option<String>,
    pub display_name_a: Option<String>,
    pub handle_b: Option<String>,
    pub gramps_id_b: Option<String>,
    pub display_name_b: Option<String>,
    pub confidence: f64,
    pub field_name: String,
    pub field_kind: String,
    pub old_value: String,
    pub new_value: String,
    #[serde(deserialize_with = "crate::csv_reader::deserialize_f64_empty_as_zero")]
    pub similarity: f64,
    // --- Merge metadata ---
    /// Which side matched: "a" (match via handle_a) or "b" (match via handle_b).
    pub side: String,
    /// Whether this row is matched, diff-only, or viz-only.
    pub row_kind: RowKind,
    // --- Visualizer selection fields ---
    /// Person's full name from the visualizer.
    pub viz_name: Option<String>,
    /// Birth date from the visualizer.
    pub viz_birth_date: Option<String>,
    /// Death date from the visualizer.
    pub viz_death_date: Option<String>,
    /// Gender from the visualizer.
    pub viz_gender: Option<String>,
    /// DSU family group ID from the visualizer.
    pub viz_family_group: Option<usize>,
}

/// Merge diff CSV rows with visualizer selections by handle (full outer join).
///
/// The function:
/// 1. Filters diff rows to `item_type == "Person"` (logs a warning if zero remain)
/// 2. Builds a `HashMap` of selections keyed by handle
/// 3. Phase 1 — For each Person diff row, tries `handle_a` first, then `handle_b`:
///    - If matched, emits a [`MergedRow`] with the side label and [`RowKind::Matched`]
///    - If unmatched, emits a [`MergedRow`] with [`RowKind::DiffOnly`] (viz fields are `None`)
/// 4. Phase 2 — Emits [`RowKind::VizOnly`] rows for unmatched selections
/// 5. Returns all rows combined
///
/// Logs a warning (via `log::warn!`) when both inputs are empty or contain no Person rows.
///
/// # Panics
///
/// Does not panic — returns an empty vec if both inputs are empty.
pub fn merge_diff_viz(diff_rows: Vec<DiffRow>, selections: Vec<Selection>) -> Vec<MergedRow> {
    // Filter to Person rows only
    let person_rows: Vec<DiffRow> = diff_rows
        .into_iter()
        .filter(|r| r.item_type == "Person")
        .collect();

    if person_rows.is_empty() && selections.is_empty() {
        log::warn!(
            "no data to integrate: diff CSV contains no Person rows and selections are empty"
        );
        return vec![];
    }

    // Build selection index
    let selection_map: std::collections::HashMap<&str, &Selection> =
        selections.iter().map(|s| (s.handle.as_str(), s)).collect();

    let mut matched_handles: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut merged = Vec::new();

    // Phase 1: Process diff rows — matched become Matched, unmatched become DiffOnly
    for row in person_rows {
        // Try handle_a first, then handle_b
        let matched = row
            .handle_a
            .as_ref()
            .and_then(|h| selection_map.get(h.as_str()))
            .or_else(|| {
                row.handle_b
                    .as_ref()
                    .and_then(|h| selection_map.get(h.as_str()))
            });

        if let Some(sel) = matched {
            matched_handles.insert(sel.handle.clone());
            let side = if selection_map.contains_key(row.handle_a.as_deref().unwrap_or("")) {
                "a"
            } else {
                "b"
            };
            merged.push(MergedRow {
                // Diff fields
                classification: row.classification,
                item_type: row.item_type,
                handle_a: row.handle_a,
                gramps_id_a: row.gramps_id_a,
                display_name_a: row.display_name_a,
                handle_b: row.handle_b,
                gramps_id_b: row.gramps_id_b,
                display_name_b: row.display_name_b,
                confidence: row.confidence,
                field_name: row.field_name,
                field_kind: row.field_kind,
                old_value: row.old_value,
                new_value: row.new_value,
                similarity: row.similarity,
                // Merge metadata
                side: side.to_string(),
                row_kind: RowKind::Matched,
                // Selection fields
                viz_name: Some(sel.name.clone()),
                viz_birth_date: sel.birth_date.clone(),
                viz_death_date: sel.death_date.clone(),
                viz_gender: Some(sel.gender.clone()),
                viz_family_group: Some(sel.family_group),
            });
        } else {
            // DiffOnly: diff fields populated, viz fields None
            merged.push(MergedRow {
                classification: row.classification,
                item_type: row.item_type,
                handle_a: row.handle_a,
                gramps_id_a: row.gramps_id_a,
                display_name_a: row.display_name_a,
                handle_b: row.handle_b,
                gramps_id_b: row.gramps_id_b,
                display_name_b: row.display_name_b,
                confidence: row.confidence,
                field_name: row.field_name,
                field_kind: row.field_kind,
                old_value: row.old_value,
                new_value: row.new_value,
                similarity: row.similarity,
                side: String::new(),
                row_kind: RowKind::DiffOnly,
                viz_name: None,
                viz_birth_date: None,
                viz_death_date: None,
                viz_gender: None,
                viz_family_group: None,
            });
        }
    }

    // Phase 2: Emit VizOnly rows for unmatched selections
    for sel in &selections {
        if !matched_handles.contains(&sel.handle) {
            merged.push(MergedRow {
                classification: String::new(),
                item_type: "Person".to_string(),
                handle_a: None,
                gramps_id_a: None,
                display_name_a: None,
                handle_b: None,
                gramps_id_b: None,
                display_name_b: None,
                confidence: 0.0,
                field_name: String::new(),
                field_kind: String::new(),
                old_value: String::new(),
                new_value: String::new(),
                similarity: 0.0,
                side: String::new(),
                row_kind: RowKind::VizOnly,
                viz_name: Some(sel.name.clone()),
                viz_birth_date: sel.birth_date.clone(),
                viz_death_date: sel.death_date.clone(),
                viz_gender: Some(sel.gender.clone()),
                viz_family_group: Some(sel.family_group),
            });
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal DiffRow.
    fn diff_row(
        classification: &str,
        handle_a: Option<&str>,
        handle_b: Option<&str>,
        field_name: &str,
    ) -> DiffRow {
        DiffRow {
            classification: classification.to_string(),
            item_type: "Person".to_string(),
            handle_a: handle_a.map(String::from),
            gramps_id_a: None,
            display_name_a: None,
            handle_b: handle_b.map(String::from),
            gramps_id_b: None,
            display_name_b: None,
            confidence: 1.0,
            field_name: field_name.to_string(),
            field_kind: String::new(),
            old_value: String::new(),
            new_value: String::new(),
            similarity: 0.0,
        }
    }

    /// Helper: build a non-Person diff row (Family).
    fn family_row(classification: &str, handle_a: Option<&str>, handle_b: Option<&str>) -> DiffRow {
        DiffRow {
            classification: classification.to_string(),
            item_type: "Family".to_string(),
            handle_a: handle_a.map(String::from),
            gramps_id_a: None,
            display_name_a: None,
            handle_b: handle_b.map(String::from),
            gramps_id_b: None,
            display_name_b: None,
            confidence: 1.0,
            field_name: String::new(),
            field_kind: String::new(),
            old_value: String::new(),
            new_value: String::new(),
            similarity: 0.0,
        }
    }

    /// Helper: build a Selection.
    fn sel(handle: &str, name: &str, family_group: usize) -> Selection {
        Selection {
            handle: handle.to_string(),
            name: name.to_string(),
            birth_date: None,
            death_date: None,
            gender: "male".to_string(),
            family_group,
        }
    }

    /// Match via handle_a → side = "a".
    #[test]
    fn match_handle_a_side_a() {
        let rows = vec![diff_row("Modified", Some("H001"), Some("H002"), "surname")];
        let selections = vec![sel("H001", "John Smith", 1)];
        let result = merge_diff_viz(rows, selections);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].side, "a");
        assert_eq!(result[0].row_kind, RowKind::Matched);
        assert_eq!(result[0].viz_name.as_deref(), Some("John Smith"));
        assert_eq!(result[0].viz_family_group, Some(1));
    }

    /// Match via handle_b → side = "b".
    #[test]
    fn match_handle_b_side_b() {
        let rows = vec![diff_row("Modified", Some("H001"), Some("H002"), "surname")];
        let selections = vec![sel("H002", "Jane Doe", 2)];
        let result = merge_diff_viz(rows, selections);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].side, "b");
        assert_eq!(result[0].row_kind, RowKind::Matched);
        assert_eq!(result[0].viz_name.as_deref(), Some("Jane Doe"));
        assert_eq!(result[0].viz_family_group, Some(2));
    }

    /// Added person (handle_a=None) matching handle_b → side = "b".
    #[test]
    fn match_added_handle_b() {
        let rows = vec![diff_row("Added", None, Some("H002"), "")];
        let selections = vec![sel("H002", "Added Person", 0)];
        let result = merge_diff_viz(rows, selections);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].side, "b");
        assert_eq!(result[0].row_kind, RowKind::Matched);
        assert_eq!(result[0].viz_name.as_deref(), Some("Added Person"));
    }

    /// Removed person (handle_b=None) matching handle_a → side = "a".
    #[test]
    fn match_removed_handle_a() {
        let rows = vec![diff_row("Removed", Some("H001"), None, "")];
        let selections = vec![sel("H001", "Removed Person", 0)];
        let result = merge_diff_viz(rows, selections);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].side, "a");
        assert_eq!(result[0].row_kind, RowKind::Matched);
        assert_eq!(result[0].viz_name.as_deref(), Some("Removed Person"));
    }

    /// Person not in selections (neither handle matches) → DiffOnly row + VizOnly row.
    #[test]
    fn no_match_diff_only() {
        let rows = vec![diff_row("Modified", Some("H001"), Some("H002"), "surname")];
        let selections = vec![sel("H999", "Other Person", 0)];
        let result = merge_diff_viz(rows, selections);
        // 1 DiffOnly row + 1 VizOnly row = 2 rows
        assert_eq!(result.len(), 2);
        // Row 0: DiffOnly — diff fields populated, viz fields None
        assert_eq!(result[0].row_kind, RowKind::DiffOnly);
        assert_eq!(result[0].side, "");
        assert_eq!(result[0].handle_a.as_deref(), Some("H001"));
        assert_eq!(result[0].viz_name, None);
        // Row 1: VizOnly — diff fields empty, viz fields populated
        assert_eq!(result[1].row_kind, RowKind::VizOnly);
        assert_eq!(result[1].side, "");
        assert_eq!(result[1].handle_a, None);
        assert_eq!(result[1].viz_name.as_deref(), Some("Other Person"));
    }

    /// Same handle on both sides → side = "a" (first match wins).
    #[test]
    fn same_handle_both_sides_side_a() {
        let rows = vec![diff_row("Same", Some("H001"), Some("H001"), "")];
        let selections = vec![sel("H001", "Same Person", 1)];
        let result = merge_diff_viz(rows, selections);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].side, "a");
        assert_eq!(result[0].row_kind, RowKind::Matched);
    }

    /// Empty selections → all diff rows emitted as DiffOnly.
    #[test]
    fn empty_selections_diff_only() {
        let rows = vec![diff_row("Modified", Some("H001"), Some("H002"), "surname")];
        let result = merge_diff_viz(rows, vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].row_kind, RowKind::DiffOnly);
        assert_eq!(result[0].side, "");
        assert_eq!(result[0].viz_name, None);
    }

    /// Person rows but none match selections → DiffOnly + VizOnly rows.
    #[test]
    fn person_rows_no_match_diff_only() {
        let rows = vec![diff_row("Modified", Some("H001"), Some("H002"), "surname")];
        let selections = vec![sel("H003", "Unrelated", 0)];
        let result = merge_diff_viz(rows, selections);
        // 1 DiffOnly + 1 VizOnly = 2 rows
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].row_kind, RowKind::DiffOnly);
        assert_eq!(result[1].row_kind, RowKind::VizOnly);
    }

    /// No Person rows in diff → VizOnly rows for selections.
    #[test]
    fn no_person_rows_viz_only() {
        let rows = vec![family_row("Same", Some("F001"), Some("F001"))];
        let selections = vec![sel("F001", "Family Name", 0)];
        let result = merge_diff_viz(rows, selections);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].row_kind, RowKind::VizOnly);
        assert_eq!(result[0].viz_name.as_deref(), Some("Family Name"));
    }

    /// Multiple rows, mixed matches and non-matches.
    #[test]
    fn mixed_matches() {
        let rows = vec![
            diff_row("Modified", Some("H001"), Some("H002"), "surname"),
            diff_row("Modified", Some("H003"), Some("H004"), "given_name"),
            diff_row("Modified", Some("H005"), Some("H006"), "surname"),
        ];
        let selections = vec![
            sel("H001", "Matched A", 1),
            sel("H004", "Matched B", 2), // H003 not in index, but H004 is
            sel("H007", "VizOnly C", 3), // no matching diff row
        ];
        let result = merge_diff_viz(rows, selections);
        // 2 matched + 1 diff-only (H005/H006) + 1 viz-only (H007) = 4
        assert_eq!(result.len(), 4);

        // H001 matched via handle_a
        assert_eq!(result[0].side, "a");
        assert_eq!(result[0].row_kind, RowKind::Matched);
        assert_eq!(result[0].viz_name.as_deref(), Some("Matched A"));

        // H003 NOT matched via handle_a, but H004 matched via handle_b
        assert_eq!(result[1].side, "b");
        assert_eq!(result[1].row_kind, RowKind::Matched);
        assert_eq!(result[1].viz_name.as_deref(), Some("Matched B"));

        // H005/H006: DiffOnly (no matching selection)
        assert_eq!(result[2].row_kind, RowKind::DiffOnly);
        assert_eq!(result[2].side, "");
        assert_eq!(result[2].handle_a.as_deref(), Some("H005"));
        assert_eq!(result[2].viz_name, None);

        // H007: VizOnly (no matching diff row)
        assert_eq!(result[3].row_kind, RowKind::VizOnly);
        assert_eq!(result[3].side, "");
        assert_eq!(result[3].handle_a, None);
        assert_eq!(result[3].viz_name.as_deref(), Some("VizOnly C"));
    }

    /// A diff-only row is emitted with RowKind::DiffOnly and None viz fields.
    #[test]
    fn emits_diff_only_row() {
        let rows = vec![diff_row("Added", Some("H001"), None, "")];
        let selections = vec![sel("H002", "Other Person", 0)]; // no match
        let result = merge_diff_viz(rows, selections);
        // Should contain at least one DiffOnly row
        let diff_only: Vec<_> = result
            .iter()
            .filter(|r| r.row_kind == RowKind::DiffOnly)
            .collect();
        assert_eq!(diff_only.len(), 1);
        assert_eq!(diff_only[0].handle_a.as_deref(), Some("H001"));
        assert_eq!(diff_only[0].viz_name, None);
        assert_eq!(diff_only[0].viz_gender, None);
        assert_eq!(diff_only[0].viz_family_group, None);
        assert_eq!(diff_only[0].side, "");
    }

    /// A viz-only row is emitted with RowKind::VizOnly and empty diff fields.
    #[test]
    fn emits_viz_only_row() {
        let rows = vec![diff_row("Modified", Some("H001"), None, "surname")];
        let selections = vec![sel("H999", "Viz Person", 5)]; // no match
        let result = merge_diff_viz(rows, selections);
        let viz_only: Vec<_> = result
            .iter()
            .filter(|r| r.row_kind == RowKind::VizOnly)
            .collect();
        assert_eq!(viz_only.len(), 1);
        assert_eq!(viz_only[0].handle_a, None);
        assert_eq!(viz_only[0].classification, "");
        assert_eq!(viz_only[0].confidence, 0.0);
        assert_eq!(viz_only[0].viz_name.as_deref(), Some("Viz Person"));
        assert_eq!(viz_only[0].viz_family_group, Some(5));
        assert_eq!(viz_only[0].side, "");
    }

    /// Full outer join: 2 diff rows + 3 selections → 1 Matched + 1 DiffOnly + 2 VizOnly
    /// = 4 rows total.
    #[test]
    fn full_outer_join_4_rows() {
        let rows = vec![
            diff_row("Modified", Some("H001"), None, "surname"),
            diff_row("Added", None, Some("H003"), ""),
        ];
        let selections = vec![
            sel("H001", "Matched Person", 1),
            sel("H999", "Viz Only A", 2),
            sel("H888", "Viz Only B", 3),
        ];
        let result = merge_diff_viz(rows, selections);
        // 1 Matched + 1 DiffOnly + 2 VizOnly = 4
        assert_eq!(result.len(), 4);

        // Row 0: H001 matched (Matched)
        assert_eq!(result[0].row_kind, RowKind::Matched);
        assert_eq!(result[0].side, "a");
        assert_eq!(result[0].viz_name.as_deref(), Some("Matched Person"));

        // Row 1: H003 not in selections (DiffOnly)
        assert_eq!(result[1].row_kind, RowKind::DiffOnly);
        assert_eq!(result[1].side, "");
        assert_eq!(result[1].handle_b.as_deref(), Some("H003"));
        assert_eq!(result[1].viz_name, None);

        // Row 2: H999 (VizOnly)
        assert_eq!(result[2].row_kind, RowKind::VizOnly);
        assert_eq!(result[2].side, "");
        assert_eq!(result[2].handle_a, None);
        assert_eq!(result[2].viz_name.as_deref(), Some("Viz Only A"));

        // Row 3: H888 (VizOnly)
        assert_eq!(result[3].row_kind, RowKind::VizOnly);
        assert_eq!(result[3].side, "");
        assert_eq!(result[3].handle_a, None);
        assert_eq!(result[3].viz_name.as_deref(), Some("Viz Only B"));
    }

    /// MergedRow preserves all diff fields from the original row.
    #[test]
    fn preserves_diff_fields() {
        let rows = vec![DiffRow {
            classification: "Modified".to_string(),
            item_type: "Person".to_string(),
            handle_a: Some("H001".to_string()),
            gramps_id_a: Some("I0001".to_string()),
            display_name_a: Some("Old Name".to_string()),
            handle_b: Some("H002".to_string()),
            gramps_id_b: Some("I0002".to_string()),
            display_name_b: Some("New Name".to_string()),
            confidence: 0.95,
            field_name: "surname".to_string(),
            field_kind: "Text".to_string(),
            old_value: "Smith".to_string(),
            new_value: "Jones".to_string(),
            similarity: 0.5,
        }];
        let selections = vec![sel("H001", "John Smith", 3)];
        let result = merge_diff_viz(rows, selections);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].row_kind, RowKind::Matched);
        assert_eq!(result[0].classification, "Modified");
        assert_eq!(result[0].gramps_id_a.as_deref(), Some("I0001"));
        assert_eq!(result[0].display_name_a.as_deref(), Some("Old Name"));
        assert_eq!(result[0].confidence, 0.95);
        assert_eq!(result[0].field_name, "surname");
        assert_eq!(result[0].old_value, "Smith");
        assert_eq!(result[0].new_value, "Jones");
        assert!((result[0].similarity - 0.5).abs() < 1e-10);
    }
}
