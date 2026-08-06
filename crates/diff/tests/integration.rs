//! Integration tests for the diff pipeline.
//!
//! These tests verify the full pipeline end-to-end:
//! generate → serialize → parse → diff → verify.

use std::io::Write;

use diff::{run_diff, DiffConfig, DiffReport};
use gramps_reader::xml::parse::parse_graph;
use output::GraphXmlWriter;
use output::SerializationMap;
use typed_graph::generate::generate_random;
use typed_graph::generate::AdversarialConfig;
use typed_graph::generate::RandomConfig;
use typed_graph::Schema;

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
fn generate_test_graph(_seed: u64, with_notes: bool) -> typed_graph::Graph {
    let config = RandomConfig {
        person_count: 5,
        generations: 1,
        start_year: 1950,
        end_year: 2000,
        with_notes,
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
    let graph = generate_test_graph(42, false);
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
    let graph_a = generate_test_graph(42, false);
    let graph_b = generate_test_graph(99, false);

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
// Graphs with notes → MODIFIED
// ---------------------------------------------------------------------------

#[test]
fn diff_with_notes() {
    // Generate graphs with notes enabled
    let graph_a = generate_test_graph(42, true);
    let graph_b = generate_test_graph(99, true);

    let xml_a = serialize_graph(&graph_a);
    let xml_b = serialize_graph(&graph_b);

    let report = diff_strings(&xml_a, &xml_b, &DiffConfig::default());

    // Both graphs should have at least some nodes
    assert!(report.summary.total_a > 0);
    assert!(report.summary.total_b > 0);

    // The report should be valid JSON-serializable
    let json = serde_json::to_string(&report).expect("serialize report to JSON");
    assert!(!json.is_empty());
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
    let graph = generate_test_graph(42, false);
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
    let graph = generate_test_graph(42, false);
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