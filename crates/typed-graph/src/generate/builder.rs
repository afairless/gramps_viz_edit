//! GraphBuilder — fluent builder API for constructing graphs programmatically.
//!
//! This module provides [`GraphBuilder`], a separate struct from [`Graph`] that
//! offers a chainable, fluent API for building graphs. It produces the same
//! [`Graph`] type as the random generator, so all graphs pass through the same
//! validation pipeline regardless of how they were constructed.
//!
//! # Example
//!
//! ```
//! use typed_graph::generate::GraphBuilder;
//! use typed_graph::Graph;
//!
//! let mut graph = Graph::new();
//! let mut builder = GraphBuilder::new(&mut graph);
//!
//! let p1 = builder.add_person("p1")
//!     .with_name("John", "Smith")
//!     .with_gender(1)
//!     .build();
//! ```

use crate::Graph;
use crate::Handle;
use crate::Name;
use crate::Node;
use crate::PersonData;
use crate::Surname;

/// Fluent builder for constructing a [`Graph`] programmatically.
///
/// Takes a `&mut Graph` reference and adds nodes/edges to it via
/// type-specific sub-builders.
///
/// # Example
///
/// ```
/// use typed_graph::generate::GraphBuilder;
/// use typed_graph::Graph;
///
/// let mut graph = Graph::new();
/// let mut builder = GraphBuilder::new(&mut graph);
/// ```
pub struct GraphBuilder<'a> {
    graph: &'a mut Graph,
}

impl<'a> GraphBuilder<'a> {
    /// Create a new [`GraphBuilder`] that writes into the given [`Graph`].
    pub fn new(graph: &'a mut Graph) -> Self {
        GraphBuilder { graph }
    }

    /// Consume the builder and return the inner graph reference.
    ///
    /// Useful after building to pass the graph to validation or serialization.
    pub fn into_graph(self) -> &'a mut Graph {
        self.graph
    }

    /// Start building a [`Person`](Node::Person) node with the given handle.
    ///
    /// The handle can be any type that implements `Into<Handle>` (e.g., `&str`,
    /// `String`).
    ///
    /// # Example
    ///
    /// ```
    /// use typed_graph::generate::GraphBuilder;
    /// use typed_graph::Graph;
    ///
    /// let mut graph = Graph::new();
    /// let mut builder = GraphBuilder::new(&mut graph);
    /// let p1 = builder.add_person("p1")
    ///     .with_name("John", "Smith")
    ///     .with_gender(1)
    ///     .build();
    /// ```
    pub fn add_person(&mut self, handle: impl Into<Handle>) -> PersonBuilder<'a, '_> {
        PersonBuilder {
            builder: self,
            data: PersonData {
                handle: handle.into(),
                ..PersonData::default()
            },
        }
    }

    /// Start building a [`Person`](Node::Person) node with an auto-generated UUID v4 handle.
    ///
    /// # Example
    ///
    /// ```
    /// use typed_graph::generate::GraphBuilder;
    /// use typed_graph::Graph;
    ///
    /// let mut graph = Graph::new();
    /// let mut builder = GraphBuilder::new(&mut graph);
    /// let p = builder.add_person_auto()
    ///     .with_name("Jane", "Doe")
    ///     .build();
    /// ```
    pub fn add_person_auto(&mut self) -> PersonBuilder<'a, '_> {
        let handle = uuid::Uuid::new_v4().to_string();
        PersonBuilder {
            builder: self,
            data: PersonData {
                handle,
                ..PersonData::default()
            },
        }
    }
}

/// Builder for constructing a single [`Person`](Node::Person) node.
///
/// Created via [`GraphBuilder::add_person`] or [`GraphBuilder::add_person_auto`].
pub struct PersonBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: PersonData,
}

