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
//!     .build()
//!     .unwrap();
//! ```

use crate::ChildRefType;
use crate::CitationData;
use crate::DateValue;
use crate::Edge;
use crate::EventData;
use crate::EventRoleType;
use crate::EventType;
use crate::FamilyData;
use crate::Graph;
use crate::Handle;
use crate::Location;
use crate::MediaData;
use crate::Name;
use crate::Node;
use crate::NoteData;
use crate::PersonData;
use crate::PlaceData;
use crate::RepositoryData;
use crate::SourceData;
use crate::Surname;
use crate::TagData;

// ---------------------------------------------------------------------------
// BuilderError
// ---------------------------------------------------------------------------

/// Errors that can occur during graph construction via the builder API.
#[derive(Clone, Debug, PartialEq)]
pub enum BuilderError {
    /// A required field was not set before calling `build()`.
    MissingRequiredField {
        /// The builder type (e.g., "Person", "Family").
        builder_type: &'static str,
        /// The missing field name.
        field: &'static str,
    },
    /// A handle reference does not point to an existing node in the graph.
    InvalidHandle {
        /// The builder type.
        builder_type: &'static str,
        /// The handle that was not found.
        handle: Handle,
        /// The target type description.
        target_type: &'static str,
    },
    /// A node with this handle already exists in the graph.
    DuplicateHandle(Handle),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuilderError::MissingRequiredField {
                builder_type,
                field,
            } => {
                write!(
                    f,
                    "{} builder: missing required field '{}' — set it before calling build()",
                    builder_type, field
                )
            }
            BuilderError::InvalidHandle {
                builder_type,
                handle,
                target_type,
            } => {
                write!(
                    f,
                    "{} builder: handle '{}' does not exist in the graph (expected {})",
                    builder_type, handle, target_type
                )
            }
            BuilderError::DuplicateHandle(h) => {
                write!(f, "duplicate handle: '{}' already exists in the graph", h)
            }
        }
    }
}

