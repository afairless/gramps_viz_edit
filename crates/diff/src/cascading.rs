//! Pass 2 cascading diff resolution — extrinsic/cascading change detection.
//!
//! After Pass 1 matching produces a [`MatchResult`] with a handle map and
//! per-item diffs, Pass 2 re-evaluates handle-reference field changes
//! through the handle map. A handle-reference change that is resolved by
//! the handle map (the B-side handle refers to the same underlying item as
//! the A-side handle, just with a different handle value) is classified as
//! **extrinsic** — it does not represent a semantic change to the item.
//!
//! Items whose only changes are extrinsic handle remappings are
//! reclassified from [`Modified`] to [`ExtrinsicOnly`].
//!
//! [`MatchResult`]: crate::matcher::MatchResult
//! [`Modified`]: crate::report::Classification::Modified
//! [`ExtrinsicOnly`]: crate::report::Classification::ExtrinsicOnly

use std::collections::{HashMap, HashSet};

use typed_graph::Handle;

use crate::report::{Classification, FieldChange, FieldKind, ItemDiff};

/// Resolve extrinsic handle-reference changes through the handle map.
///
/// For each matched pair (both `handle_a` and `handle_b` present), re-evaluates
/// every [`FieldChange`] whose [`FieldKind`] is [`HandleRef`] or [`HandleRefList`]
/// against the handle map.
///
/// A handle-ref change is **extrinsic** when the B-side handle, looked up
/// through the handle map, equals the A-side handle — meaning the referenced
/// item is the same, only the handle value changed. Non-handle-ref changes
/// are always intrinsic.
///
/// # Classification rules
///
/// | Condition | Result |
/// |---|---|
/// | Matched pair has **only** extrinsic handle-ref changes | Reclassified to [`ExtrinsicOnly`] |
/// | Matched pair has a mix of intrinsic and extrinsic changes | Remains [`Modified`] |
/// | [`Same`], [`Added`], [`Removed`], [`NeedsReview`] | Pass through unchanged |
///
/// # Extrinsic detection
///
/// For a [`HandleRef`] change: `handle_map.get(b_handle) == Some(a_handle)`
///
/// For a [`HandleRefList`] change: the set of B-side handles, when remapped
/// through the handle map, equals the set of A-side handles. The remapping
/// is checked as a set comparison (unordered) to match the set-based semantics
/// of [`compare_handle_array`].
///
/// [`HandleRef`]: FieldKind::HandleRef
/// [`HandleRefList`]: FieldKind::HandleRefList
/// [`Same`]: Classification::Same
/// [`Added`]: Classification::Added
/// [`Removed`]: Classification::Removed
/// [`NeedsReview`]: Classification::NeedsReview
/// [`compare_handle_array`]: crate::compare::compare_handle_array
pub fn resolve_extrinsic(
    item_diffs: Vec<ItemDiff>,
    handle_map: &HashMap<Handle, Handle>,
) -> Vec<ItemDiff> {
    item_diffs
        .into_iter()
        .map(|diff| resolve_item(diff, handle_map))
        .collect()
}

/// Resolve extrinsic changes for a single item.
fn resolve_item(mut diff: ItemDiff, handle_map: &HashMap<Handle, Handle>) -> ItemDiff {
    // Only matched pairs (both handles present) classified as Modified
    // can be reclassified. Unmatched or non-Modified items pass through.
    if diff.handle_a.is_none()
        || diff.handle_b.is_none()
        || diff.classification != Classification::Modified
    {
        return diff;
    }

    // Classify each field change as intrinsic or extrinsic.
    let mut all_extrinsic = true;
    let mut has_extrinsic = false;

    for change in &diff.field_changes {
        if is_extrinsic_change(change, handle_map) {
            has_extrinsic = true;
        } else {
            all_extrinsic = false;
        }
    }

    // Reclassify if all changes are extrinsic (and at least one exists).
    if all_extrinsic && has_extrinsic {
        diff.classification = Classification::ExtrinsicOnly;
    }

    diff
}

/// Determine whether a single field change is extrinsic.
///
/// Returns `true` if the change is a handle-ref change that is resolved
/// by the handle map (the B-side handle refers to the same underlying
/// item as the A-side handle).
fn is_extrinsic_change(change: &FieldChange, handle_map: &HashMap<Handle, Handle>) -> bool {
    match change.field_kind {
        FieldKind::HandleRef => is_extrinsic_handle_ref(change, handle_map),
        FieldKind::HandleRefList => is_extrinsic_handle_ref_list(change, handle_map),
        _ => false,
    }
}

