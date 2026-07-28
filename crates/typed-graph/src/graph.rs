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
    /// Edge index: source → (link_type, target).
    edges: Vec<Edge>,
    /// Reverse edge index: target → [edge_index].
    reverse_edges: HashMap<Handle, Vec<usize>>,
    /// Validation state set by the last validation pass.
    validation_state: ValidationState,
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
        self.reverse_edges.entry(target).or_default().push(index);
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
}

/// Extract the source and target handles from any [`Edge`] variant.
fn edge_source_target(edge: &Edge) -> (Handle, Handle) {
    match edge {
        Edge::CitationMediaRef { source, target }
        | Edge::CitationNote { source, target }
        | Edge::CitationSource { source, target }
        | Edge::CitationTag { source, target }
        | Edge::CitationRef { source, target }
        | Edge::NoteRef { source, target }
        | Edge::MediaRef { source, target }
        | Edge::TagRef { source, target }
        | Edge::EventCitation { source, target }
        | Edge::EventMediaRef { source, target }
        | Edge::EventNote { source, target }
        | Edge::EventPlace { source, target }
        | Edge::EventTag { source, target }
        | Edge::FamilyCitation { source, target }
        | Edge::FamilyFather { source, target }
        | Edge::FamilyMediaRef { source, target }
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
        | Edge::PersonMediaRef { source, target }
        | Edge::PersonNote { source, target }
        | Edge::PersonParentFamily { source, target }
        | Edge::PersonTag { source, target }
        | Edge::PlaceCitation { source, target }
        | Edge::PlaceMediaRef { source, target }
        | Edge::PlaceNote { source, target }
        | Edge::PlacePlaceRef { source, target }
        | Edge::PlaceTag { source, target }
        | Edge::RepositoryMediaRef { source, target }
        | Edge::RepositoryNote { source, target }
        | Edge::RepositoryTag { source, target }
        | Edge::SourceMediaRef { source, target }
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
        link: &'static str,
        target: Handle,
    },
    /// A required field is missing (e.g., Person with no primary_name).
    MissingRequired { node: Handle, field: &'static str },
    /// A cardinality constraint is violated.
    CardinalityViolation {
        node: Handle,
        field: &'static str,
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

#[cfg(test)]
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
            field: "primary_name",
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
            field: "primary_name",
        };
        let msg = format!("{}", err);
        assert!(msg.contains("p1"));
        assert!(msg.contains("primary_name"));
    }
}
