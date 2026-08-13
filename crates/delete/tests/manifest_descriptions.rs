//! Integration test: manifest descriptions are populated from the graph.
//!
//! Builds a graph with a person (name, gramps_id, birth event), runs
//! `build_manifest`, serializes to JSON, and asserts that `gramps_id`
//! and `description` fields are present in the output.

use delete::manifest::build_manifest;
use delete::types::MANIFEST_VERSION;
use typed_graph::{Graph, Node};

#[test]
fn manifest_v3_serializes_descriptions() {
    let mut graph = Graph::new();
    let person_h = "p0001".to_string();
    let event_h = "e0001".to_string();

    // Create a birth event
    graph
        .add_node(
            event_h.clone(),
            Node::Event(typed_graph::EventData {
                handle: event_h.clone(),
                gramps_id: None,
                event_type: Some(typed_graph::EventType::Birth),
                date: Some(typed_graph::DateValue {
                    year: 1800,
                    text: Some("1800".to_string()),
                    ..typed_graph::DateValue::default()
                }),
                ..typed_graph::EventData::default()
            }),
        )
        .unwrap();

    // Create a person with a name, gramps_id, and birth event reference
    graph
        .add_node(
            person_h.clone(),
            Node::Person(typed_graph::PersonData {
                handle: person_h.clone(),
                gramps_id: Some("I0001".to_string()),
                primary_name: typed_graph::Name {
                    first_name: Some("John".to_string()),
                    surname_list: vec![typed_graph::Surname {
                        surname: Some("Smith".to_string()),
                        ..typed_graph::Surname::default()
                    }],
                    ..typed_graph::Name::default()
                },
                birth_ref_index: Some(0),
                event_ref_list: vec![typed_graph::EventRef {
                    ref_field: event_h,
                    ..typed_graph::EventRef::default()
                }],
                ..typed_graph::PersonData::default()
            }),
        )
        .unwrap();

    let to_delete = vec![person_h];
    let manifest = build_manifest(
        "test.gramps",
        Some("selections.json"),
        &["p0001".to_string()],
        &to_delete,
        &graph,
    );

    assert_eq!(manifest.version, MANIFEST_VERSION);

    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Assert version is 3
    assert_eq!(parsed["version"], 3);

    // Check that the person entry has gramps_id and description
    let people = &parsed["plan"]["people"];
    let to_delete = &people["to_delete"];
    assert!(to_delete.is_array());
    let first = to_delete[0].as_object().unwrap();

    assert!(
        first.contains_key("gramps_id"),
        "v3 manifest entry must have gramps_id"
    );
    assert_eq!(first["gramps_id"], "I0001");

    assert!(
        first.contains_key("description"),
        "v3 manifest entry must have description"
    );
    assert!(
        first["description"]
            .as_str()
            .unwrap()
            .contains("John Smith"),
        "description should contain the person's name"
    );
}

#[test]
fn manifest_v3_serializes_empty_place() {
    // A place with no name/date still produces a description fallback
    // but no gramps_id.
    let mut graph = Graph::new();
    let place_h = "pl0001".to_string();

    graph
        .add_node(
            place_h.clone(),
            Node::Place(typed_graph::PlaceData {
                handle: place_h.clone(),
                ..typed_graph::PlaceData::default()
            }),
        )
        .unwrap();

    let to_delete = vec![place_h];
    let manifest = build_manifest("test.gramps", None, &[], &to_delete, &graph);

    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let places = &parsed["plan"]["places"];
    let to_delete = &places["to_delete"];
    let first = to_delete[0].as_object().unwrap();

    assert_eq!(parsed["version"], 3);
    assert!(
        !first.contains_key("gramps_id"),
        "empty place should not have gramps_id"
    );
    assert!(
        first.contains_key("description"),
        "empty place should still have a description fallback"
    );
    let desc = first["description"].as_str().unwrap();
    assert!(!desc.is_empty(), "description fallback should not be empty");
    assert_eq!(desc, "Unnamed Place");
}
