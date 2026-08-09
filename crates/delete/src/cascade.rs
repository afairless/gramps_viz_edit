//! Deletion cascade engine — computes which nodes become orphaned when a set
//! of seed nodes (people) is removed from a graph.
//!
//! # Algorithm
//!
//! The engine operates **read-only** on the graph. It walks the graph's edge
//! indexes to determine which handles become orphaned as a result of deleting
//! the seed set, and returns a final `HashSet<Handle>`.
//!
//! ## Phase A — Record pre-existing connectivity
//!
//! For every node n in the graph, record the count of incident edges. This
//! distinguishes "newly orphaned" (was in use, then all connections severed)
//! from "already orphaned" (had zero connections before the operation).
//!
//! ## Phase B — Fixed-point loop
//!
//! Starting with the seed set, repeatedly iterate over all non-deleted nodes
//! and check if they have become orphans. New orphans are added to the
//! deletion set. The loop terminates when no new orphans are found (fixed
//! point reached).
//!
//! ## Phase C — Per-type orphan rules
//!
//! Each node type has a specific rule that determines when it is considered
//! orphaned. See the `type_specific_orphan_rule` function.

use std::collections::HashMap;
use std::collections::HashSet;

use typed_graph::{Edge, Graph, Handle, Node, NodeKind};

use crate::types::{DeletePlan, NodeKindLabel};

/// Run the deletion cascade on a graph starting from a set of seed handles
/// (typically people selected for deletion).
///
/// Returns a [`DeletePlan`] containing all handles to delete, pre-connectivity
/// data, and a per-type breakdown.
///
/// The graph is **read-only** — no nodes or edges are mutated.
pub fn cascade(graph: &Graph, seeds: &HashSet<Handle>) -> DeletePlan {
    // Phase A: Record pre-existing connectivity
    let mut pre_connectivity: HashMap<Handle, usize> = HashMap::new();
    for (handle, _) in graph.iter_nodes() {
        let count = graph.edges_incident_to(handle).len();
        pre_connectivity.insert(handle.clone(), count);
    }

    // Phase B: Fixed-point loop
    let mut to_delete: HashSet<Handle> = seeds.clone();

    // Only include seeds that actually exist in the graph
    to_delete.retain(|h| graph.contains_node(h));

    let all_handles: Vec<Handle> = graph.iter_nodes().map(|(h, _)| h.clone()).collect();

    loop {
        let mut new_candidates: HashSet<Handle> = HashSet::new();

        for handle in &all_handles {
            if to_delete.contains(handle) {
                continue;
            }

            // Skip nodes that were already orphaned before the operation
            let pre_count = pre_connectivity.get(handle).copied().unwrap_or(0);
            if pre_count == 0 {
                continue;
            }

            if type_specific_orphan_rule(handle, graph, &to_delete) {
                new_candidates.insert(handle.clone());
            }
        }

        if new_candidates.is_empty() {
            break;
        }

        to_delete.extend(new_candidates);
    }

    // Build per-type breakdown
    let mut per_type: HashMap<NodeKindLabel, Vec<Handle>> = HashMap::new();
    for handle in &to_delete {
        if let Some(node) = graph.get_node(handle) {
            let label = node_kind_to_label(node);
            per_type.entry(label).or_default().push(handle.clone());
        }
    }

    // Sort each per-type list for deterministic output
    for list in per_type.values_mut() {
        list.sort();
    }

    DeletePlan {
        to_delete,
        pre_connectivity,
        seed_people: seeds.clone(),
        per_type,
    }
}

