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

use typed_graph::{
    event_type_display, get_source_handle, Address, Attribute, ChildRef, CitationData, DateValue,
    EventData, EventRef, FamilyData, Handle, LdsOrd, Location, MediaData, MediaRef, Name, NoteData,
    PersonData, PersonRef, PlaceData, PlaceRef, RepoRef, RepositoryData, SourceData, Surname,
    TagData, Url,
};

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
// Per-type compare functions
// ---------------------------------------------------------------------------

/// Compare two [`Surname`] structs field by field.
fn compare_surname(field_prefix: &str, a: &Surname, b: &Surname) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.surname"),
        a.surname.as_deref(),
        b.surname.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.prefix"),
        a.prefix.as_deref(),
        b.prefix.as_deref(),
    ));
    if a.primary != b.primary {
        changes.push(FieldChange {
            field_kind: FieldKind::Boolean,
            field_name: format!("{field_prefix}.primary"),
            old_value: Some(format!("{:?}", a.primary)),
            new_value: Some(format!("{:?}", b.primary)),
            similarity: 0.0,
        });
    }
    if a.origintype != b.origintype {
        changes.push(FieldChange {
            field_kind: FieldKind::Enum,
            field_name: format!("{field_prefix}.origintype"),
            old_value: Some(format!("{:?}", a.origintype)),
            new_value: Some(format!("{:?}", b.origintype)),
            similarity: 0.0,
        });
    }
    changes
}

/// Compare two [`Name`] structs field by field.
fn compare_name(field_prefix: &str, a: &Name, b: &Name) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.display"),
        a.display.as_deref(),
        b.display.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.first_name"),
        a.first_name.as_deref(),
        b.first_name.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.suffix"),
        a.suffix.as_deref(),
        b.suffix.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.title"),
        a.title.as_deref(),
        b.title.as_deref(),
    ));
    changes.extend(compare_enum_discriminant(
        &format!("{field_prefix}.type_field"),
        a.type_field,
        b.type_field,
    ));
    changes.extend(compare_date_value(
        &format!("{field_prefix}.date"),
        a.date.as_ref(),
        b.date.as_ref(),
    ));
    // Compare surname_list positionally
    let max_surnames = a.surname_list.len().max(b.surname_list.len());
    for i in 0..max_surnames {
        let prefix = format!("{field_prefix}.surname_list[{i}]");
        match (a.surname_list.get(i), b.surname_list.get(i)) {
            (Some(sa), Some(sb)) => {
                changes.extend(compare_surname(&prefix, sa, sb));
            }
            (Some(sa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{sa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(sb)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{sb:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }
    changes
}

/// Compare two [`Location`] structs field by field.
/// Handles `Option<Location>` for the address location field.
fn compare_location(
    field_prefix: &str,
    a: Option<&Location>,
    b: Option<&Location>,
) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    match (a, b) {
        (None, None) => {}
        (Some(la), Some(lb)) => {
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.street"),
                la.street.as_deref(),
                lb.street.as_deref(),
            ));
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.city"),
                la.city.as_deref(),
                lb.city.as_deref(),
            ));
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.county"),
                la.county.as_deref(),
                lb.county.as_deref(),
            ));
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.state"),
                la.state.as_deref(),
                lb.state.as_deref(),
            ));
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.country"),
                la.country.as_deref(),
                lb.country.as_deref(),
            ));
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.postal"),
                la.postal.as_deref(),
                lb.postal.as_deref(),
            ));
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.locality"),
                la.locality.as_deref(),
                lb.locality.as_deref(),
            ));
            changes.extend(compare_field_optional_text(
                &format!("{field_prefix}.phone"),
                la.phone.as_deref(),
                lb.phone.as_deref(),
            ));
        }
        _ => {
            changes.push(FieldChange {
                field_kind: FieldKind::Text,
                field_name: field_prefix.to_string(),
                old_value: a.map(|l| format!("{l:?}")),
                new_value: b.map(|l| format!("{l:?}")),
                similarity: 0.0,
            });
        }
    }
    changes
}

/// Compare two [`Address`] structs field by field.
fn compare_address(field_prefix: &str, a: &Address, b: &Address) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    changes.extend(compare_date_value(
        &format!("{field_prefix}.date"),
        a.date.as_ref(),
        b.date.as_ref(),
    ));
    changes.extend(compare_location(
        &format!("{field_prefix}.location"),
        a.location.as_ref(),
        b.location.as_ref(),
    ));
    changes.extend(compare_handle_array(
        &format!("{field_prefix}.citation_list"),
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        &format!("{field_prefix}.note_list"),
        &a.note_list,
        &b.note_list,
    ));
    changes
}

