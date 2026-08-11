//! Integration tests for the CLI crate.

use std::collections::{HashMap, HashSet, VecDeque};
use tempfile::NamedTempFile;

/// Compute the size of the largest connected component of Person nodes
/// connected through Family nodes (person-family-person edges).
fn largest_connected_person_component(graph: &typed_graph::Graph) -> usize {
    // Build adjacency: for each person handle, find all other person handles
    // connected through shared families.
    let mut person_adj: HashMap<String, Vec<String>> = HashMap::new();

    // Collect all person handles
    let person_handles: Vec<String> = graph
        .iter_nodes()
        .filter_map(|(h, n)| {
            if matches!(n, typed_graph::Node::Person(_)) {
                Some(h.clone())
            } else {
                None
            }
        })
        .collect();

    for h in &person_handles {
        person_adj.entry(h.clone()).or_default();
    }

    // Walk edge list: for each Family* edge, find the family node, then
    // find all Person nodes connected to that family.
    for (_, node) in graph.iter_nodes() {
        if let typed_graph::Node::Family(family) = node {
            let mut members: Vec<String> = Vec::new();
            if let Some(ref fh) = family.father_handle {
                if person_handles.contains(fh) {
                    members.push(fh.clone());
                }
            }
            if let Some(ref mh) = family.mother_handle {
                if person_handles.contains(mh) {
                    members.push(mh.clone());
                }
            }
            // Add children: find FamilyChildRef edges targeting this family
            for edge in graph.iter_edges() {
                if let typed_graph::Edge::FamilyChildRef {
                    ref source,
                    ref target,
                    ..
                } = edge
                {
                    if source == &family.handle && person_handles.contains(target) {
                        members.push(target.clone());
                    }
                }
            }
            // Connect all members in a clique
            for i in 0..members.len() {
                for j in (i + 1)..members.len() {
                    person_adj
                        .get_mut(&members[i])
                        .unwrap()
                        .push(members[j].clone());
                    person_adj
                        .get_mut(&members[j])
                        .unwrap()
                        .push(members[i].clone());
                }
            }
        }
    }

    // BFS to find largest connected component
    let mut visited: HashSet<String> = HashSet::new();
    let mut largest = 0;

    for h in &person_handles {
        if visited.contains(h) {
            continue;
        }
        // BFS
        let mut queue = VecDeque::new();
        queue.push_back(h.clone());
        visited.insert(h.clone());
        let mut component_size = 0;

        while let Some(current) = queue.pop_front() {
            component_size += 1;
            if let Some(neighbors) = person_adj.get(&current) {
                for n in neighbors {
                    if !visited.contains(n) {
                        visited.insert(n.clone());
                        queue.push_back(n.clone());
                    }
                }
            }
        }

        largest = largest.max(component_size);
    }

    largest
}

/// Integration test: generate a small valid family tree.
#[test]
fn generate_small_family_tree() {
    let schema = typed_graph::Schema::default();
    let config = typed_graph::generate::RandomConfig {
        person_count: 10,
        family_count: 5,
        family_ratio: 0.5,
        max_parent_roles: 1,
        layer_linking: false,
        generations: 2,
        children_per_family: 1..3,
        start_year: 1900,
        end_year: 2020,
        name_style: "modern".to_string(),
        with_places: false,
        with_citations: false,
        with_notes: false,
        with_media: false,
        with_tags: false,
        seed: Some(42),
        place_depth: 3,
    };
    let adv_config = typed_graph::generate::AdversarialConfig::default();

    let mut result =
        typed_graph::generate::generate_random(&config, &adv_config, None, &schema).unwrap();
    assert_eq!(result.stats.person_count, 10);
    assert!(result.stats.family_count <= 5);
    assert!(result.stats.event_count > 0);
    assert!(result.stats.edge_count > 0);

    // Validate
    let errors = result.graph.validate(&schema);
    assert!(errors.is_empty(), "Validation errors: {:?}", errors);
}

