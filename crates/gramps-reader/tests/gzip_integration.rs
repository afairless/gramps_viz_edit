//! Integration tests for gzip-compressed `.gramps` files.
//!
//! These tests verify that the full pipeline works correctly with
//! compressed fixtures: reading, counting, and graph building all
//! operate transparently on gzip-compressed input.

/// Helper to get the compressed fixture path.
fn fixture_path() -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR must be set");
    std::path::Path::new(&dir)
        .join("tests")
        .join("fixtures")
        .join("gramps-ui-gen01.gramps")
        .to_string_lossy()
        .to_string()
}

/// The expected uncompressed XML content, computed once for all tests.
fn expected_xml_content() -> String {
    let path = fixture_path();
    gramps_reader::read_gramps_file(&path).unwrap()
}

#[test]
fn gzip_round_trip_matches_known_content() {
    let content = expected_xml_content();

    // Verify it's valid XML.
    assert!(content.starts_with("<?xml"), "Should start with XML declaration");
    assert!(content.contains("<database"), "Should contain database element");
    assert!(content.contains("</database>"), "Should close database element");

    // Verify expected elements from the fixture.
    assert!(content.contains("<people>"), "Should have people section");
    assert!(content.contains("<events>"), "Should have events section");
    assert!(content.contains("<families>"), "Should have families section");
    assert!(content.contains("<header>"), "Should have header section");

    // Verify specific content.
    assert!(content.contains("Harry"), "Should contain known person: Harry");
    assert!(content.contains("Meowser"), "Should contain surname: Meowser");
    assert!(content.contains("Sally"), "Should contain person: Sally");
    assert!(content.contains("Furball"), "Should contain surname: Furball");
    assert!(content.contains("George"), "Should contain person: George");
}

#[test]
fn gzip_counts_match_expected() {
    let content = expected_xml_content();
    let report = gramps_reader::count_gramps_xml(&content).unwrap();

    // Known counts from the fixture:
    //   3 people, 3 events, 1 family
    assert_eq!(report.counts.people, 3, "Expected 3 people");
    assert_eq!(report.counts.events, 3, "Expected 3 events");
    assert_eq!(report.counts.families, 1, "Expected 1 family");
    assert_eq!(report.counts.places, 0, "Expected 0 places");
    assert_eq!(report.counts.sources, 0, "Expected 0 sources");
    assert_eq!(report.counts.citations, 0, "Expected 0 citations");
    assert_eq!(report.counts.repositories, 0, "Expected 0 repositories");
    assert_eq!(report.counts.media, 0, "Expected 0 media objects");
    assert_eq!(report.counts.notes, 0, "Expected 0 notes");
    assert_eq!(report.counts.tags, 0, "Expected 0 tags");

    // People not in any family: all 3 people are in the family.
    // Harry is a child, George is father, Sally is mother.
    assert_eq!(report.people_not_in_family, 0, "All people should be in a family");
    assert_eq!(report.dangling_refs, 0, "No dangling refs expected");
}

#[test]
fn gzip_graph_data_populated_correctly() {
    let content = expected_xml_content();

    // Build graph data from the decompressed content.
    let persons = gramps_reader::extract_persons(&content).unwrap();
    let events = gramps_reader::extract_events(&content).unwrap();
    let families = gramps_reader::extract_families(&content).unwrap();

    assert_eq!(persons.len(), 3, "Expected 3 persons");
    assert_eq!(events.len(), 3, "Expected 3 events");
    assert_eq!(families.len(), 1, "Expected 1 family");

    // Verify Harry (child) has handle reference.
    let harry = persons.iter().find(|p| {
        p.given_name.as_deref() == Some("Harry")
    }).unwrap();
    assert_eq!(harry.gender.as_deref(), Some("M"), "Harry should be male");

    let george = persons.iter().find(|p| {
        p.given_name.as_deref() == Some("George")
    }).unwrap();
    assert_eq!(george.gender.as_deref(), Some("M"), "George should be male");

    let sally = persons.iter().find(|p| {
        p.given_name.as_deref() == Some("Sally")
    }).unwrap();
    assert_eq!(sally.gender.as_deref(), Some("F"), "Sally should be female");

    // Verify the family has one parent family.
    let family = &families[0];
    assert!(family.father_handle.is_some(), "Family should have a father");
    assert!(family.mother_handle.is_some(), "Family should have a mother");
    assert_eq!(family.child_handles.len(), 1, "Family should have 1 child");
}