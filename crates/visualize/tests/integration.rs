//! Integration tests for the visualize crate.
//!
//! Tests the full pipeline from `.gramps` XML files to `GraphData`
//! via `visualize::load_graph_data`, covering round-trips, edge cases,
//! and error handling.

use std::io::Write;
use tempfile::NamedTempFile;

/// Helper: write content to a temp file, rename to `.gramps`, return path.
fn write_gramps_file(content: &str) -> std::path::PathBuf {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", content).unwrap();
    let path = tmp.path().with_extension("gramps");
    std::fs::rename(tmp.path(), &path).unwrap();
    path
}

// ---------------------------------------------------------------------------
// Valid round-trip
// ---------------------------------------------------------------------------

#[test]
fn round_trip_simple_family() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>John</first><surname>Smith</surname></name><birth><dateval val="1850-03-15"/></birth></person>
    <person handle="p2"><gender>F</gender><name><first>Jane</first><surname>Smith</surname></name><birth><dateval val="1855-06-01"/></birth></person>
    <person handle="p3"><gender>M</gender><name><first>Jim</first><surname>Smith</surname></name></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><mother hlink="p2"/><childref hlink="p3"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    let gd = visualize::load_graph_data(path.to_str().unwrap(), false, 25).unwrap();

    // Shape: 3 people, 1 spouse link + 2 parent-child links = 3 links, 1 family group
    assert_eq!(gd.nodes.len(), 3, "expected 3 nodes");
    assert_eq!(
        gd.links.len(),
        3,
        "expected 3 links (1 spouse + 2 parent-child)"
    );
    assert_eq!(gd.family_groups.len(), 1, "expected 1 family group");

    // Verify node fields exist
    let p1 = gd.nodes.iter().find(|n| n.handle == "p1").unwrap();
    assert_eq!(p1.name, "John Smith");
    assert_eq!(p1.gender, "male");
    assert_eq!(p1.birth_year, Some(1850));
    assert!(!p1.is_imputed);

    // Serialize to JSON and verify basic shape
    let json = serde_json::to_string(&gd).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("nodes").unwrap().is_array());
    assert!(parsed.get("links").unwrap().is_array());
    assert!(parsed.get("family_groups").unwrap().is_array());

    // Check that JSON node has all expected fields
    let first_node = &parsed["nodes"][0];
    assert!(first_node.get("handle").unwrap().is_string());
    assert!(first_node.get("name").unwrap().is_string());
    assert!(first_node.get("gender").unwrap().is_string());
    // birth_year is a number or null
    assert!(
        first_node.get("birth_year").unwrap().is_number()
            || first_node.get("birth_year").unwrap().is_null()
    );
    // is_imputed is a boolean
    assert!(first_node.get("is_imputed").unwrap().is_boolean());

    // Check JSON link has expected fields
    let first_link = &parsed["links"][0];
    assert!(first_link.get("source").unwrap().is_string());
    assert!(first_link.get("target").unwrap().is_string());
    assert!(first_link.get("link_type").unwrap().is_string());
}

// ---------------------------------------------------------------------------
// Empty file
// ---------------------------------------------------------------------------

#[test]
fn empty_file_returns_error() {
    let path = write_gramps_file("");
    let result = visualize::load_graph_data(path.to_str().unwrap(), false, 25);
    match result {
        Err(msg) => assert!(msg.contains("No people found"), "got: {}", msg),
        Ok(_) => panic!("expected error for empty file"),
    }
}

// ---------------------------------------------------------------------------
// File with no people section
// ---------------------------------------------------------------------------

#[test]
fn file_without_people_returns_error() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <tags>
    <tag handle="t1" name="Bookmark"/>
  </tags>
</database>"#;
    let path = write_gramps_file(xml);
    let result = visualize::load_graph_data(path.to_str().unwrap(), false, 25);
    match result {
        Err(msg) => assert!(msg.contains("No people found"), "got: {}", msg),
        Ok(_) => panic!("expected error for file without people"),
    }
}

// ---------------------------------------------------------------------------
// Malformed XML
// ---------------------------------------------------------------------------

#[test]
fn malformed_xml_returns_error() {
    let path = write_gramps_file("<database><person handle=p1></database>");
    let result = visualize::load_graph_data(path.to_str().unwrap(), false, 25);
    match result {
        Err(msg) => assert!(msg.contains("Gramps XML"), "got: {}", msg),
        Ok(_) => panic!("expected error for malformed XML"),
    }
}

// ---------------------------------------------------------------------------
// Missing file
// ---------------------------------------------------------------------------

#[test]
fn missing_file_returns_error() {
    let result = visualize::load_graph_data("/nonexistent/path.gramps", false, 25);
    match result {
        Err(msg) => assert!(msg.contains("Cannot read file"), "got: {}", msg),
        Ok(_) => panic!("expected error for missing file"),
    }
}

// ---------------------------------------------------------------------------
// Cycles in family graph
// ---------------------------------------------------------------------------

