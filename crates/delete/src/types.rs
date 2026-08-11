//! Types for the deletion cascade engine and manifest serialization.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

/// A handle string uniquely identifying a node in the graph.
pub type Handle = String;

/// Status of a handle in the deletion manifest after reconciliation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleStatus {
    /// Awaiting deletion (cascade says delete, but not yet executed by Gramps).
    Pending,
    /// Successfully deleted by Gramps.
    Deleted,
    /// User chose to keep this object.
    Kept,
}

/// An entry in the `to_delete` or `kept` list with status tracking.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandleEntry {
    pub handle: Handle,
    pub status: HandleStatus,
}

/// Untagged enum for v1 backward-compatible deserialization.
///
/// v1 manifests use flat strings: `"to_delete": ["handle1", ...]`
/// v2 manifests use objects:     `"to_delete": [{"handle": "...", "status": "..."}, ...]`
///
/// This enum is deserialization-only. Serialization always emits v2 format.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
enum HandleOrEntry {
    /// v1 format: bare handle string.
    V1(String),
    /// v2 format: handle entry object.
    V2(HandleEntry),
}

/// The decision state for reviewing deletion candidates.
#[derive(Clone, Debug, PartialEq)]
pub enum ReviewState {
    /// Deletion has been confirmed by the user.
    Confirmed,
    /// Deletion was skipped by the user.
    Skipped,
    /// Review is pending (not yet decided).
    Pending,
}

/// A candidate for deletion, with metadata for user review.
#[derive(Clone, Debug, PartialEq)]
pub struct DeleteCandidate {
    /// The handle of the node.
    pub handle: Handle,
    /// The kind of node (Person, Family, Event, etc.).
    pub node_kind: NodeKindLabel,
    /// A human-readable description (e.g., "Birth of John Smith").
    pub description: String,
    /// The review state for this candidate.
    pub state: ReviewState,
}

/// A human-readable label for node types, used in review prompts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NodeKindLabel {
    Person,
    Family,
    Event,
    Place,
    Source,
    Citation,
    Repository,
    Media,
    Note,
    Tag,
}

impl NodeKindLabel {
    /// Return the plural label for this kind.
    pub fn plural(&self) -> &'static str {
        match self {
            NodeKindLabel::Person => "people",
            NodeKindLabel::Family => "families",
            NodeKindLabel::Event => "events",
            NodeKindLabel::Place => "places",
            NodeKindLabel::Source => "sources",
            NodeKindLabel::Citation => "citations",
            NodeKindLabel::Repository => "repositories",
            NodeKindLabel::Media => "media",
            NodeKindLabel::Note => "notes",
            NodeKindLabel::Tag => "tags",
        }
    }
}

/// The complete deletion plan for one type group.
///
/// Serialization always emits v2 format (objects with `handle` + `status`).
/// Deserialization accepts both v1 (flat strings) and v2 (objects) formats.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TypePlan {
    /// Handles to delete.
    pub to_delete: Vec<HandleEntry>,
    /// Handles that were considered but kept.
    #[serde(default)]
    pub kept: Vec<HandleEntry>,
}

/// Custom deserializer for `TypePlan` that supports both v1 and v2 formats.
///
/// v1: `{"to_delete": ["h1", "h2"], "kept": ["h3"]}`
/// v2: `{"to_delete": [{"handle": "h1", "status": "pending"}], ...}`
impl<'de> Deserialize<'de> for TypePlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Intermediate helper that uses the untagged HandleOrEntry enum.
        #[derive(Deserialize)]
        struct Helper {
            to_delete: Vec<HandleOrEntry>,
            #[serde(default)]
            kept: Vec<HandleOrEntry>,
        }

        let helper = Helper::deserialize(deserializer)?;

        let to_delete = helper
            .to_delete
            .into_iter()
            .map(|h| match h {
                HandleOrEntry::V1(s) => HandleEntry {
                    handle: s,
                    status: HandleStatus::Pending,
                },
                HandleOrEntry::V2(e) => e,
            })
            .collect();

        let kept = helper
            .kept
            .into_iter()
            .map(|h| match h {
                HandleOrEntry::V1(s) => HandleEntry {
                    handle: s,
                    status: HandleStatus::Kept,
                },
                HandleOrEntry::V2(e) => e,
            })
            .collect();

        Ok(TypePlan { to_delete, kept })
    }
}