impl std::error::Error for BuilderError {}

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
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn add_person(&mut self, handle: impl Into<Handle>) -> PersonBuilder<'a, '_> {
        PersonBuilder {
            builder: self,
            data: PersonData {
                handle: handle.into(),
                gender: crate::into_gender_field(2), // Unknown
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
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn add_person_auto(&mut self) -> PersonBuilder<'a, '_> {
        let handle = uuid::Uuid::new_v4().to_string();
        PersonBuilder {
            builder: self,
            data: PersonData {
                handle,
                gender: crate::into_gender_field(2), // Unknown
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
    /// let f1 = builder.add_family("f1").build().unwrap();
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
    /// let f = builder.add_family_auto().build().unwrap();
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

    /// Start building an [`Event`](Node::Event) node with the given handle.
    pub fn add_event(&mut self, handle: impl Into<Handle>) -> EventBuilder<'a, '_> {
        EventBuilder {
            builder: self,
            data: EventData {
                handle: handle.into(),
                ..EventData::default()
            },
        }
    }

    /// Start building a [`Place`](Node::Place) node with the given handle.
    pub fn add_place(&mut self, handle: impl Into<Handle>) -> PlaceBuilder<'a, '_> {
        PlaceBuilder {
            builder: self,
            data: PlaceData {
                handle: handle.into(),
                ..PlaceData::default()
            },
        }
    }

    /// Start building a [`Source`](Node::Source) node with the given handle.
    pub fn add_source(&mut self, handle: impl Into<Handle>) -> SourceBuilder<'a, '_> {
        SourceBuilder {
            builder: self,
            data: SourceData {
                handle: handle.into(),
                ..SourceData::default()
            },
        }
    }

    /// Start building a [`Citation`](Node::Citation) node with the given handle.
    pub fn add_citation(&mut self, handle: impl Into<Handle>) -> CitationBuilder<'a, '_> {
        CitationBuilder {
            builder: self,
            data: CitationData {
                handle: handle.into(),
                ..CitationData::default()
            },
        }
    }

    /// Start building a [`Note`](Node::Note) node with the given handle.
    pub fn add_note(&mut self, handle: impl Into<Handle>) -> NoteBuilder<'a, '_> {
        NoteBuilder {
            builder: self,
            data: NoteData {
                handle: handle.into(),
                ..NoteData::default()
            },
        }
    }

    /// Start building a [`Media`](Node::Media) node with the given handle.
    pub fn add_media(&mut self, handle: impl Into<Handle>) -> MediaBuilder<'a, '_> {
        MediaBuilder {
            builder: self,
            data: MediaData {
                handle: handle.into(),
                ..MediaData::default()
            },
        }
    }

    /// Start building a [`Repository`](Node::Repository) node with the given handle.
    pub fn add_repository(&mut self, handle: impl Into<Handle>) -> RepositoryBuilder<'a, '_> {
        RepositoryBuilder {
            builder: self,
            data: RepositoryData {
                handle: handle.into(),
                ..RepositoryData::default()
            },
        }
    }

    /// Start building a [`Tag`](Node::Tag) node with the given handle.
    pub fn add_tag(&mut self, handle: impl Into<Handle>) -> TagBuilder<'a, '_> {
        TagBuilder {
            builder: self,
            data: TagData {
                handle: handle.into(),
                ..TagData::default()
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
        crate::set_gender(&mut self.data, gender);
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
    /// Returns `Ok(handle)` on success, or `Err(BuilderError)` if:
    /// - The handle is empty (use [`with_handle`](Self::with_handle) or a non-empty handle)
    /// - The gender is out of range (must be 0-3)
    /// - No name was set (use [`with_name`](Self::with_name) or [`with_primary_name`](Self::with_primary_name))
    /// - A referenced family handle does not exist in the graph
    /// - The handle already exists (duplicate)
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
    ///     .build()
    ///     .unwrap();
    /// assert_eq!(handle, "p1");
    /// ```
    pub fn build(self) -> Result<Handle, BuilderError> {
        let person_handle = self.data.handle.clone();

        // Validate required fields
        if person_handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Person",
                field: "handle",
            });
        }
        if !crate::is_gender_valid(&self.data.gender) {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Person",
                field: "gender (must be 0-3)",
            });
        }
        let has_name = self.data.primary_name.first_name.is_some()
            || !self.data.primary_name.surname_list.is_empty();
        if !has_name {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Person",
                field: "primary_name (must have at least a first name or surname)",
            });
        }

        // Validate handle references resolve
        for fh in &self.data.parent_family_list {
            if !self.builder.graph.contains_node(fh) {
                return Err(BuilderError::InvalidHandle {
                    builder_type: "Person",
                    handle: fh.clone(),
                    target_type: "Family",
                });
            }
        }
        for fh in &self.data.family_list {
            if !self.builder.graph.contains_node(fh) {
                return Err(BuilderError::InvalidHandle {
                    builder_type: "Person",
                    handle: fh.clone(),
                    target_type: "Family",
                });
            }
        }

        // Extract lists before consuming self.data
        let parent_family_list = self.data.parent_family_list.clone();
        let family_list = self.data.family_list.clone();

        // 1. Insert the person node first so edges can reference it
        let node = Node::Person(self.data);
        self.builder
            .graph
            .add_node(person_handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(person_handle.clone()),
            })?;

        // 2. Create birth event if date was set
        if let Some(date) = self.birth_date {
            let event_handle = uuid::Uuid::new_v4().to_string();
            let event = EventData {
                handle: event_handle.clone(),
                event_type: crate::into_event_type_field(EventType::Birth),
                date: Some(date),
                ..EventData::default()
            };
            self.builder
                .graph
                .add_node(event_handle.clone(), Node::Event(event))
                .map_err(|_| BuilderError::DuplicateHandle(event_handle.clone()))?;

            let edge = Edge::PersonEventRef {
                source: person_handle.clone(),
                target: event_handle,
                metadata: Box::new(crate::make_event_ref(
                    person_handle.clone(),
                    Some(EventRoleType::Primary),
                )),
            };
            self.builder
                .graph
                .add_edge(edge)
                .expect("birth event target exists (just added)");
        }

        // 3. Create death event if date was set
        if let Some(date) = self.death_date {
            let event_handle = uuid::Uuid::new_v4().to_string();
            let event = EventData {
                handle: event_handle.clone(),
                event_type: crate::into_event_type_field(EventType::Death),
                date: Some(date),
                ..EventData::default()
            };
            self.builder
                .graph
                .add_node(event_handle.clone(), Node::Event(event))
                .map_err(|_| BuilderError::DuplicateHandle(event_handle.clone()))?;

            let edge = Edge::PersonEventRef {
                source: person_handle.clone(),
                target: event_handle,
                metadata: Box::new(crate::make_event_ref(
                    person_handle.clone(),
                    Some(EventRoleType::Primary),
                )),
            };
            self.builder
                .graph
                .add_edge(edge)
                .expect("death event target exists (just added)");
        }

        // 4. Add PersonParentFamily edges (already validated above)
        for family_handle in &parent_family_list {
            let edge = Edge::PersonParentFamily {
                source: person_handle.clone(),
                target: family_handle.clone(),
            };
            let _ = self.builder.graph.add_edge(edge);
        }

        // 5. Add PersonFamily edges (already validated above)
        for family_handle in &family_list {
            let edge = Edge::PersonFamily {
                source: person_handle.clone(),
                target: family_handle.clone(),
            };
            let _ = self.builder.graph.add_edge(edge);
        }

        Ok(person_handle)
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
        self.data.child_ref_list.push(crate::make_child_ref(
            child_handle.clone(),
            Some(relation),
        ));
        self
    }

    /// Add a child with [`ChildRefType::Birth`] relation.
    pub fn add_child_birth(mut self, child_handle: &Handle) -> Self {
        self.data.child_ref_list.push(crate::make_child_ref(
            child_handle.clone(),
            Some(ChildRefType::Birth),
        ));
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
    /// Returns `Ok(handle)` on success, or `Err(BuilderError)` if:
    /// - The handle is empty
    /// - A referenced father, mother, or child handle does not exist in the graph
    /// - The handle already exists
    pub fn build(self) -> Result<Handle, BuilderError> {
        let family_handle = self.data.handle.clone();

        // Validate required fields
        if family_handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Family",
                field: "handle",
            });
        }

        // Validate that referenced person nodes exist
        if let Some(ref fh) = self.data.father_handle {
            if !self.builder.graph.contains_node(fh) {
                return Err(BuilderError::InvalidHandle {
                    builder_type: "Family",
                    handle: fh.clone(),
                    target_type: "Person (father)",
                });
            }
        }
        if let Some(ref mh) = self.data.mother_handle {
            if !self.builder.graph.contains_node(mh) {
                return Err(BuilderError::InvalidHandle {
                    builder_type: "Family",
                    handle: mh.clone(),
                    target_type: "Person (mother)",
                });
            }
        }
        for cr in &self.data.child_ref_list {
            if !self.builder.graph.contains_node(&cr.ref_field) {
                return Err(BuilderError::InvalidHandle {
                    builder_type: "Family",
                    handle: cr.ref_field.clone(),
                    target_type: "Person (child)",
                });
            }
        }

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
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(family_handle.clone()),
            })?;

        // 2. Add FamilyFather edge if father was set
        if let Some(ref fh) = father_handle {
            let edge = Edge::FamilyFather {
                source: family_handle.clone(),
                target: fh.clone(),
            };
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
                metadata: Box::new(crate::make_child_ref(
                    child_handle.clone(),
                    None,
                )),
            };
            let _ = self.builder.graph.add_edge(edge);
        }

        // 5. Create marriage event if date was set
        if let Some(date) = self.marriage_date {
            let event_handle = uuid::Uuid::new_v4().to_string();
            let event = EventData {
                handle: event_handle.clone(),
                event_type: crate::into_event_type_field(EventType::Marriage),
                date: Some(date),
                ..EventData::default()
            };
            self.builder
                .graph
                .add_node(event_handle.clone(), Node::Event(event))
                .map_err(|_| BuilderError::DuplicateHandle(event_handle.clone()))?;

            let edge = Edge::FamilyEventRef {
                source: family_handle.clone(),
                target: event_handle,
                metadata: Box::new(crate::make_event_ref(
                    family_handle.clone(),
                    Some(EventRoleType::Family),
                )),
            };
            self.builder
                .graph
                .add_edge(edge)
                .expect("marriage event target exists (just added)");
        }

        Ok(family_handle)
    }
}

