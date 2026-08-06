//! Diff report data types.
//!
//! These types define the structure of the diff report produced when
//! comparing two Gramps family trees. All types derive [`Serialize`],
//! [`Deserialize`], [`Debug`], and [`Clone`] for flexible output and
//! debugging.

use serde::{Deserialize, Serialize};

/// Top-level diff report for a comparison between two Gramps files.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DiffReport {
    /// Summary statistics for the diff.
    pub summary: DiffSummary,
    /// Per-item diff results, one entry per item in the union of both graphs.
    pub items: Vec<ItemDiff>,
    /// Ambiguous cases that could not be resolved automatically.
    pub ambiguous_cases: Vec<AmbiguousCase>,
}

/// Summary statistics for a diff operation.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct DiffSummary {
    /// Total number of items in the first graph (A).
    pub total_a: usize,
    /// Total number of items in the second graph (B).
    pub total_b: usize,
    /// Number of items classified as SAME (identical in both graphs).
    pub same: usize,
    /// Number of items classified as MODIFIED (changed between graphs).
    pub modified: usize,
    /// Number of items classified as ADDED (present in B but not A).
    pub added: usize,
    /// Number of items classified as REMOVED (present in A but not B).
    pub removed: usize,
    /// Number of items classified as NEEDS_REVIEW (ambiguous match).
    pub needs_review: usize,
    /// Number of items classified as EXTRINSIC_ONLY (only handle references changed).
    pub extrinsic_only: usize,
}

/// Classification of a single item in the diff.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Classification {
    /// Item is identical in both graphs.
    Same,
    /// Item exists in both graphs but has changed.
    Modified,
    /// Item exists in the second graph but not the first.
    Added,
    /// Item exists in the first graph but not the second.
    Removed,
    /// The match is ambiguous and needs human review.
    NeedsReview,
    /// Only extrinsic (handle-reference) fields changed; intrinsic fields match.
    ExtrinsicOnly,
}

/// Diff result for a single item (person, family, event, etc.).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ItemDiff {
    /// Handle in the first graph (A). None for ADDED items.
    pub handle_a: Option<String>,
    /// Handle in the second graph (B). None for REMOVED items.
    pub handle_b: Option<String>,
    /// Gramps database ID in the first graph (e.g. "I0002", "E0005").
    pub gramps_id_a: Option<String>,
    /// Gramps database ID in the second graph (e.g. "I0002", "E0005").
    pub gramps_id_b: Option<String>,
    /// Human-readable display name for the item in graph A (e.g. "John Smith").
    pub display_name_a: Option<String>,
    /// Human-readable display name for the item in graph B (e.g. "John Smith").
    pub display_name_b: Option<String>,
    /// The type of primary item (e.g., "Person", "Family", "Event").
    pub item_type: String,
    /// Classification of this item.
    pub classification: Classification,
    /// Individual field-level changes (empty for SAME, ADDED, REMOVED).
    pub field_changes: Vec<FieldChange>,
    /// Confidence score for the match (0.0 = uncertain, 1.0 = certain).
    /// Only meaningful for MODIFIED, SAME, and EXTRINSIC_ONLY classifications.
    pub confidence: f64,
}

/// A single field-level change between two items.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FieldChange {
    /// The kind of field that changed.
    pub field_kind: FieldKind,
    /// The name of the field (e.g., "surname", "birth_date").
    pub field_name: String,
    /// The value in the first graph (A). None if the field was added in B.
    pub old_value: Option<String>,
    /// The value in the second graph (B). None if the field was removed from B.
    pub new_value: Option<String>,
    /// Similarity score between old and new values (0.0–1.0).
    pub similarity: f64,
}

/// The kind of field that changed in a [`FieldChange`].
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    /// A text field (name, note, etc.).
    Text,
    /// A date field.
    Date,
    /// A handle reference to another item.
    HandleRef,
    /// A list of handle references.
    HandleRefList,
    /// An enum value (gender, event type, etc.).
    Enum,
    /// A boolean field.
    Boolean,
    /// A numeric field.
    Numeric,
    /// An unknown or generic field type.
    Unknown,
}