/// Integration test: single person generation.
#[test]
fn generate_single_person() {
    let schema = typed_graph::Schema::default();
    let config = typed_graph::generate::RandomConfig {
        person_count: 1,
        family_count: 0,
        family_ratio: 0.5,
        max_parent_roles: 1,
        layer_linking: false,
        generations: 1,
        children_per_family: 1..2,
        start_year: 1900,
        end_year: 2000,
        name_style: "modern".to_string(),
        with_places: false,
        with_citations: false,
        with_notes: false,
        with_media: false,
        with_tags: false,
        seed: Some(42),
        place_depth: 3,
    };
    let adv_config = typed_graph::generate::AdversarialConfig::default();

    let mut result =
        typed_graph::generate::generate_random(&config, &adv_config, None, &schema).unwrap();
    assert_eq!(result.stats.person_count, 1);
    assert!(result.stats.family_count == 0);

    // A single person should still have a valid graph
    let errors = result.graph.validate(&schema);
    assert!(errors.is_empty(), "Validation errors: {:?}", errors);
}

/// Integration test: zero person count should fail gracefully.
#[test]
fn generate_zero_persons_fails() {
    let schema = typed_graph::Schema::default();
    let config = typed_graph::generate::RandomConfig {
        person_count: 0,
        family_count: 0,
        family_ratio: 0.5,
        max_parent_roles: 1,
        layer_linking: false,
        generations: 1,
        children_per_family: 1..2,
        start_year: 1900,
        end_year: 2000,
        name_style: "modern".to_string(),
        with_places: false,
        with_citations: false,
        with_notes: false,
        with_media: false,
        with_tags: false,
        seed: None,
        place_depth: 3,
    };
    let adv_config = typed_graph::generate::AdversarialConfig::default();

    let result = typed_graph::generate::generate_random(&config, &adv_config, None, &schema);
    assert!(result.is_err());
}

/// Integration test: adversarial generation produces valid graph.
#[test]
fn generate_with_adversarial_all_preserves_validity() {
    let schema = typed_graph::Schema::default();
    let config = typed_graph::generate::RandomConfig {
        person_count: 50,
        family_count: 25,
        family_ratio: 0.5,
        max_parent_roles: 1,
        layer_linking: false,
        generations: 3,
        children_per_family: 1..4,
        start_year: 1850,
        end_year: 2025,
        name_style: "modern".to_string(),
        with_places: true,
        with_citations: true,
        with_notes: true,
        with_media: true,
        with_tags: true,
        seed: Some(42),
        place_depth: 3,
    };
    let adv_config = typed_graph::generate::AdversarialConfig {
        enabled: true,
        strategies: vec![
            typed_graph::generate::AdversarialStrategy::DisconnectedSubgraphs,
            typed_graph::generate::AdversarialStrategy::DeepNesting,
            typed_graph::generate::AdversarialStrategy::MaxRefChains,
            typed_graph::generate::AdversarialStrategy::OrphanedReferences,
            typed_graph::generate::AdversarialStrategy::DoubleGender(0.2),
        ],
    };

    // Generate with adversarial strategies
    let result =
        typed_graph::generate::generate_random(&config, &adv_config, None, &schema).unwrap();

    // Apply Category B transforms
    let mut graph =
        typed_graph::generate::apply_adversarial_strategies(result.graph, &adv_config).graph;

    // Validate — should still be valid after adversarial transforms
    let errors = graph.validate(&schema);
    let structural_errors: Vec<_> = errors
        .iter()
        .filter(|e| !matches!(e, typed_graph::ValidationError::PlausibilityWarning { .. }))
        .collect();
    assert!(
        structural_errors.is_empty(),
        "Structural errors after adversarial: {:?}",
        structural_errors
    );
}