/// Check if a single handle-ref change is extrinsic.
///
/// A handle-ref change is extrinsic when the B-side handle, looked up
/// through the handle map, equals the A-side handle.
fn is_extrinsic_handle_ref(change: &FieldChange, handle_map: &HashMap<Handle, Handle>) -> bool {
    let a_handle = match &change.old_value {
        Some(h) => h,
        None => return false,
    };
    let b_handle = match &change.new_value {
        Some(h) => h,
        None => return false,
    };

    // Look up the B-side handle in the handle map. If it maps to the
    // A-side handle, the change is extrinsic (same item, different handle).
    handle_map.get(b_handle.as_str()) == Some(a_handle)
}

/// Check if a handle-ref-list change is extrinsic.
///
/// Parses the bracket-delimited, comma-separated handle lists from both
/// old and new values, then checks whether the set of B-side handles,
/// remapped through the handle map, equals the set of A-side handles.
fn is_extrinsic_handle_ref_list(
    change: &FieldChange,
    handle_map: &HashMap<Handle, Handle>,
) -> bool {
    let a_value = match &change.old_value {
        Some(v) => v,
        None => return false,
    };
    let b_value = match &change.new_value {
        Some(v) => v,
        None => return false,
    };

    let a_handles = parse_handle_list(a_value);
    let b_handles = parse_handle_list(b_value);

    if a_handles.len() != b_handles.len() {
        return false;
    }

    // Remap B-side handles through the handle map and compare as sets.
    let remapped_b: HashSet<&str> = b_handles
        .iter()
        .filter_map(|h| handle_map.get(h.as_str()).map(|s| s.as_str()))
        .collect();

    let a_set: HashSet<&str> = a_handles.iter().map(|s| s.as_str()).collect();

    // All B-side handles must be remappable, and the remapped sets must match.
    if remapped_b.len() != a_handles.len() {
        return false;
    }

    remapped_b == a_set
}

