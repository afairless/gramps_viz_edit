//! Graph storage — concrete typed directed multigraph for Gramps genealogy data.
//!
//! This module provides the [`Graph`] struct, which stores all nodes and edges
//! in memory, and the [`ValidationState`] enum for tracking validation status.

use std::collections::HashMap;

use crate::Edge;
use crate::Handle;
use crate::Node;

/// Tracks the validation status of a [`Graph`].
#[derive(Clone, Debug, PartialEq)]
pub enum ValidationState {
    /// Graph has not been validated yet.
    Unvalidated,
    /// Last validation passed with no errors (warnings may remain).
    Valid,
    /// Last validation failed with the given errors.
    Invalid(Vec<ValidationError>),
}

/// Errors that can occur during graph construction or querying.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphError {
    /// A node with the given handle already exists in the graph.
    DuplicateHandle(Handle),
    /// An edge references a source or target handle that does not exist.
    MissingNode(Handle),
    /// The edge is structurally invalid (e.g., missing required metadata).
    InvalidEdge(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::DuplicateHandle(h) => {
                write!(f, "duplicate handle: '{}' already exists in the graph", h)
            }
            GraphError::MissingNode(h) => {
                write!(
                    f,
                    "missing node: handle '{}' does not exist in the graph",
                    h
                )
            }
            GraphError::InvalidEdge(msg) => {
                write!(f, "invalid edge: {}", msg)
            }
        }
    }
}

impl std::error::Error for GraphError {}

/// A kind identifier for node variants, used for filtering in queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// [`Node::Citation`]
    Citation,
    /// [`Node::Event`]
    Event,
    /// [`Node::Family`]
    Family,
    /// [`Node::Media`]
    Media,
    /// [`Node::Note`]
    Note,
    /// [`Node::Person`]
    Person,
    /// [`Node::Place`]
    Place,
    /// [`Node::Repository`]
    Repository,
    /// [`Node::Source`]
    Source,
    /// [`Node::Tag`]
    Tag,
}

