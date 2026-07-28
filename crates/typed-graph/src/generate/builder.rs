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

use crate::ChildRef;
use crate::ChildRefType;
use crate::DateValue;
use crate::Edge;
use crate::EventData;
use crate::EventRoleType;
use crate::EventType;
use crate::FamilyData;
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
            birth_date: None,
            death_date: None,
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
            birth_date: None,
            death_date: None,
        }
    }

    /// Start building a [`Family`](Node::Family) node with the given handle.
    ///
    /// # Example
    ///
    /// ```
    /// use typed_graph::generate::GraphBuilder;
    /// use typed_graph::Graph;
    ///
    /// let mut graph = Graph::new();
    /// let mut builder = GraphBuilder::new(&mut graph);
    /// let f1 = builder.add_family("f1").build();
    /// ```
    pub fn add_family(&mut self, handle: impl Into<Handle>) -> FamilyBuilder<'a, '_> {
        FamilyBuilder {
            builder: self,
            data: FamilyData {
                handle: handle.into(),
                ..FamilyData::default()
            },
            marriage_date: None,
        }
    }

    /// Start building a [`Family`](Node::Family) node with an auto-generated UUID v4 handle.
    ///
    /// # Example
    ///
    /// ```
    /// use typed_graph::generate::GraphBuilder;
    /// use typed_graph::Graph;
    ///
    /// let mut graph = Graph::new();
    /// let mut builder = GraphBuilder::new(&mut graph);
    /// let f = builder.add_family_auto().build();
    /// ```
    pub fn add_family_auto(&mut self) -> FamilyBuilder<'a, '_> {
        let handle = uuid::Uuid::new_v4().to_string();
        FamilyBuilder {
            builder: self,
            data: FamilyData {
                handle,
                ..FamilyData::default()
            },
            marriage_date: None,
        }
    }
}