/// Type-specific orphan rule.
///
/// Returns `true` if the node should be considered orphaned given the current
/// set of deleted nodes. Each type has different semantics for what counts as
/// "orphaned" — see §3.2 of the design document.
fn type_specific_orphan_rule(handle: &Handle, graph: &Graph, to_delete: &HashSet<Handle>) -> bool {
    let node = match graph.get_node(handle) {
        Some(n) => n,
        None => return false,
    };

    match node {
        Node::Person(_) => {
            // People are only in the deletion set if they were in the seed
            // (selected by the user). Unselected people are never deleted.
            false
        }
        Node::Family(_) => {
            // A family is orphaned if it has NO remaining connections to
            // non-deleted people (father, mother, OR children).
            let incident = graph.edges_incident_to(handle);
            let has_live_person_connection = incident.iter().any(|e| match e {
                Edge::FamilyFather { target, .. }
                | Edge::FamilyMother { target, .. }
                | Edge::FamilyChildRef { target, .. } => !to_delete.contains(target),
                _ => false,
            });
            // Must also check: is there at least one parent/child connection at all?
            // (Don't cascade purely from eventref connections)
            let has_any_person_connection = incident.iter().any(|e| {
                matches!(
                    e,
                    Edge::FamilyFather { .. }
                        | Edge::FamilyMother { .. }
                        | Edge::FamilyChildRef { .. }
                )
            });
            has_any_person_connection && !has_live_person_connection
        }
        Node::Event(_) => {
            // An event is orphaned if it has NO remaining incoming edges from
            // non-deleted Person/Family nodes (via PersonEventRef/FamilyEventRef).
            let incident = graph.edges_incident_to(handle);
            let has_live_event_ref = incident.iter().any(|e| {
                match e {
                    Edge::PersonEventRef { source, target, .. }
                    | Edge::FamilyEventRef { source, target, .. } => {
                        // The event is the target — check if source is live
                        let other = if target == handle { source } else { target };
                        !to_delete.contains(other)
                    }
                    _ => false,
                }
            });
            !has_live_event_ref
        }
        Node::Place(_) => {
            // A place is orphaned if it has NO remaining edges to non-deleted
            // nodes. Check EventPlace, PlacePlaceRef, PlaceCitation,
            // PlaceMediaRef, PlaceNote, and PlaceTag edges.
            let incident = graph.edges_incident_to(handle);
            let has_live_connection = incident.iter().any(|e| match e {
                Edge::EventPlace { source, target } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                Edge::PlacePlaceRef { source, target, .. } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                Edge::PlaceCitation { source, target } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                Edge::PlaceMediaRef { source, target, .. } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                Edge::PlaceNote { source, target } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                Edge::PlaceTag { source, target } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                _ => false,
            });
            !has_live_connection
        }
        Node::Source(_) => {
            // A source is orphaned if it has NO remaining CitationSource edges
            // from non-deleted Citations.
            let incident = graph.edges_incident_to(handle);
            let has_live_citation = incident.iter().any(|e| match e {
                Edge::CitationSource { source, target } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                _ => false,
            });
            !has_live_citation
        }
        Node::Citation(_) => {
            // A citation is orphaned if it has NO remaining incoming citationref
            // edges from any non-deleted object.
            let incident = graph.edges_incident_to(handle);
            let has_live_ref = incident.iter().any(|e| match e {
                Edge::PersonCitation { source, .. }
                | Edge::FamilyCitation { source, .. }
                | Edge::EventCitation { source, .. }
                | Edge::PlaceCitation { source, .. } => !to_delete.contains(source),
                _ => false,
            });
            !has_live_ref
        }
        Node::Repository(_) => {
            // A repository is orphaned if it has NO remaining SourceRepoRef
            // edges from non-deleted Sources.
            let incident = graph.edges_incident_to(handle);
            let has_live_repo_ref = incident.iter().any(|e| match e {
                Edge::SourceRepoRef { source, target, .. } => {
                    let other = if target == handle { source } else { target };
                    !to_delete.contains(other)
                }
                _ => false,
            });
            !has_live_repo_ref
        }
        Node::Media(_) => {
            // A media object is orphaned if it has NO remaining mediaref edges
            // from any non-deleted object.
            let incident = graph.edges_incident_to(handle);
            let has_live_media_ref = incident.iter().any(|e| match e {
                Edge::PersonMediaRef { source, .. }
                | Edge::FamilyMediaRef { source, .. }
                | Edge::EventMediaRef { source, .. }
                | Edge::PlaceMediaRef { source, .. }
                | Edge::SourceMediaRef { source, .. }
                | Edge::RepositoryMediaRef { source, .. } => !to_delete.contains(source),
                _ => false,
            });
            !has_live_media_ref
        }
        Node::Note(_) => {
            // A note is orphaned if it has NO remaining noteref edges from
            // any non-deleted object.
            let incident = graph.edges_incident_to(handle);
            let has_live_note_ref = incident.iter().any(|e| match e {
                Edge::PersonNote { source, .. }
                | Edge::FamilyNote { source, .. }
                | Edge::EventNote { source, .. }
                | Edge::PlaceNote { source, .. }
                | Edge::SourceNote { source, .. }
                | Edge::CitationNote { source, .. }
                | Edge::RepositoryNote { source, .. }
                | Edge::MediaNote { source, .. } => !to_delete.contains(source),
                _ => false,
            });
            !has_live_note_ref
        }
        Node::Tag(_) => {
            // A tag is orphaned if it has NO remaining tagref edges from any
            // non-deleted object.
            let incident = graph.edges_incident_to(handle);
            let has_live_tag_ref = incident.iter().any(|e| match e {
                Edge::PersonTag { source, .. }
                | Edge::FamilyTag { source, .. }
                | Edge::EventTag { source, .. }
                | Edge::PlaceTag { source, .. }
                | Edge::SourceTag { source, .. }
                | Edge::CitationTag { source, .. }
                | Edge::RepositoryTag { source, .. }
                | Edge::MediaTag { source, .. }
                | Edge::NoteTag { source, .. } => !to_delete.contains(source),
                // For TagTag, only incoming edges keep the tag alive.
                // An edge where this tag is the source is outgoing and
                // does not count as a keep-alive reference.
                Edge::TagTag { source, target } => {
                    if target == handle {
                        !to_delete.contains(source)
                    } else {
                        false
                    }
                }
                _ => false,
            });
            !has_live_tag_ref
        }
    }
}

