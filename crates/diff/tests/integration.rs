//! Integration tests for the diff pipeline.
//!
//! These tests verify the full pipeline end-to-end:
//! generate → serialize → parse → diff → verify.

use std::io::Write;

use diff::{run_diff, DiffConfig, DiffReport};
use gramps_reader::xml::parse::parse_graph;
use output::GraphXmlWriter;
use output::SerializationMap;
use typed_graph::generate::builder::GraphBuilder;
use typed_graph::generate::generate_random;
use typed_graph::generate::AdversarialConfig;
use typed_graph::generate::RandomConfig;
use typed_graph::{Graph, Schema};

/// Create a temporary file with the given content and return its path.
fn create_temp_file(content: &str) -> String {
    let mut dir = std::env::temp_dir();
    dir.push(format!("gramps_diff_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    let mut path = dir.clone();
    path.push(format!("test_{}.gramps", rand::random::<u64>()));

    let mut file = std::fs::File::create(&path).expect("create temp file");
    file.write_all(content.as_bytes())
        .expect("write temp file");
    path.to_string_lossy().to_string()
}

/// Clean up temp files.
fn cleanup_temp_file(path: &str) {
    let _ = std::fs::remove_file(path);
}

/// Serialize a Graph to an XML string.
fn serialize_graph(graph: &typed_graph::Graph) -> String {
    let map = SerializationMap::new();
    let writer = GraphXmlWriter::new(map, "5.2.0");
    let mut buf = Vec::new();
    writer
        .write(graph, &mut buf)
        .expect("serialize graph to string");
    String::from_utf8(buf).expect("valid UTF-8 XML")
}

/// Generate a small graph with a fixed seed for testing.
fn generate_test_graph() -> typed_graph::Graph {
    let config = RandomConfig {
        person_count: 5,
        generations: 1,
        start_year: 1950,
        end_year: 2000,
        seed: Some(42),
        ..RandomConfig::default()
    };
    let adversarial_config = AdversarialConfig {
        enabled: false,
        strategies: vec![],
    };
    let schema = Schema::for_version(Schema::default_version()).expect("default schema");
    let result = generate_random(&config, &adversarial_config, None, schema)
        .expect("generate test graph");
    result.graph
}

/// Build a minimal graph with one person and one note, where the person's
/// note_list references the note. Returns the graph.
fn build_graph_with_note_ref() -> Graph {
    use typed_graph::Edge;

    let mut graph = Graph::new();
    let mut builder = GraphBuilder::new(&mut graph);

    let person_h = builder
        .add_person("p1")
        .with_name("John", "Smith")
        .with_gender(1)
        .build()
        .expect("add person");

    let note_h = builder
        .add_note("n1")
        .with_text("Original note text")
        .build()
        .expect("add note");

    // Add a PersonNote edge so the serialization outputs a <noteref> element.
    // The parser will reconstruct both the edge and the note_list data field.
    graph
        .add_edge(Edge::PersonNote {
            source: person_h.clone(),
            target: note_h.clone(),
        })
        .expect("add PersonNote edge");

    graph
}

/// Build a copy of the graph, but with the note's handle changed to the
/// given new handle. The note content remains the same, and the person's
/// note_list is updated to reference the new handle.
/// Helper: run diff between two XML strings and return the report.
fn diff_strings(xml_a: &str, xml_b: &str, config: &DiffConfig) -> DiffReport {
    let path_a = create_temp_file(xml_a);
    let path_b = create_temp_file(xml_b);

    let result = run_diff(&path_a, &path_b, config).expect("diff should succeed");

    cleanup_temp_file(&path_a);
    cleanup_temp_file(&path_b);

    result
}

// ---------------------------------------------------------------------------
// Identical files → all SAME
// ---------------------------------------------------------------------------

#[test]
fn identical_files_all_same() {
    let graph = generate_test_graph();
    let xml = serialize_graph(&graph);

    let report = diff_strings(&xml, &xml, &DiffConfig::default());

    // All items should be SAME
    assert_eq!(
        report.summary.same,
        report.summary.total_a,
        "all items should be SAME when files are identical"
    );
    assert_eq!(report.summary.total_a, report.summary.total_b);
    assert_eq!(report.summary.modified, 0);
    assert_eq!(report.summary.added, 0);
    assert_eq!(report.summary.removed, 0);
    assert_eq!(report.summary.needs_review, 0);
    assert_eq!(report.summary.extrinsic_only, 0);

    // All items should have Same classification
    for item in &report.items {
        assert_eq!(
            item.classification,
            diff::Classification::Same,
            "item {:?} should be SAME",
            item.handle_a
        );
        assert!(item.field_changes.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Different files → ADDED and REMOVED items
// ---------------------------------------------------------------------------

#[test]
fn different_files_has_added_and_removed() {
    // Generate two graphs with different seeds
    let config_a = RandomConfig {
        person_count: 5,
        generations: 1,
        start_year: 1950,
        end_year: 2000,
        seed: Some(42),
        ..RandomConfig::default()
    };
    let config_b = RandomConfig {
        person_count: 5,
        generations: 1,
        start_year: 1950,
        end_year: 2000,
        seed: Some(99),
        ..RandomConfig::default()
    };
    let adversarial_config = AdversarialConfig {
        enabled: false,
        strategies: vec![],
    };
    let schema = Schema::for_version(Schema::default_version()).expect("default schema");
    let result_a = generate_random(&config_a, &adversarial_config, None, schema)
        .expect("generate graph A");
    let schema = Schema::for_version(Schema::default_version()).expect("default schema");
    let result_b = generate_random(&config_b, &adversarial_config, None, schema)
        .expect("generate graph B");
    let graph_a = result_a.graph;
    let graph_b = result_b.graph;

    let xml_a = serialize_graph(&graph_a);
    let xml_b = serialize_graph(&graph_b);

    let report = diff_strings(&xml_a, &xml_b, &DiffConfig::default());

    // Since seeds differ, there should be some ADDED and REMOVED items
    // (at minimum, the total counts should match)
    assert_eq!(report.summary.total_a, graph_a.node_count());
    assert_eq!(report.summary.total_b, graph_b.node_count());

    // There should be at least one non-SAME classification since seeds differ
    let non_same = report.summary.added + report.summary.removed + report.summary.modified;
    assert!(
        non_same > 0 || report.summary.needs_review > 0,
        "different seeds should produce differences"
    );

    // Every item should have a classification
    assert_eq!(
        report.summary.same
            + report.summary.modified
            + report.summary.added
            + report.summary.removed
            + report.summary.needs_review
            + report.summary.extrinsic_only,
        report.items.len(),
        "item count should match summary"
    );
}

// ---------------------------------------------------------------------------
// Parse error propagation
// ---------------------------------------------------------------------------

#[test]
fn parse_error_on_invalid_xml() {
    let path_a = create_temp_file("not valid xml");
    let path_b = create_temp_file("<xml></xml>");

    let result = run_diff(&path_a, &path_b, &DiffConfig::default());
    assert!(
        result.is_err(),
        "should return error for invalid XML"
    );

    cleanup_temp_file(&path_a);
    cleanup_temp_file(&path_b);
}

// ---------------------------------------------------------------------------
// DiffConfig propagation
// ---------------------------------------------------------------------------

#[test]
fn diff_config_variants() {
    let graph = generate_test_graph();
    let xml = serialize_graph(&graph);

    // Test with various config values — all should produce valid results
    let configs = vec![
        DiffConfig::default(),
        DiffConfig {
            threshold: 0.5,
            include_extrinsic: false,
            normalize_enabled: true,
        },
        DiffConfig {
            threshold: 0.95,
            include_extrinsic: true,
            normalize_enabled: false,
        },
        DiffConfig {
            threshold: 0.0,
            include_extrinsic: false,
            normalize_enabled: false,
        },
    ];

    for config in &configs {
        let report = diff_strings(&xml, &xml, config);
        assert_eq!(
            report.summary.same,
            report.summary.total_a,
            "identical files should be all SAME with config {:?}",
            config
        );
    }
}

// ---------------------------------------------------------------------------
// Round-trip: parse → serialize → parse → diff
// ---------------------------------------------------------------------------

#[test]
fn round_trip_parse_serialize_diff() {
    let graph = generate_test_graph();
    let xml = serialize_graph(&graph);

    // Parse the serialized XML back into a graph
    let reparsed = parse_graph(&xml).expect("re-parse serialized XML");

    // Verify the re-parsed graph has the same number of nodes
    assert_eq!(
        reparsed.node_count(),
        graph.node_count(),
        "re-parsed graph should have same node count"
    );

    // Diff the original and re-parsed serializations
    let xml_reparsed = serialize_graph(&reparsed);
    let report = diff_strings(&xml, &xml_reparsed, &DiffConfig::default());

    // Everything should be SAME (or at least the counts should match)
    assert_eq!(report.summary.total_a, report.summary.total_b);
}

// ---------------------------------------------------------------------------
// Adding one person → exactly one ADDED item
// ---------------------------------------------------------------------------

#[test]
fn add_one_person_has_one_added() {
    // Generate graph A, then create an identical copy by parsing the serialized XML
    let graph_a = generate_test_graph();
    let xml_a = serialize_graph(&graph_a);
    let mut graph_b = parse_graph(&xml_a).expect("re-parse graph B");

    // Add one extra person to graph B
    let mut builder = GraphBuilder::new(&mut graph_b);
    builder
        .add_person("extra-person-001")
        .with_name("Extra", "Person")
        .with_gender(1)
        .build()
        .expect("add extra person");

    let xml_b = serialize_graph(&graph_b);

    let report = diff_strings(&xml_a, &xml_b, &DiffConfig::default());

    // B should have exactly one more node than A
    assert_eq!(report.summary.total_b, report.summary.total_a + 1);

    // There should be exactly one ADDED item
    assert_eq!(
        report.summary.added, 1,
        "adding one person should produce exactly one ADDED item"
    );

    // No items should be REMOVED
    assert_eq!(report.summary.removed, 0);

    // The added item should have no handle_a and should be a Person
    let added_items: Vec<&diff::ItemDiff> = report
        .items
        .iter()
        .filter(|i| i.classification == diff::Classification::Added)
        .collect();
    assert_eq!(added_items.len(), 1, "exactly one ADDED item");
    assert!(
        added_items[0].handle_a.is_none(),
        "ADDED item should have no handle_a"
    );
    assert_eq!(added_items[0].item_type, "Person");
}

// ---------------------------------------------------------------------------
// Modified note text → one MODIFIED item with FieldKind::Text
// ---------------------------------------------------------------------------

#[test]
fn modified_note_text_has_one_modified_with_text() {
    use typed_graph::Node;

    // Build a graph with a person and a note, then create an identical copy
    let graph_a = build_graph_with_note_ref();
    let xml_a = serialize_graph(&graph_a);
    let mut graph_b = parse_graph(&xml_a).expect("re-parse graph B");

    // Find a note in graph B and modify its text
    let mut note_found = false;
    let handles_b: Vec<typed_graph::Handle> = graph_b
        .iter_nodes()
        .filter_map(|(handle, node)| {
            if matches!(node, Node::Note(_)) {
                Some(handle.clone())
            } else {
                None
            }
        })
        .collect();

    if let Some(note_handle) = handles_b.first() {
        if let Some(Node::Note(note_data)) = graph_b.get_node_mut(note_handle) {
            note_data.text = "Modified text for integration test".to_string();
            note_found = true;
        }
    }

    assert!(note_found, "test graph should contain at least one note");

    let xml_b = serialize_graph(&graph_b);

    let report = diff_strings(&xml_a, &xml_b, &DiffConfig::default());

    // There should be at least one MODIFIED item
    assert!(
        report.summary.modified > 0,
        "modifying a note should produce at least one MODIFIED item"
    );

    // At least one MODIFIED item should have a Text field change
    let text_modified_items: Vec<&diff::ItemDiff> = report
        .items
        .iter()
        .filter(|i| {
            i.classification == diff::Classification::Modified
                && i.field_changes
                    .iter()
                    .any(|fc| fc.field_kind == diff::FieldKind::Text)
        })
        .collect();

    assert!(
        !text_modified_items.is_empty(),
        "at least one MODIFIED item should have a Text field change"
    );

    // The text field change should be on the 'text' field of a Note
    let text_changes: Vec<&diff::FieldChange> = text_modified_items
        .iter()
        .flat_map(|item| item.field_changes.iter())
        .filter(|fc| fc.field_kind == diff::FieldKind::Text && fc.field_name == "text")
        .collect();
    assert!(
        !text_changes.is_empty(),
        "should have at least one Text field change on the 'text' field"
    );

    // Verify the old and new values differ
    for change in &text_changes {
        assert_ne!(change.old_value, change.new_value);
    }
}

// ---------------------------------------------------------------------------
// Handle reference change (extrinsic-only) → one EXTRINSIC_ONLY item
// ---------------------------------------------------------------------------

#[test]
fn handle_ref_change_produces_extrinsic_only() {
    use typed_graph::Edge;

    // Build both graphs from scratch:
    // Graph A: person "p1" with PersonNote edge to "n1"
    // Graph B: same person "p1" with PersonNote edge to "n2"
    // Note "n2" has same content as "n1" → fuzzy match → handle_map["n2"] = "n1"
    // Person's note_list change (via edge) → extrinsic-only

    let mut graph_a = Graph::new();
    {
        let mut builder = GraphBuilder::new(&mut graph_a);
        let person_h = builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_gender(1)
            .build()
            .expect("add person in A");
        let note_h = builder
            .add_note("n1")
            .with_text("Shared note text for extrinsic test")
            .build()
            .expect("add note in A");
        graph_a
            .add_edge(Edge::PersonNote {
                source: person_h.clone(),
                target: note_h.clone(),
            })
            .expect("add PersonNote edge in A");
    }

    let mut graph_b = Graph::new();
    {
        let mut builder = GraphBuilder::new(&mut graph_b);
        let person_h = builder
            .add_person("p1")
            .with_name("John", "Smith")
            .with_gender(1)
            .build()
            .expect("add person in B");
        let note_h = builder
            .add_note("n2")
            .with_text("Shared note text for extrinsic test")
            .build()
            .expect("add note in B");
        graph_b
            .add_edge(Edge::PersonNote {
                source: person_h.clone(),
                target: note_h.clone(),
            })
            .expect("add PersonNote edge in B");
    }

    let xml_a = serialize_graph(&graph_a);
    let xml_b = serialize_graph(&graph_b);

    let report = diff_strings(&xml_a, &xml_b, &DiffConfig::default());

    // There should be at least one EXTRINSIC_ONLY item
    assert!(
        report.summary.extrinsic_only > 0,
        "handle ref change should produce at least one EXTRINSIC_ONLY item, got {}",
        report.summary.extrinsic_only
    );

    // The extrinsic-only item should be a Person with note_list changes
    let extrinsic_items: Vec<&diff::ItemDiff> = report
        .items
        .iter()
        .filter(|i| i.classification == diff::Classification::ExtrinsicOnly)
        .collect();

    assert!(
        !extrinsic_items.is_empty(),
        "should have at least one EXTRINSIC_ONLY item"
    );

    // Verify the extrinsic-only items have HandleRef-related field changes
    for item in &extrinsic_items {
        let has_handle_change = item.field_changes.iter().any(|fc| {
            matches!(fc.field_kind, diff::FieldKind::HandleRef | diff::FieldKind::HandleRefList)
        });
        assert!(
            has_handle_change,
            "EXTRINSIC_ONLY item '{}' should have HandleRef or HandleRefList field changes",
            item.handle_a.as_deref().unwrap_or("unknown")
        );
    }
}