/// Compare two [`Attribute`] structs field by field.
fn compare_attribute(field_prefix: &str, a: &Attribute, b: &Attribute) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    changes.extend(compare_enum_discriminant(
        &format!("{field_prefix}.type_field"),
        a.type_field,
        b.type_field,
    ));
    changes.extend(compare_field_text(
        &format!("{field_prefix}.value"),
        &a.value,
        &b.value,
    ));
    changes.extend(compare_handle_array(
        &format!("{field_prefix}.citation_list"),
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        &format!("{field_prefix}.note_list"),
        &a.note_list,
        &b.note_list,
    ));
    changes
}

/// Compare two [`LdsOrd`] structs field by field.
fn compare_lds_ord(field_prefix: &str, a: &LdsOrd, b: &LdsOrd) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    changes.extend(compare_enum_discriminant(
        &format!("{field_prefix}.type_field"),
        a.type_field,
        b.type_field,
    ));
    changes.extend(compare_date_value(
        &format!("{field_prefix}.date"),
        a.date.as_ref(),
        b.date.as_ref(),
    ));
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.status"),
        a.status.as_deref(),
        b.status.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.temple"),
        a.temple.as_deref(),
        b.temple.as_deref(),
    ));
    changes.extend(compare_handle_ref(
        &format!("{field_prefix}.place_handle"),
        a.place_handle.as_ref(),
        b.place_handle.as_ref(),
    ));
    changes.extend(compare_handle_array(
        &format!("{field_prefix}.citation_list"),
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        &format!("{field_prefix}.note_list"),
        &a.note_list,
        &b.note_list,
    ));
    changes
}

/// Compare two [`Url`] structs field by field.
fn compare_url(field_prefix: &str, a: &Url, b: &Url) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    changes.extend(compare_field_optional_text(
        &format!("{field_prefix}.desc"),
        a.desc.as_deref(),
        b.desc.as_deref(),
    ));
    changes.extend(compare_field_text(
        &format!("{field_prefix}.href"),
        a.href.as_deref().unwrap_or(""),
        b.href.as_deref().unwrap_or(""),
    ));
    changes.extend(compare_enum_discriminant(
        &format!("{field_prefix}.type_field"),
        a.type_field,
        b.type_field,
    ));
    changes
}

