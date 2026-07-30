//! Structural and referential validation for the typed graph.
//!
//! This module provides validation passes that check the integrity of a [`Graph`]
//! against the [`Schema`] metadata. Validation runs in two layers:
//!
//! 1. **Structural**: checks required fields and cardinality constraints per node.
//! 2. **Referential**: checks that all edge source/target handles exist in the graph.

use crate::graph::ValidationError;
use crate::graph::{node_kind, NodeKind};
use crate::Edge;
use crate::Graph;
use crate::Handle;
use crate::Node;
use crate::Schema;

/// Run structural validation on the graph.
///
/// Checks that every node has all required fields (as defined in `schema.required_fields`)
/// and that array fields respect their cardinality constraints (as defined in
/// `schema.cardinality_constraints`).
pub fn structural_validation(graph: &Graph, schema: &Schema) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (handle, node) in graph.iter_nodes() {
        let type_name = node_type_name(node);

        // Check required fields
        if let Some(required_fields) = schema.required_fields.get(type_name) {
            for &field in required_fields {
                if let Some(err) = check_required_field(handle, node, field) {
                    errors.push(err);
                }
            }
        }

        // Check cardinality constraints
        for (field_key, (min, max)) in &schema.cardinality_constraints {
            // field_key is "TypeName.field_name"
            if let Some(key_type) = field_key.split('.').next() {
                if key_type != type_name {
                    continue;
                }
                if let Some(field_name) = field_key.split('.').nth(1) {
                    let actual = count_array_field(node, field_name);
                    if let Some(min_val) = min {
                        if actual < *min_val as usize {
                            errors.push(ValidationError::CardinalityViolation {
                                node: handle.clone(),
                                field: field_key.to_string(),
                                expected: format!("at least {}", min_val),
                                actual,
                            });
                        }
                    }
                    if let Some(max_val) = max {
                        if actual > *max_val as usize {
                            errors.push(ValidationError::CardinalityViolation {
                                node: handle.clone(),
                                field: field_key.to_string(),
                                expected: format!("at most {}", max_val),
                                actual,
                            });
                        }
                    }
                }
            }
        }
    }

    errors
}

/// Run referential validation on the graph.
///
/// Walks every edge and verifies that both the source and target handles
/// exist in `graph.nodes`. Reports [`ValidationError::DanglingReference`]
/// for any missing handle.
pub fn referential_validation(graph: &Graph) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for edge in graph.iter_edges() {
        let (source, target) = crate::graph::edge_source_target(edge);
        let link_name = edge_link_name(edge);

        if !graph.contains_node(&source) {
            errors.push(ValidationError::DanglingReference {
                source: source.clone(),
                link: link_name.to_string(),
                target: target.clone(),
            });
        } else if !graph.contains_node(&target) {
            errors.push(ValidationError::DanglingReference {
                source,
                link: link_name.to_string(),
                target,
            });
        }
    }

    errors
}

/// Run full validation (structural then referential), collecting all errors.
///
/// Returns a `Vec<ValidationError>` containing all structural and referential
/// errors found. If the vec is empty, the graph is valid.
pub fn validate(graph: &Graph, schema: &Schema) -> Vec<ValidationError> {
    let mut errors = structural_validation(graph, schema);
    errors.extend(referential_validation(graph));
    errors
}

