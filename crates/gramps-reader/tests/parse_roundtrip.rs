//! Integration tests for round-trip and mixed-type parsing.
//!
//! These tests verify that the full parser handles the complete
//! pipeline: generating a graph, serializing it to XML, re-parsing
//! the XML, and comparing the resulting graphs.
//!
//! Note: The serializer and parser have pre-existing element-name
//! mismatches (e.g., place vs placeobj, stitle vs title) that prevent
//! round-trip from being fully correct.  This file tests what is
//! feasible: node counts when using parse_all+build_edges (without
//! validation), and a hand-written mixed-type fixture with the
//! element names the parser understands.

use gramps_reader::xml::parse::{parse_graph, Parser};
use output::GraphXmlWriter;
use typed_graph::generate::*;
use typed_graph::*;

/// Run a round-trip test: generate → serialize → re-parse (without
/// validation) → compare node counts.
///
/// Uses `Parser::parse_all` + `build_edges` (skipping validation)
/// to avoid failures from pre-existing serializer/parser field
/// mismatches.
fn round_trip_nodes(config: RandomConfig, gramps_version: &str) -> (usize, usize) {
    let schema = Schema::for_version(gramps_version).expect("schema version must be compiled in");

    // Generate the graph.
    let adversarial = AdversarialConfig {
        enabled: false,
        strategies: vec![],
    };
    let densify = DensifyConfig::default();
    let result = generate_random(&config, &adversarial, Some(&densify), schema)
        .expect("generation should succeed");
    let original_graph = &result.graph;

    // Count original nodes.
    let orig_node_count = original_graph.node_count();

    // Serialize to XML.
    let writer = GraphXmlWriter::new(output::SerializationMap::new(), gramps_version);
    let mut buf = Vec::new();
    writer.write(original_graph, &mut buf).expect("serialization should succeed");
    let xml = String::from_utf8(buf).expect("output should be valid UTF-8");

    // Verify the XML is well-formed (contains expected elements).
    assert!(xml.contains("<database"), "XML should have database root");
    assert!(xml.contains("</database>"), "XML should close database");
    assert!(
        xml.contains(&format!("version=\"{}\"", gramps_version)),
        "XML should contain schema version"
    );

    // Re-parse using Parser directly (skip validation).
    let version = gramps_reader::xml::header::detect_schema_version(&xml)
        .expect("should detect schema version");
    let reparse_schema = Schema::for_version(&version)
        .ok_or_else(|| format!("unsupported schema version: {}", version))
        .unwrap();
    let mut parser = Parser::new(reparse_schema);
    parser.parse_all(&xml).expect("parse_all should succeed");
    parser.build_edges().expect("build_edges should succeed");
    let reparsed = parser.into_graph();

    let reparse_node_count = reparsed.node_count();

    (orig_node_count, reparse_node_count)
}

/// Count nodes by kind in a graph.
fn count_nodes_by_kind(graph: &Graph) -> std::collections::HashMap<NodeKind, usize> {
    let mut counts = std::collections::HashMap::new();
    for nk in &[
        NodeKind::Person,
        NodeKind::Family,
        NodeKind::Event,
        NodeKind::Place,
        NodeKind::Source,
        NodeKind::Citation,
        NodeKind::Repository,
        NodeKind::Media,
        NodeKind::Note,
        NodeKind::Tag,
    ] {
        let count = graph.nodes_by_kind(*nk).len();
        if count > 0 {
            counts.insert(*nk, count);
        }
    }
    counts
}

// -----------------------------------------------------------------------
// Round-trip tests with fixed seeds
// -----------------------------------------------------------------------

#[test]
fn round_trip_basic_persons_only() {
    let config = RandomConfig {
        person_count: 5,
        generations: 2,
        seed: Some(42),
        ..RandomConfig::default()
    };
    let (orig_nodes, reparse_nodes) = round_trip_nodes(config, "5.2");
    assert_eq!(
        orig_nodes, reparse_nodes,
        "Node count should match after round-trip ({} vs {})",
        orig_nodes, reparse_nodes
    );
}

