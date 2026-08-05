//! Field-level comparison functions for Gramps data types.
//!
//! This module provides shared field-comparison helpers and per-type
//! comparison functions. Helpers handle text fields (with normalization
//! and similarity scoring), optional fields, handle-ref arrays, date
//! values, enum discriminants, and ref-struct arrays.
//!
//! Per-type functions delegate to these helpers and produce
//! [`Vec<FieldChange>`][crate::report::FieldChange] for each differing field.

use std::collections::BTreeSet;

use typed_graph::{ChildRef, DateValue, EventRef, Handle, MediaRef, PersonRef, PlaceRef, RepoRef};

use crate::report::{FieldChange, FieldKind};

// ---------------------------------------------------------------------------
// Trait for ref-struct types that carry a target handle
// ---------------------------------------------------------------------------

/// Trait implemented by ref-struct types that carry a target handle
/// in their `ref_field` field.
pub trait HasRefField {
    /// Return the target handle for this reference.
    fn ref_field(&self) -> &str;
}

impl HasRefField for EventRef {
    fn ref_field(&self) -> &str {
        &self.ref_field
    }
}

impl HasRefField for ChildRef {
    fn ref_field(&self) -> &str {
        &self.ref_field
    }
}

impl HasRefField for PersonRef {
    fn ref_field(&self) -> &str {
        &self.ref_field
    }
}

impl HasRefField for RepoRef {
    fn ref_field(&self) -> &str {
        &self.ref_field
    }
}

impl HasRefField for PlaceRef {
    fn ref_field(&self) -> &str {
        &self.ref_field
    }
}

impl HasRefField for MediaRef {
    fn ref_field(&self) -> &str {
        &self.ref_field
    }
}

// ---------------------------------------------------------------------------
// Text field helpers
// ---------------------------------------------------------------------------

/// Compare two non-optional text fields.
///
/// Normalizes both values (case-fold + collapse whitespace) and scores
/// via [`tokenized_levenshtein`][crate::similarity::tokenized_levenshtein].
/// Returns an empty `Vec` when the normalized values are identical.
pub fn compare_field_text(field_name: &str, a: &str, b: &str) -> Vec<FieldChange> {
    use crate::normalize;
    use crate::similarity;

    let norm_a = normalize::collapse_whitespace(&normalize::case_fold(a));
    let norm_b = normalize::collapse_whitespace(&normalize::case_fold(b));

    if norm_a == norm_b {
        return vec![];
    }

    let score = similarity::tokenized_levenshtein(&norm_a, &norm_b);

    vec![FieldChange {
        field_kind: FieldKind::Text,
        field_name: field_name.to_string(),
        old_value: Some(a.to_string()),
        new_value: Some(b.to_string()),
        similarity: score,
    }]
}

/// Compare two optional text fields.
///
/// - `(None, None)` → empty
/// - `(Some, None)` or `(None, Some)` → [`FieldChange`] with similarity 0.0
/// - `(Some, Some)` → delegates to [`compare_field_text`]
pub fn compare_field_optional_text(
    field_name: &str,
    a: Option<&str>,
    b: Option<&str>,
) -> Vec<FieldChange> {
    match (a, b) {
        (None, None) => vec![],
        (Some(va), Some(vb)) => compare_field_text(field_name, va, vb),
        _ => vec![FieldChange {
            field_kind: FieldKind::Text,
            field_name: field_name.to_string(),
            old_value: a.map(|s| s.to_string()),
            new_value: b.map(|s| s.to_string()),
            similarity: 0.0,
        }],
    }
}

// ---------------------------------------------------------------------------
// Handle array helper
// ---------------------------------------------------------------------------

/// Compare two handle arrays using unordered set comparison.
///
/// Reordering alone produces no change. Returns a [`FieldChange`] with
/// [`FieldKind::HandleRefList`] when the sets (as sorted multisets) differ.
pub fn compare_handle_array(field_name: &str, a: &[Handle], b: &[Handle]) -> Vec<FieldChange> {
    let mut sorted_a: Vec<&str> = a.iter().map(|h| h.as_str()).collect();
    let mut sorted_b: Vec<&str> = b.iter().map(|h| h.as_str()).collect();
    sorted_a.sort_unstable();
    sorted_b.sort_unstable();

    if sorted_a == sorted_b {
        return vec![];
    }

    // Compute Jaccard similarity over distinct elements
    let set_a: BTreeSet<&str> = sorted_a.iter().copied().collect();
    let set_b: BTreeSet<&str> = sorted_b.iter().copied().collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    let similarity = if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    };

    vec![FieldChange {
        field_kind: FieldKind::HandleRefList,
        field_name: field_name.to_string(),
        old_value: Some(format!("[{}]", a.join(", "))),
        new_value: Some(format!("[{}]", b.join(", "))),
        similarity,
    }]
}

