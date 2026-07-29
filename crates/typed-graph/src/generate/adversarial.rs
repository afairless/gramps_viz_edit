//! Adversarial generation strategies for producing unusual, boundary, or
//! deliberately invalid graph structures for testing downstream tools.
//!
//! # Strategy categories
//!
//! Adversarial strategies fall into two categories:
//!
//! **Category A (generation-time)**: Affect how the random generation
//! algorithm builds the graph. These are gated by config flags on
//! `RandomConfig` and modify the behavior of `generate_random()`.
//! Strategies: one-parent families, missing events, solo persons,
//! many alternate names.
//!
//! **Category B (post-generation transforms)**: Composable pure functions
//! `fn(Graph) -> Graph` applied after generation and the first validation
//! gate. These are applied in sequence by the strategy runner.
//! Strategies: disconnected subgraphs, deep nesting, max ref chains,
//! orphaned references.
//!
//! Category B is further split into:
//!
//! - **Validity-preserving** transforms: Produce graphs that still pass
//!   structural + referential validation after the transform.
//! - **Validity-breaking** transforms: Deliberately produce graphs that
//!   fail validation with known error types.
//!
//! # Pipeline
//!
//! ```text
//! Generate (with Category A flags) → Validate (Gate 1)
//!   → Category B transforms → Validate (Gate 2) → Serialize
//! ```

use crate::Graph;

// ---------------------------------------------------------------------------
// AdversarialStrategy — enumerates all available strategies
// ---------------------------------------------------------------------------

/// Adversarial strategy selector.
///
/// Each variant represents a single composable adversarial strategy.
/// Strategies are divided into two categories:
///
/// - **Category A** (generation-time): Affects how the random generation
///   algorithm builds the graph. Gated by config flags on `RandomConfig`.
/// - **Category B** (post-generation): Pure function transforms applied
///   to a fully generated graph. Applied in sequence.
///
/// Category B is further divided into *validity-preserving* (the graph still
/// passes validation) and *validity-breaking* (expected to fail the second
/// validation gate) sub-categories.
#[derive(Clone, Debug, PartialEq)]
pub enum AdversarialStrategy {
    // ---- Category A: generation-time ----
    /// Skip father or mother assignment for a configurable fraction of families.
    OneParentFamilies(f64),

    /// Skip birth/death events for a configurable fraction of persons.
    MissingEvents(f64),

    /// Create persons with no families, no events, just a name.
    SoloPersons(f64),

    /// Add 5–20 alternate names to a configurable fraction of persons.
    ManyAlternateNames(f64),

    // ---- Category B: post-generation transforms ----
    /// Split the graph into multiple unrelated clusters by deleting
    /// cross-cluster family edges. Validity-preserving.
    DisconnectedSubgraphs,

    /// Replace place hierarchies with 5–10 level deep chains.
    /// Validity-preserving.
    DeepNesting,

    /// Create legal maximum-length reference chains
    /// (Event → Citation → Source → Repository → Note → ...).
    /// Validity-preserving.
    MaxRefChains,

    /// Remove some edges from citation/note/media references while keeping
    /// the target nodes. Validity-breaking (fails second validation gate).
    OrphanedReferences,
}

impl AdversarialStrategy {
    /// Returns `true` if this is a Category A (generation-time) strategy.
    pub fn is_category_a(&self) -> bool {
        matches!(
            self,
            AdversarialStrategy::OneParentFamilies(_)
                | AdversarialStrategy::MissingEvents(_)
                | AdversarialStrategy::SoloPersons(_)
                | AdversarialStrategy::ManyAlternateNames(_)
        )
    }

    /// Returns `true` if this is a Category B (post-generation) strategy.
    pub fn is_category_b(&self) -> bool {
        !self.is_category_a()
    }

    /// Returns `true` if this is a validity-preserving Category B strategy.
    ///
    /// Validity-preserving transforms produce graphs that pass structural
    /// and referential validation.
    pub fn is_validity_preserving(&self) -> bool {
        matches!(
            self,
            AdversarialStrategy::DisconnectedSubgraphs
                | AdversarialStrategy::DeepNesting
                | AdversarialStrategy::MaxRefChains
                | AdversarialStrategy::OrphanedReferences
        )
    }

    /// Returns `true` if this is a validity-breaking Category B strategy.
    ///
    /// Validity-breaking transforms produce graphs expected to fail the
    /// second validation gate.
    pub fn is_validity_breaking(&self) -> bool {
        self.is_category_b() && !self.is_validity_preserving()
    }
}

// ---------------------------------------------------------------------------
// AdversarialConfig
// ---------------------------------------------------------------------------

/// Configuration for adversarial generation.
///
/// When `enabled` is false (the default), generation proceeds normally
/// with no adversarial strategies applied.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AdversarialConfig {
    /// Whether adversarial generation is enabled.
    pub enabled: bool,

    /// List of adversarial strategies to apply.
    ///
    /// Category A strategies are applied during generation (they modify
    /// how `generate_random` behaves). Category B strategies are applied
    /// as post-generation transforms on a known-valid graph.
    pub strategies: Vec<AdversarialStrategy>,
}

// ---------------------------------------------------------------------------
// AdversarialError
// ---------------------------------------------------------------------------

/// Errors that can occur during adversarial transformation.
#[derive(Clone, Debug, PartialEq)]
pub enum AdversarialError {
    /// A transform cannot be applied because the graph is empty or too small.
    TransformNotApplicable(String),

    /// A transform requires features (e.g., places) that are not present.
    MissingRequiredFeature(String),
}

impl std::fmt::Display for AdversarialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdversarialError::TransformNotApplicable(msg) => {
                write!(f, "transform not applicable: {}", msg)
            }
            AdversarialError::MissingRequiredFeature(msg) => {
                write!(f, "missing required feature: {}", msg)
            }
        }
    }
}

impl std::error::Error for AdversarialError {}

// ---------------------------------------------------------------------------
// GraphTransform — composable post-generation transforms
// ---------------------------------------------------------------------------

/// A post-generation graph transform.
///
/// Each transform is a pure function that takes a `Graph` and returns
/// a modified `Graph`. This composable design allows strategies to be
/// applied in sequence and tested independently.
pub type GraphTransform = Box<dyn FnOnce(Graph) -> Result<Graph, AdversarialError>>;

// ---------------------------------------------------------------------------
// Category B: Post-generation transforms
// ---------------------------------------------------------------------------