/// Run strict validation, promoting plausibility warnings to errors.
///
/// Same as [`validate`] but treats all warnings as blocking errors.
/// Returns `Ok(())` if no errors, `Err(errors)` otherwise.
pub fn validate_strict(graph: &Graph, schema: &Schema) -> Result<(), Vec<ValidationError>> {
    let errors = validate(graph, schema);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Integration with Graph
// ---------------------------------------------------------------------------

impl Graph {
    /// Run validation on this graph and update the [`ValidationState`].
    ///
    /// After calling this method, `validation_state()` will be:
    /// - [`ValidationState::Valid`] if no errors were found.
    /// - [`ValidationState::Invalid(errors)`] if errors were found.
    ///
    /// Returns the list of errors found (empty if valid).
    pub fn validate(&mut self, schema: &Schema) -> Vec<ValidationError> {
        let errors = validate(self, schema);
        if errors.is_empty() {
            self.set_validation_state(crate::ValidationState::Valid);
        } else {
            self.set_validation_state(crate::ValidationState::Invalid(errors.clone()));
        }
        errors
    }

    /// Assert that the graph is in a valid state.
    ///
    /// Returns `Ok(())` if [`validation_state`](Graph::validation_state) is
    /// [`ValidationState::Valid`], otherwise returns `Err` with the stored errors.
    pub fn assert_valid(&self) -> Result<(), &[ValidationError]> {
        match self.validation_state() {
            crate::ValidationState::Valid => Ok(()),
            crate::ValidationState::Invalid(errors) => Err(errors.as_slice()),
            crate::ValidationState::Unvalidated => Err(&[]),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the schema type name string for a [`Node`].
fn node_type_name(node: &Node) -> &'static str {
    match node_kind(node) {
        NodeKind::Citation => "Citation",
        NodeKind::Event => "Event",
        NodeKind::Family => "Family",
        NodeKind::Media => "Media",
        NodeKind::Note => "Note",
        NodeKind::Person => "Person",
        NodeKind::Place => "Place",
        NodeKind::Repository => "Repository",
        NodeKind::Source => "Source",
        NodeKind::Tag => "Tag",
    }
}

/// Return a human-readable link name for an [`Edge`] variant.
fn edge_link_name(edge: &Edge) -> &'static str {
    match edge {
        Edge::CitationMediaRef { .. } => "citation.media_list",
        Edge::CitationNote { .. } => "citation.note_list",
        Edge::CitationSource { .. } => "citation.source_handle",
        Edge::CitationTag { .. } => "citation.tag_list",
        Edge::CitationRef { .. } => "citation_list",
        Edge::NoteRef { .. } => "note_list",
        Edge::MediaRef { .. } => "media_list",
        Edge::TagRef { .. } => "tag_list",
        Edge::EventCitation { .. } => "event.citation_list",
        Edge::EventMediaRef { .. } => "event.media_list",
        Edge::EventNote { .. } => "event.note_list",
        Edge::EventPlace { .. } => "event.place_handle",
        Edge::EventTag { .. } => "event.tag_list",
        Edge::FamilyChildRef { .. } => "family.child_ref_list",
        Edge::FamilyCitation { .. } => "family.citation_list",
        Edge::FamilyEventRef { .. } => "family.event_ref_list",
        Edge::FamilyFather { .. } => "family.father_handle",
        Edge::FamilyMediaRef { .. } => "family.media_list",
        Edge::FamilyMother { .. } => "family.mother_handle",
        Edge::FamilyNote { .. } => "family.note_list",
        Edge::FamilyTag { .. } => "family.tag_list",
        Edge::MediaCitation { .. } => "media.citation_list",
        Edge::MediaNote { .. } => "media.note_list",
        Edge::MediaTag { .. } => "media.tag_list",
        Edge::NoteCitation { .. } => "note.citation_list",
        Edge::NoteTag { .. } => "note.tag_list",
        Edge::PersonCitation { .. } => "person.citation_list",
        Edge::PersonEventRef { .. } => "person.event_ref_list",
        Edge::PersonFamily { .. } => "person.family_list",
        Edge::PersonMediaRef { .. } => "person.media_list",
        Edge::PersonNote { .. } => "person.note_list",
        Edge::PersonParentFamily { .. } => "person.parent_family_list",
        Edge::PersonPersonRef { .. } => "person.person_ref_list",
        Edge::PersonTag { .. } => "person.tag_list",
        Edge::PlaceCitation { .. } => "place.citation_list",
        Edge::PlaceMediaRef { .. } => "place.media_list",
        Edge::PlaceNote { .. } => "place.note_list",
        Edge::PlacePlaceRef { .. } => "place.place_ref_list",
        Edge::PlaceTag { .. } => "place.tag_list",
        Edge::RepositoryMediaRef { .. } => "repository.media_list",
        Edge::RepositoryNote { .. } => "repository.note_list",
        Edge::RepositoryTag { .. } => "repository.tag_list",
        Edge::SourceMediaRef { .. } => "source.media_list",
        Edge::SourceNote { .. } => "source.note_list",
        Edge::SourceRepoRef { .. } => "source.reporef_list",
        Edge::SourceTag { .. } => "source.tag_list",
        Edge::TagTag { .. } => "tag.tag_list",
    }
}

/// Check a single required field on a node.
///
/// Returns `Some(ValidationError::MissingRequired)` if the field is missing or empty.
fn check_required_field(handle: &Handle, node: &Node, field: &str) -> Option<ValidationError> {
    let missing = match node {
        Node::Person(data) => match field {
            "handle" => data.handle.is_empty(),
            "gender" => !matches!(data.gender, 0..=3),
            "primary_name" => {
                data.primary_name.first_name.is_none() && data.primary_name.surname_list.is_empty()
            }
            _ => false,
        },
        Node::Family(data) => match field {
            "handle" => data.handle.is_empty(),
            _ => false,
        },
        Node::Event(data) => match field {
            "handle" => data.handle.is_empty(),
            "event_type" => false, // EventType always has a value (Default::default() is Birth)
            _ => false,
        },
        Node::Place(data) => match field {
            "handle" => data.handle.is_empty(),
            "name" => {
                data.name.city.is_none()
                    && data.name.country.is_none()
                    && data.name.county.is_none()
                    && data.name.state.is_none()
                    && data.name.street.is_none()
            }
            _ => false,
        },
        Node::Source(data) => match field {
            "handle" => data.handle.is_empty(),
            "title" => data.title.is_empty(),
            _ => false,
        },
        Node::Citation(data) => match field {
            "handle" => data.handle.is_empty(),
            "source_handle" => data.source_handle.is_empty(),
            _ => false,
        },
        Node::Media(data) => match field {
            "handle" => data.handle.is_empty(),
            _ => false,
        },
        Node::Note(data) => match field {
            "handle" => data.handle.is_empty(),
            "text" => data.text.is_empty(),
            _ => false,
        },
        Node::Repository(data) => match field {
            "handle" => data.handle.is_empty(),
            _ => false,
        },
        Node::Tag(data) => match field {
            "handle" => data.handle.is_empty(),
            "name" => data.name.is_empty(),
            _ => false,
        },
    };

    if missing {
        Some(ValidationError::MissingRequired {
            node: handle.clone(),
            field: field.to_string(),
        })
    } else {
        None
    }
}

/// Count the number of elements in an array field of a node.
///
/// Returns 0 for non-array fields or fields that don't exist on the node type.
fn count_array_field(node: &Node, field: &str) -> usize {
    match node {
        Node::Person(data) => match field {
            "alternate_names" => data.alternate_names.len(),
            "event_ref_list" => data.event_ref_list.len(),
            "family_list" => data.family_list.len(),
            "parent_family_list" => data.parent_family_list.len(),
            "person_ref_list" => data.person_ref_list.len(),
            "citation_list" => data.citation_list.len(),
            "note_list" => data.note_list.len(),
            "media_list" => data.media_list.len(),
            "tag_list" => data.tag_list.len(),
            "attribute_list" => data.attribute_list.len(),
            "address_list" => data.address_list.len(),
            "url_list" => data.url_list.len(),
            "lds_ord_list" => data.lds_ord_list.len(),
            _ => 0,
        },
        Node::Family(data) => match field {
            "child_ref_list" => data.child_ref_list.len(),
            "event_ref_list" => data.event_ref_list.len(),
            "citation_list" => data.citation_list.len(),
            "note_list" => data.note_list.len(),
            "media_list" => data.media_list.len(),
            "tag_list" => data.tag_list.len(),
            "attribute_list" => data.attribute_list.len(),
            _ => 0,
        },
        Node::Event(data) => match field {
            "citation_list" => data.citation_list.len(),
            "note_list" => data.note_list.len(),
            "media_list" => data.media_list.len(),
            "tag_list" => data.tag_list.len(),
            "attribute_list" => data.attribute_list.len(),
            _ => 0,
        },
        Node::Place(data) => match field {
            "place_ref_list" => data.place_ref_list.len(),
            "citation_list" => data.citation_list.len(),
            "note_list" => data.note_list.len(),
            "media_list" => data.media_list.len(),
            "tag_list" => data.tag_list.len(),
            "attribute_list" => data.attribute_list.len(),
            _ => 0,
        },
        Node::Source(data) => match field {
            "reporef_list" => data.reporef_list.len(),
            "note_list" => data.note_list.len(),
            "media_list" => data.media_list.len(),
            "tag_list" => data.tag_list.len(),
            "attribute_list" => data.attribute_list.len(),
            _ => 0,
        },
        Node::Citation(data) => match field {
            "media_list" => data.media_list.len(),
            "note_list" => data.note_list.len(),
            "tag_list" => data.tag_list.len(),
            _ => 0,
        },
        Node::Media(data) => match field {
            "citation_list" => data.citation_list.len(),
            "note_list" => data.note_list.len(),
            "tag_list" => data.tag_list.len(),
            "attribute_list" => data.attribute_list.len(),
            _ => 0,
        },
        Node::Note(data) => match field {
            "citation_list" => data.citation_list.len(),
            "tag_list" => data.tag_list.len(),
            _ => 0,
        },
        Node::Repository(data) => match field {
            "address_list" => data.address_list.len(),
            "media_list" => data.media_list.len(),
            "note_list" => data.note_list.len(),
            "tag_list" => data.tag_list.len(),
            "url_list" => data.url_list.len(),
            _ => 0,
        },
        Node::Tag(data) => match field {
            "tag_list" => data.tag_list.len(),
            _ => 0,
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    // -----------------------------------------------------------------------
    // Structural validation
    // -----------------------------------------------------------------------

    #[test]
    fn structural_missing_required_handle() {
        let mut graph = Graph::new();
        let handle = "p1".to_string();
        graph
            .add_node(
                handle.clone(),
                Node::Person(PersonData {
                    handle: "".to_string(), // empty handle
                    ..PersonData::default()
                }),
            )
            .unwrap();
        let schema = Schema::default();
        let errors = structural_validation(&graph, &schema);
        assert!(
            errors.iter().any(|e| matches!(e, ValidationError::MissingRequired { node, field } if node == "p1" && field == "handle")),
            "Should find MissingRequired for handle"
        );
    }

    #[test]
    fn structural_required_present() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
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
        let schema = Schema::default();
        let errors = structural_validation(&graph, &schema);
        let missing: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, ValidationError::MissingRequired { .. }))
            .collect();
        assert!(
            missing.is_empty(),
            "Should have no missing required fields: {:?}",
            missing
        );
    }

    #[test]
    fn cardinality_violation_min_not_reached() {
        // All cardinality constraints in the schema have min=0, so this test
        // verifies that no spurious violations are reported for empty arrays.
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
                Node::Person(PersonData {
                    handle: "p1".to_string(),
                    gender: 1,
                    primary_name: Name {
                        first_name: Some("John".to_string()),
                        ..Name::default()
                    },
                    // All arrays are empty by default (min=0, so this is fine)
                    ..PersonData::default()
                }),
            )
            .unwrap();
        let schema = Schema::default();
        let errors = structural_validation(&graph, &schema);
        let cardinality: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, ValidationError::CardinalityViolation { .. }))
            .collect();
        assert!(
            cardinality.is_empty(),
            "Should have no cardinality violations: {:?}",
            cardinality
        );
    }

    // -----------------------------------------------------------------------
    // Referential validation
    // -----------------------------------------------------------------------

    #[test]
    fn referential_dangling_edge() {
        let mut graph = Graph::new();
        // Add only the source node, not the target
        graph
            .add_node("p1".to_string(), Node::Person(PersonData::default()))
            .unwrap();
        // Add an edge to a nonexistent target; this should fail with MissingNode,
        // so we can't test dangling reference this way.
        // Instead, test that add_edge prevents dangling references.
        let edge = Edge::PersonFamily {
            source: "p1".to_string(),
            target: "nonexistent".to_string(),
        };
        assert!(
            graph.add_edge(edge).is_err(),
            "add_edge should reject edges to nonexistent nodes"
        );
    }

    #[test]
    fn referential_valid_edges() {
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), Node::Person(PersonData::default()))
            .unwrap();
        graph
            .add_node("f1".to_string(), Node::Family(FamilyData::default()))
            .unwrap();
        graph
            .add_edge(Edge::PersonFamily {
                source: "p1".to_string(),
                target: "f1".to_string(),
            })
            .unwrap();
        let errors = referential_validation(&graph);
        assert!(
            errors.is_empty(),
            "Should have no dangling references: {:?}",
            errors
        );
    }

    // -----------------------------------------------------------------------
    // Combined validation
    // -----------------------------------------------------------------------

    #[test]
    fn validate_collects_all_errors() {
        let mut graph = Graph::new();
        // Person with empty handle (missing required)
        graph
            .add_node(
                "p1".to_string(),
                Node::Person(PersonData {
                    handle: "".to_string(),
                    ..PersonData::default()
                }),
            )
            .unwrap();
        // Source with empty title (missing required)
        graph
            .add_node(
                "s1".to_string(),
                Node::Source(SourceData {
                    handle: "s1".to_string(),
                    title: "".to_string(),
                    ..SourceData::default()
                }),
            )
            .unwrap();

        let schema = Schema::default();
        let errors = validate(&graph, &schema);
        assert!(
            errors.len() >= 2,
            "Should have at least 2 errors, got {}",
            errors.len()
        );
    }

    #[test]
    fn validate_ok() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
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
        let schema = Schema::default();
        let errors = validate(&graph, &schema);
        assert!(
            errors.is_empty(),
            "Valid graph should have no errors: {:?}",
            errors
        );
    }

    #[test]
    fn validate_strict_promotes_warnings() {
        // In the current implementation, validate_strict just runs validate
        // and returns Err if there are errors. Since there are no plausibility
        // warnings yet, this test verifies the basic behavior.
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
                Node::Person(PersonData {
                    handle: "".to_string(),
                    ..PersonData::default()
                }),
            )
            .unwrap();
        let schema = Schema::default();
        let result = validate_strict(&graph, &schema);
        assert!(result.is_err(), "Strict validation should fail");
    }

    // -----------------------------------------------------------------------
    // Graph integration
    // -----------------------------------------------------------------------

    #[test]
    fn graph_validate_sets_state() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
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
        let schema = Schema::default();
        let errors = graph.validate(&schema);
        assert!(errors.is_empty(), "Should have no errors: {:?}", errors);
        assert_eq!(
            graph.validation_state(),
            &crate::ValidationState::Valid,
            "State should be Valid"
        );
    }

    #[test]
    fn graph_validate_sets_invalid_state() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
                Node::Person(PersonData {
                    handle: "".to_string(),
                    ..PersonData::default()
                }),
            )
            .unwrap();
        let schema = Schema::default();
        let errors = graph.validate(&schema);
        assert!(!errors.is_empty(), "Should have errors");
        assert!(
            matches!(graph.validation_state(), crate::ValidationState::Invalid(_)),
            "State should be Invalid"
        );
    }

    #[test]
    fn graph_assert_valid_ok() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
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
        let schema = Schema::default();
        graph.validate(&schema);
        assert!(graph.assert_valid().is_ok());
    }

    #[test]
    fn graph_assert_valid_fails_on_unvalidated() {
        let graph = Graph::new();
        let result = graph.assert_valid();
        assert!(
            result.is_err(),
            "Unvalidated graph should fail assert_valid"
        );
    }

    #[test]
    fn validate_empty_graph() {
        let graph = Graph::new();
        let schema = Schema::default();
        let errors = validate(&graph, &schema);
        assert!(
            errors.is_empty(),
            "Empty graph should pass validation: {:?}",
            errors
        );
    }

    #[test]
    fn multiple_errors_reported() {
        let mut graph = Graph::new();
        // Person with empty handle and empty note with empty text
        graph
            .add_node(
                "p1".to_string(),
                Node::Person(PersonData {
                    handle: "".to_string(),
                    ..PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                "n1".to_string(),
                Node::Note(NoteData {
                    handle: "n1".to_string(),
                    text: "".to_string(),
                    ..NoteData::default()
                }),
            )
            .unwrap();

        let schema = Schema::default();
        let errors = validate(&graph, &schema);
        assert!(
            errors.len() >= 2,
            "Should have at least 2 errors, got {}",
            errors.len()
        );
    }

    #[test]
    fn validation_state_transitions() {
        let mut graph = Graph::new();
        let schema = Schema::default();

        // Initial: Unvalidated
        assert_eq!(
            graph.validation_state(),
            &crate::ValidationState::Unvalidated
        );

        // Add a valid node and validate -> Valid
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
        let errors = graph.validate(&schema);
        assert!(errors.is_empty());
        assert_eq!(graph.validation_state(), &crate::ValidationState::Valid);

        // Add an invalid node and validate -> Invalid
        graph
            .add_node(
                "p2".into(),
                Node::Person(PersonData {
                    handle: "".to_string(),
                    ..PersonData::default()
                }),
            )
            .unwrap();
        let errors = graph.validate(&schema);
        assert!(!errors.is_empty());
        assert!(matches!(
            graph.validation_state(),
            crate::ValidationState::Invalid(_)
        ));

        // Fix the invalid node by mutating it and validate -> Valid
        if let Some(Node::Person(ref mut person)) = graph.get_node_mut(&"p2".to_string()) {
            person.handle = "p2".to_string();
            person.gender = 1;
            person.primary_name = Name {
                first_name: Some("Jane".to_string()),
                ..Name::default()
            };
        }
        let errors = graph.validate(&schema);
        assert!(errors.is_empty());
        assert_eq!(graph.validation_state(), &crate::ValidationState::Valid);
    }

    #[test]
    fn validate_graph_with_all_required_fields() {
        let mut graph = Graph::new();
        let schema = Schema::default();

        // Add a fully specified Person node
        graph
            .add_node(
                "p1".into(),
                Node::Person(PersonData {
                    handle: "p1".to_string(),
                    gramps_id: Some("I0001".to_string()),
                    gender: 1,
                    primary_name: Name {
                        first_name: Some("John".to_string()),
                        surname_list: vec![Surname {
                            surname: Some("Smith".to_string()),
                            ..Surname::default()
                        }],
                        ..Name::default()
                    },
                    ..PersonData::default()
                }),
            )
            .unwrap();

        let errors = validate(&graph, &schema);
        assert!(
            errors.is_empty(),
            "Full-featured Person should pass validation: {:?}",
            errors
        );

        // Add a fully specified Family node
        graph
            .add_node(
                "f1".into(),
                Node::Family(FamilyData {
                    handle: "f1".to_string(),
                    ..FamilyData::default()
                }),
            )
            .unwrap();
        let errors = validate(&graph, &schema);
        assert!(
            errors.is_empty(),
            "Full-featured Family should pass validation: {:?}",
            errors
        );

        // Add a fully specified Event node
        graph
            .add_node(
                "e1".into(),
                Node::Event(EventData {
                    handle: "e1".to_string(),
                    event_type: EventType::Birth,
                    ..EventData::default()
                }),
            )
            .unwrap();
        let errors = validate(&graph, &schema);
        assert!(
            errors.is_empty(),
            "Full-featured Event should pass validation: {:?}",
            errors
        );

        // Add a Source with required title
        graph
            .add_node(
                "s1".into(),
                Node::Source(SourceData {
                    handle: "s1".to_string(),
                    title: "Census Records".to_string(),
                    ..SourceData::default()
                }),
            )
            .unwrap();
        let errors = validate(&graph, &schema);
        assert!(
            errors.is_empty(),
            "Full-featured Source should pass validation: {:?}",
            errors
        );
    }

    #[test]
    fn dangling_reference_in_citation_edge() {
        let mut graph = Graph::new();
        let schema = Schema::default();

        // Add a Citation node with a source_handle pointing to a non-existent Source
        graph
            .add_node(
                "c1".into(),
                Node::Citation(CitationData {
                    handle: "c1".to_string(),
                    source_handle: "s_missing".to_string(), // points to non-existent node
                    ..CitationData::default()
                }),
            )
            .unwrap();

        // The Citation has a valid handle and source_handle is non-empty string,
        // so it should pass structural validation for required fields
        let errors = structural_validation(&graph, &schema);
        let missing: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, ValidationError::MissingRequired { .. }))
            .collect();
        assert!(
            missing.is_empty(),
            "Citation should have no missing required fields: {:?}",
            missing
        );

        // But the citation source edge should still be rejected by add_edge
        let result = graph.add_edge(Edge::CitationSource {
            source: "c1".into(),
            target: "s_missing".into(),
        });
        assert!(
            result.is_err(),
            "add_edge should reject edge to non-existent target"
        );
    }
}
