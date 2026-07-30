//! typed-graph — In-memory typed directed multigraph for Gramps genealogy data.
//!
//! This crate provides the core graph model, schema-driven codegen types,
//! validation, and generation capabilities for Gramps family tree datasets.

#![deny(deprecated)]

pub mod date;
pub mod generate;
pub mod graph;
pub mod schema;
pub mod schema_convert;
pub mod validate;

/// Re-export the schema module.
/// The schema module is populated by build.rs codegen at compile time.
pub use schema::*;

/// Re-export graph module key types at the crate root.
pub use graph::{Graph, GraphError, NodeKind, ValidationError, ValidationState};
pub use graph::{
    edge_place_place_ref, event_type_eq, gender_cmp, gender_value, get_source_handle,
    into_event_type_field, into_gender_field, into_source_handle_field, is_gender_valid,
    is_source_handle_empty, make_child_ref, make_event_ref, set_gender,
    set_source_handle,
};

// The date module adds convenience methods (new, new_ymd, display_text, is_valid)
// to the generated DateValue type from the schema.

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Type shape tests: verify the generated types exist with expected variants
    // -----------------------------------------------------------------------

    #[test]
    fn test_node_enum_has_all_primary_types() {
        // Verify Node enum has variants for all 10 primary types
        let _ = Node::Person(PersonData::default());
        let _ = Node::Family(FamilyData::default());
        let _ = Node::Event(EventData::default());
        let _ = Node::Place(PlaceData::default());
        let _ = Node::Source(SourceData::default());
        let _ = Node::Citation(CitationData::default());
        let _ = Node::Repository(RepositoryData::default());
        let _ = Node::Media(MediaData::default());
        let _ = Node::Note(NoteData::default());
        let _ = Node::Tag(TagData::default());
    }

    #[test]
    fn test_node_enum_variant_names() {
        // Verify Node::Person contains the correct type
        let person = PersonData::default();
        let node = Node::Person(person);
        match node {
            Node::Person(_) => {}
            _ => panic!("Expected Node::Person"),
        }
    }

    #[test]
    fn test_edge_enum_has_handle_ref_variants() {
        // Verify handle_ref edges exist
        let _ = Edge::PersonFamily {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::PersonParentFamily {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::FamilyFather {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::FamilyMother {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::EventPlace {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::CitationSource {
            source: "s1".into(),
            target: "t1".into(),
        };
    }

    #[test]
    fn test_edge_enum_has_mixin_variants() {
        // Verify mixin-based edges exist (shared across multiple primary types)
        let _ = Edge::CitationRef {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::NoteRef {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::MediaRef {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::TagRef {
            source: "s1".into(),
            target: "t1".into(),
        };
    }

    #[test]
    fn test_edge_enum_has_embedded_ref_variants_with_metadata() {
        // Verify embedded ref edges with metadata exist
        let _ = Edge::PersonEventRef {
            source: "s1".into(),
            target: "t1".into(),
            metadata: Box::new(EventRef::default()),
        };
        let _ = Edge::FamilyChildRef {
            source: "s1".into(),
            target: "t1".into(),
            metadata: Box::new(ChildRef::default()),
        };
        let _ = Edge::FamilyEventRef {
            source: "s1".into(),
            target: "t1".into(),
            metadata: Box::new(EventRef::default()),
        };
        let _ = Edge::PersonPersonRef {
            source: "s1".into(),
            target: "t1".into(),
            metadata: Box::new(PersonRef::default()),
        };
        let _ = Edge::SourceRepoRef {
            source: "s1".into(),
            target: "t1".into(),
            metadata: Box::new(RepoRef::default()),
        };
    }

    #[test]
    fn test_edge_enum_has_embedded_ref_variants_without_metadata() {
        // Verify embedded ref edges without metadata (simple refs)
        let _ = Edge::PlacePlaceRef {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::PersonMediaRef {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::CitationMediaRef {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::EventMediaRef {
            source: "s1".into(),
            target: "t1".into(),
        };
    }

    #[test]
    fn test_edge_enum_has_citation_handle_ref_edges() {
        // Verify handle_ref edges for citation/reference fields
        let _ = Edge::PersonCitation {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::PersonNote {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::PersonTag {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::EventCitation {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::EventNote {
            source: "s1".into(),
            target: "t1".into(),
        };
        let _ = Edge::EventTag {
            source: "s1".into(),
            target: "t1".into(),
        };
    }

    // -----------------------------------------------------------------------
    // Data struct tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_person_data_fields() {
        let person = PersonData {
            handle: "h1".into(),
            gramps_id: None,
            gender: 0,
            primary_name: Name::default(),
            alternate_names: vec![],
            event_ref_list: vec![],
            family_list: vec![],
            parent_family_list: vec![],
            person_ref_list: vec![],
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            address_list: vec![],
            url_list: vec![],
            lds_ord_list: vec![],
        };
        assert_eq!(person.handle, "h1");
        assert!(person.gramps_id.is_none());
        assert!(person.event_ref_list.is_empty());
        assert!(person.family_list.is_empty());
        assert!(person.citation_list.is_empty());
    }

    #[test]
    fn test_family_data_fields() {
        let family = FamilyData {
            handle: "h1".into(),
            gramps_id: None,
            father_handle: None,
            mother_handle: None,
            child_ref_list: vec![],
            event_ref_list: vec![],
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
        };
        assert_eq!(family.handle, "h1");
        assert!(family.father_handle.is_none());
        assert!(family.child_ref_list.is_empty());
    }

    #[test]
    fn test_event_data_fields() {
        let event = EventData {
            handle: "h1".into(),
            gramps_id: None,
            event_type: EventType::Birth,
            date: None,
            place_handle: None,
            description: None,
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
        };
        assert_eq!(event.handle, "h1");
        assert_eq!(event.event_type, EventType::Birth);
        assert!(event.place_handle.is_none());
    }

    #[test]
    fn test_enum_types_exist() {
        // Verify all enum types exist with expected values
        let _ = EventType::Birth;
        let _ = EventType::Death;
        let _ = EventRoleType::Primary;
        let _ = EventRoleType::Family;
        let _ = ChildRefType::Birth;
        let _ = ChildRefType::Adopted;
        let _ = Gender::_0;
        let _ = Gender::_1;
        let _ = Gender::_2;
        let _ = Gender::_3;
        let _ = NameType::Birth;
        let _ = DateQuality::Exact;
        let _ = DateModifier::Before;
    }

    #[test]
    fn test_handle_type() {
        let h: Handle = "test-handle".into();
        assert_eq!(h, "test-handle");
    }

    // -----------------------------------------------------------------------
    // Schema metadata tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_schema_new() {
        let schema = Schema::default();
        assert_eq!(schema.version, Schema::default_version());
        // Also verify the explicit versioned API works
        let schema_default = Schema::for_version(Schema::default_version())
            .expect("default version should be available");
        assert_eq!(schema_default.version, Schema::default_version());
    }

    #[test]
    fn test_schema_required_fields() {
        let schema = Schema::default();

        // Person requires handle and gender
        let person_required = schema.required_fields.get("Person");
        assert!(
            person_required.is_some(),
            "Person should have required fields"
        );
        let person_required = person_required.unwrap();
        assert!(
            person_required.contains(&"handle"),
            "Person requires handle"
        );
        assert!(
            person_required.contains(&"gender"),
            "Person requires gender"
        );
    }

    #[test]
    fn test_schema_cardinality_constraints() {
        let schema = Schema::default();

        // Check cardinality constraints exist
        let person_event_ref = schema.cardinality_constraints.get("Person.event_ref_list");
        assert!(
            person_event_ref.is_some(),
            "Person.event_ref_list should have cardinality"
        );

        let (min, max) = person_event_ref.unwrap();
        assert_eq!(*min, Some(0));
        assert_eq!(*max, None);
    }

    #[test]
    fn test_schema_default() {
        let schema = Schema::default();
        assert_eq!(schema.version, Schema::default_version());
        // Verify default_version() returns the same
        assert_eq!(Schema::default_version(), Schema::default_version());
    }

    #[test]
    fn test_schema_available_versions() {
        let versions = Schema::available_versions();
        assert!(!versions.is_empty(), "should have at least one version");
        assert!(
            versions.contains(&Schema::default_version()),
            "default version should be available"
        );
    }

    #[test]
    fn test_schema_for_version_unknown() {
        assert!(Schema::for_version("99.99").is_none(), "unknown version should return None");
    }

    #[test]
    #[allow(deprecated)]
    fn test_schema_new_and_versioned_api_consistent() {
        // Schema::new() should produce the same data as for_version(default_version())
        let default = Schema::new();
        let explicit = Schema::for_version(Schema::default_version())
            .expect("default version should be available")
            .clone();
        assert_eq!(default.version, explicit.version);
        assert_eq!(default.required_fields, explicit.required_fields);
        assert_eq!(default.cardinality_constraints, explicit.cardinality_constraints);
    }

    // -----------------------------------------------------------------------
    // Clone, Debug, PartialEq derive tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_node_clone() {
        let node = Node::Person(PersonData::default());
        let cloned = node.clone();
        assert_eq!(node, cloned);
    }

    #[test]
    fn test_edge_clone_and_debug() {
        let edge = Edge::PersonFamily {
            source: "s1".into(),
            target: "t1".into(),
        };
        let cloned = edge.clone();
        assert_eq!(edge, cloned);
        let _debug = format!("{:?}", edge);
    }

    #[test]
    fn test_edge_with_metadata_clone() {
        let edge = Edge::PersonEventRef {
            source: "s1".into(),
            target: "t1".into(),
            metadata: Box::new(EventRef::default()),
        };
        let cloned = edge.clone();
        assert_eq!(edge, cloned);
    }

    #[test]
    fn test_data_struct_debug() {
        let person = PersonData::default();
        let _debug = format!("{:?}", person);
    }

    #[test]
    fn test_secondary_type_debug() {
        let event_ref = EventRef::default();
        let _debug = format!("{:?}", event_ref);
    }

    #[test]
    fn test_partial_eq_for_enum_types() {
        assert_eq!(EventType::Birth, EventType::Birth);
        assert_ne!(EventType::Birth, EventType::Death);
    }

    #[test]
    fn test_copy_for_enum_types() {
        let t = EventType::Birth;
        let _copied = t; // Copy, not move
        assert_eq!(t, EventType::Birth);
    }
}