// =======================================================================
// Remaining primary type builders
// =======================================================================

/// Builder for constructing a single [`Event`](Node::Event) node.
pub struct EventBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: EventData,
}

impl<'a, 'b> EventBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn with_gramps_id(mut self, id: impl Into<String>) -> Self {
        self.data.gramps_id = Some(id.into());
        self
    }
    pub fn with_event_type(mut self, event_type: EventType) -> Self {
        self.data.event_type = crate::into_event_type_field(event_type);
        self
    }
    pub fn with_date(mut self, date: DateValue) -> Self {
        self.data.date = Some(date);
        self
    }
    pub fn with_place(mut self, place_handle: &Handle) -> Self {
        self.data.place_handle = Some(place_handle.clone());
        self
    }
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.data.description = Some(description.into());
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();

        // Validate required fields
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Event",
                field: "handle",
            });
        }

        // Validate place_handle resolves if set
        if let Some(ref ph) = self.data.place_handle {
            if !self.builder.graph.contains_node(ph) {
                return Err(BuilderError::InvalidHandle {
                    builder_type: "Event",
                    handle: ph.clone(),
                    target_type: "Place",
                });
            }
        }

        // Extract place_handle before consuming self.data
        let place_handle = self.data.place_handle.clone();
        let node = Node::Event(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;

        // Add EventPlace edge if place was set
        if let Some(ph) = place_handle {
            let edge = Edge::EventPlace {
                source: handle.clone(),
                target: ph,
            };
            let _ = self.builder.graph.add_edge(edge);
        }

        Ok(handle)
    }
}