impl<'a, 'b> PersonBuilder<'a, 'b> {
    /// Override the handle for this person.
    ///
    /// Useful when using `add_person_auto` to get an auto-generated handle
    /// but then override it with a specific one.
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }

    /// Set the Gramps ID (e.g., "I0001").
    pub fn with_gramps_id(mut self, id: impl Into<String>) -> Self {
        self.data.gramps_id = Some(id.into());
        self
    }

    /// Set the gender as an integer value (0-3).
    ///
    /// Matches the codegen Gender enum: 0=Male, 1=Female, 2=Unknown, 3=Other.
    pub fn with_gender(mut self, gender: i32) -> Self {
        self.data.gender = gender;
        self
    }

    /// Set the primary name from a given name and surname.
    ///
    /// Creates a [`Name`] struct with the given first name and a single
    /// [`Surname`] entry.
    pub fn with_name(mut self, first_name: impl Into<String>, surname: impl Into<String>) -> Self {
        self.data.primary_name = Name {
            first_name: Some(first_name.into()),
            surname_list: vec![Surname {
                surname: Some(surname.into()),
                ..Surname::default()
            }],
            ..Name::default()
        };
        self
    }

    /// Set the primary name directly from a [`Name`] struct.
    ///
    /// Use this for full control over the name fields (title, suffix, etc.).
    pub fn with_primary_name(mut self, name: Name) -> Self {
        self.data.primary_name = name;
        self
    }

    /// Build the person node and insert it into the graph.
    ///
    /// Returns the handle of the inserted node.
    ///
    /// # Panics
    ///
    /// Panics if the node already exists (duplicate handle). This is a
    /// temporary limitation — Step 6 adds proper error handling.
    ///
    /// # Example
    ///
    /// ```
    /// use typed_graph::generate::GraphBuilder;
    /// use typed_graph::Graph;
    ///
    /// let mut graph = Graph::new();
    /// let mut builder = GraphBuilder::new(&mut graph);
    /// let handle = builder.add_person("p1")
    ///     .with_name("John", "Smith")
    ///     .with_gender(1)
    ///     .build();
    /// assert_eq!(handle, "p1");
    /// ```
    pub fn build(self) -> Handle {
        let handle = self.data.handle.clone();
        let node = Node::Person(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .expect("duplicate handle in builder — use unique handles");
        handle
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Graph;

    #[test]
    fn builder_new_is_empty() {
        let mut graph = Graph::new();
        let _builder = GraphBuilder::new(&mut graph);
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn builder_add_person_basic() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_gender(1)
            .build();
        assert_eq!(graph.node_count(), 1);
        let node = graph.get_node(&"p1".to_string());
        assert!(node.is_some());
        if let Some(Node::Person(person)) = node {
            assert_eq!(person.handle, "p1");
            assert_eq!(person.gender, 1);
            assert_eq!(person.primary_name.first_name, Some("John".to_string()));
            assert_eq!(
                person.primary_name.surname_list[0].surname,
                Some("Smith".to_string())
            );
        } else {
            panic!("Expected Person node");
        }
    }

    #[test]
    fn builder_add_person_auto_handle() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let handle = builder.add_person_auto().with_name("Auto", "Gen").build();
        // UUID v4 is 36 characters
        assert_eq!(handle.len(), 36);
        assert!(graph.contains_node(&handle));
    }

    #[test]
    fn builder_build_returns_handle() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let handle = builder.add_person("p1").with_name("John", "Smith").build();
        assert_eq!(handle, "p1");
    }

    #[test]
    fn builder_add_person_with_gramps_id() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_gramps_id("I0001")
            .build();
        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.gramps_id, Some("I0001".to_string()));
        } else {
            panic!("Expected Person node");
        }
    }

    #[test]
    fn builder_into_graph_returns_graph() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder.add_person("p1").with_name("John", "Smith").build();
        let graph_ref = builder.into_graph();
        assert_eq!(graph_ref.node_count(), 1);
    }

    #[test]
    fn builder_add_multiple_persons() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder.add_person("p1").with_name("John", "Smith").build();
        builder.add_person("p2").with_name("Jane", "Doe").build();
        assert_eq!(graph.node_count(), 2);
        assert!(graph.contains_node(&"p1".to_string()));
        assert!(graph.contains_node(&"p2".to_string()));
    }

    #[test]
    fn builder_person_with_primary_name_struct() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let name = Name {
            first_name: Some("Robert".to_string()),
            surname_list: vec![Surname {
                surname: Some("Johnson".to_string()),
                ..Surname::default()
            }],
            title: Some("Dr.".to_string()),
            ..Name::default()
        };
        builder
            .add_person("p1")
            .with_primary_name(name)
            .with_gender(1)
            .build();
        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.primary_name.first_name, Some("Robert".to_string()));
            assert_eq!(person.primary_name.title, Some("Dr.".to_string()));
        } else {
            panic!("Expected Person node");
        }
    }

    #[test]
    fn builder_person_with_handle_override() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let handle = builder
            .add_person("temp")
            .with_handle("custom")
            .with_name("Custom", "Handle")
            .build();
        assert_eq!(handle, "custom");
        assert!(graph.contains_node(&"custom".to_string()));
        assert!(!graph.contains_node(&"temp".to_string()));
    }

    #[test]
    #[should_panic(expected = "duplicate handle")]
    fn builder_duplicate_handle_panics() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder.add_person("p1").with_name("John", "Smith").build();
        builder.add_person("p1").with_name("Jane", "Doe").build();
    }
}
