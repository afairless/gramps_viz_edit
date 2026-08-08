//! Types for the deletion cascade engine and manifest serialization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

/// A handle string uniquely identifying a node in the graph.
pub type Handle = String;

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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypePlan {
    /// Handles to delete.
    pub to_delete: Vec<Handle>,
    /// Handles that were considered but kept.
    #[serde(default)]
    pub kept: Vec<Handle>,
}

/// Serializable deletion manifest that can be saved and reloaded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeleteManifest {
    /// Format version (currently 1).
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
    /// The deletion plan per type, in dependency order.
    pub plan: HashMap<String, TypePlan>,
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
    fn type_plan_roundtrip() {
        let plan = TypePlan {
            to_delete: vec!["p1".to_string(), "p2".to_string()],
            kept: vec!["p3".to_string()],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let restored: TypePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.to_delete, vec!["p1".to_string(), "p2".to_string()]);
        assert_eq!(restored.kept, vec!["p3".to_string()]);
    }

    #[test]
    fn delete_manifest_roundtrip() {
        let mut plan = HashMap::new();
        plan.insert(
            "people".to_string(),
            TypePlan {
                to_delete: vec!["p1".to_string()],
                kept: vec![],
            },
        );
        plan.insert(
            "events".to_string(),
            TypePlan {
                to_delete: vec!["e1".to_string()],
                kept: vec!["e2".to_string()],
            },
        );
        let manifest = DeleteManifest {
            version: 1,
            source_file: "test.gramps".to_string(),
            selections_file: Some("sel.json".to_string()),
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec!["p1".to_string()],
            plan,
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let restored: DeleteManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, 1);
        assert_eq!(restored.source_file, "test.gramps");
        assert_eq!(restored.seed_people, vec!["p1".to_string()]);
        assert_eq!(restored.plan.len(), 2);
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