/// The typed directed multigraph.
///
/// Concrete (not generic) — all nodes are [`Node`] enum variants,
/// all edges are [`Edge`] enum variants.
///
/// # Validation
///
/// The graph tracks its validation state via [`validation_state`](Graph::validation_state).
/// After construction, the graph is [`ValidationState::Unvalidated`]. Use the
/// validation module to run structural and referential checks, which update the state.
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    /// All primary nodes, keyed by handle.
    nodes: HashMap<Handle, Node>,
    /// Forward edge index: source → [edge_index].
    forward_edges: HashMap<Handle, Vec<usize>>,
    /// Reverse edge index: target → [edge_index].
    reverse_edges: HashMap<Handle, Vec<usize>>,
    /// Validation state set by the last validation pass.
    validation_state: ValidationState,
    /// All edges in insertion order.
    edges: Vec<Edge>,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// Create a new empty [`Graph`].
    ///
    /// The graph starts with zero nodes, zero edges, and
    /// [`ValidationState::Unvalidated`].
    pub fn new() -> Self {
        Graph {
            nodes: HashMap::new(),
            edges: Vec::new(),
            forward_edges: HashMap::new(),
            reverse_edges: HashMap::new(),
            validation_state: ValidationState::Unvalidated,
        }
    }

    // -----------------------------------------------------------------------
    // Mutation
    // -----------------------------------------------------------------------

    /// Add a [`Node`] to the graph under the given [`Handle`].
    ///
    /// Returns [`GraphError::DuplicateHandle`] if a node with this handle
    /// already exists.
    pub fn add_node(&mut self, handle: Handle, node: Node) -> Result<(), GraphError> {
        if self.nodes.contains_key(&handle) {
            return Err(GraphError::DuplicateHandle(handle));
        }
        self.nodes.insert(handle, node);
        self.validation_state = ValidationState::Unvalidated;
        Ok(())
    }

    /// Add an [`Edge`] to the graph.
    ///
    /// The edge's source and target handles must already exist in the graph's
    /// nodes. Returns [`GraphError::MissingNode`] if either handle is not found.
    pub fn add_edge(&mut self, edge: Edge) -> Result<(), GraphError> {
        let (source, target) = edge_source_target(&edge);
        if !self.nodes.contains_key(&source) {
            return Err(GraphError::MissingNode(source));
        }
        if !self.nodes.contains_key(&target) {
            return Err(GraphError::MissingNode(target));
        }
        let index = self.edges.len();
        self.edges.push(edge);
        self.forward_edges.entry(source).or_default().push(index);
        self.reverse_edges.entry(target).or_default().push(index);
        self.validation_state = ValidationState::Unvalidated;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Query — nodes
    // -----------------------------------------------------------------------

    /// Get an immutable reference to a [`Node`] by its [`Handle`].
    pub fn get_node(&self, handle: &Handle) -> Option<&Node> {
        self.nodes.get(handle)
    }

    /// Get a mutable reference to a [`Node`] by its [`Handle`].
    pub fn get_node_mut(&mut self, handle: &Handle) -> Option<&mut Node> {
        self.nodes.get_mut(handle)
    }

    /// Check whether a node with the given [`Handle`] exists.
    pub fn contains_node(&self, handle: &Handle) -> bool {
        self.nodes.contains_key(handle)
    }

    /// Return the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    // -----------------------------------------------------------------------
    // Query — edges
    // -----------------------------------------------------------------------

    /// Get an immutable reference to an [`Edge`] by its index.
    pub fn get_edge(&self, index: usize) -> Option<&Edge> {
        self.edges.get(index)
    }

    /// Return the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    // -----------------------------------------------------------------------
    // Iteration
    // -----------------------------------------------------------------------

    /// Iterate over all nodes in the graph as `(&Handle, &Node)` pairs.
    pub fn iter_nodes(&self) -> impl Iterator<Item = (&Handle, &Node)> {
        self.nodes.iter()
    }

    /// Return the handles of all nodes matching the given [`NodeKind`].
    pub fn nodes_by_kind(&self, kind: NodeKind) -> Vec<&Handle> {
        self.nodes
            .iter()
            .filter(|(_, node)| node_kind(node) == kind)
            .map(|(handle, _)| handle)
            .collect()
    }

    /// Iterate over all edges in insertion order.
    pub fn iter_edges(&self) -> impl Iterator<Item = &Edge> {
        self.edges.iter()
    }

    /// Return all edges whose source is the given [`Handle`].
    ///
    /// Uses the forward edge index for O(1) lookup.
    pub fn edges_from(&self, handle: &Handle) -> Vec<&Edge> {
        self.forward_edges
            .get(handle)
            .map(|indices| indices.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    /// Return all edges whose target is the given [`Handle`].
    ///
    /// Uses the reverse edge index for O(1) lookup.
    pub fn edges_to(&self, handle: &Handle) -> Vec<&Edge> {
        self.reverse_edges
            .get(handle)
            .map(|indices| indices.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Validation state
    // -----------------------------------------------------------------------

    /// Get the current [`ValidationState`].
    pub fn validation_state(&self) -> &ValidationState {
        &self.validation_state
    }

    /// Set the current [`ValidationState`].
    pub fn set_validation_state(&mut self, state: ValidationState) {
        self.validation_state = state;
    }

    /// Remove all edges matching the given predicate.
    ///
    /// Returns the number of removed edges. Rebuilds the forward and reverse
    /// edge indexes after removal. Resets `validation_state` to `Unvalidated`.
    pub fn remove_edges(&mut self, predicate: impl Fn(&Edge) -> bool) -> usize {
        let before = self.edges.len();
        self.edges.retain(|e| !predicate(e));
        let removed = before - self.edges.len();
        if removed > 0 {
            self.rebuild_edge_index();
            self.validation_state = ValidationState::Unvalidated;
        }
        removed
    }

    /// Internal: rebuild the forward and reverse edge indexes from scratch.
    fn rebuild_edge_index(&mut self) {
        self.forward_edges.clear();
        self.reverse_edges.clear();
        for (i, edge) in self.edges.iter().enumerate() {
            let (source, target) = edge_source_target(edge);
            self.forward_edges.entry(source).or_default().push(i);
            self.reverse_edges.entry(target).or_default().push(i);
        }
    }
}

/// Return the [`NodeKind`] for a given [`Node`].
pub fn node_kind(node: &Node) -> NodeKind {
    match node {
        Node::Citation(_) => NodeKind::Citation,
        Node::Event(_) => NodeKind::Event,
        Node::Family(_) => NodeKind::Family,
        Node::Media(_) => NodeKind::Media,
        Node::Note(_) => NodeKind::Note,
        Node::Person(_) => NodeKind::Person,
        Node::Place(_) => NodeKind::Place,
        Node::Repository(_) => NodeKind::Repository,
        Node::Source(_) => NodeKind::Source,
        Node::Tag(_) => NodeKind::Tag,
    }
}

/// Extract the source and target handles from any [`Edge`] variant.
pub(crate) fn edge_source_target(edge: &Edge) -> (Handle, Handle) {
    match edge {
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
    }
}

// ---------------------------------------------------------------------------
// Helper functions for schema-version-specific edge construction
// ---------------------------------------------------------------------------

/// Create a PlacePlaceRef edge with appropriate fields for the current schema.
#[cfg(feature = "schema-5-1")]
pub fn edge_place_place_ref(source: Handle, target: Handle) -> Edge {
    Edge::PlacePlaceRef {
        source,
        target: target.clone(),
        metadata: Box::new(crate::PlaceRef {
            ref_field: target,
            date: None,
        }),
    }
}

/// Create a PlacePlaceRef edge with appropriate fields for the current schema.
#[cfg(not(feature = "schema-5-1"))]
pub fn edge_place_place_ref(source: Handle, target: Handle) -> Edge {
    Edge::PlacePlaceRef { source, target }
}

/// Create an EventRef with appropriate fields for the current schema.
#[cfg(feature = "schema-5-1")]
pub fn make_event_ref(ref_field: Handle, role: Option<crate::EventRoleType>) -> crate::EventRef {
    crate::EventRef {
        ref_field,
        role,
        attribute_list: vec![],
        note_list: vec![],
    }
}

/// Create an EventRef with appropriate fields for the current schema.
#[cfg(not(feature = "schema-5-1"))]
pub fn make_event_ref(ref_field: Handle, role: Option<crate::EventRoleType>) -> crate::EventRef {
    crate::EventRef { ref_field, role }
}

/// Create a ChildRef with appropriate fields for the current schema.
#[cfg(feature = "schema-5-1")]
pub fn make_child_ref(ref_field: Handle, relation: Option<crate::ChildRefType>) -> crate::ChildRef {
    crate::ChildRef {
        ref_field,
        relation,
        citation_list: vec![],
        note_list: vec![],
        frel: None,
        mrel: None,
    }
}

/// Create a ChildRef with appropriate fields for the current schema.
#[cfg(not(feature = "schema-5-1"))]
pub fn make_child_ref(ref_field: Handle, relation: Option<crate::ChildRefType>) -> crate::ChildRef {
    crate::ChildRef {
        ref_field,
        relation,
    }
}

/// Get the gender value from a PersonData, adapting Option<i32> (5.1) to i32 (5.2).
#[cfg(feature = "schema-5-1")]
pub fn gender_value(gender: Option<i32>) -> i32 {
    gender.unwrap_or(0)
}

/// Get the gender value from a PersonData, adapting Option<i32> (5.1) to i32 (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn gender_value(gender: i32) -> i32 {
    gender
}

/// Check if a source_handle field is empty, handling Option<String> (5.1) vs String (5.2).
#[cfg(feature = "schema-5-1")]
pub fn is_source_handle_empty(source_handle: &Option<String>) -> bool {
    source_handle.as_ref().is_none_or(|s| s.is_empty())
}

/// Check if a source_handle field is empty, handling Option<String> (5.1) vs String (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn is_source_handle_empty(source_handle: &str) -> bool {
    source_handle.is_empty()
}

/// Get the source_handle as a String, unwrapping Option<String> (5.1) or cloning String (5.2).
#[cfg(feature = "schema-5-1")]
pub fn get_source_handle(source_handle: &Option<String>) -> String {
    source_handle.clone().unwrap_or_default()
}

/// Get the source_handle as a String, unwrapping Option<String> (5.1) or cloning String (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn get_source_handle(source_handle: &str) -> String {
    source_handle.to_owned()
}

/// Set the source_handle on a CitationData, handling Option<String> (5.1) vs String (5.2).
#[cfg(feature = "schema-5-1")]
pub fn set_source_handle(citation: &mut crate::CitationData, handle: Handle) {
    citation.source_handle = Some(handle);
}

/// Set the source_handle on a CitationData, handling Option<String> (5.1) vs String (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn set_source_handle(citation: &mut crate::CitationData, handle: Handle) {
    citation.source_handle = handle;
}