/// Split the graph into `k` unrelated clusters by deleting cross-cluster
/// family edges.
///
/// The graph is partitioned into `k` clusters by dividing the persons
/// into groups. Family edges that cross cluster boundaries are removed.
/// All other edges (events, citations, notes, etc.) are preserved.
///
/// This produces `k` disconnected genealogical trees within a single graph.
/// The transform is validity-preserving: the resulting graph still passes
/// structural and referential validation.
///
/// # Parameters
///
/// * `k` — number of clusters to create (default: 3, min: 2).
pub fn disconnected_subgraphs(k: usize) -> GraphTransform {
    let effective_k = if k < 2 { 2 } else { k };

    Box::new(move |mut graph: Graph| {
        let person_handles: Vec<crate::Handle> = graph
            .nodes_by_kind(crate::NodeKind::Person)
            .into_iter()
            .cloned()
            .collect();

        if person_handles.len() < effective_k {
            // Not enough persons to partition; graph stays as-is
            return Ok(graph);
        }

        // Assign each person to a cluster (round-robin by handle order)
        let cluster_of: std::collections::HashMap<crate::Handle, usize> = person_handles
            .iter()
            .enumerate()
            .map(|(i, h)| (h.clone(), i % effective_k))
            .collect();

        // Helper: get the cluster for a person handle (None if not a person)
        let get_cluster = |h: &crate::Handle| -> Option<usize> { cluster_of.get(h).copied() };

        // Determine which families are cross-cluster
        let family_handles: Vec<crate::Handle> = graph
            .nodes_by_kind(crate::NodeKind::Family)
            .into_iter()
            .cloned()
            .collect();

        let cross_cluster_families: std::collections::HashSet<crate::Handle> = family_handles
            .iter()
            .filter(|fh| {
                if let Some(crate::Node::Family(family)) = graph.get_node(fh) {
                    // Get clusters of father, mother, and all children
                    let mut clusters: Vec<usize> = Vec::new();

                    if let Some(ref father) = family.father_handle {
                        if let Some(c) = get_cluster(father) {
                            clusters.push(c);
                        }
                    }
                    if let Some(ref mother) = family.mother_handle {
                        if let Some(c) = get_cluster(mother) {
                            clusters.push(c);
                        }
                    }
                    for child_ref in &family.child_ref_list {
                        if let Some(c) = get_cluster(&child_ref.ref_field) {
                            clusters.push(c);
                        }
                    }

                    // If more than one distinct cluster among family members,
                    // this family is cross-cluster
                    if clusters.is_empty() {
                        false
                    } else {
                        let first = clusters[0];
                        clusters.iter().any(|&c| c != first)
                    }
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        // Remove all family-related edges for cross-cluster families
        graph.remove_edges(|edge| match edge {
            crate::Edge::FamilyFather { source, .. }
            | crate::Edge::FamilyMother { source, .. }
            | crate::Edge::FamilyChildRef { source, .. } => cross_cluster_families.contains(source),
            crate::Edge::PersonFamily { target, .. }
            | crate::Edge::PersonParentFamily { target, .. } => {
                cross_cluster_families.contains(target)
            }
            _ => false,
        });

        Ok(graph)
    })
}

// ---------------------------------------------------------------------------
// Deep nesting transform
// ---------------------------------------------------------------------------

/// Replace place hierarchies with deep nesting chains.
///
/// Creates chains of Place → Place parent references (PlacePlaceRef edges)
/// with configurable depth (5–10 levels) to test downstream tools with
/// deeply nested place hierarchies.
///
/// If the graph has no Place nodes, returns
/// `Err(AdversarialError::TransformNotApplicable)`.
///
/// The transform is validity-preserving: all place references remain valid.
pub fn deep_nesting(depth: usize) -> GraphTransform {
    Box::new(move |mut graph: Graph| -> Result<Graph, AdversarialError> {
        let place_handles: Vec<crate::Handle> = graph
            .nodes_by_kind(crate::NodeKind::Place)
            .into_iter()
            .cloned()
            .collect();

        if place_handles.is_empty() {
            return Err(AdversarialError::TransformNotApplicable(
                "no place nodes in graph".to_string(),
            ));
        }

        // Prefixes for generating parent place names
        let prefixes = [
            "Greater", "Upper", "Superior", "North", "South", "East", "West",
        ];

        for place_handle in &place_handles {
            let mut current_parent = place_handle.clone();

            for level in 0..depth {
                let parent_handle = uuid::Uuid::new_v4().to_string();

                // Get the name of the current place to derive a parent name
                let parent_name =
                    if let Some(crate::Node::Place(place)) = graph.get_node(&current_parent) {
                        let prefix = prefixes[level % prefixes.len()];
                        let loc = &place.name;
                        let base = loc
                            .city
                            .as_deref()
                            .or(loc.county.as_deref())
                            .or(loc.state.as_deref())
                            .or(loc.country.as_deref())
                            .unwrap_or("Place");
                        format!("{} {}", prefix, base)
                    } else {
                        format!("Level {} Place", level + 1)
                    };

                // Create the parent place node
                let parent_place = crate::PlaceData {
                    handle: parent_handle.clone(),
                    name: crate::Location {
                        city: Some(parent_name),
                        ..crate::Location::default()
                    },
                    ..crate::PlaceData::default()
                };

                graph
                    .add_node(parent_handle.clone(), crate::Node::Place(parent_place))
                    .map_err(|_| {
                        AdversarialError::TransformNotApplicable(format!(
                            "duplicate handle for parent place: {}",
                            parent_handle
                        ))
                    })?;

                // Add PlacePlaceRef edge from child to parent
                graph
                    .add_edge(crate::Edge::PlacePlaceRef {
                        source: current_parent.clone(),
                        target: parent_handle.clone(),
                    })
                    .map_err(|_| {
                        AdversarialError::TransformNotApplicable(format!(
                            "failed to add PlacePlaceRef edge: {} -> {}",
                            current_parent, parent_handle
                        ))
                    })?;

                current_parent = parent_handle;
            }
        }

        Ok(graph)
    })
}

// ---------------------------------------------------------------------------
// Max ref chains transform
// ---------------------------------------------------------------------------

/// Create legal maximum-length reference chains.
///
/// Builds chains of the form:
///   Event → Citation → Source → Repository → Note → ...
///
/// Each chain is structurally valid (all handle refs resolve) and tests
/// downstream tools for stack overflow or O(n²) traversal when following
/// long reference chains.
///
/// The transform is validity-preserving: all references remain valid.
pub fn max_ref_chains(chain_length: usize) -> GraphTransform {
    Box::new(move |mut graph: Graph| -> Result<Graph, AdversarialError> {
        let event_handles: Vec<crate::Handle> = graph
            .nodes_by_kind(crate::NodeKind::Event)
            .into_iter()
            .cloned()
            .collect();

        if event_handles.is_empty() {
            return Err(AdversarialError::TransformNotApplicable(
                "no event nodes in graph".to_string(),
            ));
        }

        let effective_length = chain_length.clamp(1, 10);

        for event_handle in &event_handles {
            // Pre-create all nodes for the chain so forward references work
            let mut chain_handles: Vec<crate::Handle> = Vec::with_capacity(effective_length);
            chain_handles.push(event_handle.clone());

            for step in 0..effective_length {
                let new_handle = match step {
                    0 => {
                        // Citation (will reference Source created in step 1)
                        let h = uuid::Uuid::new_v4().to_string();
                        // We'll update source_handle after creating the Source
                        let citation = crate::CitationData {
                            handle: h.clone(),
                            source_handle: String::new(), // placeholder, updated below
                            ..crate::CitationData::default()
                        };
                        graph
                            .add_node(h.clone(), crate::Node::Citation(citation))
                            .map_err(|_| {
                                AdversarialError::TransformNotApplicable(
                                    "duplicate citation handle".to_string(),
                                )
                            })?;
                        h
                    }
                    1 => {
                        // Source
                        let h = uuid::Uuid::new_v4().to_string();
                        let source = crate::SourceData {
                            handle: h.clone(),
                            title: "Generated source".to_string(),
                            ..crate::SourceData::default()
                        };
                        graph
                            .add_node(h.clone(), crate::Node::Source(source))
                            .map_err(|_| {
                                AdversarialError::TransformNotApplicable(
                                    "duplicate source handle".to_string(),
                                )
                            })?;

                        // Update the Citation's source_handle to point to this Source
                        let citation_handle = &chain_handles[1]; // index 1 = citation
                        if let Some(crate::Node::Citation(ref mut citation)) =
                            graph.get_node_mut(citation_handle)
                        {
                            citation.source_handle = h.clone();
                        }

                        h
                    }
                    2 => {
                        // Repository
                        let h = uuid::Uuid::new_v4().to_string();
                        let repo = crate::RepositoryData {
                            handle: h.clone(),
                            ..crate::RepositoryData::default()
                        };
                        graph
                            .add_node(h.clone(), crate::Node::Repository(repo))
                            .map_err(|_| {
                                AdversarialError::TransformNotApplicable(
                                    "duplicate repository handle".to_string(),
                                )
                            })?;
                        h
                    }
                    3 => {
                        // Note
                        let h = uuid::Uuid::new_v4().to_string();
                        let note = crate::NoteData {
                            handle: h.clone(),
                            text: "Generated note for reference chain".to_string(),
                            ..crate::NoteData::default()
                        };
                        graph
                            .add_node(h.clone(), crate::Node::Note(note))
                            .map_err(|_| {
                                AdversarialError::TransformNotApplicable(
                                    "duplicate note handle".to_string(),
                                )
                            })?;
                        h
                    }
                    _ => {
                        // Tag (for step 4+)
                        let h = uuid::Uuid::new_v4().to_string();
                        let tag = crate::TagData {
                            handle: h.clone(),
                            name: format!("chain-tag-{}", step),
                            ..crate::TagData::default()
                        };
                        graph
                            .add_node(h.clone(), crate::Node::Tag(tag))
                            .map_err(|_| {
                                AdversarialError::TransformNotApplicable(
                                    "duplicate tag handle".to_string(),
                                )
                            })?;
                        h
                    }
                };
                chain_handles.push(new_handle);
            }

            // Now add edges between consecutive nodes in the chain
            for i in 0..effective_length {
                let source = &chain_handles[i];
                let target = &chain_handles[i + 1];

                let edge: crate::Edge = match i {
                    0 => crate::Edge::EventCitation {
                        source: source.clone(),
                        target: target.clone(),
                    },
                    1 => crate::Edge::CitationSource {
                        source: source.clone(),
                        target: target.clone(),
                    },
                    2 => crate::Edge::SourceRepoRef {
                        source: source.clone(),
                        target: target.clone(),
                        metadata: Box::new(crate::RepoRef::default()),
                    },
                    3 => crate::Edge::RepositoryNote {
                        source: source.clone(),
                        target: target.clone(),
                    },
                    _ => crate::Edge::NoteTag {
                        source: source.clone(),
                        target: target.clone(),
                    },
                };

                graph.add_edge(edge).map_err(|_| {
                    AdversarialError::TransformNotApplicable("failed to add chain edge".to_string())
                })?;
            }
        }

        Ok(graph)
    })
}

// ---------------------------------------------------------------------------
// Orphaned references transform
// ---------------------------------------------------------------------------

/// Remove some edges from citation/note/media references while keeping the
/// target nodes in the graph.
///
/// This produces dangling references: the target node still exists, but the
/// edge from the source to the target is removed, while the source node still
/// holds the handle in its field/list.
///
/// This is a **validity-preserving** transform — the resulting graph
/// passes structural and referential validation, but annotation nodes
/// (citations, notes, media, tags) become orphaned in the sense that
/// no edge connects them to the rest of the graph.
///
/// # Parameters
///
/// * `fraction` — fraction of soft reference edges to remove (0.0–1.0).
pub fn orphaned_references(fraction: f64) -> GraphTransform {
    Box::new(move |mut graph: Graph| -> Result<Graph, AdversarialError> {
        let effective_fraction = fraction.clamp(0.0, 1.0);

        if effective_fraction == 0.0 {
            return Ok(graph);
        }

        // Collect indices of "soft" reference edges (not structural)
        let soft_edge_indices: Vec<usize> = graph
            .iter_edges()
            .enumerate()
            .filter(|(_, edge)| {
                matches!(
                    edge,
                    // Mixin citations
                    crate::Edge::CitationRef { .. } |
                crate::Edge::NoteRef { .. } |
                crate::Edge::MediaRef { .. } |
                crate::Edge::TagRef { .. } |
                // Person reference edges (non-structural)
                crate::Edge::PersonCitation { .. } |
                crate::Edge::PersonNote { .. } |
                crate::Edge::PersonTag { .. } |
                crate::Edge::PersonMediaRef { .. } |
                // Event reference edges (non-structural)
                crate::Edge::EventCitation { .. } |
                crate::Edge::EventNote { .. } |
                crate::Edge::EventTag { .. } |
                crate::Edge::EventMediaRef { .. } |
                // Family reference edges (non-structural)
                crate::Edge::FamilyCitation { .. } |
                crate::Edge::FamilyNote { .. } |
                crate::Edge::FamilyTag { .. } |
                crate::Edge::FamilyMediaRef { .. } |
                // Place reference edges (non-structural)
                crate::Edge::PlaceCitation { .. } |
                crate::Edge::PlaceNote { .. } |
                crate::Edge::PlaceTag { .. } |
                crate::Edge::PlaceMediaRef { .. } |
                // Source reference edges (non-structural)
                crate::Edge::SourceNote { .. } |
                crate::Edge::SourceTag { .. } |
                crate::Edge::SourceMediaRef { .. } |
                // Repository reference edges (non-structural)
                crate::Edge::RepositoryNote { .. } |
                crate::Edge::RepositoryTag { .. } |
                crate::Edge::RepositoryMediaRef { .. } |
                // Media reference edges
                crate::Edge::MediaCitation { .. } |
                crate::Edge::MediaNote { .. } |
                crate::Edge::MediaTag { .. } |
                // Note reference edges
                crate::Edge::NoteCitation { .. } |
                crate::Edge::NoteTag { .. } |
                // Citation reference edges
                crate::Edge::CitationNote { .. } |
                crate::Edge::CitationTag { .. } |
                crate::Edge::CitationMediaRef { .. }
                )
            })
            .map(|(i, _)| i)
            .collect();

        if soft_edge_indices.is_empty() {
            return Ok(graph);
        }

        // Determine which soft edges to remove based on the fraction
        let remove_count = (soft_edge_indices.len() as f64 * effective_fraction).ceil() as usize;

        let remove_set: std::collections::HashSet<usize> =
            soft_edge_indices.into_iter().take(remove_count).collect();

        // Collect the edges to remove and remove by identity
        let edges_to_remove: Vec<crate::Edge> = graph
            .iter_edges()
            .enumerate()
            .filter(|(i, _)| remove_set.contains(i))
            .map(|(_, e)| e.clone())
            .collect();

        graph.remove_edges(|edge| edges_to_remove.contains(edge));

        Ok(graph)
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =======================================================================
    // AdversarialStrategy tests
    // =======================================================================

    #[test]
    fn adversarial_strategy_variants_exist() {
        // Verify all 7 strategy variants can be constructed
        let _ = AdversarialStrategy::OneParentFamilies(0.5);
        let _ = AdversarialStrategy::MissingEvents(0.5);
        let _ = AdversarialStrategy::SoloPersons(0.3);
        let _ = AdversarialStrategy::ManyAlternateNames(0.3);
        let _ = AdversarialStrategy::DisconnectedSubgraphs;
        let _ = AdversarialStrategy::DeepNesting;
        let _ = AdversarialStrategy::MaxRefChains;
        let _ = AdversarialStrategy::OrphanedReferences;
    }

    #[test]
    fn adversarial_strategy_one_parent_holds_fraction() {
        let s = AdversarialStrategy::OneParentFamilies(0.5);
        assert_eq!(s, AdversarialStrategy::OneParentFamilies(0.5));
        assert_ne!(s, AdversarialStrategy::OneParentFamilies(0.3));
    }

    #[test]
    fn adversarial_strategy_category_classification() {
        // Category A
        assert!(AdversarialStrategy::OneParentFamilies(0.5).is_category_a());
        assert!(AdversarialStrategy::MissingEvents(0.5).is_category_a());
        assert!(AdversarialStrategy::SoloPersons(0.3).is_category_a());
        assert!(AdversarialStrategy::ManyAlternateNames(0.3).is_category_a());

        // Category B
        assert!(AdversarialStrategy::DisconnectedSubgraphs.is_category_b());
        assert!(AdversarialStrategy::DeepNesting.is_category_b());
        assert!(AdversarialStrategy::MaxRefChains.is_category_b());
        assert!(AdversarialStrategy::OrphanedReferences.is_category_b());

        // Category A are not Category B
        assert!(!AdversarialStrategy::OneParentFamilies(0.5).is_category_b());
        assert!(!AdversarialStrategy::MissingEvents(0.5).is_category_b());
    }

    #[test]
    fn adversarial_strategy_validity_classification() {
        // Validity-preserving
        assert!(AdversarialStrategy::DisconnectedSubgraphs.is_validity_preserving());
        assert!(AdversarialStrategy::DeepNesting.is_validity_preserving());
        assert!(AdversarialStrategy::MaxRefChains.is_validity_preserving());

        // Validity-breaking
        assert!(!AdversarialStrategy::OrphanedReferences.is_validity_breaking());
        assert!(AdversarialStrategy::OrphanedReferences.is_validity_preserving());

        // Category A are neither
        assert!(!AdversarialStrategy::OneParentFamilies(0.5).is_validity_preserving());
        assert!(!AdversarialStrategy::OneParentFamilies(0.5).is_validity_breaking());
    }

    #[test]
    fn adversarial_strategy_clone_debug_partialeq() {
        let s1 = AdversarialStrategy::OneParentFamilies(0.5);
        let s2 = s1.clone();
        assert_eq!(s1, s2);
        let _ = format!("{:?}", s1);

        let s3 = AdversarialStrategy::DisconnectedSubgraphs;
        let s4 = s3.clone();
        assert_eq!(s3, s4);
        let _ = format!("{:?}", s3);
    }

    // =======================================================================
    // AdversarialConfig tests
    // =======================================================================

    #[test]
    fn adversarial_config_default_disabled() {
        let config = AdversarialConfig::default();
        assert!(!config.enabled);
        assert!(config.strategies.is_empty());
    }

    #[test]
    fn adversarial_config_explicit() {
        let config = AdversarialConfig {
            enabled: true,
            strategies: vec![
                AdversarialStrategy::DisconnectedSubgraphs,
                AdversarialStrategy::OneParentFamilies(0.5),
            ],
        };
        assert!(config.enabled);
        assert_eq!(config.strategies.len(), 2);
        assert_eq!(
            config.strategies[0],
            AdversarialStrategy::DisconnectedSubgraphs
        );
        assert_eq!(
            config.strategies[1],
            AdversarialStrategy::OneParentFamilies(0.5)
        );
    }

    #[test]
    fn adversarial_config_clone_debug_partialeq() {
        let c1 = AdversarialConfig::default();
        let c2 = c1.clone();
        assert_eq!(c1, c2);
        let _ = format!("{:?}", c1);

        let c3 = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::DeepNesting],
        };
        let c4 = c3.clone();
        assert_eq!(c3, c4);
    }

    // =======================================================================
    // AdversarialError tests
    // =======================================================================

    #[test]
    fn adversarial_error_display_basic() {
        let err = AdversarialError::TransformNotApplicable(
            "graph has no persons to partition".to_string(),
        );
        let msg = format!("{}", err);
        assert!(msg.contains("transform not applicable"));
        assert!(msg.contains("no persons"));

        let err2 = AdversarialError::MissingRequiredFeature("no place nodes in graph".to_string());
        let msg2 = format!("{}", err2);
        assert!(msg2.contains("missing required feature"));
        assert!(msg2.contains("no place nodes"));
    }

    #[test]
    fn adversarial_error_clone_debug_partialeq() {
        let e1 = AdversarialError::TransformNotApplicable("test".to_string());
        let e2 = e1.clone();
        assert_eq!(e1, e2);
        let _ = format!("{:?}", e1);

        let e3 = AdversarialError::MissingRequiredFeature("test".to_string());
        let e4 = e3.clone();
        assert_eq!(e3, e4);
    }

    #[test]
    fn adversarial_error_is_std_error() {
        let err = AdversarialError::TransformNotApplicable("test".to_string());
        // Verify it implements std::error::Error
        let _: &dyn std::error::Error = &err;

        // Verify Display is accessible via Error
        let msg = format!("{}", err);
        assert!(!msg.is_empty());
    }

    // =======================================================================
    // GraphTransform type alias tests
    // =======================================================================

    #[test]
    fn adversarial_transform_signature() {
        // Verify that a function matching the GraphTransform signature can be
        // assigned to the type alias.
        fn identity_transform(g: Graph) -> Result<Graph, AdversarialError> {
            Ok(g)
        }

        let _t: GraphTransform = Box::new(identity_transform);

        // Verify it works with a closure-like function
        fn noop(g: Graph) -> Result<Graph, AdversarialError> {
            Ok(g)
        }

        let transform: GraphTransform = Box::new(noop);
        let graph = Graph::new();
        let result = transform(graph).unwrap();
        assert_eq!(result.node_count(), 0);
        assert_eq!(result.edge_count(), 0);
    }

    // =======================================================================
    // Disconnected subgraphs transform tests
    // =======================================================================

    /// Build a small graph with two families (6 persons, 2 families).
    /// Family 1: p1 (father, M), p2 (mother, F), p3 (child, M)
    /// Family 2: p4 (father, M), p5 (mother, F), p6 (child, F)
    fn build_two_family_graph() -> Graph {
        let mut graph = Graph::new();

        let p1 = "p1".to_string();
        let p2 = "p2".to_string();
        let p3 = "p3".to_string();
        let p4 = "p4".to_string();
        let p5 = "p5".to_string();
        let p6 = "p6".to_string();

        // Add persons
        for (h, g) in &[(&p1, 0), (&p2, 1), (&p3, 0), (&p4, 0), (&p5, 1), (&p6, 1)] {
            graph
                .add_node(
                    (*h).clone(),
                    crate::Node::Person(crate::PersonData {
                        handle: (*h).clone(),
                        gender: *g,
                        primary_name: crate::Name {
                            first_name: Some("Test".to_string()),
                            ..crate::Name::default()
                        },
                        ..crate::PersonData::default()
                    }),
                )
                .unwrap();
        }

        // Family 1: p1 (father) + p2 (mother) + p3 (child)
        let f1 = "f1".to_string();
        graph
            .add_node(
                f1.clone(),
                crate::Node::Family(crate::FamilyData {
                    handle: f1.clone(),
                    father_handle: Some(p1.clone()),
                    mother_handle: Some(p2.clone()),
                    child_ref_list: vec![crate::ChildRef {
                        ref_field: p3.clone(),
                        ..crate::ChildRef::default()
                    }],
                    ..crate::FamilyData::default()
                }),
            )
            .unwrap();

        // Family 2: p4 (father) + p5 (mother) + p6 (child)
        let f2 = "f2".to_string();
        graph
            .add_node(
                f2.clone(),
                crate::Node::Family(crate::FamilyData {
                    handle: f2.clone(),
                    father_handle: Some(p4.clone()),
                    mother_handle: Some(p5.clone()),
                    child_ref_list: vec![crate::ChildRef {
                        ref_field: p6.clone(),
                        ..crate::ChildRef::default()
                    }],
                    ..crate::FamilyData::default()
                }),
            )
            .unwrap();

        // Add family-person edges
        graph
            .add_edge(crate::Edge::FamilyFather {
                source: f1.clone(),
                target: p1.clone(),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::FamilyMother {
                source: f1.clone(),
                target: p2.clone(),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::FamilyChildRef {
                source: f1.clone(),
                target: p3.clone(),
                metadata: Box::new(crate::ChildRef {
                    ref_field: p3.clone(),
                    ..crate::ChildRef::default()
                }),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::FamilyFather {
                source: f2.clone(),
                target: p4.clone(),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::FamilyMother {
                source: f2.clone(),
                target: p5.clone(),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::FamilyChildRef {
                source: f2.clone(),
                target: p6.clone(),
                metadata: Box::new(crate::ChildRef {
                    ref_field: p6.clone(),
                    ..crate::ChildRef::default()
                }),
            })
            .unwrap();

        // Add PersonFamily and PersonParentFamily reverse edges
        graph
            .add_edge(crate::Edge::PersonFamily {
                source: p1.clone(),
                target: f1.clone(),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonFamily {
                source: p2.clone(),
                target: f1.clone(),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonFamily {
                source: p4.clone(),
                target: f2.clone(),
            })
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonFamily {
                source: p5.clone(),
                target: f2.clone(),
            })
            .unwrap();

        // Add some event/citation edges to verify they're preserved
        let evt1 = "evt1".to_string();
        graph
            .add_node(
                evt1.clone(),
                crate::Node::Event(crate::EventData {
                    handle: evt1.clone(),
                    event_type: crate::EventType::Birth,
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: p1,
                target: evt1,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        graph
    }

    #[test]
    fn disconnected_subgraphs_k_2_produces_two_clusters() {
        let graph = build_two_family_graph();
        let transform = disconnected_subgraphs(2);
        let result = transform(graph).unwrap();

        // Both families still exist
        assert!(result.contains_node(&"f1".to_string()));
        assert!(result.contains_node(&"f2".to_string()));

        // All persons still exist
        assert!(result.contains_node(&"p1".to_string()));
        assert!(result.contains_node(&"p2".to_string()));
        assert!(result.contains_node(&"p3".to_string()));
        assert!(result.contains_node(&"p4".to_string()));
        assert!(result.contains_node(&"p5".to_string()));
        assert!(result.contains_node(&"p6".to_string()));

        // Event edges should be preserved
        assert!(result.contains_node(&"evt1".to_string()));

        // No nodes removed
        assert_eq!(result.node_count(), 9); // 6 persons + 2 families + 1 event
    }

    #[test]
    fn disconnected_subgraphs_no_nodes_removed() {
        let graph = build_two_family_graph();
        let node_count_before = graph.node_count();

        let transform = disconnected_subgraphs(2);
        let result = transform(graph).unwrap();

        assert_eq!(
            result.node_count(),
            node_count_before,
            "No nodes should be removed"
        );
    }

    #[test]
    fn disconnected_subgraphs_empty_graph() {
        let graph = Graph::new();
        let transform = disconnected_subgraphs(2);
        let result = transform(graph).unwrap();
        assert_eq!(result.node_count(), 0);
        assert_eq!(result.edge_count(), 0);
    }

    #[test]
    fn disconnected_subgraphs_k_1_clamps_to_min() {
        let graph = build_two_family_graph();
        // k=1 should be treated as k=2
        let transform = disconnected_subgraphs(1);
        let result = transform(graph).unwrap();
        // Graph should still be valid (no crash)
        assert_eq!(result.node_count(), 9);
    }

    #[test]
    fn disconnected_subgraphs_single_person() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
                crate::Node::Person(crate::PersonData {
                    handle: "p1".to_string(),
                    gender: 0,
                    primary_name: crate::Name {
                        first_name: Some("Test".to_string()),
                        ..crate::Name::default()
                    },
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();

        let transform = disconnected_subgraphs(2);
        let result = transform(graph).unwrap();
        assert_eq!(result.node_count(), 1);
        assert!(result.contains_node(&"p1".to_string()));
    }

    #[test]
    fn disconnected_subgraphs_all_event_edges_preserved() {
        let graph = build_two_family_graph();
        let _edge_count_before = graph.edge_count();

        let transform = disconnected_subgraphs(2);
        let result = transform(graph).unwrap();

        // Event edges should still be present
        let evt_edges: Vec<_> = result
            .iter_edges()
            .filter(|e| matches!(e, crate::Edge::PersonEventRef { .. }))
            .collect();
        assert_eq!(evt_edges.len(), 1, "Event edges should be preserved");
    }

    #[test]
    fn disconnected_subgraphs_validates_ok() {
        // Use the random generation engine to create a valid graph,
        // then apply disconnected_subgraphs and verify it still validates.
        let schema = crate::Schema::new();
        let config = crate::generate::RandomConfig {
            person_count: 20,
            family_count: 6,
            generations: 2,
            seed: Some(42),
            ..crate::generate::RandomConfig::default()
        };
        let result = crate::generate::generate_random(
            &config,
            &crate::generate::AdversarialConfig::default(),
            &schema,
        )
        .expect("generation should succeed");

        let mut graph = result.graph;
        // Validate first (Gate 1)
        let errors = graph.validate(&schema);
        assert!(
            errors.is_empty(),
            "Gate 1 validation should pass: {:?}",
            errors
        );

        // Apply disconnected subgraphs
        let transform = disconnected_subgraphs(3);
        let mut graph = transform(graph).unwrap();

        // Should still pass validation (validity-preserving)
        let errors = graph.validate(&schema);
        assert!(
            errors.is_empty(),
            "Disconnected subgraphs should preserve validity: {:?}",
            errors
        );
    }

    #[test]
    fn disconnected_subgraphs_k_3_produces_three_clusters() {
        let schema = crate::Schema::new();
        let config = crate::generate::RandomConfig {
            person_count: 30,
            family_count: 10,
            generations: 3,
            seed: Some(123),
            ..crate::generate::RandomConfig::default()
        };
        let result = crate::generate::generate_random(
            &config,
            &crate::generate::AdversarialConfig::default(),
            &schema,
        )
        .expect("generation should succeed");

        let mut graph = result.graph;
        let _ = graph.validate(&schema);
        let node_count_before = graph.node_count();

        let transform = disconnected_subgraphs(3);
        let result = transform(graph).unwrap();

        // No nodes removed
        assert_eq!(result.node_count(), node_count_before);
    }

    // =======================================================================
    // Deep nesting transform tests
    // =======================================================================

    /// Build a graph with a single Place node.
    fn build_graph_with_place() -> Graph {
        let mut graph = Graph::new();
        graph
            .add_node(
                "pl1".to_string(),
                crate::Node::Place(crate::PlaceData {
                    handle: "pl1".to_string(),
                    name: crate::Location {
                        city: Some("Springfield".to_string()),
                        county: Some("Springfield County".to_string()),
                        state: Some("Northumbria".to_string()),
                        country: Some("Albion".to_string()),
                        ..crate::Location::default()
                    },
                    ..crate::PlaceData::default()
                }),
            )
            .unwrap();
        graph
    }

    #[test]
    fn deep_nesting_depth_5() {
        let graph = build_graph_with_place();
        let transform = deep_nesting(5);
        let result = transform(graph).unwrap();

        // Original place + 5 parent places = 6 places
        let places: Vec<_> = result
            .iter_nodes()
            .filter(|(_, n)| matches!(n, crate::Node::Place(_)))
            .collect();
        assert_eq!(places.len(), 6, "Should have 6 places with depth 5");

        // Should have 5 PlacePlaceRef edges
        let ref_edges: Vec<_> = result
            .iter_edges()
            .filter(|e| matches!(e, crate::Edge::PlacePlaceRef { .. }))
            .collect();
        assert_eq!(ref_edges.len(), 5, "Should have 5 PlacePlaceRef edges");
    }

    #[test]
    fn deep_nesting_depth_10() {
        let graph = build_graph_with_place();
        let transform = deep_nesting(10);
        let result = transform(graph).unwrap();

        let places: Vec<_> = result
            .iter_nodes()
            .filter(|(_, n)| matches!(n, crate::Node::Place(_)))
            .collect();
        assert_eq!(places.len(), 11, "Should have 11 places with depth 10");

        let ref_edges: Vec<_> = result
            .iter_edges()
            .filter(|e| matches!(e, crate::Edge::PlacePlaceRef { .. }))
            .collect();
        assert_eq!(ref_edges.len(), 10, "Should have 10 PlacePlaceRef edges");
    }

    #[test]
    fn deep_nesting_new_places_have_unique_handles() {
        let graph = build_graph_with_place();
        let transform = deep_nesting(5);
        let result = transform(graph).unwrap();

        let place_handles: std::collections::HashSet<_> = result
            .iter_nodes()
            .filter(|(_, n)| matches!(n, crate::Node::Place(_)))
            .map(|(h, _)| h.clone())
            .collect();
        assert_eq!(
            place_handles.len(),
            6,
            "All 6 places should have unique handles"
        );
    }

    #[test]
    fn deep_nesting_empty_graph() {
        let graph = Graph::new();
        let transform = deep_nesting(5);
        let result = transform(graph);
        assert!(
            matches!(result, Err(AdversarialError::TransformNotApplicable(_))),
            "Empty graph should return TransformNotApplicable"
        );
    }

    #[test]
    fn deep_nesting_no_places_noop() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        let transform = deep_nesting(5);
        let result = transform(graph);
        assert!(
            matches!(result, Err(AdversarialError::TransformNotApplicable(_))),
            "Graph with no places should return TransformNotApplicable"
        );
    }

    #[test]
    fn deep_nesting_node_count_increases() {
        let graph = build_graph_with_place();
        let node_count_before = graph.node_count();

        let transform = deep_nesting(5);
        let result = transform(graph).unwrap();

        assert_eq!(
            result.node_count(),
            node_count_before + 5,
            "Node count should increase by 5"
        );
    }

    #[test]
    fn deep_nesting_preserves_existing_edges() {
        let mut graph = build_graph_with_place();

        // Add an existing PlacePlaceRef edge
        let existing_parent = "existing_parent".to_string();
        graph
            .add_node(
                existing_parent.clone(),
                crate::Node::Place(crate::PlaceData {
                    handle: existing_parent.clone(),
                    name: crate::Location {
                        city: Some("Existing".to_string()),
                        ..crate::Location::default()
                    },
                    ..crate::PlaceData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PlacePlaceRef {
                source: "pl1".to_string(),
                target: existing_parent,
            })
            .unwrap();

        let transform = deep_nesting(3);
        let result = transform(graph).unwrap();

        // The existing PlacePlaceRef edge should still be present
        let existing_refs: Vec<_> = result
            .iter_edges()
            .filter(|e| matches!(e, crate::Edge::PlacePlaceRef { source, target } if source == "pl1" && target == "existing_parent"))
            .collect();
        assert_eq!(
            existing_refs.len(),
            1,
            "Existing PlacePlaceRef edge should be preserved"
        );
    }

    #[test]
    fn deep_nesting_validates_ok() {
        let schema = crate::Schema::new();

        // Create a graph with a place and rebuild validation
        let graph = build_graph_with_place();

        let transform = deep_nesting(5);
        let mut graph = transform(graph).unwrap();

        let errors = graph.validate(&schema);
        assert!(
            errors.is_empty(),
            "Deep nesting should be validity-preserving: {:?}",
            errors
        );
    }

    // =======================================================================
    // Max ref chains transform tests
    // =======================================================================

    /// Build a graph with a single Event node.
    fn build_graph_with_event() -> Graph {
        let mut graph = Graph::new();
        graph
            .add_node(
                "evt1".to_string(),
                crate::Node::Event(crate::EventData {
                    handle: "evt1".to_string(),
                    event_type: crate::EventType::Birth,
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
    }

    #[test]
    fn max_ref_chains_length_3() {
        let graph = build_graph_with_event();
        let transform = max_ref_chains(3);
        let result = transform(graph).unwrap();

        // 1 original event + 3 new nodes = 4 nodes
        assert_eq!(
            result.node_count(),
            4,
            "Should have 4 nodes with chain length 3"
        );
        // 3 edges (Event→Citation, Citation→Source, Source→Repository)
        assert_eq!(
            result.edge_count(),
            3,
            "Should have 3 edges with chain length 3"
        );
    }

    #[test]
    fn max_ref_chains_length_5() {
        let graph = build_graph_with_event();
        let transform = max_ref_chains(5);
        let result = transform(graph).unwrap();

        // 1 original event + 5 new nodes = 6 nodes
        assert_eq!(
            result.node_count(),
            6,
            "Should have 6 nodes with chain length 5"
        );
        // 5 edges
        assert_eq!(
            result.edge_count(),
            5,
            "Should have 5 edges with chain length 5"
        );
    }

    #[test]
    fn max_ref_chains_all_refs_resolve() {
        let graph = build_graph_with_event();
        let transform = max_ref_chains(5);
        let result = transform(graph).unwrap();

        // Every edge's source and target handles must exist in the graph
        for edge in result.iter_edges() {
            let (source, target) = crate::graph::edge_source_target(edge);
            assert!(
                result.contains_node(&source),
                "Source handle {} should exist",
                source
            );
            assert!(
                result.contains_node(&target),
                "Target handle {} should exist",
                target
            );
        }
    }

    #[test]
    fn max_ref_chains_no_circular_refs() {
        let graph = build_graph_with_event();
        let transform = max_ref_chains(5);
        let result = transform(graph).unwrap();

        // Each edge should form a forward chain; no edge should have its target
        // also be the source of an edge pointing back to the original source.
        // Collect all sources and targets
        let mut edge_pairs: Vec<(crate::Handle, crate::Handle)> = Vec::new();
        for edge in result.iter_edges() {
            let (source, target) = crate::graph::edge_source_target(edge);
            edge_pairs.push((source, target));
        }

        // Check no direct 2-edge cycles: A->B and B->A should not coexist
        for (s1, t1) in &edge_pairs {
            for (s2, t2) in &edge_pairs {
                if s1 == t2 && t1 == s2 {
                    panic!("Circular reference detected: {} <-> {}", s1, t1);
                }
            }
        }
    }

    #[test]
    fn max_ref_chains_validates_ok() {
        let schema = crate::Schema::new();
        let graph = build_graph_with_event();
        let transform = max_ref_chains(5);
        let mut graph = transform(graph).unwrap();

        let errors = graph.validate(&schema);
        assert!(
            errors.is_empty(),
            "Max ref chains should be validity-preserving: {:?}",
            errors
        );
    }

    #[test]
    fn max_ref_chains_node_count_increases() {
        let graph = build_graph_with_event();
        let node_count_before = graph.node_count();

        let transform = max_ref_chains(5);
        let result = transform(graph).unwrap();

        assert_eq!(
            result.node_count(),
            node_count_before + 5,
            "Node count should increase by 5"
        );
    }

    #[test]
    fn max_ref_chains_empty_graph() {
        let graph = Graph::new();
        let transform = max_ref_chains(5);
        let result = transform(graph);
        assert!(
            matches!(result, Err(AdversarialError::TransformNotApplicable(_))),
            "Empty graph should return TransformNotApplicable"
        );
    }

    // =======================================================================
    // orphaned_references tests
    // =======================================================================

    /// Build a graph with persons, families, events, places, sources,
    /// repositories, media, notes, and tags so there are plenty of soft
    /// reference edges to remove.
    fn build_graph_with_soft_edges() -> Graph {
        use crate::generate::random::RandomConfig;

        let schema = crate::Schema::new();
        let config = RandomConfig {
            person_count: 3,
            family_count: 2,
            with_places: true,
            with_citations: true,
            with_notes: true,
            with_media: true,
            with_tags: true,
            seed: Some(42),
            ..RandomConfig::default()
        };
        let adversarial_config = crate::generate::AdversarialConfig::default();
        let result =
            crate::generate::random::generate_random(&config, &adversarial_config, &schema)
                .unwrap();
        result.graph
    }

    #[test]
    fn orphaned_references_validity_preserving() {
        let schema = crate::Schema::new();
        let graph = build_graph_with_soft_edges();

        let transform = orphaned_references(0.5);
        let mut result = transform(graph).unwrap();

        let errors = result.validate(&schema);
        assert!(
            errors.is_empty(),
            "orphaned_references is validity-preserving: {:?}",
            errors
        );
    }

    #[test]
    fn orphaned_references_removes_some_edges() {
        let graph = build_graph_with_soft_edges();
        let edge_count_before = graph.edge_count();

        let transform = orphaned_references(0.5);
        let result = transform(graph).unwrap();

        let edge_count_after = result.edge_count();
        assert!(
            edge_count_after < edge_count_before,
            "orphaned_references(0.5) should remove some edges (before={}, after={})",
            edge_count_before,
            edge_count_after
        );
        assert!(
            edge_count_after > 0,
            "At least some structural edges should remain"
        );
    }

    #[test]
    fn orphaned_references_keeps_all_nodes() {
        let graph = build_graph_with_soft_edges();
        let node_count_before = graph.node_count();

        let transform = orphaned_references(0.5);
        let result = transform(graph).unwrap();

        assert_eq!(
            result.node_count(),
            node_count_before,
            "No nodes should be removed by orphaned_references"
        );
    }

    #[test]
    fn orphaned_references_orphans_targets() {
        let graph = build_graph_with_soft_edges();

        let transform = orphaned_references(1.0);
        let result = transform(graph).unwrap();

        // The graph is still structurally valid (edge targets still exist),
        // but citation/note/media nodes are orphaned — no edge references them.
        let has_orphans = result.iter_nodes().any(|(handle, node)| {
            let is_annotation = matches!(
                node,
                crate::Node::Citation(_)
                    | crate::Node::Note(_)
                    | crate::Node::Media(_)
                    | crate::Node::Tag(_)
            );
            if !is_annotation {
                return false;
            }
            // Check if this node has any incoming edges
            let has_incoming = !result.edges_to(handle).is_empty();
            !has_incoming
        });

        assert!(
            has_orphans,
            "orphaned_references should create orphaned annotation nodes"
        );
    }

    #[test]
    fn orphaned_references_keeps_structural_edges() {
        let graph = build_graph_with_soft_edges();
        let structural_before: Vec<crate::Edge> = graph
            .iter_edges()
            .filter(|e| {
                matches!(
                    e,
                    crate::Edge::PersonFamily { .. }
                        | crate::Edge::PersonParentFamily { .. }
                        | crate::Edge::FamilyFather { .. }
                        | crate::Edge::FamilyMother { .. }
                        | crate::Edge::FamilyChildRef { .. }
                        | crate::Edge::PersonEventRef { .. }
                        | crate::Edge::FamilyEventRef { .. }
                        | crate::Edge::EventPlace { .. }
                        | crate::Edge::CitationSource { .. }
                        | crate::Edge::PlacePlaceRef { .. }
                        | crate::Edge::PersonPersonRef { .. }
                        | crate::Edge::SourceRepoRef { .. }
                )
            })
            .cloned()
            .collect();

        let transform = orphaned_references(0.5);
        let result = transform(graph).unwrap();

        let structural_after: Vec<crate::Edge> = result
            .iter_edges()
            .filter(|e| {
                matches!(
                    e,
                    crate::Edge::PersonFamily { .. }
                        | crate::Edge::PersonParentFamily { .. }
                        | crate::Edge::FamilyFather { .. }
                        | crate::Edge::FamilyMother { .. }
                        | crate::Edge::FamilyChildRef { .. }
                        | crate::Edge::PersonEventRef { .. }
                        | crate::Edge::FamilyEventRef { .. }
                        | crate::Edge::EventPlace { .. }
                        | crate::Edge::CitationSource { .. }
                        | crate::Edge::PlacePlaceRef { .. }
                        | crate::Edge::PersonPersonRef { .. }
                        | crate::Edge::SourceRepoRef { .. }
                )
            })
            .cloned()
            .collect();

        assert_eq!(
            structural_before.len(),
            structural_after.len(),
            "Structural edges should not be removed by orphaned_references"
        );
    }

    #[test]
    fn orphaned_references_fraction_zero() {
        let graph = build_graph_with_soft_edges();
        let edge_count_before = graph.edge_count();

        let transform = orphaned_references(0.0);
        let result = transform(graph).unwrap();

        assert_eq!(
            result.edge_count(),
            edge_count_before,
            "fraction=0.0 should not remove any edges"
        );
    }

    #[test]
    fn orphaned_references_fraction_one() {
        let graph = build_graph_with_soft_edges();

        let transform = orphaned_references(1.0);
        let result = transform(graph).unwrap();

        // All soft edges should be removed; only structural edges remain
        let soft_remaining: usize = result
            .iter_edges()
            .filter(|e| {
                matches!(
                    e,
                    crate::Edge::CitationRef { .. }
                        | crate::Edge::NoteRef { .. }
                        | crate::Edge::MediaRef { .. }
                        | crate::Edge::TagRef { .. }
                        | crate::Edge::PersonCitation { .. }
                        | crate::Edge::PersonNote { .. }
                        | crate::Edge::PersonTag { .. }
                        | crate::Edge::PersonMediaRef { .. }
                        | crate::Edge::EventCitation { .. }
                        | crate::Edge::EventNote { .. }
                        | crate::Edge::EventTag { .. }
                        | crate::Edge::EventMediaRef { .. }
                        | crate::Edge::FamilyCitation { .. }
                        | crate::Edge::FamilyNote { .. }
                        | crate::Edge::FamilyTag { .. }
                        | crate::Edge::FamilyMediaRef { .. }
                        | crate::Edge::PlaceCitation { .. }
                        | crate::Edge::PlaceNote { .. }
                        | crate::Edge::PlaceTag { .. }
                        | crate::Edge::PlaceMediaRef { .. }
                        | crate::Edge::SourceNote { .. }
                        | crate::Edge::SourceTag { .. }
                        | crate::Edge::SourceMediaRef { .. }
                        | crate::Edge::RepositoryNote { .. }
                        | crate::Edge::RepositoryTag { .. }
                        | crate::Edge::RepositoryMediaRef { .. }
                        | crate::Edge::MediaCitation { .. }
                        | crate::Edge::MediaNote { .. }
                        | crate::Edge::MediaTag { .. }
                        | crate::Edge::NoteCitation { .. }
                        | crate::Edge::NoteTag { .. }
                        | crate::Edge::CitationNote { .. }
                        | crate::Edge::CitationTag { .. }
                        | crate::Edge::CitationMediaRef { .. }
                )
            })
            .count();

        assert_eq!(
            soft_remaining, 0,
            "fraction=1.0 should remove all soft edges"
        );
        assert!(
            result.edge_count() > 0,
            "Structural edges should remain after removing all soft edges"
        );
    }
}