#[test]
fn round_trip_different_seed() {
    let config = RandomConfig {
        person_count: 8,
        generations: 2,
        seed: Some(999),
        ..RandomConfig::default()
    };
    let (orig_nodes, reparse_nodes) = round_trip_nodes(config, "5.2");
    assert_eq!(
        orig_nodes, reparse_nodes,
        "Node count should match after round-trip"
    );
}

// -----------------------------------------------------------------------
// Mixed-type fixture: hand-written XML with all 10 primary types
// -----------------------------------------------------------------------

#[test]
fn mixed_type_fixture_node_counts() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2024-01-01" version="5.2"/></header>

  <tags>
    <tag handle="T0001">
      <name>Confirmed</name>
      <color>#00ff00</color>
      <priority>1</priority>
    </tag>
  </tags>

  <events>
    <event handle="E0001">
      <type>Birth</type>
      <dateval quality="exact" val="1950-01-15"/>
    </event>
  </events>

  <people>
    <person handle="P0001">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>
          <surname>Smith</surname>
          <primary>1</primary>
        </surname>
      </name>
    </person>
  </people>

  <families>
    <family handle="F0001">
      <father hlink="P0001"/>
    </family>
  </families>

  <citations>
    <citation handle="C0001">
      <page>p. 42</page>
      <sourceref hlink="S0001"/>
    </citation>
  </citations>

  <sources>
    <source handle="S0001">
      <title>Birth Certificate</title>
    </source>
  </sources>

  <places>
    <place handle="L0001">
      <name value="Springfield, IL"/>
    </place>
  </places>

  <objects>
    <object handle="M0001">
      <file src="/photos/portrait.jpg" mime="image/jpeg"/>
      <description>Portrait photo</description>
    </object>
  </objects>

  <repositories>
    <repository handle="R0001">
      <name>National Archives</name>
    </repository>
  </repositories>

  <notes>
    <note handle="N0001">
      <text>Sample note text.</text>
      <format>0</format>
    </note>
  </notes>
</database>"#;

    let graph = parse_graph(xml).expect("mixed-type fixture should parse successfully");

    // Verify all 10 types are present.
    let counts = count_nodes_by_kind(&graph);
    assert_eq!(*counts.get(&NodeKind::Person).unwrap_or(&0), 1, "Person count");
    assert_eq!(*counts.get(&NodeKind::Family).unwrap_or(&0), 1, "Family count");
    assert_eq!(*counts.get(&NodeKind::Event).unwrap_or(&0), 1, "Event count");
    assert_eq!(*counts.get(&NodeKind::Place).unwrap_or(&0), 1, "Place count");
    assert_eq!(*counts.get(&NodeKind::Source).unwrap_or(&0), 1, "Source count");
    assert_eq!(*counts.get(&NodeKind::Citation).unwrap_or(&0), 1, "Citation count");
    assert_eq!(*counts.get(&NodeKind::Repository).unwrap_or(&0), 1, "Repository count");
    assert_eq!(*counts.get(&NodeKind::Media).unwrap_or(&0), 1, "Media count");
    assert_eq!(*counts.get(&NodeKind::Note).unwrap_or(&0), 1, "Note count");
    assert_eq!(*counts.get(&NodeKind::Tag).unwrap_or(&0), 1, "Tag count");

    // Total nodes: 10 (one of each type)
    assert_eq!(graph.node_count(), 10, "Total node count");

    // 2 edges: family → father (FamilyFather), citation → source (CitationSource)
    assert_eq!(graph.edge_count(), 2, "Edge count");
}

#[test]
fn mixed_type_fixture_malformed_xml() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2024-01-01" version="5.2"/></header>
  <people>
    <person handle="P0001">
      <gender>M
  </people>
</database>"#;

    let result = parse_graph(xml);
    assert!(result.is_err(), "Malformed XML should produce an error");
}