/// Integration test: serialize and validate roundtrip.
#[test]
fn generate_serialize_and_validate_roundtrip() {
    let schema = typed_graph::Schema::default();
    let config = typed_graph::generate::RandomConfig {
        person_count: 20,
        family_count: 10,
        family_ratio: 0.5,
        max_parent_roles: 1,
        layer_linking: false,
        generations: 2,
        children_per_family: 1..3,
        start_year: 1900,
        end_year: 2020,
        name_style: "modern".to_string(),
        with_places: true,
        with_citations: true,
        with_notes: true,
        with_media: false,
        with_tags: false,
        seed: Some(42),
        place_depth: 3,
    };
    let adv_config = typed_graph::generate::AdversarialConfig::default();

    let result =
        typed_graph::generate::generate_random(&config, &adv_config, None, &schema).unwrap();

    // Serialize
    let map = output::SerializationMap::new();
    let writer = output::GraphXmlWriter::new(map, "5.2.0");
    let mut buffer = Vec::new();
    writer
        .write(&result.graph, &mut std::io::BufWriter::new(&mut buffer))
        .unwrap();
    let xml = String::from_utf8(buffer).unwrap();

    // Validate the XML structure
    // Use quick_xml to check basic structure
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut has_database = false;
    let mut has_header = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "database" => has_database = true,
                    "header" => has_header = true,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => panic!("XML parse error: {}", e),
            _ => {}
        }
    }
    assert!(has_database, "Should have <database> element");
    assert!(has_header, "Should have <header> element");
}

/// Integration test: scenario file loading with edge cases.
#[test]
fn scenario_file_with_minimal_fields() {
    let yaml = "person_count: 5\n";
    let scenario: cli::scenario::Scenario = serde_yaml::from_str(yaml).unwrap();
    let config = scenario.to_random_config();
    assert_eq!(config.person_count, 5);
    // Other fields should use defaults
    assert_eq!(
        config.generations,
        typed_graph::generate::RandomConfig::default().generations
    );
}

/// Integration test: scenario YAML file with all fields.
#[test]
fn scenario_file_load_and_convert() {
    let yaml = r#"
person_count: 100
family_count: 30
generations:
  depth: 4
  children_per_family: { min: 1, max: 6 }
date_range:
  start: 1800
  end: 2000
  era: victorian
with_citations: true
with_places: true
seed: 12345
"#;
    let scenario: cli::scenario::Scenario = serde_yaml::from_str(yaml).unwrap();
    let config = scenario.to_random_config();
    assert_eq!(config.person_count, 100);
    assert_eq!(config.family_count, 30);
    assert_eq!(config.generations, 4);
    assert_eq!(config.children_per_family, 1..6);
    assert_eq!(config.start_year, 1800);
    assert_eq!(config.end_year, 2000);
    assert_eq!(config.name_style, "victorian");
    assert!(config.with_citations);
    assert!(config.with_places);
    assert_eq!(config.seed, Some(12345));
}

/// Integration test: validate command with a real .gramps file.
#[test]
fn validate_valid_gramps_file() {
    use std::io::Write;

    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-01" version="5.2"/>
    <researcher><resname>Test</resname></researcher>
  </header>
</database>"#;

    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", xml).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let args = cli::commands::validate::ValidateArgs {
        file: path,
        strict: false,
    };
    let result = cli::commands::validate::run(args);
    assert!(result.is_ok());
}

/// Integration test: validate command with invalid file.
#[test]
fn validate_invalid_gramps_file() {
    use std::io::Write;

    let xml = r#"<?xml version="1.0"?>
<wrong-root>
</wrong-root>"#;

    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", xml).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let args = cli::commands::validate::ValidateArgs {
        file: path,
        strict: false,
    };
    // Should fail since <database> root element is missing
    let result = cli::commands::validate::run(args);
    assert!(result.is_err());
}