/// Set the gender on a PersonData, handling Option<i32> (5.1) vs i32 (5.2).
#[cfg(feature = "schema-5-1")]
pub fn set_gender(person: &mut crate::PersonData, gender: i32) {
    person.gender = Some(gender);
}

/// Set the gender on a PersonData, handling Option<i32> (5.1) vs i32 (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn set_gender(person: &mut crate::PersonData, gender: i32) {
    person.gender = gender;
}

/// Check if gender is valid (0-3), handling Option<i32> (5.1) vs i32 (5.2).
#[cfg(feature = "schema-5-1")]
pub fn is_gender_valid(gender: &Option<i32>) -> bool {
    matches!(gender, Some(0..=3))
}

/// Check if gender is valid (0-3), handling Option<i32> (5.1) vs i32 (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn is_gender_valid(gender: &i32) -> bool {
    matches!(gender, 0..=3)
}

/// Create a CitationData with appropriate fields for the current schema.
#[cfg(feature = "schema-5-1")]
pub fn make_citation(handle: Handle, source_handle: Handle) -> crate::CitationData {
    crate::CitationData {
        handle,
        source_handle: Some(source_handle),
        ..crate::CitationData::default()
    }
}

/// Create a CitationData with appropriate fields for the current schema.
#[cfg(not(feature = "schema-5-1"))]
pub fn make_citation(handle: Handle, source_handle: Handle) -> crate::CitationData {
    crate::CitationData {
        handle,
        source_handle,
        ..crate::CitationData::default()
    }
}