/// Compare two [`PersonData`] structs field by field.
///
/// Compares all fields except `handle` (which is the join key used by
/// the matcher). Delegates text fields to similarity scoring, uses set
/// comparison for handle arrays, and deep comparison for nested types.
pub fn compare_person(a: &PersonData, b: &PersonData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    // gramps_id
    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));

    // gender (i32, compared as discriminant)
    changes.extend(compare_enum_discriminant("gender", a.gender, b.gender));

    // primary_name
    changes.extend(compare_name(
        "primary_name",
        &a.primary_name,
        &b.primary_name,
    ));

    // alternate_names: positional comparison
    let max_alts = a.alternate_names.len().max(b.alternate_names.len());
    for i in 0..max_alts {
        let prefix = format!("alternate_names[{i}]");
        match (a.alternate_names.get(i), b.alternate_names.get(i)) {
            (Some(na), Some(nb)) => {
                changes.extend(compare_name(&prefix, na, nb));
            }
            (Some(na), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{na:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(nb)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{nb:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    // Handle lists
    changes.extend(compare_handle_array(
        "family_list",
        &a.family_list,
        &b.family_list,
    ));
    changes.extend(compare_handle_array(
        "parent_family_list",
        &a.parent_family_list,
        &b.parent_family_list,
    ));
    changes.extend(compare_handle_array(
        "citation_list",
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    // Ref arrays with metadata
    changes.extend(compare_ref_array(
        "event_ref_list",
        &a.event_ref_list,
        &b.event_ref_list,
        |x, y| compare_enum_discriminant("event_ref_list.role", x.role, y.role),
    ));
    changes.extend(compare_ref_array(
        "person_ref_list",
        &a.person_ref_list,
        &b.person_ref_list,
        |x, y| compare_enum_discriminant("person_ref_list.relation", x.relation, y.relation),
    ));
    changes.extend(compare_ref_array(
        "media_list",
        &a.media_list,
        &b.media_list,
        |_, _| vec![],
    ));

    // Nested struct lists
    let max_addrs = a.address_list.len().max(b.address_list.len());
    for i in 0..max_addrs {
        let prefix = format!("address_list[{i}]");
        match (a.address_list.get(i), b.address_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_address(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    let max_attrs = a.attribute_list.len().max(b.attribute_list.len());
    for i in 0..max_attrs {
        let prefix = format!("attribute_list[{i}]");
        match (a.attribute_list.get(i), b.attribute_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_attribute(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    let max_lds = a.lds_ord_list.len().max(b.lds_ord_list.len());
    for i in 0..max_lds {
        let prefix = format!("lds_ord_list[{i}]");
        match (a.lds_ord_list.get(i), b.lds_ord_list.get(i)) {
            (Some(la), Some(lb)) => {
                changes.extend(compare_lds_ord(&prefix, la, lb));
            }
            (Some(la), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{la:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(lb)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{lb:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    let max_urls = a.url_list.len().max(b.url_list.len());
    for i in 0..max_urls {
        let prefix = format!("url_list[{i}]");
        match (a.url_list.get(i), b.url_list.get(i)) {
            (Some(ua), Some(ub)) => {
                changes.extend(compare_url(&prefix, ua, ub));
            }
            (Some(ua), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{ua:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ub)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ub:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    changes
}

/// Compare two [`FamilyData`] structs field by field.
///
/// Compares all fields except `handle` (the join key).
pub fn compare_family(a: &FamilyData, b: &FamilyData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    changes.extend(compare_handle_ref(
        "father_handle",
        a.father_handle.as_ref(),
        b.father_handle.as_ref(),
    ));
    changes.extend(compare_handle_ref(
        "mother_handle",
        a.mother_handle.as_ref(),
        b.mother_handle.as_ref(),
    ));

    // Ref arrays with metadata
    changes.extend(compare_ref_array(
        "child_ref_list",
        &a.child_ref_list,
        &b.child_ref_list,
        |x, y| compare_enum_discriminant("child_ref_list.relation", x.relation, y.relation),
    ));
    changes.extend(compare_ref_array(
        "event_ref_list",
        &a.event_ref_list,
        &b.event_ref_list,
        |x, y| compare_enum_discriminant("event_ref_list.role", x.role, y.role),
    ));
    changes.extend(compare_ref_array(
        "media_list",
        &a.media_list,
        &b.media_list,
        |_, _| vec![],
    ));

    // Handle arrays
    changes.extend(compare_handle_array(
        "citation_list",
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    // Attribute list
    let max_attrs = a.attribute_list.len().max(b.attribute_list.len());
    for i in 0..max_attrs {
        let prefix = format!("attribute_list[{i}]");
        match (a.attribute_list.get(i), b.attribute_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_attribute(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    changes
}

/// Compare two [`EventData`] structs field by field.
///
/// Compares all fields except `handle` (the join key).
pub fn compare_event(a: &EventData, b: &EventData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    if a.event_type != b.event_type {
        changes.push(FieldChange {
            field_kind: FieldKind::Enum,
            field_name: "event_type".to_string(),
            old_value: Some(event_type_display(&a.event_type)),
            new_value: Some(event_type_display(&b.event_type)),
            similarity: 0.0,
        });
    }
    changes.extend(compare_date_value("date", a.date.as_ref(), b.date.as_ref()));
    changes.extend(compare_field_optional_text(
        "description",
        a.description.as_deref(),
        b.description.as_deref(),
    ));
    changes.extend(compare_handle_ref(
        "place_handle",
        a.place_handle.as_ref(),
        b.place_handle.as_ref(),
    ));

    // Handle arrays
    changes.extend(compare_handle_array(
        "citation_list",
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    // Ref arrays
    changes.extend(compare_ref_array(
        "media_list",
        &a.media_list,
        &b.media_list,
        |_, _| vec![],
    ));

    // Attribute list
    let max_attrs = a.attribute_list.len().max(b.attribute_list.len());
    for i in 0..max_attrs {
        let prefix = format!("attribute_list[{i}]");
        match (a.attribute_list.get(i), b.attribute_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_attribute(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    changes
}

/// Compare two [`PlaceData`] structs field by field.
pub fn compare_place(a: &PlaceData, b: &PlaceData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    changes.extend(compare_location("name", Some(&a.name), Some(&b.name)));
    changes.extend(compare_ref_array(
        "place_ref_list",
        &a.place_ref_list,
        &b.place_ref_list,
        |_, _| vec![],
    ));
    changes.extend(compare_handle_array(
        "citation_list",
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));
    changes.extend(compare_ref_array(
        "media_list",
        &a.media_list,
        &b.media_list,
        |_, _| vec![],
    ));

    let max_attrs = a.attribute_list.len().max(b.attribute_list.len());
    for i in 0..max_attrs {
        let prefix = format!("attribute_list[{i}]");
        match (a.attribute_list.get(i), b.attribute_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_attribute(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    changes
}

/// Compare two [`SourceData`] structs field by field.
pub fn compare_source(a: &SourceData, b: &SourceData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    changes.extend(compare_field_text("title", &a.title, &b.title));
    changes.extend(compare_field_optional_text(
        "author",
        a.author.as_deref(),
        b.author.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        "pubinfo",
        a.pubinfo.as_deref(),
        b.pubinfo.as_deref(),
    ));
    changes.extend(compare_ref_array(
        "reporef_list",
        &a.reporef_list,
        &b.reporef_list,
        |x, y| {
            let mut meta = Vec::new();
            meta.extend(compare_field_optional_text(
                "reporef_list.call_number",
                x.call_number.as_deref(),
                y.call_number.as_deref(),
            ));
            if x.media_type != y.media_type {
                meta.push(FieldChange {
                    field_kind: FieldKind::Enum,
                    field_name: "reporef_list.media_type".into(),
                    old_value: Some(format!("{:?}", x.media_type)),
                    new_value: Some(format!("{:?}", y.media_type)),
                    similarity: 0.0,
                });
            }
            meta
        },
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));
    changes.extend(compare_ref_array(
        "media_list",
        &a.media_list,
        &b.media_list,
        |_, _| vec![],
    ));

    let max_attrs = a.attribute_list.len().max(b.attribute_list.len());
    for i in 0..max_attrs {
        let prefix = format!("attribute_list[{i}]");
        match (a.attribute_list.get(i), b.attribute_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_attribute(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    changes
}

/// Compare two [`CitationData`] structs field by field.
pub fn compare_citation(a: &CitationData, b: &CitationData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    if a.source_handle != b.source_handle {
        changes.push(FieldChange {
            field_kind: FieldKind::HandleRef,
            field_name: "source_handle".into(),
            old_value: Some(get_source_handle(&a.source_handle)),
            new_value: Some(get_source_handle(&b.source_handle)),
            similarity: 0.0,
        });
    }
    changes.extend(compare_field_optional_text(
        "page",
        a.page.as_deref(),
        b.page.as_deref(),
    ));
    if a.confidence != b.confidence {
        changes.push(FieldChange {
            field_kind: FieldKind::Numeric,
            field_name: "confidence".into(),
            old_value: a.confidence.map(|v| v.to_string()),
            new_value: b.confidence.map(|v| v.to_string()),
            similarity: 0.0,
        });
    }
    changes.extend(compare_ref_array(
        "media_list",
        &a.media_list,
        &b.media_list,
        |_, _| vec![],
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    changes
}

/// Compare two [`RepositoryData`] structs field by field.
pub fn compare_repository(a: &RepositoryData, b: &RepositoryData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        "name",
        a.name.as_deref(),
        b.name.as_deref(),
    ));
    if a.type_field != b.type_field {
        changes.push(FieldChange {
            field_kind: FieldKind::Enum,
            field_name: "type_field".into(),
            old_value: a.type_field.map(|v| format!("{v:?}")),
            new_value: b.type_field.map(|v| format!("{v:?}")),
            similarity: 0.0,
        });
    }
    changes.extend(compare_ref_array(
        "media_list",
        &a.media_list,
        &b.media_list,
        |_, _| vec![],
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    let max_addrs = a.address_list.len().max(b.address_list.len());
    for i in 0..max_addrs {
        let prefix = format!("address_list[{i}]");
        match (a.address_list.get(i), b.address_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_address(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    let max_urls = a.url_list.len().max(b.url_list.len());
    for i in 0..max_urls {
        let prefix = format!("url_list[{i}]");
        match (a.url_list.get(i), b.url_list.get(i)) {
            (Some(ua), Some(ub)) => {
                changes.extend(compare_url(&prefix, ua, ub));
            }
            (Some(ua), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{ua:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ub)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ub:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    changes
}

/// Compare two [`MediaData`] structs field by field.
pub fn compare_media(a: &MediaData, b: &MediaData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        "desc",
        a.desc.as_deref(),
        b.desc.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        "path",
        a.path.as_deref(),
        b.path.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        "mime_type",
        a.mime_type.as_deref(),
        b.mime_type.as_deref(),
    ));
    changes.extend(compare_field_optional_text(
        "checksum",
        a.checksum.as_deref(),
        b.checksum.as_deref(),
    ));
    changes.extend(compare_handle_array(
        "citation_list",
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array(
        "note_list",
        &a.note_list,
        &b.note_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    let max_attrs = a.attribute_list.len().max(b.attribute_list.len());
    for i in 0..max_attrs {
        let prefix = format!("attribute_list[{i}]");
        match (a.attribute_list.get(i), b.attribute_list.get(i)) {
            (Some(aa), Some(ab)) => {
                changes.extend(compare_attribute(&prefix, aa, ab));
            }
            (Some(aa), None) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: Some(format!("{aa:?}")),
                    new_value: None,
                    similarity: 0.0,
                });
            }
            (None, Some(ab)) => {
                changes.push(FieldChange {
                    field_kind: FieldKind::Text,
                    field_name: prefix,
                    old_value: None,
                    new_value: Some(format!("{ab:?}")),
                    similarity: 0.0,
                });
            }
            (None, None) => {}
        }
    }

    changes
}

/// Compare two [`NoteData`] structs field by field.
pub fn compare_note(a: &NoteData, b: &NoteData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    changes.extend(compare_field_text("text", &a.text, &b.text));
    if a.format != b.format {
        changes.push(FieldChange {
            field_kind: FieldKind::Numeric,
            field_name: "format".into(),
            old_value: a.format.map(|v| v.to_string()),
            new_value: b.format.map(|v| v.to_string()),
            similarity: 0.0,
        });
    }
    if a.type_field != b.type_field {
        changes.push(FieldChange {
            field_kind: FieldKind::Text,
            field_name: "type_field".into(),
            old_value: a.type_field.clone(),
            new_value: b.type_field.clone(),
            similarity: 0.0,
        });
    }
    changes.extend(compare_handle_array(
        "citation_list",
        &a.citation_list,
        &b.citation_list,
    ));
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    changes
}

/// Compare two [`TagData`] structs field by field.
pub fn compare_tag(a: &TagData, b: &TagData) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    changes.extend(compare_field_optional_text(
        "gramps_id",
        a.gramps_id.as_deref(),
        b.gramps_id.as_deref(),
    ));
    changes.extend(compare_field_text("name", &a.name, &b.name));
    changes.extend(compare_field_optional_text(
        "color",
        a.color.as_deref(),
        b.color.as_deref(),
    ));
    if a.priority != b.priority {
        changes.push(FieldChange {
            field_kind: FieldKind::Numeric,
            field_name: "priority".into(),
            old_value: a.priority.map(|v| v.to_string()),
            new_value: b.priority.map(|v| v.to_string()),
            similarity: 0.0,
        });
    }
    changes.extend(compare_handle_array("tag_list", &a.tag_list, &b.tag_list));

    changes
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use typed_graph::{DateValue, EventRoleType, FamilyRelType, RepositoryType};

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
            ..Default::default()
        }];
        let b = vec![EventRef {
            ref_field: "E001".into(),
            role: None,
            ..Default::default()
        }];
        assert!(compare_ref_array("event_ref_list", &a, &b, |_, _| vec![]).is_empty());
    }

    #[test]
    fn ref_array_reordered() {
        let a = vec![
            EventRef {
                ref_field: "E001".into(),
                role: None,
                ..Default::default()
            },
            EventRef {
                ref_field: "E002".into(),
                role: None,
                ..Default::default()
            },
        ];
        let b = vec![
            EventRef {
                ref_field: "E002".into(),
                role: None,
                ..Default::default()
            },
            EventRef {
                ref_field: "E001".into(),
                role: None,
                ..Default::default()
            },
        ];
        assert!(compare_ref_array("event_ref_list", &a, &b, |_, _| vec![]).is_empty());
    }

    #[test]
    fn ref_array_different_handle() {
        let a = vec![EventRef {
            ref_field: "E001".into(),
            role: None,
            ..Default::default()
        }];
        let b = vec![EventRef {
            ref_field: "E002".into(),
            role: None,
            ..Default::default()
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
            ..Default::default()
        }];
        let b = vec![EventRef {
            ref_field: "E001".into(),
            role: Some(typed_graph::EventRoleType::Witness),
            ..Default::default()
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

    // -----------------------------------------------------------------------
    // compare_person
    // -----------------------------------------------------------------------

    fn make_person() -> PersonData {
        PersonData {
            handle: "H001".into(),
            gramps_id: Some("I0001".into()),
            gender: Some(0),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            alternate_names: vec![],
            event_ref_list: vec![],
            family_list: vec!["F001".into()],
            parent_family_list: vec![],
            person_ref_list: vec![],
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            address_list: vec![],
            url_list: vec![],
            lds_ord_list: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn person_identical() {
        let a = make_person();
        let b = make_person();
        assert!(compare_person(&a, &b).is_empty());
    }

    #[test]
    fn person_change_surname() {
        let a = make_person();
        let mut b = make_person();
        b.primary_name.surname_list[0].surname = Some("Jones".into());
        let changes = compare_person(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].field_name,
            "primary_name.surname_list[0].surname"
        );
        assert_eq!(changes[0].field_kind, FieldKind::Text);
        assert_eq!(changes[0].old_value.as_deref(), Some("Smith"));
        assert_eq!(changes[0].new_value.as_deref(), Some("Jones"));
    }

    #[test]
    fn person_change_gender() {
        let a = make_person();
        let mut b = make_person();
        b.gender = Some(1);
        let changes = compare_person(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "gender");
        assert_eq!(changes[0].field_kind, FieldKind::Enum);
    }

    #[test]
    fn person_add_alternate_name() {
        let a = make_person();
        let mut b = make_person();
        b.alternate_names.push(Name {
            first_name: Some("Jonathan".into()),
            ..Name::default()
        });
        let changes = compare_person(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "alternate_names[0]");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn person_reorder_family_list() {
        let mut a = make_person();
        let mut b = make_person();
        a.family_list = vec!["F001".into(), "F002".into()];
        b.family_list = vec!["F002".into(), "F001".into()];
        assert!(compare_person(&a, &b).is_empty());
    }

    #[test]
    fn person_change_gramps_id() {
        let a = make_person();
        let mut b = make_person();
        b.gramps_id = Some("I0002".into());
        let changes = compare_person(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "gramps_id");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn person_empty_vs_empty() {
        let a = PersonData::default();
        let b = PersonData::default();
        assert!(compare_person(&a, &b).is_empty());
    }

    #[test]
    fn person_change_event_ref_role() {
        let mut a = make_person();
        let mut b = make_person();
        a.event_ref_list = vec![EventRef {
            ref_field: "E001".into(),
            role: Some(EventRoleType::Primary),
            ..Default::default()
        }];
        b.event_ref_list = vec![EventRef {
            ref_field: "E001".into(),
            role: Some(EventRoleType::Witness),
            ..Default::default()
        }];
        let changes = compare_person(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "event_ref_list.role");
        assert_eq!(changes[0].field_kind, FieldKind::Enum);
    }

    #[test]
    fn person_change_person_ref_relation() {
        let mut a = make_person();
        let mut b = make_person();
        a.person_ref_list = vec![PersonRef {
            ref_field: "P001".into(),
            relation: Some(FamilyRelType::Married),
            ..Default::default()
        }];
        b.person_ref_list = vec![PersonRef {
            ref_field: "P001".into(),
            relation: Some(FamilyRelType::Birth),
            ..Default::default()
        }];
        let changes = compare_person(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "person_ref_list.relation");
        assert_eq!(changes[0].field_kind, FieldKind::Enum);
    }

    #[test]
    fn person_multi_field_changes() {
        let a = make_person();
        let mut b = make_person();
        b.gramps_id = Some("I0009".into());
        b.gender = Some(2);
        b.note_list = vec!["N001".into()];
        let changes = compare_person(&a, &b);
        assert_eq!(changes.len(), 3);
        assert!(changes.iter().any(|c| c.field_name == "gramps_id"));
        assert!(changes.iter().any(|c| c.field_name == "gender"));
        assert!(changes.iter().any(|c| c.field_name == "note_list"));
    }

    // -----------------------------------------------------------------------
    // compare_family
    // -----------------------------------------------------------------------

    fn make_family() -> FamilyData {
        FamilyData {
            handle: "F001".into(),
            gramps_id: Some("F0001".into()),
            father_handle: Some("P001".into()),
            mother_handle: Some("P002".into()),
            child_ref_list: vec![],
            event_ref_list: vec![],
            attribute_list: vec![],
            citation_list: vec![],
            media_list: vec![],
            note_list: vec![],
            tag_list: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn family_identical() {
        let a = make_family();
        let b = make_family();
        assert!(compare_family(&a, &b).is_empty());
    }

    #[test]
    fn family_change_father_handle() {
        let a = make_family();
        let mut b = make_family();
        b.father_handle = Some("P003".into());
        let changes = compare_family(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "father_handle");
        assert_eq!(changes[0].field_kind, FieldKind::HandleRef);
    }

    #[test]
    fn family_reorder_child_ref_list() {
        let mut a = make_family();
        let mut b = make_family();
        a.child_ref_list = vec![
            ChildRef {
                ref_field: "P003".into(),
                relation: None,
                ..Default::default()
            },
            ChildRef {
                ref_field: "P004".into(),
                relation: None,
                ..Default::default()
            },
        ];
        b.child_ref_list = vec![
            ChildRef {
                ref_field: "P004".into(),
                relation: None,
                ..Default::default()
            },
            ChildRef {
                ref_field: "P003".into(),
                relation: None,
                ..Default::default()
            },
        ];
        assert!(compare_family(&a, &b).is_empty());
    }

    #[test]
    fn family_change_gramps_id() {
        let a = make_family();
        let mut b = make_family();
        b.gramps_id = Some("F0002".into());
        let changes = compare_family(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "gramps_id");
    }

    #[test]
    fn family_empty_vs_empty() {
        let a = FamilyData::default();
        let b = FamilyData::default();
        assert!(compare_family(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_event
    // -----------------------------------------------------------------------

    fn make_event() -> EventData {
        EventData {
            handle: "E001".into(),
            gramps_id: Some("E0001".into()),
            event_type: Some(typed_graph::EventType::Birth),
            date: Some(DateValue::new_ymd(1870, 6, 15)),
            description: Some("Birth of John Smith".into()),
            place_handle: Some("PL001".into()),
            attribute_list: vec![],
            citation_list: vec![],
            media_list: vec![],
            note_list: vec![],
            tag_list: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn event_identical() {
        let a = make_event();
        let b = make_event();
        assert!(compare_event(&a, &b).is_empty());
    }

    #[test]
    fn event_change_event_type() {
        let a = make_event();
        let mut b = make_event();
        b.event_type = Some(typed_graph::EventType::Death);
        let changes = compare_event(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "event_type");
        assert_eq!(changes[0].field_kind, FieldKind::Enum);
        assert_eq!(changes[0].old_value.as_deref(), Some("Birth"));
        assert_eq!(changes[0].new_value.as_deref(), Some("Death"));
    }

    #[test]
    fn event_change_date() {
        let a = make_event();
        let mut b = make_event();
        b.date = Some(DateValue::new_ymd(1900, 1, 1));
        let changes = compare_event(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "date");
        assert_eq!(changes[0].field_kind, FieldKind::Date);
    }

    #[test]
    fn event_change_description() {
        let a = make_event();
        let mut b = make_event();
        b.description = Some("Death of John Smith".into());
        let changes = compare_event(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "description");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
        assert!(changes[0].similarity > 0.0 && changes[0].similarity < 1.0);
    }

    #[test]
    fn event_change_place_handle() {
        let a = make_event();
        let mut b = make_event();
        b.place_handle = Some("PL002".into());
        let changes = compare_event(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "place_handle");
        assert_eq!(changes[0].field_kind, FieldKind::HandleRef);
    }

    #[test]
    fn event_empty_vs_empty() {
        let a = EventData::default();
        let b = EventData::default();
        assert!(compare_event(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_place
    // -----------------------------------------------------------------------

    fn make_place() -> PlaceData {
        PlaceData {
            handle: "PL001".into(),
            gramps_id: Some("P0001".into()),
            name: Location {
                street: Some("123 Main St".into()),
                city: Some("Springfield".into()),
                ..Location::default()
            },
            place_ref_list: vec![],
            citation_list: vec![],
            media_list: vec![],
            note_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn place_identical() {
        let a = make_place();
        let b = make_place();
        assert!(compare_place(&a, &b).is_empty());
    }

    #[test]
    fn place_change_location_street() {
        let a = make_place();
        let mut b = make_place();
        b.name.street = Some("456 Oak Ave".into());
        let changes = compare_place(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "name.street");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn place_empty_vs_empty() {
        let a = PlaceData::default();
        let b = PlaceData::default();
        assert!(compare_place(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_source
    // -----------------------------------------------------------------------

    fn make_source() -> SourceData {
        SourceData {
            handle: "S001".into(),
            gramps_id: Some("S0001".into()),
            title: "Birth Records".into(),
            author: Some("County Clerk".into()),
            pubinfo: Some("Vol 1, pg 45".into()),
            reporef_list: vec![],
            attribute_list: vec![],
            media_list: vec![],
            note_list: vec![],
            tag_list: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn source_identical() {
        let a = make_source();
        let b = make_source();
        assert!(compare_source(&a, &b).is_empty());
    }

    #[test]
    fn source_change_title() {
        let a = make_source();
        let mut b = make_source();
        b.title = "Death Records".into();
        let changes = compare_source(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "title");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn source_reorder_reporef_list() {
        let mut a = make_source();
        let mut b = make_source();
        a.reporef_list = vec![
            RepoRef {
                ref_field: "R001".into(),
                call_number: None,
                media_type: None,
                ..Default::default()
            },
            RepoRef {
                ref_field: "R002".into(),
                call_number: None,
                media_type: None,
                ..Default::default()
            },
        ];
        b.reporef_list = vec![
            RepoRef {
                ref_field: "R002".into(),
                call_number: None,
                media_type: None,
                ..Default::default()
            },
            RepoRef {
                ref_field: "R001".into(),
                call_number: None,
                media_type: None,
                ..Default::default()
            },
        ];
        assert!(compare_source(&a, &b).is_empty());
    }

    #[test]
    fn source_empty_vs_empty() {
        let a = SourceData::default();
        let b = SourceData::default();
        assert!(compare_source(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_citation
    // -----------------------------------------------------------------------

    fn make_citation() -> CitationData {
        CitationData {
            handle: "C001".into(),
            gramps_id: Some("C0001".into()),
            source_handle: Some("S001".to_string()),
            page: Some("45".into()),
            confidence: Some(2),
            media_list: vec![],
            note_list: vec![],
            tag_list: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn citation_identical() {
        let a = make_citation();
        let b = make_citation();
        assert!(compare_citation(&a, &b).is_empty());
    }

    #[test]
    fn citation_change_page() {
        let a = make_citation();
        let mut b = make_citation();
        b.page = Some("46".into());
        let changes = compare_citation(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "page");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn citation_empty_vs_empty() {
        let a = CitationData::default();
        let b = CitationData::default();
        assert!(compare_citation(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_repository
    // -----------------------------------------------------------------------

    fn make_repository() -> RepositoryData {
        RepositoryData {
            handle: "R001".into(),
            gramps_id: Some("R0001".into()),
            name: Some("National Archives".into()),
            type_field: Some(RepositoryType::Library),
            address_list: vec![],
            url_list: vec![],
            media_list: vec![],
            note_list: vec![],
            tag_list: vec![],
        }
    }

    #[test]
    fn repository_identical() {
        let a = make_repository();
        let b = make_repository();
        assert!(compare_repository(&a, &b).is_empty());
    }

    #[test]
    fn repository_change_type() {
        let a = make_repository();
        let mut b = make_repository();
        b.type_field = Some(RepositoryType::Archive);
        let changes = compare_repository(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "type_field");
        assert_eq!(changes[0].field_kind, FieldKind::Enum);
    }

    #[test]
    fn repository_empty_vs_empty() {
        let a = RepositoryData::default();
        let b = RepositoryData::default();
        assert!(compare_repository(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_media
    // -----------------------------------------------------------------------

    fn make_media() -> MediaData {
        MediaData {
            handle: "M001".into(),
            gramps_id: Some("O0001".into()),
            desc: Some("Family photo".into()),
            path: Some("photos/family.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            checksum: Some("abc123".into()),
            attribute_list: vec![],
            citation_list: vec![],
            note_list: vec![],
            tag_list: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn media_identical() {
        let a = make_media();
        let b = make_media();
        assert!(compare_media(&a, &b).is_empty());
    }

    #[test]
    fn media_change_desc() {
        let a = make_media();
        let mut b = make_media();
        b.desc = Some("Wedding photo".into());
        let changes = compare_media(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "desc");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
        assert!(changes[0].similarity > 0.0 && changes[0].similarity < 1.0);
    }

    #[test]
    fn media_change_checksum() {
        let a = make_media();
        let mut b = make_media();
        b.checksum = Some("def456".into());
        let changes = compare_media(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "checksum");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn media_reorder_tag_list() {
        let mut a = make_media();
        let mut b = make_media();
        a.tag_list = vec!["T001".into(), "T002".into()];
        b.tag_list = vec!["T002".into(), "T001".into()];
        assert!(compare_media(&a, &b).is_empty());
    }

    #[test]
    fn media_empty_vs_empty() {
        let a = MediaData::default();
        let b = MediaData::default();
        assert!(compare_media(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_note
    // -----------------------------------------------------------------------

    fn make_note() -> NoteData {
        NoteData {
            handle: "N001".into(),
            gramps_id: Some("N0001".into()),
            text: "Married on June 15, 1870".into(),
            format: Some(0),
            type_field: Some("General".to_string()),
            citation_list: vec![],
            tag_list: vec![],
        }
    }

    #[test]
    fn note_identical() {
        let a = make_note();
        let b = make_note();
        assert!(compare_note(&a, &b).is_empty());
    }

    #[test]
    fn note_change_text() {
        let a = make_note();
        let mut b = make_note();
        b.text = "Married on June 20, 1870".into();
        let changes = compare_note(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "text");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
        assert!(changes[0].similarity > 0.0 && changes[0].similarity < 1.0);
    }

    #[test]
    fn note_change_format() {
        let a = make_note();
        let mut b = make_note();
        b.format = Some(1);
        let changes = compare_note(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "format");
        assert_eq!(changes[0].field_kind, FieldKind::Numeric);
    }

    #[test]
    fn note_change_type() {
        let a = make_note();
        let mut b = make_note();
        b.type_field = Some("Research".to_string());
        let changes = compare_note(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "type_field");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn note_reorder_tag_list() {
        let mut a = make_note();
        let mut b = make_note();
        a.tag_list = vec!["T001".into(), "T002".into()];
        b.tag_list = vec!["T002".into(), "T001".into()];
        assert!(compare_note(&a, &b).is_empty());
    }

    #[test]
    fn note_empty_vs_empty() {
        let a = NoteData::default();
        let b = NoteData::default();
        assert!(compare_note(&a, &b).is_empty());
    }

    // -----------------------------------------------------------------------
    // compare_tag
    // -----------------------------------------------------------------------

    fn make_tag() -> TagData {
        TagData {
            handle: "T001".into(),
            gramps_id: Some("T0001".into()),
            name: "Important".into(),
            color: Some("#ff0000".into()),
            priority: Some(1),
            tag_list: vec![],
        }
    }

    #[test]
    fn tag_identical() {
        let a = make_tag();
        let b = make_tag();
        assert!(compare_tag(&a, &b).is_empty());
    }

    #[test]
    fn tag_change_color() {
        let a = make_tag();
        let mut b = make_tag();
        b.color = Some("#00ff00".into());
        let changes = compare_tag(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "color");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn tag_change_name() {
        let a = make_tag();
        let mut b = make_tag();
        b.name = "Urgent".into();
        let changes = compare_tag(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "name");
        assert_eq!(changes[0].field_kind, FieldKind::Text);
    }

    #[test]
    fn tag_change_priority() {
        let a = make_tag();
        let mut b = make_tag();
        b.priority = Some(2);
        let changes = compare_tag(&a, &b);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field_name, "priority");
        assert_eq!(changes[0].field_kind, FieldKind::Numeric);
    }

    #[test]
    fn tag_reorder_tag_list() {
        let mut a = make_tag();
        let mut b = make_tag();
        a.tag_list = vec!["T001".into(), "T002".into()];
        b.tag_list = vec!["T002".into(), "T001".into()];
        assert!(compare_tag(&a, &b).is_empty());
    }

    #[test]
    fn tag_empty_vs_empty() {
        let a = TagData::default();
        let b = TagData::default();
        assert!(compare_tag(&a, &b).is_empty());
    }
}