// ---------------------------------------------------------------------------
// Handle ref helper (single optional handle)
// ---------------------------------------------------------------------------

/// Compare two optional handle references.
///
/// - `(None, None)` → empty
/// - `(Some, None)` or `(None, Some)` → [`FieldChange`] with similarity 0.0
/// - `(Some(a), Some(b))` → change only if handles differ
pub fn compare_handle_ref(
    field_name: &str,
    a: Option<&Handle>,
    b: Option<&Handle>,
) -> Vec<FieldChange> {
    match (a, b) {
        (None, None) => vec![],
        (Some(ha), Some(hb)) if ha == hb => vec![],
        _ => vec![FieldChange {
            field_kind: FieldKind::HandleRef,
            field_name: field_name.to_string(),
            old_value: a.map(|h| h.to_string()),
            new_value: b.map(|h| h.to_string()),
            similarity: 0.0,
        }],
    }
}

// ---------------------------------------------------------------------------
// Date value helper
// ---------------------------------------------------------------------------

/// Compare two optional [`DateValue`] fields.
///
/// Delegates to [`DateValue::display_text()`] for textual comparison.
/// Returns a [`FieldChange`] with [`FieldKind::Date`] when the display
/// texts differ.
pub fn compare_date_value(
    field_name: &str,
    a: Option<&DateValue>,
    b: Option<&DateValue>,
) -> Vec<FieldChange> {
    match (a, b) {
        (None, None) => vec![],
        (Some(_), None) | (None, Some(_)) => vec![FieldChange {
            field_kind: FieldKind::Date,
            field_name: field_name.to_string(),
            old_value: a.map(|d| d.display_text()),
            new_value: b.map(|d| d.display_text()),
            similarity: 0.0,
        }],
        (Some(da), Some(db)) => {
            let text_a = da.display_text();
            let text_b = db.display_text();
            if text_a == text_b {
                return vec![];
            }
            let score = crate::similarity::tokenized_levenshtein(&text_a, &text_b);
            vec![FieldChange {
                field_kind: FieldKind::Date,
                field_name: field_name.to_string(),
                old_value: Some(text_a),
                new_value: Some(text_b),
                similarity: score,
            }]
        }
    }
}

// ---------------------------------------------------------------------------
// Enum discriminant helper
// ---------------------------------------------------------------------------

/// Compare two enum discriminants.
///
/// Returns a [`FieldChange`] with [`FieldKind::Enum`] when the values
/// differ. Uses `Debug` formatting for the string representation.
pub fn compare_enum_discriminant<T: PartialEq + std::fmt::Debug>(
    field_name: &str,
    a: T,
    b: T,
) -> Vec<FieldChange> {
    if a == b {
        return vec![];
    }
    vec![FieldChange {
        field_kind: FieldKind::Enum,
        field_name: field_name.to_string(),
        old_value: Some(format!("{:?}", a)),
        new_value: Some(format!("{:?}", b)),
        similarity: 0.0,
    }]
}

// ---------------------------------------------------------------------------
// Ref-struct array helper
// ---------------------------------------------------------------------------

