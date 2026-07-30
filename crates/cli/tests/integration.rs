//! Integration tests for the CLI crate.

use tempfile::NamedTempFile;

/// Integration test: generate a small valid family tree.
#[test]
fn generate_small_family_tree() {
    let schema = typed_graph::Schema::new();
    let config = typed_graph::generate::RandomConfig {
        person_count: 10,
        family_count: 5,
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

    let mut result = typed_graph::generate::generate_random(&config, &adv_config, &schema).unwrap();
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
    let schema = typed_graph::Schema::new();
    let config = typed_graph::generate::RandomConfig {
        person_count: 1,
        family_count: 0,
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

    let mut result = typed_graph::generate::generate_random(&config, &adv_config, &schema).unwrap();
    assert_eq!(result.stats.person_count, 1);
    assert!(result.stats.family_count == 0);

    // A single person should still have a valid graph
    let errors = result.graph.validate(&schema);
    assert!(errors.is_empty(), "Validation errors: {:?}", errors);
}

/// Integration test: zero person count should fail gracefully.
#[test]
fn generate_zero_persons_fails() {
    let schema = typed_graph::Schema::new();
    let config = typed_graph::generate::RandomConfig {
        person_count: 0,
        family_count: 0,
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

    let result = typed_graph::generate::generate_random(&config, &adv_config, &schema);
    assert!(result.is_err());
}

/// Integration test: adversarial generation produces valid graph.
#[test]
fn generate_with_adversarial_all_preserves_validity() {
    let schema = typed_graph::Schema::new();
    let config = typed_graph::generate::RandomConfig {
        person_count: 50,
        family_count: 25,
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
    let result = typed_graph::generate::generate_random(&config, &adv_config, &schema).unwrap();

    // Apply Category B transforms
    let mut graph = typed_graph::generate::apply_adversarial_strategies(result.graph, &adv_config).graph;

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
    let schema = typed_graph::Schema::new();
    let config = typed_graph::generate::RandomConfig {
        person_count: 20,
        family_count: 10,
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

    let result = typed_graph::generate::generate_random(&config, &adv_config, &schema).unwrap();

    // Serialize
    let map = output::SerializationMap::new();
    let writer = output::GraphXmlWriter::new(map, "5.2");
    let mut buffer = Vec::new();
    writer.write(&result.graph, &mut std::io::BufWriter::new(&mut buffer)).unwrap();
    let xml = String::from_utf8(buffer).unwrap();

    // Validate the XML structure
    // Use quick_xml to check basic structure
    use quick_xml::Reader;
    use quick_xml::events::Event;

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
    assert_eq!(config.generations, typed_graph::generate::RandomConfig::default().generations);
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