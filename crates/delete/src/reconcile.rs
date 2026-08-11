//! Manifest reconciliation engine.
//!
//! Compares the pre-deletion manifest (all `Pending`) against the Python
//! backend's `surviving` report to determine which handles Gramps actually
//! deleted versus which ones it kept (e.g., orphaned events that Gramps
//! never removes).
//!
//! # Reconciliation Rules
//!
//! | Handle in manifest | In surviving set? | Result status |
//! |---|---|---|
//! | `to_delete` entry | No | `Deleted` — Gramps removed it |
//! | `to_delete` entry | Yes | `Pending` — Gramps kept it (e.g., orphaned event) |
//! | `kept` entry | Ignored | `Kept` — user chose to keep it |
//!
//! # Error Conditions
//!
//! - `UnrecognizedDeletionMode` — the manifest has an unrecognized `deletion_mode`
//!   (e.g. not `"people_only"`).
//! - `SurvivingHandlesNotInManifest` — a handle in the surviving set is not
//!   present in any manifest entry. This indicates a data contract mismatch
//!   between the Rust and Python sides.

use std::collections::HashSet;

use crate::types::{Handle, HandleStatus, TypePlan};

/// Errors that can occur during reconciliation.
#[derive(Clone, Debug, PartialEq)]
pub enum ReconciliationError {
    /// A handle in the surviving set is not present in the manifest.
    /// This indicates a data contract mismatch between Rust and Python.
    SurvivingHandlesNotInManifest(Vec<Handle>),
    /// The manifest's `deletion_mode` is unrecognized.
    UnrecognizedDeletionMode(String),
}

impl std::fmt::Display for ReconciliationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconciliationError::SurvivingHandlesNotInManifest(handles) => {
                write!(
                    f,
                    "surviving handles not found in manifest: {}",
                    handles.join(", ")
                )
            }
            ReconciliationError::UnrecognizedDeletionMode(mode) => {
                write!(f, "unrecognized deletion_mode: '{}'", mode)
            }
        }
    }
}

impl std::error::Error for ReconciliationError {}