/// Compare two ref-struct arrays by their target handle identity
/// (unordered set comparison). When the handle sets match, the
/// `compare_meta` closure is called for each pair of refs (matched
/// by sorted handle) to produce metadata-specific [`FieldChange`] entries.
pub fn compare_ref_array<T, F>(
    field_name: &str,
    a: &[T],
    b: &[T],
    compare_meta: F,
) -> Vec<FieldChange>
where
    T: HasRefField,
    F: Fn(&T, &T) -> Vec<FieldChange>,
{
    let mut changes: Vec<FieldChange> = Vec::new();

    // Build sorted lists of ref_field handles
    let mut a_fields: Vec<&str> = a.iter().map(|r| r.ref_field()).collect();
    let mut b_fields: Vec<&str> = b.iter().map(|r| r.ref_field()).collect();
    a_fields.sort_unstable();
    b_fields.sort_unstable();

    if a_fields != b_fields {
        changes.push(FieldChange {
            field_kind: FieldKind::HandleRefList,
            field_name: field_name.to_string(),
            old_value: Some(format!("[{}]", a_fields.join(", "))),
            new_value: Some(format!("[{}]", b_fields.join(", "))),
            similarity: 0.0,
        });
        return changes;
    }

    // Sets match — compare metadata pairwise
    let mut a_sorted: Vec<&T> = a.iter().collect();
    let mut b_sorted: Vec<&T> = b.iter().collect();
    a_sorted.sort_by_key(|r| r.ref_field());
    b_sorted.sort_by_key(|r| r.ref_field());

    for (ra, rb) in a_sorted.iter().zip(b_sorted.iter()) {
        changes.extend(compare_meta(ra, rb));
    }

    changes
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use typed_graph::DateValue;

    // -----------------------------------------------------------------------
    // compare_field_text
    // -----------------------------------------------------------------------

    #[test]
    fn text_identical() {
        assert!(compare_field_text("name", "John", "John").is_empty());
    }

    #[test]
    fn text_different() {
        let changes = compare_field_text("name", "John", "Jane");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::Text);
        assert_eq!(changes[0].field_name, "name");
        assert_eq!(changes[0].old_value.as_deref(), Some("John"));
        assert_eq!(changes[0].new_value.as_deref(), Some("Jane"));
        assert!(changes[0].similarity > 0.0 && changes[0].similarity < 1.0);
    }

    #[test]
    fn text_normalized_identical() {
        // Case and whitespace normalization should make these identical
        assert!(compare_field_text("name", "John Smith", "john  smith").is_empty());
    }

    #[test]
    fn text_empty() {
        assert!(compare_field_text("name", "", "").is_empty());
        let changes = compare_field_text("name", "a", "");
        assert_eq!(changes.len(), 1);
        assert!((changes[0].similarity - 0.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // compare_field_optional_text
    // -----------------------------------------------------------------------

    #[test]
    fn opt_text_both_none() {
        assert!(compare_field_optional_text("desc", None, None).is_empty());
    }

    #[test]
    fn opt_text_one_none() {
        let changes = compare_field_optional_text("desc", Some("hello"), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::Text);
        assert!((changes[0].similarity - 0.0).abs() < 1e-9);
    }

    #[test]
    fn opt_text_both_some() {
        let changes = compare_field_optional_text("desc", Some("hello"), Some("hallo"));
        assert_eq!(changes.len(), 1);
        assert!(changes[0].similarity > 0.0 && changes[0].similarity < 1.0);
    }

    #[test]
    fn opt_text_both_some_identical() {
        assert!(compare_field_optional_text("desc", Some("hello"), Some("hello")).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_handle_array
    // -----------------------------------------------------------------------

    #[test]
    fn handle_array_identical_order() {
        let a = vec!["H001".into(), "H002".into()];
        let b = vec!["H001".into(), "H002".into()];
        assert!(compare_handle_array("family_list", &a, &b).is_empty());
    }

    #[test]
    fn handle_array_reordered() {
        let a = vec!["H001".into(), "H002".into()];
        let b = vec!["H002".into(), "H001".into()];
        assert!(compare_handle_array("family_list", &a, &b).is_empty());
    }

    #[test]
    fn handle_array_different() {
        let a = vec!["H001".into(), "H002".into()];
        let b = vec!["H001".into(), "H003".into()];
        let changes = compare_handle_array("family_list", &a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::HandleRefList);
    }

    #[test]
    fn handle_array_empty() {
        assert!(compare_handle_array("list", &[], &[]).is_empty());
    }

    #[test]
    fn handle_array_one_empty() {
        let a = vec!["H001".into()];
        let changes = compare_handle_array("list", &a, &[]);
        assert_eq!(changes.len(), 1);
        assert!((changes[0].similarity - 0.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // compare_handle_ref
    // -----------------------------------------------------------------------

    #[test]
    fn handle_ref_both_none() {
        assert!(compare_handle_ref("father_handle", None, None).is_empty());
    }

    #[test]
    fn handle_ref_one_none() {
        let changes = compare_handle_ref("father_handle", Some(&"H001".into()), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::HandleRef);
        assert!((changes[0].similarity - 0.0).abs() < 1e-9);
    }

    #[test]
    fn handle_ref_same_handle() {
        let h: Handle = "H001".into();
        assert!(compare_handle_ref("father_handle", Some(&h), Some(&h)).is_empty());
    }

    #[test]
    fn handle_ref_different_handle() {
        let a: Handle = "H001".into();
        let b: Handle = "H002".into();
        let changes = compare_handle_ref("father_handle", Some(&a), Some(&b));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].old_value.as_deref(), Some("H001"));
        assert_eq!(changes[0].new_value.as_deref(), Some("H002"));
    }

    // -----------------------------------------------------------------------
    // compare_date_value
    // -----------------------------------------------------------------------

    #[test]
    fn date_both_none() {
        assert!(compare_date_value("date", None, None).is_empty());
    }

    #[test]
    fn date_one_none() {
        let d = DateValue::new(1870);
        let changes = compare_date_value("date", Some(&d), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::Date);
        assert!((changes[0].similarity - 0.0).abs() < 1e-9);
    }

    #[test]
    fn date_identical() {
        let d = DateValue::new(1870);
        assert!(compare_date_value("date", Some(&d), Some(&d)).is_empty());
    }

    #[test]
    fn date_different() {
        let a = DateValue::new(1870);
        let b = DateValue::new(1900);
        let changes = compare_date_value("date", Some(&a), Some(&b));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::Date);
        assert!(changes[0].similarity > 0.0 && changes[0].similarity <= 1.0);
    }

    // -----------------------------------------------------------------------
    // compare_enum_discriminant
    // -----------------------------------------------------------------------

    #[test]
    fn enum_identical() {
        assert!(compare_enum_discriminant("gender", 0i32, 0i32).is_empty());
    }

    #[test]
    fn enum_different() {
        let changes = compare_enum_discriminant("gender", 0i32, 1i32);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::Enum);
        assert!((changes[0].similarity - 0.0).abs() < 1e-9);
    }

    #[test]
    fn enum_typed_identical() {
        use typed_graph::EventType;
        assert!(
            compare_enum_discriminant("event_type", EventType::Birth, EventType::Birth).is_empty()
        );
    }

    #[test]
    fn enum_typed_different() {
        use typed_graph::EventType;
        let changes = compare_enum_discriminant("event_type", EventType::Birth, EventType::Death);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].old_value.as_deref(), Some("Birth"));
        assert_eq!(changes[0].new_value.as_deref(), Some("Death"));
    }

    // -----------------------------------------------------------------------
    // compare_ref_array
    // -----------------------------------------------------------------------

    #[test]
    fn ref_array_identical() {
        let a = vec![EventRef {
            ref_field: "E001".into(),
            role: None,
        }];
        let b = vec![EventRef {
            ref_field: "E001".into(),
            role: None,
        }];
        assert!(compare_ref_array("event_ref_list", &a, &b, |_, _| vec![]).is_empty());
    }

    #[test]
    fn ref_array_reordered() {
        let a = vec![
            EventRef {
                ref_field: "E001".into(),
                role: None,
            },
            EventRef {
                ref_field: "E002".into(),
                role: None,
            },
        ];
        let b = vec![
            EventRef {
                ref_field: "E002".into(),
                role: None,
            },
            EventRef {
                ref_field: "E001".into(),
                role: None,
            },
        ];
        assert!(compare_ref_array("event_ref_list", &a, &b, |_, _| vec![]).is_empty());
    }

    #[test]
    fn ref_array_different_handle() {
        let a = vec![EventRef {
            ref_field: "E001".into(),
            role: None,
        }];
        let b = vec![EventRef {
            ref_field: "E002".into(),
            role: None,
        }];
        let changes = compare_ref_array("event_ref_list", &a, &b, |_, _| vec![]);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::HandleRefList);
    }

    #[test]
    fn ref_array_metadata_compared() {
        let a = vec![EventRef {
            ref_field: "E001".into(),
            role: Some(typed_graph::EventRoleType::Primary),
        }];
        let b = vec![EventRef {
            ref_field: "E001".into(),
            role: Some(typed_graph::EventRoleType::Witness),
        }];
        let changes = compare_ref_array("event_ref_list", &a, &b, |x, y| {
            compare_enum_discriminant("event_ref_list.role", x.role, y.role)
        });
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_kind, FieldKind::Enum);
        assert_eq!(changes[0].field_name, "event_ref_list.role");
    }

    #[test]
    fn ref_array_empty() {
        let a: Vec<EventRef> = vec![];
        let b: Vec<EventRef> = vec![];
        assert!(compare_ref_array("event_ref_list", &a, &b, |_, _| vec![]).is_empty());
    }
}