/// An ambiguous match case that requires human review.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AmbiguousCase {
    /// The handle in the first graph (A) that could not be matched.
    pub handle_a: String,
    /// The type of item in graph A.
    pub item_type_a: String,
    /// Context information about the item in graph A.
    pub context_a: AmbiguousContext,
    /// Candidate matches from graph B.
    pub candidates: Vec<Candidate>,
}

/// A candidate match for an ambiguous item.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The handle in the second graph (B).
    pub handle_b: String,
    /// Similarity score between the candidate and the target (0.0–1.0).
    pub score: f64,
    /// Context information about the candidate.
    pub context_b: AmbiguousContext,
}

/// Context information for an ambiguous item.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AmbiguousContext {
    /// Display name or label for the item.
    pub display_name: String,
    /// Related items (spouses, parents, children, events, etc.).
    pub related_items: Vec<RelatedItem>,
}

/// A reference to a related item in the diff context.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RelatedItem {
    /// The handle of the related item.
    pub handle: String,
    /// The type of relationship (e.g., "spouse", "parent", "child", "event").
    pub relationship: String,
    /// Display name or label for the related item.
    pub display_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that all types implement the expected traits.
    #[test]
    fn type_shape_existence() {
        // DiffReport
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };
        let _cloned = report.clone();
        let _debug = format!("{report:?}");
        let _partial = report == report;

        // DiffSummary
        let summary = DiffSummary::default();
        let _ = format!("{summary:?}");

        // ItemDiff
        let item = ItemDiff {
            handle_a: None,
            handle_b: None,
            gramps_id_a: None,
            gramps_id_b: None,
            display_name_a: None,
            display_name_b: None,
            item_type: "Person".into(),
            classification: Classification::Same,
            field_changes: vec![],
            confidence: 1.0,
        };
        let _ = format!("{item:?}");

        // Classification
        for &c in &[
            Classification::Same,
            Classification::Modified,
            Classification::Added,
            Classification::Removed,
            Classification::NeedsReview,
            Classification::ExtrinsicOnly,
        ] {
            let _ = format!("{c:?}");
        }

        // FieldChange
        let fc = FieldChange {
            field_kind: FieldKind::Text,
            field_name: "surname".into(),
            old_value: Some("Smith".into()),
            new_value: Some("Jones".into()),
            similarity: 0.5,
        };
        let _ = format!("{fc:?}");

        // FieldKind
        for &k in &[
            FieldKind::Text,
            FieldKind::Date,
            FieldKind::HandleRef,
            FieldKind::HandleRefList,
            FieldKind::Enum,
            FieldKind::Boolean,
            FieldKind::Numeric,
            FieldKind::Unknown,
        ] {
            let _ = format!("{k:?}");
        }

        // AmbiguousCase
        let ac = AmbiguousCase {
            handle_a: "H001".into(),
            item_type_a: "Person".into(),
            context_a: AmbiguousContext {
                display_name: "John Smith".into(),
                related_items: vec![],
            },
            candidates: vec![],
        };
        let _ = format!("{ac:?}");

        // Candidate
        let cand = Candidate {
            handle_b: "H002".into(),
            score: 0.85,
            context_b: AmbiguousContext {
                display_name: "Johnny Smith".into(),
                related_items: vec![],
            },
        };
        let _ = format!("{cand:?}");

        // RelatedItem
        let ri = RelatedItem {
            handle: "F001".into(),
            relationship: "spouse".into(),
            display_name: "Jane Smith".into(),
        };
        let _ = format!("{ri:?}");
    }

    /// Verify serde round-trip for all types.
    #[test]
    fn serde_roundtrip() {
        let report = DiffReport {
            summary: DiffSummary {
                total_a: 10,
                total_b: 12,
                same: 5,
                modified: 2,
                added: 3,
                removed: 1,
                needs_review: 1,
                extrinsic_only: 0,
            },
            items: vec![
                ItemDiff {
                    handle_a: Some("H001".into()),
                    handle_b: Some("H001".into()),
                    gramps_id_a: Some("I0001".into()),
                    gramps_id_b: Some("I0001".into()),
                    display_name_a: Some("John Smith".into()),
                    display_name_b: Some("John Smith".into()),
                    item_type: "Person".into(),
                    classification: Classification::Same,
                    field_changes: vec![],
                    confidence: 1.0,
                },
                ItemDiff {
                    handle_a: Some("H002".into()),
                    handle_b: Some("H002".into()),
                    gramps_id_a: Some("I0002".into()),
                    gramps_id_b: Some("I0003".into()),
                    display_name_a: Some("John Smith".into()),
                    display_name_b: Some("John Jones".into()),
                    item_type: "Person".into(),
                    classification: Classification::Modified,
                    field_changes: vec![FieldChange {
                        field_kind: FieldKind::Text,
                        field_name: "surname".into(),
                        old_value: Some("Smith".into()),
                        new_value: Some("Jones".into()),
                        similarity: 0.5,
                    }],
                    confidence: 1.0,
                },
                ItemDiff {
                    handle_a: None,
                    handle_b: Some("H003".into()),
                    gramps_id_a: None,
                    gramps_id_b: Some("I0004".into()),
                    display_name_a: None,
                    display_name_b: Some("Jane Doe".into()),
                    item_type: "Person".into(),
                    classification: Classification::Added,
                    field_changes: vec![],
                    confidence: 1.0,
                },
                ItemDiff {
                    handle_a: Some("H004".into()),
                    handle_b: None,
                    gramps_id_a: Some("I0005".into()),
                    gramps_id_b: None,
                    display_name_a: Some("Bob Brown".into()),
                    display_name_b: None,
                    item_type: "Person".into(),
                    classification: Classification::Removed,
                    field_changes: vec![],
                    confidence: 1.0,
                },
            ],
            ambiguous_cases: vec![AmbiguousCase {
                handle_a: "H005".into(),
                item_type_a: "Person".into(),
                context_a: AmbiguousContext {
                    display_name: "John Smith".into(),
                    related_items: vec![RelatedItem {
                        handle: "F001".into(),
                        relationship: "spouse".into(),
                        display_name: "Jane Smith".into(),
                    }],
                },
                candidates: vec![
                    Candidate {
                        handle_b: "H010".into(),
                        score: 0.85,
                        context_b: AmbiguousContext {
                            display_name: "Johnny Smith".into(),
                            related_items: vec![],
                        },
                    },
                    Candidate {
                        handle_b: "H011".into(),
                        score: 0.72,
                        context_b: AmbiguousContext {
                            display_name: "Jon Smith".into(),
                            related_items: vec![],
                        },
                    },
                ],
            }],
        };

        // Serialize to JSON
        let json = serde_json::to_string(&report).expect("serialize DiffReport");
        assert!(!json.is_empty());

        // Deserialize back
        let deserialized: DiffReport = serde_json::from_str(&json).expect("deserialize DiffReport");
        assert_eq!(report, deserialized);
    }

    /// Verify an empty report round-trips correctly.
    #[test]
    fn serde_roundtrip_empty() {
        let report = DiffReport {
            summary: DiffSummary::default(),
            items: vec![],
            ambiguous_cases: vec![],
        };

        let json = serde_json::to_string(&report).expect("serialize empty");
        let deserialized: DiffReport = serde_json::from_str(&json).expect("deserialize empty");
        assert_eq!(report, deserialized);
    }

    /// Verify that all classification variants are serializable.
    #[test]
    fn classification_serde() {
        let variants = [
            Classification::Same,
            Classification::Modified,
            Classification::Added,
            Classification::Removed,
            Classification::NeedsReview,
            Classification::ExtrinsicOnly,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).expect("serialize classification");
            let deserialized: Classification =
                serde_json::from_str(&json).expect("deserialize classification");
            assert_eq!(*v, deserialized);
        }
    }

    /// Verify that all field kind variants are serializable.
    #[test]
    fn field_kind_serde() {
        let variants = [
            FieldKind::Text,
            FieldKind::Date,
            FieldKind::HandleRef,
            FieldKind::HandleRefList,
            FieldKind::Enum,
            FieldKind::Boolean,
            FieldKind::Numeric,
            FieldKind::Unknown,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).expect("serialize field_kind");
            let deserialized: FieldKind =
                serde_json::from_str(&json).expect("deserialize field_kind");
            assert_eq!(*v, deserialized);
        }
    }
}