/// Return the gender value wrapped as needed for the current schema's PersonData type.
#[cfg(feature = "schema-5-1")]
pub fn into_gender_field(gender: i32) -> Option<i32> {
    Some(gender)
}

/// Return the gender value wrapped as needed for the current schema's PersonData type.
#[cfg(not(feature = "schema-5-1"))]
pub fn into_gender_field(gender: i32) -> i32 {
    gender
}

/// Return the event_type wrapped as needed for the current schema's EventData type.
#[cfg(feature = "schema-5-1")]
pub fn into_event_type_field(event_type: crate::EventType) -> Option<crate::EventType> {
    Some(event_type)
}

/// Return the event_type wrapped as needed for the current schema's EventData type.
#[cfg(not(feature = "schema-5-1"))]
pub fn into_event_type_field(event_type: crate::EventType) -> crate::EventType {
    event_type
}

/// Return the source_handle wrapped as needed for the current schema's CitationData type.
#[cfg(feature = "schema-5-1")]
pub fn into_source_handle_field(handle: Handle) -> Option<Handle> {
    Some(handle)
}

/// Return the source_handle wrapped as needed for the current schema's CitationData type.
#[cfg(not(feature = "schema-5-1"))]
pub fn into_source_handle_field(handle: Handle) -> Handle {
    handle
}