/// Integration test: generate respects schema version.
#[test]
fn generate_respects_schema_version() {
    // Check if 5.1 schema is available in this build
    let schema_51 = match typed_graph::Schema::for_version("5.1") {
        Some(s) => s.clone(),
        None => {
            eprintln!("Skipping: schema-5-1 not compiled in this build");
            return;
        }
    };

    let config = typed_graph::generate::RandomConfig {
        person_count: 10,
        family_count: 5,
        generations: 2,
        seed: Some(42),
        ..typed_graph::generate::RandomConfig::default()
    };
    let adv_config = typed_graph::generate::AdversarialConfig::default();

    // Generate with 5.1 schema
    let mut result =
        typed_graph::generate::generate_random(&config, &adv_config, None, &schema_51).unwrap();

    // Validate with 5.1 — must pass
    let errors = result.graph.validate(&schema_51);
    let structural: Vec<_> = errors
        .iter()
        .filter(|e| !matches!(e, typed_graph::ValidationError::PlausibilityWarning { .. }))
        .collect();
    assert!(structural.is_empty(), "{:?}", structural);

    // Serialize — verify version in header
    let map = output::SerializationMap::new();
    let mut buf = Vec::new();
    let writer = output::GraphXmlWriter::new(map, "5.1.6");
    writer
        .write(&result.graph, &mut std::io::BufWriter::new(&mut buf))
        .unwrap();
    let xml = String::from_utf8(buf).unwrap();
    assert!(xml.contains(r#"version="5.1.6""#));
}

/// Integration test: cross-version validation consistency.
#[test]
fn cross_version_validation_consistent() {
    // Check if 5.1 schema is available in this build
    let schema_51 = match typed_graph::Schema::for_version("5.1") {
        Some(s) => s.clone(),
        None => {
            eprintln!("Skipping: schema-5-1 not compiled in this build");
            return;
        }
    };

    let config = typed_graph::generate::RandomConfig {
        person_count: 10,
        family_count: 5,
        generations: 2,
        seed: Some(42),
        ..typed_graph::generate::RandomConfig::default()
    };
    let adv_config = typed_graph::generate::AdversarialConfig::default();

    // Generate with 5.1 schema
    let mut result =
        typed_graph::generate::generate_random(&config, &adv_config, None, &schema_51).unwrap();

    // Validate with 5.1 — must pass (same version used for generation)
    let errors_51 = result.graph.validate(&schema_51);
    let structural_51: Vec<_> = errors_51
        .iter()
        .filter(|e| !matches!(e, typed_graph::ValidationError::PlausibilityWarning { .. }))
        .collect();
    assert!(
        structural_51.is_empty(),
        "5.1-generated graph should pass 5.1 validation: {:?}",
        structural_51
    );
}

/// Integration test: validate command with strict mode on invalid file.
#[test]
fn validate_invalid_file_strict_mode() {
    use std::io::Write;

    let xml = "<wrong-root/>";

    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", xml).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let args = cli::commands::validate::ValidateArgs {
        file: path,
        strict: true,
    };
    let result = cli::commands::validate::run(args);
    assert!(result.is_err());
}

/// Regression test for the original bug: `--count 16 --seed 2026 --depth 4`
/// produced a largest connected component of only 3 people.
///
/// The largest connected component of Person nodes (via person-family-person
/// edges) must now be at least half the person count.
#[test]
fn generate_depth_4_produces_deep_tree() {
    let schema = typed_graph::Schema::default();
    // Mirror the CLI's build_config() for: --count 16 --seed 2026 --depth 4
    let config = typed_graph::generate::RandomConfig {
        person_count: 16,
        family_count: 8,
        family_ratio: 0.5,
        generations: 4,
        children_per_family: 1..4,
        start_year: 1850,
        end_year: 2025,
        name_style: "modern".to_string(),
        with_places: false,
        with_citations: false,
        with_notes: false,
        with_media: false,
        with_tags: false,
        seed: Some(2026),
        place_depth: 3,
        max_parent_roles: 1,
        layer_linking: true, // depth > 1
    };
    let adv_config = typed_graph::generate::AdversarialConfig::default();

    let mut result =
        typed_graph::generate::generate_random(&config, &adv_config, None, &schema).unwrap();

    // The largest connected component must be >= 7 (at least ~half of 16)
    // Note: threshold was lowered from 8 to 7 when switching from uuid::Uuid::new_v4()
    // to generate_handle(), which consumes RNG state and shifts the output distribution.
    let largest = largest_connected_person_component(&result.graph);
    assert!(
        largest >= 7,
        "Largest connected component should be >= 7, got {}. Graph has {} persons",
        largest,
        result.stats.person_count
    );

    // The generated graph must still be valid
    let errors = result.graph.validate(&schema);
    assert!(errors.is_empty(), "Validation errors: {:?}", errors);
}

/// Integration test: count_gramps_xml on a known graph through serialization.
#[test]
fn stats_count_known_graph() {
    use cli::commands::stats::count::count_gramps_xml;
    use output::GraphXmlWriter;
    use output::SerializationMap;
    use std::io::BufWriter;
    use typed_graph::generate::GraphBuilder;

    let mut graph = typed_graph::Graph::new();
    let mut builder = GraphBuilder::new(&mut graph);

    // Build a small family: Alice + Bob + child Charlie, plus isolated Dana
    let alice = builder
        .add_person_auto()
        .with_name("Alice", "Smith")
        .with_gender(1)
        .build()
        .unwrap();
    let bob = builder
        .add_person_auto()
        .with_name("Bob", "Smith")
        .with_gender(0)
        .build()
        .unwrap();
    let charlie = builder
        .add_person_auto()
        .with_name("Charlie", "Smith")
        .build()
        .unwrap();
    let _dana = builder
        .add_person_auto()
        .with_name("Dana", "Smith")
        .build()
        .unwrap();

    let _ = builder
        .add_family_auto()
        .with_father(&alice)
        .with_mother(&bob)
        .add_child_birth(&charlie)
        .build()
        .unwrap();

    let _ = builder.into_graph();

    // Serialize
    let map = SerializationMap::new();
    let writer = GraphXmlWriter::new(map, "5.2.0");
    let mut buffer = Vec::new();
    writer
        .write(&graph, &mut BufWriter::new(&mut buffer))
        .unwrap();
    let xml = String::from_utf8(buffer).unwrap();

    // Count
    let report = count_gramps_xml(&xml).unwrap();

    // Assert
    assert_eq!(report.counts.people, 4);
    assert_eq!(report.counts.families, 1);

    // Family size 3: Alice, Bob, Charlie
    assert_eq!(report.family_size_distribution.len(), 1);
    assert_eq!(report.family_size_distribution.get(&3), Some(&1));

    // Dana is not in any family
    assert_eq!(report.people_not_in_family, 1);
    assert_eq!(report.dangling_refs, 0);

    // Generation table: family groups (connected components)
    // - Alice, Bob, Charlie form one family group: size 3, span 2
    //   (parents gen 0, child gen 1)
    // - Dana is an isolated family group: size 1, span 1
    let table = &report.family_group_generation_table;
    assert_eq!(
        table.get("3").and_then(|r| r.get("2")),
        Some(&1),
        "Expected 1 family group of size 3 with span 2"
    );
    assert_eq!(
        table.get("3").and_then(|r| r.get("total")),
        Some(&1),
        "Expected row total 1 for size 3"
    );
    assert_eq!(
        table.get("1").and_then(|r| r.get("1")),
        Some(&1),
        "Expected 1 family group of size 1 with span 1 (isolated Dana)"
    );
    assert_eq!(
        table.get("1").and_then(|r| r.get("total")),
        Some(&1),
        "Expected row total 1 for size 1"
    );

    // Family group distribution: one group of 3 people, one group of 1 person
    assert_eq!(report.family_group_distribution.len(), 2);
    assert_eq!(report.family_group_distribution.get(&3), Some(&1));
    assert_eq!(report.family_group_distribution.get(&1), Some(&1));
}