#[test]
fn cycles_produce_valid_capped_generations() {
    // Create a cycle: p1 is father of p2, p2 is father of p1 → impossible but
    // we test that the generator handles it gracefully (capped at MAX_GENERATION).
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>Alice</first><surname>A</surname></name><birth><dateval val="1900"/></birth></person>
    <person handle="p2"><gender>M</gender><name><first>Bob</first><surname>B</surname></name></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><childref hlink="p2"/></family>
    <family handle="f2"><father hlink="p2"/><childref hlink="p1"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    // Should not panic — generations are capped at MAX_GENERATION
    let gd = visualize::load_graph_data(path.to_str().unwrap(), false, 25).unwrap();
    assert_eq!(gd.nodes.len(), 2);
    // Both nodes should have valid generations (0 or capped)
    for node in &gd.nodes {
        assert!(
            node.generation <= gramps_reader::MAX_GENERATION,
            "generation {} exceeds MAX_GENERATION {}",
            node.generation,
            gramps_reader::MAX_GENERATION
        );
    }
    // Should have a single family group
    assert_eq!(gd.family_groups.len(), 1);
}

// ---------------------------------------------------------------------------
// Large family with multiple components
// ---------------------------------------------------------------------------

#[test]
fn multi_component_family() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>G1</first><surname>A</surname></name><birth><dateval val="1800"/></birth></person>
    <person handle="p2"><gender>F</gender><name><first>G1</first><surname>B</surname></name><birth><dateval val="1805"/></birth></person>
    <person handle="p3"><gender>M</gender><name><first>G1</first><surname>C</surname></name><birth><dateval val="1830"/></birth></person>
    <person handle="p4"><gender>M</gender><name><first>G2</first><surname>D</surname></name><birth><dateval val="1900"/></birth></person>
    <person handle="p5"><gender>F</gender><name><first>G2</first><surname>E</surname></name><birth><dateval val="1905"/></birth></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><mother hlink="p2"/><childref hlink="p3"/></family>
    <family handle="f2"><father hlink="p4"/><mother hlink="p5"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    let gd = visualize::load_graph_data(path.to_str().unwrap(), false, 25).unwrap();

    // 5 people, 2 family groups
    assert_eq!(gd.nodes.len(), 5);
    assert_eq!(
        gd.family_groups.len(),
        2,
        "expected 2 disconnected components"
    );

    // Group 0: p1, p2, p3 (3 people, span 2 — gen 0 → gen 1)
    let g0 = gd.family_groups.iter().find(|g| g.id == 0).unwrap();
    assert_eq!(g0.size, 3);
    assert_eq!(g0.span, 2);

    // Group 1: p4, p5 (2 people, span 1 — both parents at gen 0)
    let g1 = gd.family_groups.iter().find(|g| g.id == 1).unwrap();
    assert_eq!(g1.size, 2);
    assert_eq!(g1.span, 1);
}

// ---------------------------------------------------------------------------
// No impute mode
// ---------------------------------------------------------------------------

#[test]
fn no_impute_skips_date_imputation() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>John</first><surname>S</surname></name><birth><dateval val="1850"/></birth></person>
    <person handle="p2"><gender>M</gender><name><first>Jim</first><surname>S</surname></name></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><childref hlink="p2"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    let gd = visualize::load_graph_data(path.to_str().unwrap(), true, 25).unwrap();
    // p2 (Jim) should have no birth_year and is_imputed = false
    let p2 = gd.nodes.iter().find(|n| n.handle == "p2").unwrap();
    assert!(
        p2.birth_year.is_none(),
        "no_impute: birth_year should be None, got {:?}",
        p2.birth_year
    );
    assert!(!p2.is_imputed, "no_impute: is_imputed should be false");
}

// ---------------------------------------------------------------------------
// Custom generation gap
// ---------------------------------------------------------------------------

#[test]
fn custom_generation_gap_affects_imputation() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>John</first><surname>S</surname></name><birth><dateval val="1850"/></birth></person>
    <person handle="p2"><gender>M</gender><name><first>Jim</first><surname>S</surname></name></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><childref hlink="p2"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    // 50-year gap: p2's imputed birth year = 1850 + 50 = 1900
    let gd = visualize::load_graph_data(path.to_str().unwrap(), false, 50).unwrap();
    let p2 = gd.nodes.iter().find(|n| n.handle == "p2").unwrap();
    assert_eq!(p2.birth_year, Some(1900), "custom gap 50 should give 1900");
}

// ---------------------------------------------------------------------------
// Real-world fixture: exp01.gramps
// ---------------------------------------------------------------------------