/// Compare event_type for equality, handling Option<EventType> (5.1) vs EventType (5.2).
#[cfg(feature = "schema-5-1")]
pub fn event_type_eq(event_type: &Option<crate::EventType>, target: crate::EventType) -> bool {
    *event_type == Some(target)
}

/// Compare event_type for equality, handling Option<EventType> (5.1) vs EventType (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn event_type_eq(event_type: &crate::EventType, target: crate::EventType) -> bool {
    *event_type == target
}

/// Get the gender from PersonData for comparison, handling Option<i32> (5.1) vs i32 (5.2).
#[cfg(feature = "schema-5-1")]
pub fn gender_cmp(gender: &Option<i32>) -> Option<i32> {
    *gender
}

/// Get the gender from PersonData for comparison, handling Option<i32> (5.1) vs i32 (5.2).
#[cfg(not(feature = "schema-5-1"))]
pub fn gender_cmp(gender: &i32) -> i32 {
    *gender
}

/// Display an event type as a string, handling Option<EventType> (5.1) vs EventType (5.2).
///
/// Returns the enum variant name (e.g. "Birth", "Death", "Marriage") without
/// any `Option` wrapping artifacts.
#[cfg(feature = "schema-5-1")]
pub fn event_type_display(event_type: &Option<crate::EventType>) -> String {
    format!("{:?}", event_type.unwrap_or(crate::EventType::Birth))
}

/// Display an event type as a string, handling Option<EventType> (5.1) vs EventType (5.2).
///
/// Returns the enum variant name (e.g. "Birth", "Death", "Marriage") without
/// any `Option` wrapping artifacts.
#[cfg(not(feature = "schema-5-1"))]
pub fn event_type_display(event_type: &crate::EventType) -> String {
    format!("{:?}", event_type)
}

// ---------------------------------------------------------------------------
// ValidationError (placeholder for Phase 2, Step 4)
// ---------------------------------------------------------------------------