/// Builder for constructing a single [`Place`](Node::Place) node.
pub struct PlaceBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: PlaceData,
}

impl<'a, 'b> PlaceBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn with_name(mut self, name: Location) -> Self {
        self.data.name = name;
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Place",
                field: "handle",
            });
        }
        let node = Node::Place(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;
        Ok(handle)
    }
}

/// Builder for constructing a single [`Source`](Node::Source) node.
pub struct SourceBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: SourceData,
}

impl<'a, 'b> SourceBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.data.title = title.into();
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Source",
                field: "handle",
            });
        }
        if self.data.title.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Source",
                field: "title",
            });
        }
        let node = Node::Source(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;
        Ok(handle)
    }
}

/// Builder for constructing a single [`Citation`](Node::Citation) node.
pub struct CitationBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: CitationData,
}

impl<'a, 'b> CitationBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn with_source(mut self, source_handle: &Handle) -> Self {
        self.data.source_handle = crate::into_source_handle_field(source_handle.clone());
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Citation",
                field: "handle",
            });
        }
        if crate::is_source_handle_empty(&self.data.source_handle) {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Citation",
                field: "source_handle",
            });
        }
        if !self.builder.graph.contains_node(&crate::get_source_handle(&self.data.source_handle)) {
            return Err(BuilderError::InvalidHandle {
                builder_type: "Citation",
                handle: crate::get_source_handle(&self.data.source_handle),
                target_type: "Source",
            });
        }

        let source_handle = crate::get_source_handle(&self.data.source_handle);
        let node = Node::Citation(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;

        // Add CitationSource edge
        let edge = Edge::CitationSource {
            source: handle.clone(),
            target: source_handle,
        };
        let _ = self.builder.graph.add_edge(edge);

        Ok(handle)
    }
}

/// Builder for constructing a single [`Note`](Node::Note) node.
pub struct NoteBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: NoteData,
}

impl<'a, 'b> NoteBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.data.text = text.into();
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Note",
                field: "handle",
            });
        }
        if self.data.text.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Note",
                field: "text",
            });
        }
        let node = Node::Note(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;
        Ok(handle)
    }
}

/// Builder for constructing a single [`Media`](Node::Media) node.
pub struct MediaBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: MediaData,
}

impl<'a, 'b> MediaBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Media",
                field: "handle",
            });
        }
        let node = Node::Media(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;
        Ok(handle)
    }
}

/// Builder for constructing a single [`Repository`](Node::Repository) node.
pub struct RepositoryBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: RepositoryData,
}

impl<'a, 'b> RepositoryBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Repository",
                field: "handle",
            });
        }
        let node = Node::Repository(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;
        Ok(handle)
    }
}