/// Parse a handle list from the string format produced by
/// [`compare_handle_array`].
///
/// The format is `[handle1, handle2, handle3]` — handles are comma-separated
/// within square brackets.
///
/// Returns an empty vec for empty lists (`[]`) or malformed input.
fn parse_handle_list(value: &str) -> Vec<String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or("");

    if inner.is_empty() {
        return vec![];
    }

    inner
        .split(", ")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Classification;

    // -----------------------------------------------------------------------
    // Helper: build a simple FieldChange
    // -----------------------------------------------------------------------

    fn make_field_change(
        field_kind: FieldKind,
        field_name: &str,
        old_value: Option<&str>,
        new_value: Option<&str>,
    ) -> FieldChange {
        FieldChange {
            field_kind,
            field_name: field_name.to_string(),
            old_value: old_value.map(|s| s.to_string()),
            new_value: new_value.map(|s| s.to_string()),
            similarity: 0.0,
        }
    }

    fn make_handle_ref(old: &str, new: &str) -> FieldChange {
        make_field_change(FieldKind::HandleRef, "source_handle", Some(old), Some(new))
    }

    fn make_handle_ref_list(old: &[&str], new: &[&str]) -> FieldChange {
        let old_str = format!("[{}]", old.join(", "));
        let new_str = format!("[{}]", new.join(", "));
        make_field_change(
            FieldKind::HandleRefList,
            "citation_ref_list",
            Some(&old_str),
            Some(&new_str),
        )
    }

    fn make_text_change(field_name: &str, old: &str, new: &str) -> FieldChange {
        make_field_change(FieldKind::Text, field_name, Some(old), Some(new))
    }

    fn make_item_diff(
        handle_a: Option<&str>,
        handle_b: Option<&str>,
        classification: Classification,
        field_changes: Vec<FieldChange>,
    ) -> ItemDiff {
        ItemDiff {
            handle_a: handle_a.map(|s| s.to_string()),
            handle_b: handle_b.map(|s| s.to_string()),
            item_type: "Citation".to_string(),
            classification,
            field_changes,
            confidence: 1.0,
        }
    }

    // -----------------------------------------------------------------------
    // parse_handle_list
    // -----------------------------------------------------------------------

    #[test]
    fn parse_handle_list_empty_brackets() {
        assert!(parse_handle_list("[]").is_empty());
    }

    #[test]
    fn parse_handle_list_single() {
        assert_eq!(parse_handle_list("[H001]"), vec!["H001"]);
    }

    #[test]
    fn parse_handle_list_multiple() {
        assert_eq!(
            parse_handle_list("[H001, H002, H003]"),
            vec!["H001", "H002", "H003"]
        );
    }

    #[test]
    fn parse_handle_list_malformed_no_brackets() {
        assert!(parse_handle_list("H001, H002").is_empty());
    }

    // -----------------------------------------------------------------------
    // Extrinsic-only: citation with same source_handle content but different
    // handle value, resolved by the handle map
    // -----------------------------------------------------------------------

    #[test]
    fn extrinsic_only_handle_ref() {
        // Citation S001 in A has source_handle = "SRC_A"
        // Citation S001 in B has source_handle = "SRC_B"
        // handle_map maps SRC_B → SRC_A (same source, different handle)
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B".to_string(), "SRC_A".to_string())].into();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![make_handle_ref("SRC_A", "SRC_B")],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::ExtrinsicOnly);
        // Field changes should still be present
        assert_eq!(result[0].field_changes.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Intrinsic + extrinsic mix: same citation but page text also changed
    // -----------------------------------------------------------------------

    #[test]
    fn intrinsic_and_extrinsic_mix_remains_modified() {
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B".to_string(), "SRC_A".to_string())].into();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![
                make_handle_ref("SRC_A", "SRC_B"),
                make_text_change("page", "10", "20"),
            ],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Modified);
        // Both field changes should still be present
        assert_eq!(result[0].field_changes.len(), 2);
    }

    // -----------------------------------------------------------------------
    // No remap needed: identical handles and fields → unchanged Same
    // -----------------------------------------------------------------------

    #[test]
    fn unchanged_same_passes_through() {
        let handle_map: HashMap<Handle, Handle> = HashMap::new();

        let diff = make_item_diff(Some("S001"), Some("S001"), Classification::Same, vec![]);

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Same);
    }

    // -----------------------------------------------------------------------
    // Unmatched items pass through unchanged
    // -----------------------------------------------------------------------

    #[test]
    fn added_passes_through() {
        let handle_map: HashMap<Handle, Handle> = HashMap::new();

        let diff = make_item_diff(None, Some("S001"), Classification::Added, vec![]);

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Added);
    }

    #[test]
    fn removed_passes_through() {
        let handle_map: HashMap<Handle, Handle> = HashMap::new();

        let diff = make_item_diff(Some("S001"), None, Classification::Removed, vec![]);

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Removed);
    }

    #[test]
    fn needs_review_passes_through() {
        let handle_map: HashMap<Handle, Handle> = HashMap::new();

        let diff = make_item_diff(
            Some("S001"),
            Some("S002"),
            Classification::NeedsReview,
            vec![],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::NeedsReview);
    }

    // -----------------------------------------------------------------------
    // Edge case: B-side handle not in handle_map → treated as intrinsic
    // -----------------------------------------------------------------------

    #[test]
    fn b_side_handle_not_in_map_is_intrinsic() {
        let handle_map: HashMap<Handle, Handle> = HashMap::new();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![make_handle_ref("SRC_A", "SRC_B")],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        // SRC_B is not in the handle_map, so the change is intrinsic → stays Modified
        assert_eq!(result[0].classification, Classification::Modified);
    }

    // -----------------------------------------------------------------------
    // Edge case: empty handle_map → all handle-ref changes are intrinsic
    // -----------------------------------------------------------------------

    #[test]
    fn empty_handle_map_all_handle_ref_changes_intrinsic() {
        let handle_map: HashMap<Handle, Handle> = HashMap::new();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![
                make_handle_ref("SRC_A", "SRC_B"),
                make_handle_ref("REPO_A", "REPO_B"),
            ],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Modified);
    }

    // -----------------------------------------------------------------------
    // Edge case: empty item_diffs → empty output
    // -----------------------------------------------------------------------

    #[test]
    fn empty_item_diffs_returns_empty() {
        let handle_map: HashMap<Handle, Handle> = HashMap::new();
        let result = resolve_extrinsic(vec![], &handle_map);
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // HandleRefList extrinsic detection
    // -----------------------------------------------------------------------

    #[test]
    fn extrinsic_only_handle_ref_list() {
        // Citation with citation_ref_list = [SRC_A1, SRC_A2] in A,
        // [SRC_B1, SRC_B2] in B. handle_map maps B1→A1, B2→A2.
        let handle_map: HashMap<Handle, Handle> = [
            ("SRC_B1".to_string(), "SRC_A1".to_string()),
            ("SRC_B2".to_string(), "SRC_A2".to_string()),
        ]
        .into();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![make_handle_ref_list(
                &["SRC_A1", "SRC_A2"],
                &["SRC_B1", "SRC_B2"],
            )],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::ExtrinsicOnly);
    }

    #[test]
    fn extrinsic_handle_ref_list_with_mixed_intrinsic_remains_modified() {
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B1".to_string(), "SRC_A1".to_string())].into();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![
                make_handle_ref_list(&["SRC_A1"], &["SRC_B1"]),
                make_text_change("page", "10", "20"),
            ],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Modified);
    }

    #[test]
    fn extrinsic_handle_ref_list_not_all_remappable_remains_modified() {
        // SRC_B2 is not in the handle_map → the list change is not fully extrinsic
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B1".to_string(), "SRC_A1".to_string())].into();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![make_handle_ref_list(
                &["SRC_A1", "SRC_A2"],
                &["SRC_B1", "SRC_B2"],
            )],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Modified);
    }

    #[test]
    fn extrinsic_handle_ref_list_different_sizes_remains_modified() {
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B1".to_string(), "SRC_A1".to_string())].into();

        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![make_handle_ref_list(&["SRC_A1", "SRC_A2"], &["SRC_B1"])],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Modified);
    }

    // -----------------------------------------------------------------------
    // Multiple items: some extrinsic, some intrinsic, some unmatched
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_items_mixed_classifications() {
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B".to_string(), "SRC_A".to_string())].into();

        let diffs = vec![
            // ExtrinsicOnly: only handle-ref change, resolved by map
            make_item_diff(
                Some("S001"),
                Some("S001"),
                Classification::Modified,
                vec![make_handle_ref("SRC_A", "SRC_B")],
            ),
            // Modified: handle-ref change + text change
            make_item_diff(
                Some("S002"),
                Some("S002"),
                Classification::Modified,
                vec![
                    make_handle_ref("SRC_A", "SRC_B"),
                    make_text_change("page", "10", "20"),
                ],
            ),
            // Same: passes through unchanged
            make_item_diff(Some("S003"), Some("S003"), Classification::Same, vec![]),
            // Added: passes through unchanged
            make_item_diff(None, Some("S004"), Classification::Added, vec![]),
            // Removed: passes through unchanged
            make_item_diff(Some("S005"), None, Classification::Removed, vec![]),
        ];

        let result = resolve_extrinsic(diffs, &handle_map);
        assert_eq!(result.len(), 5);

        assert_eq!(result[0].classification, Classification::ExtrinsicOnly);
        assert_eq!(result[1].classification, Classification::Modified);
        assert_eq!(result[2].classification, Classification::Same);
        assert_eq!(result[3].classification, Classification::Added);
        assert_eq!(result[4].classification, Classification::Removed);
    }

    // -----------------------------------------------------------------------
    // Modified with no extrinsic changes stays Modified
    // -----------------------------------------------------------------------

    #[test]
    fn modified_with_only_intrinsic_changes_stays_modified() {
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B".to_string(), "SRC_A".to_string())].into();

        // Only text changes, no handle-ref changes
        let diff = make_item_diff(
            Some("S001"),
            Some("S001"),
            Classification::Modified,
            vec![make_text_change("page", "10", "20")],
        );

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].classification, Classification::Modified);
    }

    // -----------------------------------------------------------------------
    // Modified with no field changes (shouldn't happen, but handle gracefully)
    // -----------------------------------------------------------------------

    #[test]
    fn modified_with_no_field_changes_stays_modified() {
        let handle_map: HashMap<Handle, Handle> =
            [("SRC_B".to_string(), "SRC_A".to_string())].into();

        let diff = make_item_diff(Some("S001"), Some("S001"), Classification::Modified, vec![]);

        let result = resolve_extrinsic(vec![diff], &handle_map);
        assert_eq!(result.len(), 1);
        // No field changes means `all_extrinsic` is true but `has_extrinsic` is false
        // → stays Modified
        assert_eq!(result[0].classification, Classification::Modified);
    }
}