/// Errors detected during graph validation.
///
/// This type is defined here because [`ValidationState::Invalid`] references it.
/// The full validation logic lives in the `validate` module (Step 4).
#[derive(Clone, Debug, PartialEq)]
pub enum ValidationError {
    /// An edge references a handle that doesn't exist in the graph.
    DanglingReference {
        source: Handle,
        link: String,
        target: Handle,
    },
    /// A required field is missing (e.g., Person with no primary_name).
    MissingRequired { node: Handle, field: String },
    /// A cardinality constraint is violated.
    CardinalityViolation {
        node: Handle,
        field: String,
        expected: String,
        actual: usize,
    },
    /// Genealogical plausibility warning.
    PlausibilityWarning { node: Handle, message: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::DanglingReference {
                source,
                link,
                target,
            } => {
                write!(
                    f,
                    "dangling reference: {} → {} → {}: target handle '{}' not found in graph",
                    source, link, target, target
                )
            }
            ValidationError::MissingRequired { node, field } => {
                write!(
                    f,
                    "missing required field: node '{}' is missing required field '{}'",
                    node, field
                )
            }
            ValidationError::CardinalityViolation {
                node,
                field,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "cardinality violation: node '{}' field '{}': expected {}, got {}",
                    node, field, expected, actual
                )
            }
            ValidationError::PlausibilityWarning { node, message } => {
                write!(f, "plausibility warning: node '{}': {}", node, message)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, not(feature = "schema-5-1")))]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn graph_new_is_empty() {
        let graph = Graph::new();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert_eq!(graph.validation_state(), &ValidationState::Unvalidated);
    }

    #[test]
    fn add_node_ok() {
        let mut graph = Graph::new();
        let handle = "p1".to_string();
        let node = Node::Person(PersonData::default());
        assert!(graph.add_node(handle, node).is_ok());
        assert_eq!(graph.node_count(), 1);
    }

    #[test]
    fn add_node_duplicate_handle() {
        let mut graph = Graph::new();
        let handle = "p1".to_string();
        graph
            .add_node(handle.clone(), Node::Person(PersonData::default()))
            .unwrap();
        let result = graph.add_node(handle.clone(), Node::Person(PersonData::default()));
        assert_eq!(result, Err(GraphError::DuplicateHandle(handle)));
    }

    #[test]
    fn add_edge_ok() {
        let mut graph = Graph::new();
        let h1 = "p1".to_string();
        let h2 = "p2".to_string();
        graph
            .add_node(h1.clone(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node(h2.clone(), Node::Person(PersonData::default()))
            .unwrap();
        let edge = Edge::PersonFamily {
            source: h1.clone(),
            target: h2.clone(),
        };
        assert!(graph.add_edge(edge).is_ok());
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn add_edge_missing_source_node() {
        let mut graph = Graph::new();
        let h1 = "p1".to_string();
        let h2 = "p2".to_string();
        graph
            .add_node(h2.clone(), Node::Person(PersonData::default()))
            .unwrap();
        let edge = Edge::PersonFamily {
            source: h1.clone(),
            target: h2.clone(),
        };
        assert_eq!(graph.add_edge(edge), Err(GraphError::MissingNode(h1)));
    }

    #[test]
    fn add_edge_missing_target_node() {
        let mut graph = Graph::new();
        let h1 = "p1".to_string();
        let h2 = "p2".to_string();
        graph
            .add_node(h1.clone(), Node::Person(PersonData::default()))
            .unwrap();
        let edge = Edge::PersonFamily {
            source: h1.clone(),
            target: h2.clone(),
        };
        assert_eq!(graph.add_edge(edge), Err(GraphError::MissingNode(h2)));
    }

    #[test]
    fn get_node_returns_node() {
        let mut graph = Graph::new();
        let handle = "p1".to_string();
        let node = Node::Person(PersonData::default());
        graph.add_node(handle.clone(), node.clone()).unwrap();
        let result = graph.get_node(&handle);
        assert!(result.is_some());
        assert_eq!(*result.unwrap(), node);
    }

    #[test]
    fn get_node_returns_none() {
        let graph = Graph::new();
        let result = graph.get_node(&"nonexistent".to_string());
        assert!(result.is_none());
    }

    #[test]
    fn get_edge_returns_edge() {
        let mut graph = Graph::new();
        let h1 = "p1".to_string();
        let h2 = "f1".to_string();
        graph
            .add_node(h1.clone(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node(h2.clone(), Node::Family(FamilyData::default()))
            .unwrap();
        let edge = Edge::PersonFamily {
            source: h1.clone(),
            target: h2.clone(),
        };
        let edge_clone = edge.clone();
        graph.add_edge(edge).unwrap();
        let result = graph.get_edge(0);
        assert!(result.is_some());
        assert_eq!(*result.unwrap(), edge_clone);
    }

    #[test]
    fn get_edge_out_of_bounds() {
        let graph = Graph::new();
        assert!(graph.get_edge(0).is_none());
        assert!(graph.get_edge(5).is_none());
    }

    #[test]
    fn validation_state_initially_unvalidated() {
        let graph = Graph::new();
        assert_eq!(graph.validation_state(), &ValidationState::Unvalidated);
    }

    #[test]
    fn set_validation_state_updates() {
        let mut graph = Graph::new();
        assert_eq!(graph.validation_state(), &ValidationState::Unvalidated);

        graph.set_validation_state(ValidationState::Valid);
        assert_eq!(graph.validation_state(), &ValidationState::Valid);

        let errors = vec![ValidationError::MissingRequired {
            node: "p1".to_string(),
            field: "primary_name".to_string(),
        }];
        graph.set_validation_state(ValidationState::Invalid(errors));
        assert!(matches!(
            graph.validation_state(),
            ValidationState::Invalid(_)
        ));
    }

    #[test]
    fn get_node_mut_allows_mutation() {
        let mut graph = Graph::new();
        let handle = "p1".to_string();
        let node = Node::Person(PersonData {
            handle: handle.clone(),
            ..PersonData::default()
        });
        graph.add_node(handle.clone(), node).unwrap();

        // Mutate the node's gender
        if let Some(Node::Person(ref mut person)) = graph.get_node_mut(&handle) {
            person.gender = 1;
        }

        // Verify the mutation
        let retrieved = graph.get_node(&handle).unwrap();
        if let Node::Person(person) = retrieved {
            assert_eq!(person.gender, 1);
        } else {
            panic!("Expected Person node");
        }
    }

    #[test]
    fn contains_node_true_for_existing() {
        let mut graph = Graph::new();
        let handle = "p1".to_string();
        graph
            .add_node(handle.clone(), Node::Person(PersonData::default()))
            .unwrap();
        assert!(graph.contains_node(&handle));
    }

    #[test]
    fn contains_node_false_for_missing() {
        let graph = Graph::new();
        assert!(!graph.contains_node(&"missing".to_string()));
    }

    #[test]
    fn graph_default_is_empty() {
        let graph = Graph::default();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn graph_error_display() {
        let err = GraphError::DuplicateHandle("p1".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("p1"));

        let err = GraphError::MissingNode("p1".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("p1"));
    }

    #[test]
    fn validation_error_display() {
        let err = ValidationError::MissingRequired {
            node: "p1".to_string(),
            field: "primary_name".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("p1"));
        assert!(msg.contains("primary_name"));
    }

    // -----------------------------------------------------------------------
    // Query method tests
    // -----------------------------------------------------------------------

    #[test]
    fn iter_nodes_empty() {
        let graph = Graph::new();
        assert_eq!(graph.iter_nodes().count(), 0);
    }

    #[test]
    fn iter_nodes_with_nodes() {
        let mut graph = Graph::new();
        graph
            .add_node("p1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("f1".into(), Node::Family(FamilyData::default()))
            .unwrap();
        assert_eq!(graph.iter_nodes().count(), 3);
    }

    #[test]
    fn nodes_by_kind() {
        let mut graph = Graph::new();
        graph
            .add_node("p1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("f1".into(), Node::Family(FamilyData::default()))
            .unwrap();

        let persons = graph.nodes_by_kind(NodeKind::Person);
        assert_eq!(persons.len(), 2);
        assert!(persons.contains(&&"p1".to_string()));
        assert!(persons.contains(&&"p2".to_string()));

        let families = graph.nodes_by_kind(NodeKind::Family);
        assert_eq!(families.len(), 1);
        assert!(families.contains(&&"f1".to_string()));

        let events = graph.nodes_by_kind(NodeKind::Event);
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn iter_edges_empty() {
        let graph = Graph::new();
        assert_eq!(graph.iter_edges().count(), 0);
    }

    #[test]
    fn iter_edges_with_edges() {
        let mut graph = Graph::new();
        graph
            .add_node("p1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("f1".into(), Node::Family(FamilyData::default()))
            .unwrap();

        graph
            .add_edge(Edge::PersonFamily {
                source: "p1".into(),
                target: "f1".into(),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonFamily {
                source: "p2".into(),
                target: "f1".into(),
            })
            .unwrap();

        assert_eq!(graph.iter_edges().count(), 2);
    }

    #[test]
    fn edges_from_returns_correct_edges() {
        let mut graph = Graph::new();
        graph
            .add_node("p1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("f1".into(), Node::Family(FamilyData::default()))
            .unwrap();

        graph
            .add_edge(Edge::PersonFamily {
                source: "p1".into(),
                target: "f1".into(),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonFamily {
                source: "p2".into(),
                target: "f1".into(),
            })
            .unwrap();

        let from_p1 = graph.edges_from(&"p1".to_string());
        assert_eq!(from_p1.len(), 1);

        let from_p2 = graph.edges_from(&"p2".to_string());
        assert_eq!(from_p2.len(), 1);

        let from_f1 = graph.edges_from(&"f1".to_string());
        assert_eq!(from_f1.len(), 0);
    }

    #[test]
    fn edges_to_returns_correct_edges() {
        let mut graph = Graph::new();
        graph
            .add_node("p1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("f1".into(), Node::Family(FamilyData::default()))
            .unwrap();

        graph
            .add_edge(Edge::PersonFamily {
                source: "p1".into(),
                target: "f1".into(),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonFamily {
                source: "p2".into(),
                target: "f1".into(),
            })
            .unwrap();

        let to_f1 = graph.edges_to(&"f1".to_string());
        assert_eq!(to_f1.len(), 2);

        let to_p1 = graph.edges_to(&"p1".to_string());
        assert_eq!(to_p1.len(), 0);
    }

    #[test]
    fn edges_from_missing_handle() {
        let graph = Graph::new();
        let edges = graph.edges_from(&"nonexistent".to_string());
        assert!(edges.is_empty());
    }

    #[test]
    fn edges_to_missing_handle() {
        let graph = Graph::new();
        let edges = graph.edges_to(&"nonexistent".to_string());
        assert!(edges.is_empty());
    }

    #[test]
    fn reverse_edge_index_maintained() {
        let mut graph = Graph::new();
        graph
            .add_node("p1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("f1".into(), Node::Family(FamilyData::default()))
            .unwrap();
        graph
            .add_node("f2".into(), Node::Family(FamilyData::default()))
            .unwrap();

        graph
            .add_edge(Edge::PersonFamily {
                source: "p1".into(),
                target: "f1".into(),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonFamily {
                source: "p1".into(),
                target: "f2".into(),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonFamily {
                source: "p2".into(),
                target: "f1".into(),
            })
            .unwrap();

        // p1 -> 2 edges (f1, f2)
        assert_eq!(graph.edges_from(&"p1".to_string()).len(), 2);
        // p2 -> 1 edge (f1)
        assert_eq!(graph.edges_from(&"p2".to_string()).len(), 1);
        // f1 <- 2 edges (p1, p2)
        assert_eq!(graph.edges_to(&"f1".to_string()).len(), 2);
        // f2 <- 1 edge (p1)
        assert_eq!(graph.edges_to(&"f2".to_string()).len(), 1);
    }

    #[test]
    fn add_node_empty_handle() {
        let mut graph = Graph::new();
        // Empty string handles are allowed syntactically (they're just strings)
        let result = graph.add_node("".into(), Node::Person(PersonData::default()));
        assert!(result.is_ok(), "Empty handle should be accepted");
        assert_eq!(graph.node_count(), 1);
    }

    #[test]
    fn add_node_after_validation_resets_state() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".into(),
                Node::Person(PersonData {
                    handle: "p1".to_string(),
                    gender: 1,
                    primary_name: Name {
                        first_name: Some("John".to_string()),
                        ..Name::default()
                    },
                    ..PersonData::default()
                }),
            )
            .unwrap();

        // Validate -> Valid
        let schema = Schema::default();
        graph.validate(&schema);
        assert_eq!(graph.validation_state(), &ValidationState::Valid);

        // Add a new node -> should reset to Unvalidated
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();
        assert_eq!(
            graph.validation_state(),
            &ValidationState::Unvalidated,
            "Adding a node should reset validation state to Unvalidated"
        );
    }

    #[test]
    fn add_edge_after_validation_resets_state() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".into(),
                Node::Person(PersonData {
                    handle: "p1".to_string(),
                    gender: 1,
                    primary_name: Name {
                        first_name: Some("John".to_string()),
                        ..Name::default()
                    },
                    ..PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                "f1".into(),
                Node::Family(FamilyData {
                    handle: "f1".to_string(),
                    ..FamilyData::default()
                }),
            )
            .unwrap();

        // Validate -> Valid
        let schema = Schema::default();
        graph.validate(&schema);
        assert_eq!(graph.validation_state(), &ValidationState::Valid);

        // Add an edge -> should reset to Unvalidated
        graph
            .add_edge(Edge::PersonFamily {
                source: "p1".into(),
                target: "f1".into(),
            })
            .unwrap();
        assert_eq!(
            graph.validation_state(),
            &ValidationState::Unvalidated,
            "Adding an edge should reset validation state to Unvalidated"
        );
    }
}