/// Serializable deletion manifest that can be saved and reloaded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeleteManifest {
    /// Format version (currently 2).
    pub version: u32,
    /// Name of the source file.
    pub source_file: String,
    /// Name of the selections file used.
    #[serde(default)]
    pub selections_file: Option<String>,
    /// ISO timestamp of when the manifest was created.
    pub created_at: String,
    /// The seed handles (people selected for deletion).
    pub seed_people: Vec<Handle>,
    /// Deletion mode (e.g. "people_only").
    #[serde(default = "default_deletion_mode")]
    pub deletion_mode: String,
    /// The deletion plan per type, in dependency order.
    pub plan: HashMap<String, TypePlan>,
}

fn default_deletion_mode() -> String {
    "people_only".to_string()
}

/// The result of running the cascade engine.
#[derive(Clone, Debug, PartialEq)]
pub struct DeletePlan {
    /// All handles that will be deleted.
    pub to_delete: HashSet<Handle>,
    /// Pre-connectivity counts (number of incident edges before deletion).
    /// Used to distinguish newly orphaned from already orphaned.
    pub pre_connectivity: HashMap<Handle, usize>,
    /// The seed set that started the cascade.
    pub seed_people: HashSet<Handle>,
    /// Per-type breakdown of the deletion set.
    pub per_type: HashMap<NodeKindLabel, Vec<Handle>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_kind_label_plural_matches() {
        assert_eq!(NodeKindLabel::Person.plural(), "people");
        assert_eq!(NodeKindLabel::Family.plural(), "families");
        assert_eq!(NodeKindLabel::Event.plural(), "events");
        assert_eq!(NodeKindLabel::Place.plural(), "places");
        assert_eq!(NodeKindLabel::Source.plural(), "sources");
        assert_eq!(NodeKindLabel::Citation.plural(), "citations");
        assert_eq!(NodeKindLabel::Repository.plural(), "repositories");
        assert_eq!(NodeKindLabel::Media.plural(), "media");
        assert_eq!(NodeKindLabel::Note.plural(), "notes");
        assert_eq!(NodeKindLabel::Tag.plural(), "tags");
    }

    #[test]
    fn delete_candidate_default_state_is_pending() {
        let c = DeleteCandidate {
            handle: "p0001".to_string(),
            node_kind: NodeKindLabel::Person,
            description: "John Smith".to_string(),
            state: ReviewState::Pending,
        };
        assert_eq!(c.state, ReviewState::Pending);
        assert_eq!(c.handle, "p0001");
    }

    #[test]
    fn handle_status_serde() {
        let json = serde_json::to_string(&HandleStatus::Pending).unwrap();
        assert_eq!(json, "\"pending\"");
        let restored: HandleStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, HandleStatus::Pending);