/// Builder for constructing a single [`Tag`](Node::Tag) node.
pub struct TagBuilder<'a, 'b> {
    builder: &'b mut GraphBuilder<'a>,
    data: TagData,
}

impl<'a, 'b> TagBuilder<'a, 'b> {
    pub fn with_handle(mut self, handle: impl Into<Handle>) -> Self {
        self.data.handle = handle.into();
        self
    }
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.data.name = name.into();
        self
    }
    pub fn build(self) -> Result<Handle, BuilderError> {
        let handle = self.data.handle.clone();
        if handle.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Tag",
                field: "handle",
            });
        }
        if self.data.name.is_empty() {
            return Err(BuilderError::MissingRequiredField {
                builder_type: "Tag",
                field: "name",
            });
        }
        let node = Node::Tag(self.data);
        self.builder
            .graph
            .add_node(handle.clone(), node)
            .map_err(|e| match e {
                crate::GraphError::DuplicateHandle(h) => BuilderError::DuplicateHandle(h),
                _ => BuilderError::DuplicateHandle(handle.clone()),
            })?;
        Ok(handle)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, not(feature = "schema-5-1")))]
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
            .build()
            .unwrap();
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
        let handle = builder
            .add_person_auto()
            .with_name("Auto", "Gen")
            .build()
            .unwrap();
        // UUID v4 is 36 characters
        assert_eq!(handle.len(), 36);
        assert!(graph.contains_node(&handle));
    }

    #[test]
    fn builder_build_returns_handle() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let handle = builder
            .add_person("p1")
            .with_name("John", "Smith")
            .build()
            .unwrap();
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
            .build()
            .unwrap();
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
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .build()
            .unwrap();
        let graph_ref = builder.into_graph();
        assert_eq!(graph_ref.node_count(), 1);
    }

    #[test]
    fn builder_add_multiple_persons() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .build()
            .unwrap();
        builder
            .add_person("p2")
            .with_name("Jane", "Doe")
            .build()
            .unwrap();
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
            .build()
            .unwrap();
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
            .build()
            .unwrap();
        assert_eq!(handle, "custom");
        assert!(graph.contains_node(&"custom".to_string()));
        assert!(!graph.contains_node(&"temp".to_string()));
    }

    #[test]
    fn builder_duplicate_handle_returns_error() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .build()
            .unwrap();
        let result = builder.add_person("p1").with_name("Jane", "Doe").build();
        assert!(matches!(
            result,
            Err(BuilderError::DuplicateHandle(h)) if h == "p1"
        ));
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
            .build()
            .unwrap();
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
            .build()
            .unwrap();
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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
        builder.add_family("f1").build().unwrap();
        assert_eq!(graph.node_count(), 1);
        assert!(graph.contains_node(&"f1".to_string()));
    }

    #[test]
    fn builder_family_auto_handle() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let handle = builder.add_family_auto().build().unwrap();
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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

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
            .build()
            .unwrap();

        let node = graph.get_node(&"p1".to_string()).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.family_list, vec!["f1".to_string()]);
        } else {
            panic!("Expected Person node");
        }
    }

    // -----------------------------------------------------------------------
    // Remaining type builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn builder_event_basic() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_event("e1")
            .with_event_type(EventType::Birth)
            .build()
            .unwrap();
        assert_eq!(graph.node_count(), 1);
        assert!(graph.contains_node(&"e1".to_string()));
    }

    #[test]
    fn builder_event_with_type_date_place() {
        let mut graph = Graph::new();
        graph
            .add_node("pl1".into(), Node::Place(PlaceData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_event("e1")
            .with_event_type(EventType::Marriage)
            .with_date(DateValue::new(1895))
            .with_place(&"pl1".to_string())
            .with_description("Church wedding")
            .build()
            .unwrap();

        let node = graph.get_node(&"e1".to_string()).unwrap();
        if let Node::Event(event) = node {
            assert_eq!(event.event_type, EventType::Marriage);
            assert_eq!(event.date, Some(DateValue::new(1895)));
            assert_eq!(event.place_handle, Some("pl1".to_string()));
            assert_eq!(event.description, Some("Church wedding".to_string()));
        } else {
            panic!("Expected Event node");
        }
    }

    #[test]
    fn builder_place_basic() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        let name = Location {
            city: Some("Springfield".to_string()),
            state: Some("IL".to_string()),
            ..Location::default()
        };
        builder.add_place("pl1").with_name(name).build().unwrap();
        assert!(graph.contains_node(&"pl1".to_string()));
    }

    #[test]
    fn builder_source_basic() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_source("s1")
            .with_title("Census Records")
            .build()
            .unwrap();
        assert!(graph.contains_node(&"s1".to_string()));
        let node = graph.get_node(&"s1".to_string()).unwrap();
        if let Node::Source(source) = node {
            assert_eq!(source.title, "Census Records");
        } else {
            panic!("Expected Source node");
        }
    }

    #[test]
    fn builder_citation_with_source() {
        let mut graph = Graph::new();
        graph
            .add_node("s1".into(), Node::Source(SourceData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_citation("c1")
            .with_source(&"s1".to_string())
            .build()
            .unwrap();
        assert!(graph.contains_node(&"c1".to_string()));
        let node = graph.get_node(&"c1".to_string()).unwrap();
        if let Node::Citation(citation) = node {
            assert_eq!(citation.source_handle, "s1");
        } else {
            panic!("Expected Citation node");
        }
    }

    #[test]
    fn builder_note_with_text() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_note("n1")
            .with_text("Some notes here")
            .build()
            .unwrap();
        assert!(graph.contains_node(&"n1".to_string()));
        let node = graph.get_node(&"n1".to_string()).unwrap();
        if let Node::Note(note) = node {
            assert_eq!(note.text, "Some notes here");
        } else {
            panic!("Expected Note node");
        }
    }

    #[test]
    fn builder_all_types() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);
        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .build()
            .unwrap();
        builder.add_family("f1").build().unwrap();
        builder
            .add_event("e1")
            .with_event_type(EventType::Birth)
            .build()
            .unwrap();
        builder.add_place("pl1").build().unwrap();
        builder.add_source("s1").with_title("T").build().unwrap();
        builder
            .add_citation("c1")
            .with_source(&"s1".to_string())
            .build()
            .unwrap();
        builder.add_note("n1").with_text("N").build().unwrap();
        builder.add_media("m1").build().unwrap();
        builder.add_repository("r1").build().unwrap();
        builder.add_tag("t1").with_name("Tag").build().unwrap();

        assert_eq!(graph.node_count(), 10);
    }

    // -----------------------------------------------------------------------
    // Integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn builder_complex_tree() {
        // Build a 3-generation family tree:
        // Grandfather + Grandmother -> Family1
        // Family1 -> Child1 (Father), Child2 (Aunt)
        // Father + Mother -> Family2
        // Family2 -> Grandchild1, Grandchild2
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);

        // Generation 1: Grandparents
        let gf = builder
            .add_person("gf")
            .with_name("John", "Smith")
            .with_gender(1)
            .build()
            .unwrap();
        let gm = builder
            .add_person("gm")
            .with_name("Jane", "Smith")
            .with_gender(2)
            .build()
            .unwrap();

        // Family 1: Grandparents
        let f1 = builder
            .add_family("f1")
            .with_father(&gf)
            .with_mother(&gm)
            .with_marriage_date(DateValue::new(1920))
            .build()
            .unwrap();

        // Generation 2: Children of Grandparents
        let father = builder
            .add_person("father")
            .with_name("Robert", "Smith")
            .with_gender(1)
            .with_parent_family(&f1)
            .with_birth_date(DateValue::new(1925))
            .build()
            .unwrap();
        let aunt = builder
            .add_person("aunt")
            .with_name("Alice", "Smith")
            .with_gender(2)
            .with_parent_family(&f1)
            .with_birth_date(DateValue::new(1928))
            .build()
            .unwrap();

        // Generation 2: Mother (from a different family)
        let mother = builder
            .add_person("mother")
            .with_name("Mary", "Johnson")
            .with_gender(2)
            .with_birth_date(DateValue::new(1930))
            .build()
            .unwrap();

        // Family 2: Father + Mother
        let f2 = builder
            .add_family("f2")
            .with_father(&father)
            .with_mother(&mother)
            .with_marriage_date(DateValue::new(1950))
            .build()
            .unwrap();

        // Generation 3: Grandchildren
        let gc1 = builder
            .add_person("gc1")
            .with_name("James", "Smith")
            .with_gender(1)
            .with_parent_family(&f2)
            .with_birth_date(DateValue::new(1955))
            .build()
            .unwrap();
        let gc2 = builder
            .add_person("gc2")
            .with_name("Emily", "Smith")
            .with_gender(2)
            .with_parent_family(&f2)
            .with_birth_date(DateValue::new(1958))
            .build()
            .unwrap();

        // Verify node counts
        // 6 persons + 2 families + 5 events (gf/gm marriage, father/mother marriage,
        // father birth, aunt birth, mother birth, gc1 birth, gc2 birth)
        // But wait: gf and gm don't have birth/death dates, so no events for them.
        // father has birth, aunt has birth, mother has birth, gc1 birth, gc2 birth = 5 events
        // f1 marriage, f2 marriage = 2 events
        // 7 persons + 2 families + 7 events = 16
        assert_eq!(graph.node_count(), 16);

        // Verify all nodes exist
        assert!(graph.contains_node(&gf));
        assert!(graph.contains_node(&gm));
        assert!(graph.contains_node(&father));
        assert!(graph.contains_node(&aunt));
        assert!(graph.contains_node(&mother));
        assert!(graph.contains_node(&gc1));
        assert!(graph.contains_node(&gc2));

        // Verify family relationships
        let node = graph.get_node(&father).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.parent_family_list, vec!["f1".to_string()]);
        } else {
            panic!("Expected Person");
        }

        // Verify edges exist
        assert!(graph
            .edges_from(&f1)
            .iter()
            .any(|e| matches!(e, Edge::FamilyFather { .. })));
        assert!(graph
            .edges_from(&f1)
            .iter()
            .any(|e| matches!(e, Edge::FamilyMother { .. })));
        assert!(graph
            .edges_from(&f2)
            .iter()
            .any(|e| matches!(e, Edge::FamilyFather { .. })));
        assert!(graph
            .edges_from(&f2)
            .iter()
            .any(|e| matches!(e, Edge::FamilyMother { .. })));
    }

    #[test]
    fn builder_person_with_all_fields() {
        let mut graph = Graph::new();

        // Create a family first for reference
        graph
            .add_node("f1".into(), Node::Family(crate::FamilyData::default()))
            .unwrap();

        let mut builder = GraphBuilder::new(&mut graph);
        let alt_name = Name {
            first_name: Some("Johnny".to_string()),
            surname_list: vec![Surname {
                surname: Some("Smithy".to_string()),
                ..Surname::default()
            }],
            ..Name::default()
        };

        let handle = builder
            .add_person("p1")
            .with_gramps_id("I0001")
            .with_gender(1)
            .with_name("John", "Smith")
            .with_birth_date(DateValue::new(1870))
            .with_death_date(DateValue::new(1945))
            .with_parent_family(&"f1".to_string())
            .add_alternate_name(alt_name)
            .build()
            .unwrap();

        assert_eq!(handle, "p1");

        let node = graph.get_node(&handle).unwrap();
        if let Node::Person(person) = node {
            assert_eq!(person.gramps_id, Some("I0001".to_string()));
            assert_eq!(person.gender, 1);
            assert_eq!(person.primary_name.first_name, Some("John".to_string()));
            assert_eq!(person.parent_family_list, vec!["f1".to_string()]);
            assert_eq!(person.alternate_names.len(), 1);
        } else {
            panic!("Expected Person");
        }

        // Verify events and edges
        assert!(graph.node_count() >= 3); // person + birth event + death event
        assert!(graph.edge_count() >= 2); // PersonEventRef x2 + PersonParentFamily
    }

    #[test]
    fn builder_graph_validates_after_build() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);

        builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_gender(1)
            .build()
            .unwrap();

        // Run validation
        let schema = crate::Schema::default();
        let errors = graph.validate(&schema);
        assert!(
            errors.is_empty(),
            "Builder-produced graph should pass validation: {:?}",
            errors
        );
        assert_eq!(graph.validation_state(), &crate::ValidationState::Valid);
    }

    #[test]
    fn builder_empty_graph_after_new() {
        let mut graph = Graph::new();
        let _builder = GraphBuilder::new(&mut graph);
        // Graph should not be modified until build() is called
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn builder_mixed_auto_and_explicit_handles() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);

        let explicit = builder
            .add_person("p1")
            .with_name("Explicit", "Handle")
            .build()
            .unwrap();
        assert_eq!(explicit, "p1");

        let auto = builder
            .add_person_auto()
            .with_name("Auto", "Handle")
            .build()
            .unwrap();
        assert_eq!(auto.len(), 36);
        assert_ne!(explicit, auto);

        assert_eq!(graph.node_count(), 2);
    }

    #[test]
    fn builder_error_cases_comprehensive() {
        let mut graph = Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);

        // Person missing name
        let result = builder.add_person("p1").with_gender(1).build();
        assert!(matches!(
            result,
            Err(BuilderError::MissingRequiredField {
                builder_type: "Person",
                field: _,
            })
        ));

        // Person with invalid gender
        let result = builder
            .add_person("p2")
            .with_name("X", "Y")
            .with_gender(99)
            .build();
        assert!(matches!(
            result,
            Err(BuilderError::MissingRequiredField {
                builder_type: "Person",
                field: _, // gender (must be 0-3)
            })
        ));

        // Person with non-existent parent family
        let result = builder
            .add_person("p3")
            .with_name("Child", "Smith")
            .with_parent_family(&"nonexistent".to_string())
            .build();
        assert!(matches!(
            result,
            Err(BuilderError::InvalidHandle {
                builder_type: "Person",
                handle: _,
                target_type: "Family",
            })
        ));

        // Family with non-existent father
        let result = builder
            .add_family("f1")
            .with_father(&"nonexistent".to_string())
            .build();
        assert!(matches!(
            result,
            Err(BuilderError::InvalidHandle {
                builder_type: "Family",
                handle: _,
                target_type: _, // Person (father)
            })
        ));

        // Family with non-existent child
        let result = builder
            .add_family("f2")
            .add_child(&"nonexistent".to_string(), ChildRefType::Birth)
            .build();
        assert!(matches!(
            result,
            Err(BuilderError::InvalidHandle {
                builder_type: "Family",
                handle: _,
                target_type: _, // Person (child)
            })
        ));

        // Event with non-existent place
        let result = builder
            .add_event("e1")
            .with_event_type(EventType::Birth)
            .with_place(&"nonexistent".to_string())
            .build();
        assert!(matches!(
            result,
            Err(BuilderError::InvalidHandle {
                builder_type: "Event",
                handle: _,
                target_type: "Place",
            })
        ));

        // Citation without source
        let result = builder.add_citation("c1").build();
        assert!(matches!(
            result,
            Err(BuilderError::MissingRequiredField {
                builder_type: "Citation",
                field: "source_handle",
            })
        ));

        // Note without text
        let result = builder.add_note("n1").build();
        assert!(matches!(
            result,
            Err(BuilderError::MissingRequiredField {
                builder_type: "Note",
                field: "text",
            })
        ));

        // Source without title
        let result = builder.add_source("s1").build();
        assert!(matches!(
            result,
            Err(BuilderError::MissingRequiredField {
                builder_type: "Source",
                field: "title",
            })
        ));

        // Tag without name
        let result = builder.add_tag("t1").build();
        assert!(matches!(
            result,
            Err(BuilderError::MissingRequiredField {
                builder_type: "Tag",
                field: "name",
            })
        ));

        // Duplicate handle
        builder
            .add_person("existing")
            .with_name("Existing", "Person")
            .build()
            .unwrap();
        let result = builder
            .add_person("existing")
            .with_name("Duplicate", "Person")
            .build();
        assert!(matches!(
            result,
            Err(BuilderError::DuplicateHandle(h)) if h == "existing"
        ));

        // BuilderError Display
        let err = BuilderError::MissingRequiredField {
            builder_type: "Person",
            field: "handle",
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Person"));
        assert!(msg.contains("handle"));

        let err = BuilderError::InvalidHandle {
            builder_type: "Person",
            handle: "bad".to_string(),
            target_type: "Family",
        };
        let msg = format!("{}", err);
        assert!(msg.contains("bad"));
        assert!(msg.contains("Family"));
    }
}