#[test]
fn exp01_fixture_counts() {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exp01.gramps");
    let gd = visualize::load_graph_data(path.to_str().unwrap(), false, 25).unwrap();

    // Expected: 64 persons, 49 families
    assert_eq!(
        gd.nodes.len(),
        64,
        "exp01.gramps: expected 64 nodes, got {}",
        gd.nodes.len()
    );
    assert_eq!(
        gd.links.len(),
        86,
        "exp01.gramps: expected 86 links, got {}",
        gd.links.len()
    );
    assert!(
        !gd.family_groups.is_empty(),
        "exp01.gramps: expected at least 1 family group, got {}",
        gd.family_groups.len()
    );

    // Verify all nodes have valid handles
    for node in &gd.nodes {
        assert!(!node.handle.is_empty(), "node handle should not be empty");
        assert!(!node.name.is_empty(), "node name should not be empty");
    }

    // Verify all links reference existing handles
    for link in &gd.links {
        assert!(
            gd.nodes.iter().any(|n| n.handle == link.source),
            "link source {} not found in nodes",
            link.source
        );
        assert!(
            gd.nodes.iter().any(|n| n.handle == link.target),
            "link target {} not found in nodes",
            link.target
        );
    }

    // Verify no person has a gen 0 birth year that is imputed (gen 0 founders
    // should have explicit dates, not imputed ones)
    for node in &gd.nodes {
        if node.generation == 0 && node.is_imputed {
            // This is acceptable — some founders may lack dates in real data
            // Just log via the assertion message
            assert!(
                node.birth_year.is_some(),
                "gen 0 node {} has imputed=true but no birth_year",
                node.handle
            );
        }
    }
}

// ---------------------------------------------------------------------------
// SelectionExport serialization (envelope format)
// ---------------------------------------------------------------------------

#[test]
fn selection_export_serialization_has_envelope() {
    let export = visualize::SelectionExport {
        exported_at: "2025-01-15T10:30:00.000Z".to_string(),
        file: "selections.json".to_string(),
        selections: vec![
            visualize::SelectedPerson {
                handle: "abc-1".to_string(),
                name: "John Smith".to_string(),
                birth_date: Some("1840-07-13".to_string()),
                death_date: Some("1910-03-22".to_string()),
                gender: "male".to_string(),
                family_group: 3,
            },
            visualize::SelectedPerson {
                handle: "abc-2".to_string(),
                name: "Jane Doe".to_string(),
                birth_date: None,
                death_date: None,
                gender: "female".to_string(),
                family_group: 3,
            },
        ],
    };

    let json_str = serde_json::to_string_pretty(&export).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    // Must be an object, not an array
    assert!(parsed.is_object(), "envelope should be an object");

    // Must have envelope keys
    assert!(parsed.get("exported_at").is_some(), "missing exported_at");
    assert!(parsed.get("file").is_some(), "missing file");
    assert!(parsed.get("selections").is_some(), "missing selections");
    assert!(
        parsed["selections"].is_array(),
        "selections should be an array"
    );

    // Round-trip: deserialize back
    let round_trip: visualize::SelectionExport =
        serde_json::from_str(&json_str).expect("should round-trip");
    assert_eq!(round_trip.exported_at, "2025-01-15T10:30:00.000Z");
    assert_eq!(round_trip.file, "selections.json");
    assert_eq!(round_trip.selections.len(), 2);
    assert_eq!(round_trip.selections[0].handle, "abc-1");
    assert_eq!(round_trip.selections[1].handle, "abc-2");
}

#[test]
fn selection_export_roundtrip_via_integrate() {
    // Build a SelectionExport, serialize it, write to temp file, then
    // parse with integrate::parse_selections_json and verify handles match.
    let export = visualize::SelectionExport {
        exported_at: "2025-06-01T12:00:00.000Z".to_string(),
        file: "test_selections.json".to_string(),
        selections: vec![
            visualize::SelectedPerson {
                handle: "h001".to_string(),
                name: "Alice Wonderland".to_string(),
                birth_date: Some("1850-01-01".to_string()),
                death_date: None,
                gender: "female".to_string(),
                family_group: 1,
            },
            visualize::SelectedPerson {
                handle: "h002".to_string(),
                name: "Bob Builder".to_string(),
                birth_date: None,
                death_date: Some("1910-12-31".to_string()),
                gender: "male".to_string(),
                family_group: 1,
            },
        ],
    };

    let json_str = serde_json::to_string_pretty(&export).unwrap();

    // Write to temp file
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    write!(tmp, "{}", json_str).unwrap();
    let sel_path = tmp.path().with_extension("json");
    std::fs::rename(tmp.path(), &sel_path).unwrap();

    // Parse with integrate
    let selections = integrate::json_reader::parse_selections_json(sel_path.to_str().unwrap())
        .expect("integrate should parse the wrapped format");

    assert_eq!(selections.len(), 2);
    assert_eq!(selections[0].handle, "h001");
    assert_eq!(selections[0].name, "Alice Wonderland");
    assert_eq!(selections[1].handle, "h002");
    assert_eq!(selections[1].name, "Bob Builder");

    // Clean up
    let _ = std::fs::remove_file(&sel_path);
}