        let json = serde_json::to_string(&HandleStatus::Deleted).unwrap();
        assert_eq!(json, "\"deleted\"");
        let restored: HandleStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, HandleStatus::Deleted);

        let json = serde_json::to_string(&HandleStatus::Kept).unwrap();
        assert_eq!(json, "\"kept\"");
        let restored: HandleStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, HandleStatus::Kept);
    }

    #[test]
    fn handle_entry_serde_v2() {
        let entry = HandleEntry {
            handle: "a1b2c3d4".to_string(),
            status: HandleStatus::Pending,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(json, r#"{"handle":"a1b2c3d4","status":"pending"}"#);
        let restored: HandleEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, entry);
    }

    #[test]
    fn type_plan_v2_roundtrip() {
        let plan = TypePlan {
            to_delete: vec![
                HandleEntry {
                    handle: "p1".to_string(),
                    status: HandleStatus::Pending,
                },
                HandleEntry {
                    handle: "p2".to_string(),
                    status: HandleStatus::Deleted,
                },
            ],
            kept: vec![HandleEntry {
                handle: "p3".to_string(),
                status: HandleStatus::Kept,
            }],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let restored: TypePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.to_delete.len(), 2);
        assert_eq!(restored.to_delete[0].handle, "p1");
        assert_eq!(restored.to_delete[0].status, HandleStatus::Pending);
        assert_eq!(restored.to_delete[1].handle, "p2");
        assert_eq!(restored.to_delete[1].status, HandleStatus::Deleted);
        assert_eq!(restored.kept.len(), 1);
        assert_eq!(restored.kept[0].handle, "p3");
        assert_eq!(restored.kept[0].status, HandleStatus::Kept);
    }

    #[test]
    fn type_plan_v1_deserialization() {
        // v1 format: flat strings — each string becomes HandleEntry with status=Pending
        let json = r#"{"to_delete":["p1","p2"],"kept":["p3"]}"#;
        let restored: TypePlan = serde_json::from_str(json).unwrap();
        assert_eq!(restored.to_delete.len(), 2);
        assert_eq!(restored.to_delete[0].handle, "p1");
        assert_eq!(restored.to_delete[0].status, HandleStatus::Pending);
        assert_eq!(restored.to_delete[1].handle, "p2");
        assert_eq!(restored.to_delete[1].status, HandleStatus::Pending);
        assert_eq!(restored.kept.len(), 1);
        assert_eq!(restored.kept[0].handle, "p3");
        assert_eq!(restored.kept[0].status, HandleStatus::Kept);
    }

    #[test]
    fn type_plan_mixed_v1_v2_still_works() {
        // v2 format with explicit statuses
        let json = r#"{"to_delete":[{"handle":"p1","status":"pending"},{"handle":"p2","status":"deleted"}],"kept":[]}"#;
        let restored: TypePlan = serde_json::from_str(json).unwrap();
        assert_eq!(restored.to_delete.len(), 2);
        assert_eq!(restored.to_delete[0].handle, "p1");
        assert_eq!(restored.to_delete[0].status, HandleStatus::Pending);
        assert_eq!(restored.to_delete[1].handle, "p2");
        assert_eq!(restored.to_delete[1].status, HandleStatus::Deleted);
    }

    #[test]
    fn delete_manifest_roundtrip() {
        let mut plan = HashMap::new();
        plan.insert(
            "people".to_string(),
            TypePlan {
                to_delete: vec![HandleEntry {
                    handle: "p1".to_string(),
                    status: HandleStatus::Pending,
                }],
                kept: vec![],
            },
        );
        plan.insert(
            "events".to_string(),
            TypePlan {
                to_delete: vec![HandleEntry {
                    handle: "e1".to_string(),
                    status: HandleStatus::Pending,
                }],
                kept: vec![HandleEntry {
                    handle: "e2".to_string(),
                    status: HandleStatus::Kept,
                }],
            },
        );
        let manifest = DeleteManifest {
            version: 2,
            source_file: "test.gramps".to_string(),
            selections_file: Some("sel.json".to_string()),
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec!["p1".to_string()],
            deletion_mode: "people_only".to_string(),
            plan,
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let restored: DeleteManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, 2);
        assert_eq!(restored.source_file, "test.gramps");
        assert_eq!(restored.seed_people, vec!["p1".to_string()]);
        assert_eq!(restored.deletion_mode, "people_only");
        assert_eq!(restored.plan.len(), 2);
    }

    #[test]
    fn delete_manifest_default_deletion_mode() {
        let manifest = DeleteManifest {
            version: 2,
            source_file: "test.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec![],
            deletion_mode: "people_only".to_string(),
            plan: HashMap::new(),
        };
        // When serialized and deserialized without deletion_mode, it should get the default
        let json = serde_json::to_string(&manifest).unwrap();
        let restored: DeleteManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.deletion_mode, "people_only");
    }

    #[test]
    fn delete_plan_empty() {
        let plan = DeletePlan {
            to_delete: HashSet::new(),
            pre_connectivity: HashMap::new(),
            seed_people: HashSet::new(),
            per_type: HashMap::new(),
        };
        assert!(plan.to_delete.is_empty());
    }
}