/// Map a `Node` to its human-readable label.
pub fn node_kind_to_label(node: &Node) -> NodeKindLabel {
    match node {
        Node::Person(_) => NodeKindLabel::Person,
        Node::Family(_) => NodeKindLabel::Family,
        Node::Event(_) => NodeKindLabel::Event,
        Node::Place(_) => NodeKindLabel::Place,
        Node::Source(_) => NodeKindLabel::Source,
        Node::Citation(_) => NodeKindLabel::Citation,
        Node::Repository(_) => NodeKindLabel::Repository,
        Node::Media(_) => NodeKindLabel::Media,
        Node::Note(_) => NodeKindLabel::Note,
        Node::Tag(_) => NodeKindLabel::Tag,
    }
}

/// Map a `NodeKind` to its human-readable label.
pub fn node_kind_to_label_from_kind(kind: NodeKind) -> NodeKindLabel {
    match kind {
        NodeKind::Person => NodeKindLabel::Person,
        NodeKind::Family => NodeKindLabel::Family,
        NodeKind::Event => NodeKindLabel::Event,
        NodeKind::Place => NodeKindLabel::Place,
        NodeKind::Source => NodeKindLabel::Source,
        NodeKind::Citation => NodeKindLabel::Citation,
        NodeKind::Repository => NodeKindLabel::Repository,
        NodeKind::Media => NodeKindLabel::Media,
        NodeKind::Note => NodeKindLabel::Note,
        NodeKind::Tag => NodeKindLabel::Tag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // -----------------------------------------------------------------------
    // Helpers: build small graphs for testing
    // -----------------------------------------------------------------------

    /// Create a graph with a single person node.
    fn single_person_graph() -> (Graph, Handle) {
        let mut graph = Graph::new();
        let h = "p0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: h.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        (graph, h)
    }

    /// Create a graph with two people and a family connecting them.
    fn family_graph() -> (Graph, Handle, Handle, Handle) {
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let p2 = "p0002".to_string();
        let f1 = "f0001".to_string();

        graph
            .add_node(
                p1.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p1.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                p2.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p2.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                f1.clone(),
                Node::Family(typed_graph::FamilyData {
                    handle: f1.clone(),
                    ..typed_graph::FamilyData::default()
                }),
            )
            .unwrap();

        graph
            .add_edge(Edge::FamilyFather {
                source: f1.clone(),
                target: p1.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::FamilyMother {
                source: f1.clone(),
                target: p2.clone(),
            })
            .unwrap();

        (graph, p1, p2, f1)
    }

    /// Create a graph with a person, event, and connection.
    fn person_event_graph() -> (Graph, Handle, Handle) {
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let e1 = "e0001".to_string();

        graph
            .add_node(
                p1.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p1.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                e1.clone(),
                Node::Event(typed_graph::EventData {
                    handle: e1.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();

        graph
            .add_edge(Edge::PersonEventRef {
                source: p1.clone(),
                target: e1.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: e1.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();

        (graph, p1, e1)
    }

    // -----------------------------------------------------------------------
    // Comprehensive test helpers for cascade tests
    // -----------------------------------------------------------------------

    /// Create a person node with the given handle string.
    fn make_person(graph: &mut Graph, handle: &str) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: h.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        h
    }

    /// Create a family with two parents (FamilyFather + FamilyMother edges).
    /// Returns the family handle.
    fn make_family_with_parents(
        graph: &mut Graph,
        handle: &str,
        father: &Handle,
        mother: &Handle,
    ) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Family(typed_graph::FamilyData {
                    handle: h.clone(),
                    ..typed_graph::FamilyData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::FamilyFather {
                source: h.clone(),
                target: father.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::FamilyMother {
                source: h.clone(),
                target: mother.clone(),
            })
            .unwrap();
        h
    }

    /// Create a family with both parents and one child.
    /// The child gets a FamilyChildRef edge from the family.
    /// The parents also get PersonParentFamily edges back to the family.
    fn make_family_with_parents_and_child(
        graph: &mut Graph,
        handle: &str,
        father: &Handle,
        mother: &Handle,
        child: &Handle,
    ) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Family(typed_graph::FamilyData {
                    handle: h.clone(),
                    ..typed_graph::FamilyData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::FamilyFather {
                source: h.clone(),
                target: father.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::FamilyMother {
                source: h.clone(),
                target: mother.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::FamilyChildRef {
                source: h.clone(),
                target: child.clone(),
                metadata: Box::new(typed_graph::ChildRef {
                    ref_field: child.clone(),
                    ..typed_graph::ChildRef::default()
                }),
            })
            .unwrap();
        // Parent back-edges
        graph
            .add_edge(Edge::PersonParentFamily {
                source: father.clone(),
                target: h.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonParentFamily {
                source: mother.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create an event referenced by a person (PersonEventRef).
    fn make_event(graph: &mut Graph, handle: &str, person: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Event(typed_graph::EventData {
                    handle: h.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PersonEventRef {
                source: person.clone(),
                target: h.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: h.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();
        h
    }

    /// Create an event referenced by a person and linked to a place.
    /// PersonEventRef(person → event) + EventPlace(event → place).
    fn make_event_with_place(
        graph: &mut Graph,
        event_h: &str,
        person: &Handle,
        place_h: &Handle,
    ) -> Handle {
        let h = event_h.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Event(typed_graph::EventData {
                    handle: h.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PersonEventRef {
                source: person.clone(),
                target: h.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: h.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();
        graph
            .add_edge(Edge::EventPlace {
                source: h.clone(),
                target: place_h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a place and connect it to an event via EventPlace.
    fn make_place(graph: &mut Graph, handle: &str, event: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Place(typed_graph::PlaceData {
                    handle: h.clone(),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::EventPlace {
                source: event.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a PlacePlaceRef edge from source to target.
    fn make_place_with_place_ref(graph: &mut Graph, source_h: &Handle, target_h: &Handle) {
        graph
            .add_edge(Edge::PlacePlaceRef {
                source: source_h.clone(),
                target: target_h.clone(),
                metadata: Box::new(typed_graph::PlaceRef {
                    ref_field: target_h.clone(),
                    ..typed_graph::PlaceRef::default()
                }),
            })
            .unwrap();
    }

    /// Create a citation referenced by a person (PersonCitation).
    fn citation_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: h.clone(),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PersonCitation {
                source: person.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a citation referenced by an event (EventCitation).
    fn citation_from_event(graph: &mut Graph, handle: &str, event: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: h.clone(),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::EventCitation {
                source: event.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a citation referenced by a family (FamilyCitation).
    fn citation_from_family(graph: &mut Graph, handle: &str, family: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: h.clone(),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::FamilyCitation {
                source: family.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a citation referenced by a place (PlaceCitation).
    fn citation_from_place(graph: &mut Graph, handle: &str, place: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: h.clone(),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PlaceCitation {
                source: place.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a source referenced by a citation (CitationSource).
    fn source_from_citation(graph: &mut Graph, handle: &str, citation: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: h.clone(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::CitationSource {
                source: citation.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a repository referenced by a source (SourceRepoRef).
    fn repository_from_source(graph: &mut Graph, handle: &str, source: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Repository(typed_graph::RepositoryData {
                    handle: h.clone(),
                    ..typed_graph::RepositoryData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::SourceRepoRef {
                source: source.clone(),
                target: h.clone(),
                metadata: Box::new(typed_graph::RepoRef {
                    ref_field: h.clone(),
                    ..typed_graph::RepoRef::default()
                }),
            })
            .unwrap();
        h
    }

    /// Create a media object referenced by a person (PersonMediaRef).
    fn media_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Media(typed_graph::MediaData {
                    handle: h.clone(),
                    ..typed_graph::MediaData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PersonMediaRef {
                source: person.clone(),
                target: h.clone(),
                metadata: Box::new(typed_graph::MediaRef {
                    ref_field: h.clone(),
                    ..typed_graph::MediaRef::default()
                }),
            })
            .unwrap();
        h
    }

    /// Create a media object referenced by a citation (CitationMediaRef).
    fn media_from_citation(graph: &mut Graph, handle: &str, citation: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Media(typed_graph::MediaData {
                    handle: h.clone(),
                    ..typed_graph::MediaData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::CitationMediaRef {
                source: citation.clone(),
                target: h.clone(),
                metadata: Box::new(typed_graph::MediaRef {
                    ref_field: h.clone(),
                    ..typed_graph::MediaRef::default()
                }),
            })
            .unwrap();
        h
    }

    /// Create a media object referenced by a source (SourceMediaRef).
    fn media_from_source(graph: &mut Graph, handle: &str, source: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Media(typed_graph::MediaData {
                    handle: h.clone(),
                    ..typed_graph::MediaData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::SourceMediaRef {
                source: source.clone(),
                target: h.clone(),
                metadata: Box::new(typed_graph::MediaRef {
                    ref_field: h.clone(),
                    ..typed_graph::MediaRef::default()
                }),
            })
            .unwrap();
        h
    }

    /// Create a note referenced by a person (PersonNote).
    fn note_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Note(typed_graph::NoteData {
                    handle: h.clone(),
                    ..typed_graph::NoteData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PersonNote {
                source: person.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a note referenced by a citation (CitationNote).
    fn note_from_citation(graph: &mut Graph, handle: &str, citation: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Note(typed_graph::NoteData {
                    handle: h.clone(),
                    ..typed_graph::NoteData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::CitationNote {
                source: citation.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a tag referenced by a person (PersonTag).
    fn tag_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Tag(typed_graph::TagData {
                    handle: h.clone(),
                    ..typed_graph::TagData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PersonTag {
                source: person.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a tag referenced by an event (EventTag).
    fn tag_from_event(graph: &mut Graph, handle: &str, event: &Handle) -> Handle {
        let h = handle.to_string();
        graph
            .add_node(
                h.clone(),
                Node::Tag(typed_graph::TagData {
                    handle: h.clone(),
                    ..typed_graph::TagData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::EventTag {
                source: event.clone(),
                target: h.clone(),
            })
            .unwrap();
        h
    }

    /// Create a TagTag edge from source to target.
    fn tag_tag(graph: &mut Graph, source_h: &Handle, target_h: &Handle) {
        graph
            .add_edge(Edge::TagTag {
                source: source_h.clone(),
                target: target_h.clone(),
            })
            .unwrap();
    }

    // -----------------------------------------------------------------------
    // Cascade engine tests
    // -----------------------------------------------------------------------

    #[test]
    fn empty_seed_set_returns_empty() {
        let (graph, _) = single_person_graph();
        let seeds = HashSet::new();
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.is_empty());
    }

    #[test]
    fn non_existent_handle_is_ignored() {
        let (graph, _) = single_person_graph();
        let mut seeds = HashSet::new();
        seeds.insert("nonexistent".to_string());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.is_empty());
    }

    #[test]
    fn seed_person_is_deleted() {
        let (graph, p1) = single_person_graph();
        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert_eq!(plan.to_delete.len(), 1);
        assert!(plan.to_delete.contains(&p1));
    }

    #[test]
    fn orphaned_family_is_cascaded() {
        let (graph, p1, p2, f1) = family_graph();
        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());
        let plan = cascade(&graph, &seeds);
        // Both people + family should be deleted
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&p2));
        assert!(plan.to_delete.contains(&f1));
    }

    #[test]
    fn family_stays_if_one_parent_remains() {
        let (graph, p1, _p2, f1) = family_graph();
        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // p1 deleted, p2 remains, so family should stay
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&f1));
    }

    #[test]
    fn orphaned_event_is_cascaded() {
        let (graph, p1, e1) = person_event_graph();
        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
    }

    #[test]
    fn event_stays_if_live_person_remains() {
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let p2 = "p0002".to_string();
        let e1 = "e0001".to_string();

        graph
            .add_node(
                p1.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p1.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                p2.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p2.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                e1.clone(),
                Node::Event(typed_graph::EventData {
                    handle: e1.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();

        // Both people reference the same event
        graph
            .add_edge(Edge::PersonEventRef {
                source: p1.clone(),
                target: e1.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: e1.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonEventRef {
                source: p2.clone(),
                target: e1.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: e1.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // p1 deleted, but p2 still references e1, so event stays
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&e1));
    }

    #[test]
    fn already_orphaned_node_is_not_deleted() {
        let mut graph = Graph::new();
        // An event with no incoming edges (already orphaned)
        let e1 = "e0001".to_string();
        graph
            .add_node(
                e1.clone(),
                Node::Event(typed_graph::EventData {
                    handle: e1.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();

        // A person not connected to the event
        let p1 = "p0001".to_string();
        graph
            .add_node(
                p1.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p1.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // Person is deleted, but already-orphaned event is NOT deleted
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&e1));
    }

    #[test]
    fn person_person_ref_does_not_cascade() {
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let p2 = "p0002".to_string();

        graph
            .add_node(
                p1.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p1.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                p2.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p2.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();

        // Association edge (PersonPersonRef) — does NOT cascade
        graph
            .add_edge(Edge::PersonPersonRef {
                source: p1.clone(),
                target: p2.clone(),
                metadata: Box::new(typed_graph::PersonRef {
                    ref_field: p2.clone(),
                    ..typed_graph::PersonRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(
            !plan.to_delete.contains(&p2),
            "PersonPersonRef should not cascade"
        );
    }

    #[test]
    fn transitive_cascade() {
        // A -> B -> C chain: A has event B, B is connected to place C
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let e1 = "e0001".to_string();
        let pl1 = "pl0001".to_string();

        graph
            .add_node(
                p1.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p1.clone(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                e1.clone(),
                Node::Event(typed_graph::EventData {
                    handle: e1.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                pl1.clone(),
                Node::Place(typed_graph::PlaceData {
                    handle: pl1.clone(),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();

        graph
            .add_edge(Edge::PersonEventRef {
                source: p1.clone(),
                target: e1.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: e1.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();
        graph
            .add_edge(Edge::EventPlace {
                source: e1.clone(),
                target: pl1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1), "Event should be cascaded");
        assert!(
            plan.to_delete.contains(&pl1),
            "Place should be transitively cascaded"
        );
    }

    #[test]
    fn place_kept_alive_by_place_citation() {
        // A place referenced by a citation that is NOT being deleted
        // should be kept alive via PlaceCitation edge.
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let e1 = "e0001".to_string();
        let pl1 = "pl0001".to_string();
        let c1 = "c0001".to_string();

        graph.add_node(p1.clone(), Node::Person(typed_graph::PersonData {
            handle: p1.clone(),
            ..typed_graph::PersonData::default()
        })).unwrap();
        graph.add_node(e1.clone(), Node::Event(typed_graph::EventData {
            handle: e1.clone(),
            ..typed_graph::EventData::default()
        })).unwrap();
        graph.add_node(pl1.clone(), Node::Place(typed_graph::PlaceData {
            handle: pl1.clone(),
            ..typed_graph::PlaceData::default()
        })).unwrap();
        graph.add_node(c1.clone(), Node::Citation(typed_graph::CitationData {
            handle: c1.clone(),
            ..typed_graph::CitationData::default()
        })).unwrap();

        graph.add_edge(Edge::PersonEventRef {
            source: p1.clone(),
            target: e1.clone(),
            metadata: Box::new(typed_graph::EventRef {
                ref_field: e1.clone(),
                ..typed_graph::EventRef::default()
            }),
        }).unwrap();
        graph.add_edge(Edge::EventPlace {
            source: e1.clone(),
            target: pl1.clone(),
        }).unwrap();
        // Place also connected to a live citation
        graph.add_edge(Edge::PlaceCitation {
            source: pl1.clone(),
            target: c1.clone(),
        }).unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // Person and event are cascaded, but place is kept alive by citation
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&pl1),
            "Place should be kept alive by PlaceCitation to live citation");
    }

    #[test]
    fn place_kept_alive_by_place_media_ref() {
        // A place kept alive by a PlaceMediaRef edge to non-deleted media
        // should not cascade even when its EventPlace connection is deleted.
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let e1 = "e0001".to_string();
        let pl1 = "pl0001".to_string();
        let m1 = "m0001".to_string();

        graph.add_node(p1.clone(), Node::Person(typed_graph::PersonData {
            handle: p1.clone(),
            ..typed_graph::PersonData::default()
        })).unwrap();
        graph.add_node(e1.clone(), Node::Event(typed_graph::EventData {
            handle: e1.clone(),
            ..typed_graph::EventData::default()
        })).unwrap();
        graph.add_node(pl1.clone(), Node::Place(typed_graph::PlaceData {
            handle: pl1.clone(),
            ..typed_graph::PlaceData::default()
        })).unwrap();
        graph.add_node(m1.clone(), Node::Media(typed_graph::MediaData {
            handle: m1.clone(),
            ..typed_graph::MediaData::default()
        })).unwrap();

        graph.add_edge(Edge::PersonEventRef {
            source: p1.clone(),
            target: e1.clone(),
            metadata: Box::new(typed_graph::EventRef {
                ref_field: e1.clone(),
                ..typed_graph::EventRef::default()
            }),
        }).unwrap();
        graph.add_edge(Edge::EventPlace {
            source: e1.clone(),
            target: pl1.clone(),
        }).unwrap();
        // Place also connected to live media
        graph.add_edge(Edge::PlaceMediaRef {
            source: pl1.clone(),
            target: m1.clone(),
            metadata: Box::new(typed_graph::MediaRef {
                ref_field: m1.clone(),
                ..typed_graph::MediaRef::default()
            }),
        }).unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&pl1),
            "Place should be kept alive by PlaceMediaRef to live media");
    }

    // -----------------------------------------------------------------------
    // Category A: Directly associated, isolated → DELETED
    // -----------------------------------------------------------------------
    // A1: seed person alone -> deleted
    #[test]
    fn a1_seed_person_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert_eq!(plan.to_delete.len(), 1);
        assert!(plan.to_delete.contains(&p1));
    }

    // A2: family with both parents deleted
    #[test]
    fn a2_family_all_parents_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let f1 = make_family_with_parents(&mut graph, "f1", &p1, &p2);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&p2));
        assert!(plan.to_delete.contains(&f1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // A3: family with both parents and child all deleted
    #[test]
    fn a3_family_all_parents_and_children_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let p3 = make_person(&mut graph, "p3");
        let f1 = make_family_with_parents_and_child(&mut graph, "f1", &p1, &p2, &p3);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());
        seeds.insert(p3.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&p2));
        assert!(plan.to_delete.contains(&p3));
        assert!(plan.to_delete.contains(&f1));
        assert_eq!(plan.to_delete.len(), 4);
    }

    // A4: single event from single person -> deleted
    #[test]
    fn a4_event_birth_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // A5: event shared by two people, both deleted -> event deleted
    #[test]
    fn a5_event_shared_two_people_both_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let e1 = make_event(&mut graph, "e1", &p1);
        // Connect p2 to same event manually
        graph
            .add_edge(Edge::PersonEventRef {
                source: p2.clone(),
                target: e1.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: e1.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&p2));
        assert!(plan.to_delete.contains(&e1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // A6: marriage event on family, both parents deleted -> event deleted
    #[test]
    fn a6_event_marriage_both_parents_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let f1 = make_family_with_parents(&mut graph, "f1", &p1, &p2);
        let e1 = "e1".to_string();
        graph
            .add_node(
                e1.clone(),
                Node::Event(typed_graph::EventData {
                    handle: e1.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::FamilyEventRef {
                source: f1.clone(),
                target: e1.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: e1.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&p2));
        assert!(plan.to_delete.contains(&f1));
        assert!(plan.to_delete.contains(&e1));
        assert_eq!(plan.to_delete.len(), 4);
    }

    // A7: citation from person -> deleted
    #[test]
    fn a7_citation_person_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let c1 = citation_from_person(&mut graph, "c1", &p1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // A8: source cascaded from citation
    #[test]
    fn a8_source_cascade_from_citation() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        let s1 = source_from_citation(&mut graph, "s1", &c1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert!(plan.to_delete.contains(&s1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // A9: repository cascaded from source
    #[test]
    fn a9_repository_cascade_from_source() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        let s1 = source_from_citation(&mut graph, "s1", &c1);
        let r1 = repository_from_source(&mut graph, "r1", &s1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert!(plan.to_delete.contains(&s1));
        assert!(plan.to_delete.contains(&r1));
        assert_eq!(plan.to_delete.len(), 4);
    }

    // A10: media cascaded from person
    #[test]
    fn a10_media_cascade_from_person() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let m1 = media_from_person(&mut graph, "m1", &p1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&m1));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // A11: media cascaded from source
    #[test]
    fn a11_media_cascade_from_source() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        let s1 = source_from_citation(&mut graph, "s1", &c1);
        let m1 = media_from_source(&mut graph, "m1", &s1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert!(plan.to_delete.contains(&s1));
        assert!(plan.to_delete.contains(&m1));
        assert_eq!(plan.to_delete.len(), 4);
    }

    // A12: media cascaded from citation
    #[test]
    fn a12_media_cascade_from_citation() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        let m1 = media_from_citation(&mut graph, "m1", &c1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert!(plan.to_delete.contains(&m1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // A13: note cascaded from person
    #[test]
    fn a13_note_cascade_from_person() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let n1 = note_from_person(&mut graph, "n1", &p1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&n1));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // A14: note cascaded from citation
    #[test]
    fn a14_note_cascade_from_citation() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        let n1 = note_from_citation(&mut graph, "n1", &c1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert!(plan.to_delete.contains(&n1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // A15: tag cascaded from person
    #[test]
    fn a15_tag_cascade_from_person() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let t1 = tag_from_person(&mut graph, "t1", &p1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&t1));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // A16: tag cascaded from event
    #[test]
    fn a16_tag_cascade_from_event() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let t1 = tag_from_event(&mut graph, "t1", &e1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&t1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // A17: tag-to-tag cascade
    #[test]
    fn a17_tag_tag_cascade() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let t1 = tag_from_person(&mut graph, "t1", &p1);
        let t2 = "t2".to_string();
        graph
            .add_node(
                t2.clone(),
                Node::Tag(typed_graph::TagData {
                    handle: t2.clone(),
                    ..typed_graph::TagData::default()
                }),
            )
            .unwrap();
        tag_tag(&mut graph, &t1, &t2);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&t1));
        assert!(plan.to_delete.contains(&t2));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // A18: place transitive cascade via event
    #[test]
    fn a18_place_transitive_cascade() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let pl1 = make_place(&mut graph, "pl1", &e1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&pl1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // -----------------------------------------------------------------------
    // Property-like invariants
    // -----------------------------------------------------------------------

    #[test]
    fn seeds_are_always_in_result() {
        let (graph, p1) = single_person_graph();
        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
    }

    #[test]
    fn idempotency() {
        let (graph, p1, p2, _f1) = family_graph();
        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());

        let plan1 = cascade(&graph, &seeds);
        let plan2 = cascade(&graph, &plan1.to_delete);
        assert_eq!(plan1.to_delete, plan2.to_delete);
    }

    #[test]
    fn pre_connectivity_positive_for_connected_node() {
        let (graph, p1, _p2, _f1) = family_graph();
        let seeds = HashSet::new();
        let plan = cascade(&graph, &seeds);
        // p1 has 1 incident edge (FamilyFather)
        assert_eq!(*plan.pre_connectivity.get(&p1).unwrap_or(&0), 1);
    }

    #[test]
    fn pre_connectivity_zero_for_isolated_node() {
        let mut graph = Graph::new();
        let e1 = "e0001".to_string();
        graph
            .add_node(
                e1.clone(),
                Node::Event(typed_graph::EventData {
                    handle: e1.clone(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        let plan = cascade(&graph, &HashSet::new());
        assert_eq!(*plan.pre_connectivity.get(&e1).unwrap_or(&0), 0);
    }
}
