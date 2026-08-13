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
//! For places, an additional pre-existing **in-use** count is recorded:
//! the number of incoming keep-alive edges (`EventPlace` and `PlacePlaceRef`
//! where the place is the `target`). A place with zero incoming keep-alive
//! edges before the operation was already orphaned — even if it has outgoing
//! references such as `PlaceNote` — and is never flagged by the cascade.
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
/// Extract the other endpoint of an edge, given one endpoint.
fn edge_other_endpoint(edge: &Edge, handle: &Handle) -> Handle {
    let (source, target) = match edge {
        Edge::CitationMediaRef { source, target, .. }
        | Edge::CitationNote { source, target }
        | Edge::CitationSource { source, target }
        | Edge::CitationTag { source, target }
        | Edge::CitationRef { source, target }
        | Edge::NoteRef { source, target }
        | Edge::MediaRef { source, target }
        | Edge::TagRef { source, target }
        | Edge::EventCitation { source, target }
        | Edge::EventMediaRef { source, target, .. }
        | Edge::EventNote { source, target }
        | Edge::EventPlace { source, target }
        | Edge::EventTag { source, target }
        | Edge::FamilyCitation { source, target }
        | Edge::FamilyFather { source, target }
        | Edge::FamilyMediaRef { source, target, .. }
        | Edge::FamilyMother { source, target }
        | Edge::FamilyNote { source, target }
        | Edge::FamilyTag { source, target }
        | Edge::MediaCitation { source, target }
        | Edge::MediaNote { source, target }
        | Edge::MediaTag { source, target }
        | Edge::NoteCitation { source, target }
        | Edge::NoteTag { source, target }
        | Edge::PersonCitation { source, target }
        | Edge::PersonFamily { source, target }
        | Edge::PersonMediaRef { source, target, .. }
        | Edge::PersonNote { source, target }
        | Edge::PersonParentFamily { source, target }
        | Edge::PersonTag { source, target }
        | Edge::PlaceCitation { source, target }
        | Edge::PlaceMediaRef { source, target, .. }
        | Edge::PlaceNote { source, target }
        | Edge::PlacePlaceRef { source, target, .. }
        | Edge::PlaceTag { source, target }
        | Edge::RepositoryMediaRef { source, target, .. }
        | Edge::RepositoryNote { source, target }
        | Edge::RepositoryTag { source, target }
        | Edge::SourceMediaRef { source, target, .. }
        | Edge::SourceNote { source, target }
        | Edge::SourceTag { source, target }
        | Edge::TagTag { source, target } => (source.clone(), target.clone()),
        Edge::FamilyChildRef {
            source,
            target,
            metadata: _,
        }
        | Edge::FamilyEventRef {
            source,
            target,
            metadata: _,
        }
        | Edge::PersonEventRef {
            source,
            target,
            metadata: _,
        }
        | Edge::PersonPersonRef {
            source,
            target,
            metadata: _,
        }
        | Edge::SourceRepoRef {
            source,
            target,
            metadata: _,
        } => (source.clone(), target.clone()),
    };
    if source == *handle {
        target
    } else {
        source
    }
}