/// Builder for constructing a single [`Person`](Node::Person) node.
///
/// Created via [`GraphBuilder::add_person`] or [`GraphBuilder::add_person_auto`].
pub struct PersonBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: PersonData,
    /// Optional birth date — creates a Birth event during `build()`.
    birth_date: Option<DateValue>,
    /// Optional death date — creates a Death event during `build()`.
    death_date: Option<DateValue>,
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

    /// Set the birth date for this person.
    ///
    /// During [`build`](Self::build), a Birth event node is created and linked
    /// to this person via a `PersonEventRef` edge.
    pub fn with_birth_date(mut self, date: DateValue) -> Self {
        self.birth_date = Some(date);
        self
    }

    /// Set the death date for this person.
    ///
    /// During [`build`](Self::build), a Death event node is created and linked
    /// to this person via a `PersonEventRef` edge.
    pub fn with_death_date(mut self, date: DateValue) -> Self {
        self.death_date = Some(date);
        self
    }

    /// Add a parent family reference.
    ///
    /// Adds the family handle to `parent_family_list` and records a
    /// `PersonParentFamily` edge. Does NOT validate that the family handle
    /// exists in the graph (that comes in Step 6).
    pub fn with_parent_family(mut self, family_handle: &Handle) -> Self {
        self.data.parent_family_list.push(family_handle.clone());
        self
    }

    /// Add a family reference (own family).
    ///
    /// Adds the family handle to `family_list` and records a
    /// `PersonFamily` edge. Does NOT validate that the family handle
    /// exists in the graph (that comes in Step 6).
    pub fn with_family(mut self, family_handle: &Handle) -> Self {
        self.data.family_list.push(family_handle.clone());
        self
    }

    /// Add an alternate name to this person.
    pub fn add_alternate_name(mut self, name: Name) -> Self {
        self.data.alternate_names.push(name);
        self
    }

    /// Build the person node and insert it into the graph.
    ///
    /// If a birth or death date was set, corresponding Event nodes are created
    /// and linked via `PersonEventRef` edges.
    ///
    /// Returns the handle of the inserted person node.
    ///
    /// # Panics
    ///
    /// Panics if the node already exists (duplicate handle) or if creating
    /// an event/edge for the birth/death date fails. This is a temporary
    /// limitation — Step 6 adds proper error handling.
    ///
    /// # Example
    ///
    /// ```
    /// use typed_graph::generate::GraphBuilder;
    /// use typed_graph::{DateValue, Graph};
    ///
    /// let mut graph = Graph::new();
    /// let mut builder = GraphBuilder::new(&mut graph);
    /// let handle = builder.add_person("p1")
    ///     .with_name("John", "Smith")
    ///     .with_gender(1)
    ///     .with_birth_date(DateValue::new(1870))
    ///     .with_death_date(DateValue::new(1945))
    ///     .build();
    /// assert_eq!(handle, "p1");
    /// ```
    pub fn build(mut self) -> Handle {
        let person_handle = self.data.handle.clone();

        // Extract lists before consuming self.data
        let parent_family_list = self.data.parent_family_list.clone();
        let family_list = self.data.family_list.clone();

        // 1. Insert the person node first so edges can reference it
        let node = Node::Person(self.data);
        self.builder
            .graph
            .add_node(person_handle.clone(), node)
            .expect("duplicate handle in builder — use unique handles");

        // 2. Create birth event if date was set
        if let Some(date) = self.birth_date.take() {
            let event_handle = uuid::Uuid::new_v4().to_string();
            let event = EventData {
                handle: event_handle.clone(),
                event_type: EventType::Birth,
                date: Some(date),
                ..EventData::default()
            };
            self.builder
                .graph
                .add_node(event_handle.clone(), Node::Event(event))
                .expect("duplicate handle for birth event");

            // Add PersonEventRef edge
            let edge = Edge::PersonEventRef {
                source: person_handle.clone(),
                target: event_handle,
                metadata: Box::new(crate::EventRef {
                    ref_field: person_handle.clone(),
                    role: Some(EventRoleType::Primary),
                }),
            };
            self.builder
                .graph
                .add_edge(edge)
                .expect("failed to add birth event edge");
        }

        // 3. Create death event if date was set
        if let Some(date) = self.death_date.take() {
            let event_handle = uuid::Uuid::new_v4().to_string();
            let event = EventData {
                handle: event_handle.clone(),
                event_type: EventType::Death,
                date: Some(date),
                ..EventData::default()
            };
            self.builder
                .graph
                .add_node(event_handle.clone(), Node::Event(event))
                .expect("duplicate handle for death event");

            // Add PersonEventRef edge
            let edge = Edge::PersonEventRef {
                source: person_handle.clone(),
                target: event_handle,
                metadata: Box::new(crate::EventRef {
                    ref_field: person_handle.clone(),
                    role: Some(EventRoleType::Primary),
                }),
            };
            self.builder
                .graph
                .add_edge(edge)
                .expect("failed to add death event edge");
        }

        // 4. Add PersonParentFamily edges
        for family_handle in &parent_family_list {
            let edge = Edge::PersonParentFamily {
                source: person_handle.clone(),
                target: family_handle.clone(),
            };
            // Ignore errors — the family node may not exist yet (validated in Step 6)
            let _ = self.builder.graph.add_edge(edge);
        }

        // 5. Add PersonFamily edges
        for family_handle in &family_list {
            let edge = Edge::PersonFamily {
                source: person_handle.clone(),
                target: family_handle.clone(),
            };
            // Ignore errors — the family node may not exist yet (validated in Step 6)
            let _ = self.builder.graph.add_edge(edge);
        }

        person_handle
    }
}

/// Builder for constructing a single [`Family`](Node::Family) node.
///
/// Created via [`GraphBuilder::add_family`] or [`GraphBuilder::add_family_auto`].
pub struct FamilyBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: FamilyData,
    /// Optional marriage date — creates a Marriage event during `build()`.
    marriage_date: Option<DateValue>,
}