/// Reconcile the manifest against what Gramps actually deleted.
///
/// For each handle in the manifest's `to_delete` lists:
/// - If handle is NOT in `surviving` → status = `Deleted`
/// - If handle IS in `surviving` → status = `Pending` (Gramps kept it)
///
/// Handles in `kept` lists always stay `Kept`.
///
/// # Errors
///
/// Returns `Err` if the manifest's `deletion_mode` is unrecognized
/// or if the surviving set contains handles not present in the manifest
/// (which indicates a data contract mismatch).
pub fn reconcile(
    manifest: &mut crate::types::DeleteManifest,
    surviving: &HashSet<Handle>,
) -> Result<(), ReconciliationError> {
    // Validate deletion_mode
    match manifest.deletion_mode.as_str() {
        "people_only" => {}
        other => {
            return Err(ReconciliationError::UnrecognizedDeletionMode(
                other.to_string(),
            ));
        }
    }

    // Collect all manifest handles for validation
    let all_manifest_handles: HashSet<Handle> = manifest
        .plan
        .values()
        .flat_map(|plan: &TypePlan| {
            plan.to_delete
                .iter()
                .map(|e| e.handle.clone())
                .chain(plan.kept.iter().map(|e| e.handle.clone()))
        })
        .collect();

    // Validate: every surviving handle must be in the manifest
    let orphaned_surviving: Vec<Handle> = surviving
        .difference(&all_manifest_handles)
        .cloned()
        .collect();

    if !orphaned_surviving.is_empty() {
        return Err(ReconciliationError::SurvivingHandlesNotInManifest(
            orphaned_surviving,
        ));
    }

    // Reconcile each type's to_delete entries
    for type_plan in manifest.plan.values_mut() {
        for entry in &mut type_plan.to_delete {
            if surviving.contains(&entry.handle) {
                // Gramps kept it — stay Pending
                entry.status = HandleStatus::Pending;
            } else {
                // Gramps removed it — mark Deleted
                entry.status = HandleStatus::Deleted;
            }
        }
        // kept entries always stay Kept (already set by deserialization)
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DeleteManifest, HandleEntry, HandleStatus, TypePlan};
    use std::collections::HashMap;

    fn make_manifest(
        to_delete: Vec<(&str, HandleStatus)>,
        kept: Vec<&str>,
    ) -> DeleteManifest {
        let mut plan = HashMap::new();
        plan.insert(
            "people".to_string(),
            TypePlan {
                to_delete: to_delete
                    .iter()
                    .map(|(h, s)| HandleEntry {
                        handle: h.to_string(),
                        status: s.clone(),
                    })
                    .collect(),
                kept: kept
                    .iter()
                    .map(|h| HandleEntry {
                        handle: h.to_string(),
                        status: HandleStatus::Kept,
                    })
                    .collect(),
            },
        );
        // Add empty families, events, etc. The reconcile function iterates
        // all plan entries, so these should be handled gracefully.
        for key in &["families", "events", "notes", "places"] {
            plan.insert(
                key.to_string(),
                TypePlan {
                    to_delete: vec![],
                    kept: vec![],
                },
            );
        }
        DeleteManifest {
            version: 2,
            source_file: "test.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec![],
            deletion_mode: "people_only".to_string(),
            plan,
        }
    }

    fn make_manifest_multi(
        entries: Vec<(&str, Vec<(&str, HandleStatus)>)>,
        kept: Vec<&str>,
    ) -> DeleteManifest {
        let mut plan = HashMap::new();
        for (type_key, handles) in entries {
            plan.insert(
                type_key.to_string(),
                TypePlan {
                    to_delete: handles
                        .iter()
                        .map(|(h, s)| HandleEntry {
                            handle: h.to_string(),
                            status: s.clone(),
                        })
                        .collect(),
                    kept: kept
                        .iter()
                        .map(|h| HandleEntry {
                            handle: h.to_string(),
                            status: HandleStatus::Kept,
                        })
                        .collect(),
                },
            );
        }
        DeleteManifest {
            version: 2,
            source_file: "test.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec![],
            deletion_mode: "people_only".to_string(),
            plan,
        }
    }

    fn make_surviving(handles: &[&str]) -> HashSet<Handle> {
        handles.iter().map(|h| h.to_string()).collect()
    }

    #[test]
    fn all_people_deleted() {
        let mut manifest = make_manifest(
            vec![
                ("p1", HandleStatus::Pending),
                ("p2", HandleStatus::Pending),
            ],
            vec![],
        );
        let surviving = make_surviving(&[]);
        reconcile(&mut manifest, &surviving).unwrap();

        let people = manifest.plan.get("people").unwrap();
        assert_eq!(people.to_delete[0].status, HandleStatus::Deleted);
        assert_eq!(people.to_delete[1].status, HandleStatus::Deleted);
    }

    #[test]
    fn all_people_pending_when_surviving() {
        let mut manifest = make_manifest(
            vec![("p1", HandleStatus::Pending)],
            vec![],
        );
        let surviving = make_surviving(&["p1"]);
        reconcile(&mut manifest, &surviving).unwrap();

        let people = manifest.plan.get("people").unwrap();
        assert_eq!(people.to_delete[0].status, HandleStatus::Pending);
    }

    #[test]
    fn family_kept_by_gramps_becomes_pending() {
        let mut manifest = make_manifest_multi(
            vec![
                ("people", vec![("p1", HandleStatus::Pending)]),
                ("families", vec![("f1", HandleStatus::Pending)]),
            ],
            vec![],
        );
        // Person deleted, but family survived (Gramps kept it)
        let surviving = make_surviving(&["f1"]);
        reconcile(&mut manifest, &surviving).unwrap();

        let people = manifest.plan.get("people").unwrap();
        let families = manifest.plan.get("families").unwrap();
        assert_eq!(people.to_delete[0].status, HandleStatus::Deleted);
        assert_eq!(families.to_delete[0].status, HandleStatus::Pending);
    }

    #[test]
    fn events_always_survive() {
        let mut manifest = make_manifest_multi(
            vec![
                ("people", vec![("p1", HandleStatus::Pending)]),
                ("events", vec![("e1", HandleStatus::Pending)]),
            ],
            vec![],
        );
        let surviving = make_surviving(&["e1"]);
        reconcile(&mut manifest, &surviving).unwrap();

        let events = manifest.plan.get("events").unwrap();
        assert_eq!(events.to_delete[0].status, HandleStatus::Pending);
    }

    #[test]
    fn kept_items_stay_kept() {
        let mut manifest = make_manifest(
            vec![("p1", HandleStatus::Pending)],
            vec!["p2"],
        );
        let surviving = make_surviving(&[]);
        reconcile(&mut manifest, &surviving).unwrap();

        let people = manifest.plan.get("people").unwrap();
        assert_eq!(people.to_delete[0].status, HandleStatus::Deleted);
        assert_eq!(people.kept[0].status, HandleStatus::Kept);
    }

    #[test]
    fn empty_surviving_everything_deleted() {
        let mut manifest = make_manifest_multi(
            vec![
                ("people", vec![("p1", HandleStatus::Pending)]),
                ("families", vec![("f1", HandleStatus::Pending)]),
                ("events", vec![("e1", HandleStatus::Pending)]),
            ],
            vec![],
        );
        let surviving = make_surviving(&[]);
        reconcile(&mut manifest, &surviving).unwrap();

        for key in &["people", "families", "events"] {
            for entry in &manifest.plan[*key].to_delete {
                assert_eq!(
                    entry.status,
                    HandleStatus::Deleted,
                    "{} should be Deleted",
                    entry.handle
                );
            }
        }
    }

    #[test]
    fn surviving_handles_not_in_manifest_errors() {
        let mut manifest = make_manifest(
            vec![("p1", HandleStatus::Pending)],
            vec![],
        );
        let surviving = make_surviving(&["p1", "unknown_handle"]);
        let result = reconcile(&mut manifest, &surviving);
        assert!(matches!(
            result,
            Err(ReconciliationError::SurvivingHandlesNotInManifest(_))
        ));
    }

    #[test]
    fn idempotent_reconciliation() {
        // Reconcile twice should give the same result as once.
        let mut manifest = make_manifest(
            vec![("p1", HandleStatus::Pending)],
            vec![],
        );
        let surviving = make_surviving(&[]);

        // Reconcile once
        reconcile(&mut manifest, &surviving).unwrap();
        let state_after_once = manifest.plan.get("people").unwrap().to_delete[0].status.clone();

        // Reconcile again
        reconcile(&mut manifest, &surviving).unwrap();
        let state_after_twice = manifest.plan.get("people").unwrap().to_delete[0].status.clone();

        assert_eq!(state_after_once, state_after_twice);
        assert_eq!(state_after_twice, HandleStatus::Deleted);
    }

    #[test]
    fn zero_to_delete_is_noop() {
        let mut manifest = make_manifest(vec![], vec![]);
        let surviving = make_surviving(&[]);
        assert!(reconcile(&mut manifest, &surviving).is_ok());
    }

    #[test]
    fn unrecognized_deletion_mode_errors() {
        let mut manifest = make_manifest(
            vec![("p1", HandleStatus::Pending)],
            vec![],
        );
        manifest.deletion_mode = "delete_everything".to_string();
        let surviving = make_surviving(&[]);
        let result = reconcile(&mut manifest, &surviving);
        assert!(matches!(
            result,
            Err(ReconciliationError::UnrecognizedDeletionMode(_))
        ));
    }

    #[test]
    fn reconciliation_error_display() {
        let err = ReconciliationError::SurvivingHandlesNotInManifest(vec![
            "h1".to_string(),
            "h2".to_string(),
        ]);
        let msg = err.to_string();
        assert!(msg.contains("h1"));
        assert!(msg.contains("h2"));

        let err = ReconciliationError::UnrecognizedDeletionMode("bad_mode".to_string());
        let msg = err.to_string();
        assert!(msg.contains("bad_mode"));
    }
}