/// Count a place's pre-existing incoming keep-alive edges.
///
/// Incoming keep-alive edges are `EventPlace` and `PlacePlaceRef` where the
/// place is the `target`. Outgoing edges (`PlaceCitation`, `PlaceMediaRef`,
/// `PlaceNote`, `PlaceTag`, and outgoing `PlacePlaceRef`) do not count.
fn pre_place_in_use_count(graph: &Graph, handle: &Handle) -> usize {
    graph
        .edges_incident_to(handle)
        .iter()
        .filter(|e| match e {
            Edge::EventPlace { target, .. } => target == handle,
            Edge::PlacePlaceRef { target, .. } => target == handle,
            _ => false,
        })
        .count()
}

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
    let mut pre_place_in_use: HashMap<Handle, usize> = HashMap::new();
    for (handle, node) in graph.iter_nodes() {
        let count = graph.edges_incident_to(handle).len();
        pre_connectivity.insert(handle.clone(), count);
        if matches!(node, Node::Place(_)) {
            pre_place_in_use.insert(handle.clone(), pre_place_in_use_count(graph, handle));
        }
    }

    // Phase B: Frontier-based BFS cascade
    let mut to_delete: HashSet<Handle> = seeds.clone();

    // Only include seeds that actually exist in the graph
    to_delete.retain(|h| graph.contains_node(h));

    // Frontier: nodes whose neighbors should be evaluated for orphanhood.
    // Using Vec as a stack for DFS-like traversal. The frontier is re-sorted
    // at the top of each iteration so processing order is deterministic and
    // independent of HashSet/Vec iteration order. Sorting guarantees the
    // cascade converges to the same `to_delete` set regardless of the
    // (nondeterministic) order in which seeds and neighbors are iterated.
    let mut frontier: Vec<Handle> = to_delete.iter().cloned().collect();
    frontier.sort_unstable();

    while let Some(handle) = frontier.pop() {
        // Re-sort the remaining frontier so each pop is deterministic.
        frontier.sort_unstable();
        for edge in graph.edges_incident_to(&handle) {
            let neighbor = edge_other_endpoint(edge, &handle);

            if to_delete.contains(&neighbor) {
                continue;
            }

            // Skip nodes that were already orphaned before the operation
            let pre_count = pre_connectivity.get(&neighbor).copied().unwrap_or(0);
            if pre_count == 0 {
                continue;
            }

            // A place with zero incoming keep-alive edges before the operation
            // was already orphaned — never flag it, even if it has outgoing refs.
            // (A place may have pre_count > 0 solely from outgoing edges such as
            // PlaceNote; such a place is semantically already orphaned.)
            if let Some(node) = graph.get_node(&neighbor) {
                if matches!(node, Node::Place(_))
                    && pre_place_in_use.get(&neighbor).copied().unwrap_or(0) == 0
                {
                    continue;
                }
            }

            if type_specific_orphan_rule(&neighbor, graph, &to_delete) {
                to_delete.insert(neighbor.clone());
                frontier.push(neighbor);
            }
        }
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

    // Hard invariant: non-seed people are never deleted by cascade.
    // Seed people are placed in to_delete before the cascade starts, so
    // this function is never called for them — only for non-seed people
    // who are neighbors of deleted nodes. Such people must never be
    // flagged as orphaned, regardless of their connectivity.
    if matches!(node, Node::Person(_)) {
        return false;
    }

    match node {
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
            // A place is orphaned if it has NO remaining INCOMING edges from
            // non-deleted nodes. Only incoming edges (EventPlace target,
            // PlacePlaceRef target) keep a place alive. Outgoing edges like
            // PlaceCitation, PlaceMediaRef, PlaceNote, PlaceTag are the place's
            // own references and do not count as keep-alive connections.
            let incident = graph.edges_incident_to(handle);
            let has_live_incoming = incident.iter().any(|e| match e {
                // EventPlace: event -> place (incoming). Check if event is alive.
                Edge::EventPlace { source, target } => {
                    if target == handle {
                        !to_delete.contains(source)
                    } else {
                        false
                    }
                }
                // PlacePlaceRef: source -> target. Only incoming (this place
                // is the target) keeps us alive.
                Edge::PlacePlaceRef { source, target, .. } => {
                    if target == handle {
                        !to_delete.contains(source)
                    } else {
                        false
                    }
                }
                // Outgoing edges (PlaceCitation, PlaceMediaRef, PlaceNote,
                // PlaceTag) do NOT keep the place alive.
                _ => false,
            });
            !has_live_incoming
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
        // Catch-all: Person is handled by the guard at the top of this function;
        // any other unexpected node type is also kept alive.
        _ => false,
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
    fn place_outgoing_citation_cascades() {
        // PlaceCitation is an outgoing edge from Place → Citation.
        // It does NOT keep the place alive. When the place's only incoming
        // edge (EventPlace) is severed, the place is orphaned and cascades,
        // which then orphanes the citation too.
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let e1 = "e0001".to_string();
        let pl1 = "pl0001".to_string();
        let c1 = "c0001".to_string();

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
            .add_node(
                c1.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: c1.clone(),
                    ..typed_graph::CitationData::default()
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
        // PlaceCitation is outgoing — does not keep place alive
        graph
            .add_edge(Edge::PlaceCitation {
                source: pl1.clone(),
                target: c1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // Person, event, place, and citation all cascade
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&pl1));
        assert!(plan.to_delete.contains(&c1));
        assert_eq!(plan.to_delete.len(), 4);
    }

    #[test]
    fn place_outgoing_media_ref_cascades() {
        // PlaceMediaRef is an outgoing edge from Place → Media.
        // It does NOT keep the place alive.
        let mut graph = Graph::new();
        let p1 = "p0001".to_string();
        let e1 = "e0001".to_string();
        let pl1 = "pl0001".to_string();
        let m1 = "m0001".to_string();

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
            .add_node(
                m1.clone(),
                Node::Media(typed_graph::MediaData {
                    handle: m1.clone(),
                    ..typed_graph::MediaData::default()
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
        // PlaceMediaRef is outgoing — does not keep place alive
        graph
            .add_edge(Edge::PlaceMediaRef {
                source: pl1.clone(),
                target: m1.clone(),
                metadata: Box::new(typed_graph::MediaRef {
                    ref_field: m1.clone(),
                    ..typed_graph::MediaRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // Person, event, place, and media all cascade
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&pl1));
        assert!(plan.to_delete.contains(&m1));
        assert_eq!(plan.to_delete.len(), 4);
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
    // Category B: Directly associated, NOT isolated → KEPT
    // -----------------------------------------------------------------------

    // B1: one parent deleted, other keeps family alive
    #[test]
    fn b1_family_one_parent_remains() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let f1 = make_family_with_parents(&mut graph, "f1", &p1, &p2);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&f1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B2: child keeps family alive when parent is deleted
    #[test]
    fn b2_family_child_keeps_family_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p3 = make_person(&mut graph, "p3");
        let f1 = "f1".to_string();
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
            .add_edge(Edge::FamilyChildRef {
                source: f1.clone(),
                target: p3.clone(),
                metadata: Box::new(typed_graph::ChildRef {
                    ref_field: p3.clone(),
                    ..typed_graph::ChildRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&f1));
        assert!(!plan.to_delete.contains(&p3));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B3: event shared by two people, one remains
    #[test]
    fn b3_event_shared_two_people_one_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let e1 = make_event(&mut graph, "e1", &p1);
        // Connect p2 to same event
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
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B4: marriage event kept alive by surviving parent
    #[test]
    fn b4_event_marriage_one_parent_remains() {
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
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&f1));
        assert!(!plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B5: citation shared by two people, one remains
    #[test]
    fn b5_citation_shared_person_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        // Connect p2 to same citation
        graph
            .add_edge(Edge::PersonCitation {
                source: p2.clone(),
                target: c1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&c1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B6: source shared across two citations, one keeps it alive
    #[test]
    fn b6_source_shared_citation_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let s1 = "s1".to_string();
        graph
            .add_node(
                s1.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: s1.clone(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        // Both citations reference the same source
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        // Manually set up c1→s1 by adding CitationSource edge
        graph
            .add_edge(Edge::CitationSource {
                source: c1.clone(),
                target: s1.clone(),
            })
            .unwrap();
        let c2 = citation_from_person(&mut graph, "c2", &p2);
        graph
            .add_edge(Edge::CitationSource {
                source: c2.clone(),
                target: s1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert!(!plan.to_delete.contains(&s1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&c2));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // B7: repository shared across two sources, one keeps it alive
    #[test]
    fn b7_repository_shared_source_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let r1 = "r1".to_string();
        graph
            .add_node(
                r1.clone(),
                Node::Repository(typed_graph::RepositoryData {
                    handle: r1.clone(),
                    ..typed_graph::RepositoryData::default()
                }),
            )
            .unwrap();
        // Build chain: P1→C1→S1→R1 and P2→C2→S2→R1
        let s1 = "s1".to_string();
        graph
            .add_node(
                s1.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: s1.clone(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::SourceRepoRef {
                source: s1.clone(),
                target: r1.clone(),
                metadata: Box::new(typed_graph::RepoRef {
                    ref_field: r1.clone(),
                    ..typed_graph::RepoRef::default()
                }),
            })
            .unwrap();

        let s2 = "s2".to_string();
        graph
            .add_node(
                s2.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: s2.clone(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::SourceRepoRef {
                source: s2.clone(),
                target: r1.clone(),
                metadata: Box::new(typed_graph::RepoRef {
                    ref_field: r1.clone(),
                    ..typed_graph::RepoRef::default()
                }),
            })
            .unwrap();

        let c1 = citation_from_person(&mut graph, "c1", &p1);
        graph
            .add_edge(Edge::CitationSource {
                source: c1.clone(),
                target: s1.clone(),
            })
            .unwrap();
        let c2 = citation_from_person(&mut graph, "c2", &p2);
        graph
            .add_edge(Edge::CitationSource {
                source: c2.clone(),
                target: s2.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&c1));
        assert!(plan.to_delete.contains(&s1));
        assert!(!plan.to_delete.contains(&r1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&c2));
        assert!(!plan.to_delete.contains(&s2));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // B8: media shared across two people, one remains
    #[test]
    fn b8_media_shared_person_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let m1 = media_from_person(&mut graph, "m1", &p1);
        // Connect p2 to same media
        graph
            .add_edge(Edge::PersonMediaRef {
                source: p2.clone(),
                target: m1.clone(),
                metadata: Box::new(typed_graph::MediaRef {
                    ref_field: m1.clone(),
                    ..typed_graph::MediaRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&m1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B9: note shared across two people, one remains
    #[test]
    fn b9_note_shared_person_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let n1 = note_from_person(&mut graph, "n1", &p1);
        // Connect p2 to same note
        graph
            .add_edge(Edge::PersonNote {
                source: p2.clone(),
                target: n1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&n1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B10: tag shared across two people, one remains
    #[test]
    fn b10_tag_shared_person_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let t1 = tag_from_person(&mut graph, "t1", &p1);
        // Connect p2 to same tag
        graph
            .add_edge(Edge::PersonTag {
                source: p2.clone(),
                target: t1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&t1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // B11: place shared across two events, one remains
    #[test]
    fn b11_place_shared_event_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let e1 = make_event(&mut graph, "e1", &p1);
        let e2 = make_event(&mut graph, "e2", &p2);
        let pl1 = "pl1".to_string();
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
            .add_edge(Edge::EventPlace {
                source: e1.clone(),
                target: pl1.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::EventPlace {
                source: e2.clone(),
                target: pl1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&pl1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&e2));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // B12: place kept alive by PlacePlaceRef target
    #[test]
    fn b12_place_place_ref_target_keeps_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let e1 = make_event(&mut graph, "e1", &p1);
        let e2 = make_event(&mut graph, "e2", &p2);
        let pl1 = "pl1".to_string();
        let pl2 = "pl2".to_string();
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
            .add_node(
                pl2.clone(),
                Node::Place(typed_graph::PlaceData {
                    handle: pl2.clone(),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();
        // P1→E1→Pl1
        graph
            .add_edge(Edge::EventPlace {
                source: e1.clone(),
                target: pl1.clone(),
            })
            .unwrap();
        // Pl2→Pl1 (Pl2 is a child of Pl1 via PlacePlaceRef)
        make_place_with_place_ref(&mut graph, &pl2, &pl1);
        // P2→E2→Pl2
        graph
            .add_edge(Edge::EventPlace {
                source: e2.clone(),
                target: pl2.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&pl1));
        assert!(!plan.to_delete.contains(&pl2));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&e2));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Category C: Indirectly associated, isolated → DELETED (transitive cascade)
    // -----------------------------------------------------------------------

    // C1: citation cascaded from event
    #[test]
    fn c1_citation_event_cascade() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let c1 = citation_from_event(&mut graph, "c1", &e1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&c1));
        assert_eq!(plan.to_delete.len(), 3);
    }

    // C2: citation cascaded from place via PlaceCitation
    #[test]
    fn c2_citation_place_cascade() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let pl1 = make_place(&mut graph, "pl1", &e1);
        let c1 = citation_from_place(&mut graph, "c1", &pl1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&pl1));
        assert!(plan.to_delete.contains(&c1));
        assert_eq!(plan.to_delete.len(), 4);
    }

    // -----------------------------------------------------------------------
    // Category D: Indirectly associated, NOT isolated → KEPT
    // -----------------------------------------------------------------------

    // D1: citation shared by two events, one remains
    #[test]
    fn d1_event_shared_indirect_keeps_citation_alive() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let e1 = make_event(&mut graph, "e1", &p1);
        let e2 = make_event(&mut graph, "e2", &p2);
        let c1 = citation_from_event(&mut graph, "c1", &e1);
        // Connect e2 to same citation
        graph
            .add_edge(Edge::EventCitation {
                source: e2.clone(),
                target: c1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&c1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&e2));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Category E: Unrelated — no connection to seeds → NOT deleted
    // -----------------------------------------------------------------------

    // E1: unrelated citation/source not touched
    #[test]
    fn e1_unrelated_citation_not_deleted() {
        let mut graph = Graph::new();
        // Seed subgraph
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let _pl1 = make_place(&mut graph, "pl1", &e1);
        // Unrelated: citation → source
        let cx = "cx".to_string();
        let sx = "sx".to_string();
        graph
            .add_node(
                cx.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: cx.clone(),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                sx.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: sx.clone(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::CitationSource {
                source: cx.clone(),
                target: sx.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&cx));
        assert!(!plan.to_delete.contains(&sx));
    }

    // E2: unrelated source/repository/media chain not touched
    #[test]
    fn e2_unrelated_source_not_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let _e1 = make_event(&mut graph, "e1", &p1);
        // Unrelated chain: Sx→Rx→Mx
        let sx = "sx".to_string();
        let rx = "rx".to_string();
        let mx = "mx".to_string();
        graph
            .add_node(
                sx.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: sx.clone(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                rx.clone(),
                Node::Repository(typed_graph::RepositoryData {
                    handle: rx.clone(),
                    ..typed_graph::RepositoryData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                mx.clone(),
                Node::Media(typed_graph::MediaData {
                    handle: mx.clone(),
                    ..typed_graph::MediaData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::SourceRepoRef {
                source: sx.clone(),
                target: rx.clone(),
                metadata: Box::new(typed_graph::RepoRef {
                    ref_field: rx.clone(),
                    ..typed_graph::RepoRef::default()
                }),
            })
            .unwrap();
        graph
            .add_edge(Edge::SourceMediaRef {
                source: sx.clone(),
                target: mx.clone(),
                metadata: Box::new(typed_graph::MediaRef {
                    ref_field: mx.clone(),
                    ..typed_graph::MediaRef::default()
                }),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&sx));
        assert!(!plan.to_delete.contains(&rx));
        assert!(!plan.to_delete.contains(&mx));
    }

    // E3: unrelated media connected to non-seed person not touched
    #[test]
    fn e3_unrelated_media_not_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let mx = media_from_person(&mut graph, "mx", &p2);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&mx));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // E4: unrelated note not touched
    #[test]
    fn e4_unrelated_note_not_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let nx = note_from_person(&mut graph, "nx", &p2);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&nx));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // E5: unrelated tag not touched
    #[test]
    fn e5_unrelated_tag_not_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let tx = tag_from_person(&mut graph, "tx", &p2);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&tx));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // E6: unrelated full chain not touched
    #[test]
    fn e6_unrelated_full_chain_not_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let _pl1 = make_place(&mut graph, "pl1", &e1);
        // Unrelated: Cx→Sx→Rx→Mx→Nx→Tx
        let cx = "cx".to_string();
        let sx = "sx".to_string();
        let rx = "rx".to_string();
        let mx = "mx".to_string();
        let nx = "nx".to_string();
        let tx = "tx".to_string();
        graph
            .add_node(
                cx.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: cx.clone(),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                sx.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: sx.clone(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                rx.clone(),
                Node::Repository(typed_graph::RepositoryData {
                    handle: rx.clone(),
                    ..typed_graph::RepositoryData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                mx.clone(),
                Node::Media(typed_graph::MediaData {
                    handle: mx.clone(),
                    ..typed_graph::MediaData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                nx.clone(),
                Node::Note(typed_graph::NoteData {
                    handle: nx.clone(),
                    ..typed_graph::NoteData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                tx.clone(),
                Node::Tag(typed_graph::TagData {
                    handle: tx.clone(),
                    ..typed_graph::TagData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::CitationSource {
                source: cx.clone(),
                target: sx.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::SourceRepoRef {
                source: sx.clone(),
                target: rx.clone(),
                metadata: Box::new(typed_graph::RepoRef {
                    ref_field: rx.clone(),
                    ..typed_graph::RepoRef::default()
                }),
            })
            .unwrap();
        graph
            .add_edge(Edge::SourceMediaRef {
                source: sx.clone(),
                target: mx.clone(),
                metadata: Box::new(typed_graph::MediaRef {
                    ref_field: mx.clone(),
                    ..typed_graph::MediaRef::default()
                }),
            })
            .unwrap();
        graph
            .add_edge(Edge::MediaNote {
                source: mx.clone(),
                target: nx.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::MediaTag {
                source: mx.clone(),
                target: tx.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&cx));
        assert!(!plan.to_delete.contains(&sx));
        assert!(!plan.to_delete.contains(&rx));
        assert!(!plan.to_delete.contains(&mx));
        assert!(!plan.to_delete.contains(&nx));
        assert!(!plan.to_delete.contains(&tx));
    }

    // -----------------------------------------------------------------------
    // Category F: Distant-relative sharing (regression for user's scenario)
    // -----------------------------------------------------------------------

    // F1: distant relative shares citation with deleted path -> citation kept
    #[test]
    fn f1_distant_relative_shared_citation_kept() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let e1 = make_event(&mut graph, "e1", &p1);
        let c1 = citation_from_event(&mut graph, "c1", &e1);
        // P2 also references the same citation
        graph
            .add_edge(Edge::PersonCitation {
                source: p2.clone(),
                target: c1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(!plan.to_delete.contains(&c1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 2);
    }

    // F2: distant relative shares source via citation -> source kept
    #[test]
    fn f2_distant_relative_shared_source_kept() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        let s1 = source_from_citation(&mut graph, "s1", &c1);
        // P2 also references C1, which references S1
        graph
            .add_edge(Edge::PersonCitation {
                source: p2.clone(),
                target: c1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // P1 deleted, C1 kept (P2 references it), S1 kept (C1 kept)
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&c1));
        assert!(!plan.to_delete.contains(&s1));
        assert!(!plan.to_delete.contains(&p2));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // F3: family chain — P1 in F1←P2; P2 in F2←P3; P3→C1. No shared items with P1.
    #[test]
    fn f3_distant_relative_via_family_chain() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let p3 = make_person(&mut graph, "p3");
        // P1 in F1, P2 is child in F1
        let _f1 = make_family_with_parents_and_child(&mut graph, "f1", &p1, &p2, &p2);
        // P2 in F2, P3 is child in F2
        let f2 = make_family_with_parents_and_child(&mut graph, "f2", &p2, &p3, &p3);
        // P3→C1
        let c1 = citation_from_person(&mut graph, "c1", &p3);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // Only p1 should be deleted (non-seed people never deleted)
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&p2));
        assert!(!plan.to_delete.contains(&p3));
        assert!(!plan.to_delete.contains(&f2));
        assert!(!plan.to_delete.contains(&c1));
        assert_eq!(plan.to_delete.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Category G: evaluated-set regression tests (false negatives)
    // -----------------------------------------------------------------------

    // G1: citation referenced by both person (direct) and event (indirect)
    // Both referents are deleted, so citation should be deleted too.
    #[test]
    fn g1_re_evaluate_when_referent_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        // C1 referenced by both person and event
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        graph
            .add_edge(Edge::EventCitation {
                source: e1.clone(),
                target: c1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(
            plan.to_delete.contains(&c1),
            "C1 should be deleted: both P1 and E1 are in to_delete"
        );
    }

    // G2: citation shared by two people, both deleted, both events also reference it
    #[test]
    fn g2_re_evaluate_shared_citation_all_deleted() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let e1 = make_event(&mut graph, "e1", &p1);
        let e2 = make_event(&mut graph, "e2", &p2);
        // C1 referenced by both people and both events
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        graph
            .add_edge(Edge::PersonCitation {
                source: p2.clone(),
                target: c1.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::EventCitation {
                source: e1.clone(),
                target: c1.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::EventCitation {
                source: e2.clone(),
                target: c1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&p2));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&e2));
        assert!(
            plan.to_delete.contains(&c1),
            "C1 should be deleted: all referents are in to_delete"
        );
    }

    // G3: multi-hop: P1→E1→C1→S1, also P1→C2→S1 (two citation paths to same source)
    #[test]
    fn g3_re_evaluate_multi_hop() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let c1 = citation_from_event(&mut graph, "c1", &e1);
        let s1 = source_from_citation(&mut graph, "s1", &c1);
        // Second path: P1→C2→S1
        let c2 = citation_from_person(&mut graph, "c2", &p1);
        graph
            .add_edge(Edge::CitationSource {
                source: c2.clone(),
                target: s1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        assert!(plan.to_delete.contains(&p1));
        assert!(plan.to_delete.contains(&e1));
        assert!(plan.to_delete.contains(&c1));
        assert!(plan.to_delete.contains(&c2));
        assert!(
            plan.to_delete.contains(&s1),
            "S1 should be deleted: both C1 and C2 are in to_delete"
        );
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

    // -----------------------------------------------------------------------
    // Category H: Property invariants (H6-H9)
    // -----------------------------------------------------------------------

    // H6: non-seed people are never deleted
    #[test]
    fn h6_non_seed_people_never_deleted() {
        // Build a graph with a seed person, a non-seed person connected via
        // family, and a completely unrelated person.
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let p3 = make_person(&mut graph, "p3");
        // P1 and P2 are in a family together
        make_family_with_parents(&mut graph, "f1", &p1, &p2);
        // P3 is completely unrelated

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // P1 is seed → deleted. P2 is non-seed → never deleted.
        // P3 is non-seed → never deleted.
        assert!(plan.to_delete.contains(&p1));
        assert!(
            !plan.to_delete.contains(&p2),
            "Non-seed person P2 should never be deleted"
        );
        assert!(
            !plan.to_delete.contains(&p3),
            "Non-seed person P3 should never be deleted"
        );
        assert_eq!(plan.to_delete.len(), 1);
    }

    // H7: nodes with no path to seeds are never deleted
    #[test]
    fn h7_unrelated_subgraph_untouched() {
        let mut graph = Graph::new();
        // Seed subgraph
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let _pl1 = make_place(&mut graph, "pl1", &e1);
        // Unrelated subgraph: Px→Cx→Sx→Rx
        let px = make_person(&mut graph, "px");
        let cx = citation_from_person(&mut graph, "cx", &px);
        let sx = source_from_citation(&mut graph, "sx", &cx);
        let _rx = repository_from_source(&mut graph, "rx", &sx);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);
        // Only seed subgraph nodes should be in to_delete
        assert!(plan.to_delete.contains(&p1));
        assert!(!plan.to_delete.contains(&px));
        assert!(!plan.to_delete.contains(&cx));
        assert!(!plan.to_delete.contains(&sx));
    }

    // H8: same graph + same seeds = same to_delete set (deterministic)
    #[test]
    fn h8_deterministic_output() {
        // Build a complex graph with multiple paths
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let _f1 = make_family_with_parents(&mut graph, "f1", &p1, &p2);
        let e1 = make_event(&mut graph, "e1", &p1);
        let e2 = make_event(&mut graph, "e2", &p2);
        let pl1 = "pl1".to_string();
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
            .add_edge(Edge::EventPlace {
                source: e1.clone(),
                target: pl1.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::EventPlace {
                source: e2.clone(),
                target: pl1.clone(),
            })
            .unwrap();
        let c1 = citation_from_person(&mut graph, "c1", &p1);
        let _s1 = source_from_citation(&mut graph, "s1", &c1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());

        // Run cascade 10 times and verify identical results
        let result = cascade(&graph, &seeds);
        for _ in 0..10 {
            let plan = cascade(&graph, &seeds);
            assert_eq!(
                plan.to_delete, result.to_delete,
                "Deterministic output: cascade must produce the same to_delete set every time"
            );
        }
    }

    // H9: to_delete only grows monotonically through the cascade
    #[test]
    fn h9_monotonic_growth() {
        let mut graph = Graph::new();
        let p1 = make_person(&mut graph, "p1");
        let p2 = make_person(&mut graph, "p2");
        let _f1 = make_family_with_parents(&mut graph, "f1", &p1, &p2);
        let e1 = make_event(&mut graph, "e1", &p1);
        let c1 = citation_from_event(&mut graph, "c1", &e1);
        let _s1 = source_from_citation(&mut graph, "s1", &c1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        seeds.insert(p2.clone());

        let plan = cascade(&graph, &seeds);
        // Verify that seed set is a subset of to_delete
        for seed in &seeds {
            assert!(plan.to_delete.contains(seed), "Seed must be in to_delete");
        }
    }

    // -----------------------------------------------------------------------
    // Category I: Place hardening — pre_place_in_use + already-orphaned skip
    // -----------------------------------------------------------------------

    // I1: pre_place_in_use_count counts only incoming keep-alive edges
    #[test]
    fn pre_place_in_use_counts_incoming_only() {
        let mut graph = Graph::new();

        // Create a place
        let pl1 = "pl1".to_string();
        graph
            .add_node(
                pl1.clone(),
                Node::Place(typed_graph::PlaceData {
                    handle: pl1.clone(),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();

        // Attach outgoing edges: PlaceNote, PlaceCitation, PlaceMediaRef, PlaceTag
        // These should NOT count as incoming keep-alive edges.
        let n1 = "n1".to_string();
        graph
            .add_node(
                n1.clone(),
                Node::Note(typed_graph::NoteData {
                    handle: n1.clone(),
                    ..typed_graph::NoteData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PlaceNote {
                source: pl1.clone(),
                target: n1.clone(),
            })
            .unwrap();

        let c1 = "c1".to_string();
        graph
            .add_node(
                c1.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: c1.clone(),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PlaceCitation {
                source: pl1.clone(),
                target: c1.clone(),
            })
            .unwrap();

        // pre_place_in_use_count should be 0 — only outgoing edges exist
        assert_eq!(pre_place_in_use_count(&graph, &pl1), 0);

        // Now add an incoming EventPlace edge — should count as 1
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
            .add_edge(Edge::EventPlace {
                source: e1.clone(),
                target: pl1.clone(),
            })
            .unwrap();

        assert_eq!(pre_place_in_use_count(&graph, &pl1), 1);

        // Add an incoming PlacePlaceRef — should count as 2
        let pl2 = "pl2".to_string();
        graph
            .add_node(
                pl2.clone(),
                Node::Place(typed_graph::PlaceData {
                    handle: pl2.clone(),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PlacePlaceRef {
                source: pl2.clone(),
                target: pl1.clone(),
                metadata: Box::new(typed_graph::PlaceRef {
                    ref_field: pl1.clone(),
                    ..typed_graph::PlaceRef::default()
                }),
            })
            .unwrap();

        assert_eq!(pre_place_in_use_count(&graph, &pl1), 2);
    }

    // I2: already-orphaned place with only outgoing refs is NOT deleted
    #[test]
    fn already_orphaned_place_with_outgoing_refs_not_deleted() {
        let mut graph = Graph::new();

        // Person 1 -> Event 1 -> Place 1 (newly orphaned, should be deleted)
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let pl1 = make_place(&mut graph, "pl1", &e1);

        // Place 2 has only outgoing PlaceNote (no incoming keep-alive edges)
        // — already orphaned, should NOT be deleted
        let pl2 = "pl2".to_string();
        graph
            .add_node(
                pl2.clone(),
                Node::Place(typed_graph::PlaceData {
                    handle: pl2.clone(),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();
        let n1 = "n1".to_string();
        graph
            .add_node(
                n1.clone(),
                Node::Note(typed_graph::NoteData {
                    handle: n1.clone(),
                    ..typed_graph::NoteData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::PlaceNote {
                source: pl2.clone(),
                target: n1.clone(),
            })
            .unwrap();

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);

        // pl1 is newly orphaned — should be deleted
        assert!(plan.to_delete.contains(&pl1), "Newly-orphaned place pl1 should be deleted");
        // pl2 is already orphaned — should NOT be deleted
        assert!(
            !plan.to_delete.contains(&pl2),
            "Already-orphaned place pl2 should NOT be deleted"
        );
        // The note referenced by pl2 should also not be deleted (since pl2 is not deleted)
        assert!(!plan.to_delete.contains(&n1), "Note n1 should not be deleted");
    }

    // I3: newly-orphaned place is still deleted (regression)
    #[test]
    fn newly_orphaned_place_still_deleted() {
        let mut graph = Graph::new();

        // Person 1 -> Event 1 -> Place 1
        let p1 = make_person(&mut graph, "p1");
        let e1 = make_event(&mut graph, "e1", &p1);
        let pl1 = make_place(&mut graph, "pl1", &e1);

        let mut seeds = HashSet::new();
        seeds.insert(p1.clone());
        let plan = cascade(&graph, &seeds);

        // The place should still be deleted when the only referencing event is deleted
        assert!(plan.to_delete.contains(&pl1), "Place pl1 should be transitively cascaded");
        assert!(plan.to_delete.contains(&e1), "Event e1 should be cascaded");
        assert!(plan.to_delete.contains(&p1), "Person p1 should be deleted");
        assert_eq!(plan.to_delete.len(), 3);
    }
}