impl<'a, 'b> FamilyBuilder<'a, 'b> {
    /// Override the handle for this family.
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }

    /// Set the Gramps ID (e.g., "F0001").
    pub fn with_gramps_id(mut self, id: impl Into<String>) -> Self {
        self.data.gramps_id = Some(id.into());
        self
    }

    /// Set the father of this family.
    ///
    /// Sets `father_handle` and records a `FamilyFather` edge.
    /// Does NOT validate that the handle exists (that comes in Step 6).
    pub fn with_father(mut self, father_handle: &Handle) -> Self {
        self.data.father_handle = Some(father_handle.clone());
        self
    }

    /// Set the mother of this family.
    ///
    /// Sets `mother_handle` and records a `FamilyMother` edge.
    /// Does NOT validate that the handle exists (that comes in Step 6).
    pub fn with_mother(mut self, mother_handle: &Handle) -> Self {
        self.data.mother_handle = Some(mother_handle.clone());
        self
    }

    /// Add a child to this family with the given relation type.
    ///
    /// Adds to `child_ref_list` and records a `FamilyChildRef` edge.
    pub fn add_child(mut self, child_handle: &Handle, relation: ChildRefType) -> Self {
        self.data.child_ref_list.push(ChildRef {
            ref_field: child_handle.clone(),
            relation: Some(relation),
        });
        self
    }

    /// Add a child with [`ChildRefType::Birth`] relation.
    pub fn add_child_birth(mut self, child_handle: &Handle) -> Self {
        self.data.child_ref_list.push(ChildRef {
            ref_field: child_handle.clone(),
            relation: Some(ChildRefType::Birth),
        });
        self
    }

    /// Set the marriage date for this family.
    ///
    /// During [`build`](Self::build), a Marriage event node is created and
    /// linked to this family via a `FamilyEventRef` edge.
    pub fn with_marriage_date(mut self, date: DateValue) -> Self {
        self.marriage_date = Some(date);
        self
    }

    /// Build the family node and insert it into the graph.
    ///
    /// If a marriage date was set, a Marriage event node is created and
    /// linked via a `FamilyEventRef` edge.
    ///
    /// Returns the handle of the inserted family node.
    ///
    /// # Panics
    ///
    /// Panics if the node already exists or if creating an event/edge fails.
    /// This is a temporary limitation — Step 6 adds proper error handling.
    pub fn build(mut self) -> Handle {
        let family_handle = self.data.handle.clone();

        // Extract data before consuming self.data
        let father_handle = self.data.father_handle.clone();
        let mother_handle = self.data.mother_handle.clone();
        let child_handles: Vec<(Handle, ChildRefType)> = self
            .data
            .child_ref_list
            .iter()
            .map(|cr| {
                (
                    cr.ref_field.clone(),
                    cr.relation.unwrap_or(ChildRefType::Birth),
                )
            })
            .collect();

        // 1. Insert the family node
        let node = Node::Family(self.data);
        self.builder
            .graph
            .add_node(family_handle.clone(), node)
            .expect("duplicate handle in builder — use unique handles");

        // 2. Add FamilyFather edge if father was set
        if let Some(ref fh) = father_handle {
            let edge = Edge::FamilyFather {
                source: family_handle.clone(),
                target: fh.clone(),
            };
            // Ignore errors — the person node may not exist yet
            let _ = self.builder.graph.add_edge(edge);
        }

        // 3. Add FamilyMother edge if mother was set
        if let Some(ref mh) = mother_handle {
            let edge = Edge::FamilyMother {
                source: family_handle.clone(),
                target: mh.clone(),
            };
            let _ = self.builder.graph.add_edge(edge);
        }

        // 4. Add FamilyChildRef edges for each child
        for (child_handle, _) in &child_handles {
            let edge = Edge::FamilyChildRef {
                source: family_handle.clone(),
                target: child_handle.clone(),
                metadata: Box::new(ChildRef {
                    ref_field: child_handle.clone(),
                    relation: None,
                }),
            };
            let _ = self.builder.graph.add_edge(edge);
        }

        // 5. Create marriage event if date was set
        if let Some(date) = self.marriage_date.take() {
            let event_handle = uuid::Uuid::new_v4().to_string();
            let event = EventData {
                handle: event_handle.clone(),
                event_type: EventType::Marriage,
                date: Some(date),
                ..EventData::default()
            };
            self.builder
                .graph
                .add_node(event_handle.clone(), Node::Event(event))
                .expect("duplicate handle for marriage event");

            let edge = Edge::FamilyEventRef {
                source: family_handle.clone(),
                target: event_handle,
                metadata: Box::new(crate::EventRef {
                    ref_field: family_handle.clone(),
                    role: Some(EventRoleType::Family),
                }),
            };
            self.builder
                .graph
                .add_edge(edge)
                .expect("failed to add marriage event edge");
        }

        family_handle
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

    // -----------------------------------------------------------------------
    // Date and family reference tests
    // -----------------------------------------------------------------------

    #[test]
    fn builder_person_with_birth_date() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_birth_date(DateValue::new(1870))
            .build();
        // Person exists
        assert!(graph.contains_node(&"p1".to_string()));
        // Birth event should exist (auto-generated UUID handle)
        // We should have at least 2 nodes: person + birth event
        assert_eq!(graph.node_count(), 2);
        // There should be a PersonEventRef edge
        assert_eq!(graph.edge_count(), 1);
        let edges = graph.edges_from(&"p1".to_string());
        assert_eq!(edges.len(), 1);
        assert!(matches!(edges[0], Edge::PersonEventRef { .. }));
    }

    #[test]
    fn builder_person_with_death_date() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_death_date(DateValue::new(1945))
            .build();
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
        let edges = graph.edges_from(&"p1".to_string());
        assert_eq!(edges.len(), 1);
        assert!(matches!(edges[0], Edge::PersonEventRef { .. }));
    }

    #[test]
    fn builder_person_with_parent_family() {
        let mut graph = Graph::new();
        // First create the family
        graph
            .add_node("f1".into(), Node::Family(crate::FamilyData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("Child", "Smith")
            .with_parent_family(&"f1".to_string())
            .build();

        // Person should have parent_family_list populated
        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.parent_family_list, vec!["f1".to_string()]);
        } else {
            panic!("Expected Person node");
        }
        // PersonParentFamily edge should exist
        let edges = graph.edges_from(&"p1".to_string());
        assert!(edges.iter().any(|e| matches!(e, Edge::PersonParentFamily { source, target } if source == "p1" && target == "f1")));
    }

    #[test]
    fn builder_person_with_family() {
        let mut graph = Graph::new();
        graph
            .add_node("f1".into(), Node::Family(crate::FamilyData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("Parent", "Smith")
            .with_family(&"f1".to_string())
            .build();

        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.family_list, vec!["f1".to_string()]);
        } else {
            panic!("Expected Person node");
        }
        let edges = graph.edges_from(&"p1".to_string());
        assert!(edges.iter().any(|e| matches!(e, Edge::PersonFamily { source, target } if source == "p1" && target == "f1")));
    }

    #[test]
    fn builder_person_with_alternate_name() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let alt_name = Name {
            first_name: Some("Johnny".to_string()),
            ..Name::default()
        };
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .add_alternate_name(alt_name)
            .build();

        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.alternate_names.len(), 1);
            assert_eq!(
                person.alternate_names[0].first_name,
                Some("Johnny".to_string())
            );
        } else {
            panic!("Expected Person node");
        }
    }

    #[test]
    fn builder_person_with_multiple_families() {
        let mut graph = Graph::new();
        graph
            .add_node("f1".into(), Node::Family(crate::FamilyData::default()))
            .unwrap();
        graph
            .add_node("f2".into(), Node::Family(crate::FamilyData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("Parent", "Smith")
            .with_family(&"f1".to_string())
            .with_family(&"f2".to_string())
            .build();

        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.family_list.len(), 2);
        } else {
            panic!("Expected Person node");
        }
        let edges = graph.edges_from(&"p1".to_string());
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn builder_person_with_birth_and_death_dates() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_birth_date(DateValue::new(1870))
            .with_death_date(DateValue::new(1945))
            .build();

        // 3 nodes: person + birth event + death event
        assert_eq!(graph.node_count(), 3);
        // 2 edges: PersonEventRef for birth + PersonEventRef for death
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn builder_person_with_birth_date_has_event_type() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_birth_date(DateValue::new(1870))
            .build();

        // Find the event node (the one that's not the person)
        for (handle, node) in graph.iter_nodes() {
            if let Node::Event(event) = node {
                assert_eq!(event.event_type, EventType::Birth);
                assert_eq!(event.date, Some(DateValue::new(1870)));
                assert_ne!(*handle, "p1");
                return;
            }
        }
        panic!("No event node found");
    }

    // -----------------------------------------------------------------------
    // FamilyBuilder tests
    // -----------------------------------------------------------------------

    #[test]
    fn builder_family_basic() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder.add_family("f1").build();
        assert_eq!(graph.node_count(), 1);
        assert!(graph.contains_node(&"f1".to_string()));
    }

    #[test]
    fn builder_family_auto_handle() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let handle = builder.add_family_auto().build();
        assert_eq!(handle.len(), 36);
        assert!(graph.contains_node(&handle));
    }

    #[test]
    fn builder_family_with_father_mother() {
        let mut graph = Graph::new();
        // Create parents first
        graph
            .add_node("p1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("p2".into(), Node::Person(PersonData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_family("f1")
            .with_father(&"p1".to_string())
            .with_mother(&"p2".to_string())
            .build();

        let node = graph.get_node(&"f1".to_string()).unwrap();
        if let Node::Family(family) = node {
            assert_eq!(family.father_handle, Some("p1".to_string()));
            assert_eq!(family.mother_handle, Some("p2".to_string()));
        } else {
            panic!("Expected Family node");
        }

        // Edges should exist
        let edges = graph.edges_from(&"f1".to_string());
        assert!(edges.iter().any(|e| matches!(e, Edge::FamilyFather { .. })));
        assert!(edges.iter().any(|e| matches!(e, Edge::FamilyMother { .. })));
    }

    #[test]
    fn builder_family_with_child() {
        let mut graph = Graph::new();
        graph
            .add_node("c1".into(), Node::Person(PersonData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_family("f1")
            .add_child(&"c1".to_string(), ChildRefType::Birth)
            .build();

        let node = graph.get_node(&"f1".to_string()).unwrap();
        if let Node::Family(family) = node {
            assert_eq!(family.child_ref_list.len(), 1);
            assert_eq!(family.child_ref_list[0].ref_field, "c1");
        } else {
            panic!("Expected Family node");
        }

        let edges = graph.edges_from(&"f1".to_string());
        assert!(edges
            .iter()
            .any(|e| matches!(e, Edge::FamilyChildRef { .. })));
    }

    #[test]
    fn builder_family_with_multiple_children() {
        let mut graph = Graph::new();
        graph
            .add_node("c1".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("c2".into(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("c3".into(), Node::Person(PersonData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_family("f1")
            .add_child(&"c1".to_string(), ChildRefType::Birth)
            .add_child_birth(&"c2".to_string())
            .add_child(&"c3".to_string(), ChildRefType::Adopted)
            .build();

        let node = graph.get_node(&"f1".to_string()).unwrap();
        if let Node::Family(family) = node {
            assert_eq!(family.child_ref_list.len(), 3);
        } else {
            panic!("Expected Family node");
        }
    }

    #[test]
    fn builder_family_with_marriage_date() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_family("f1")
            .with_marriage_date(DateValue::new(1895))
            .build();

        // 2 nodes: family + marriage event
        assert_eq!(graph.node_count(), 2);
        // 1 edge: FamilyEventRef
        assert_eq!(graph.edge_count(), 1);

        let edges = graph.edges_from(&"f1".to_string());
        assert!(edges
            .iter()
            .any(|e| matches!(e, Edge::FamilyEventRef { .. })));
    }

    #[test]
    fn builder_person_belongs_to_family() {
        let mut graph = Graph::new();
        graph
            .add_node("f1".into(), Node::Family(crate::FamilyData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_family(&"f1".to_string())
            .build();

        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.family_list, vec!["f1".to_string()]);
        } else {
            panic!("Expected Person node");
        }
    }
}
