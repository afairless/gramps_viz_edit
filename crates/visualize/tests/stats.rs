//! Integration tests for `visualize::get_stats`.
//!
//! Tests the `get_stats` function end-to-end, verifying that a `.gramps`
//! file is parsed correctly into a `StatsReport` with accurate counts,
//! distributions, and data quality indicators.

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
// Simple family
// ---------------------------------------------------------------------------

#[test]
fn stats_simple_family() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>John</first><surname>Smith</surname></name></person>
    <person handle="p2"><gender>F</gender><name><first>Jane</first><surname>Smith</surname></name></person>
    <person handle="p3"><gender>M</gender><name><first>Jim</first><surname>Smith</surname></name></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><mother hlink="p2"/><childref hlink="p3"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    let report = visualize::get_stats(path.to_str().unwrap()).unwrap();

    // 3 people, 1 family
    assert_eq!(report.counts.people, 3);
    assert_eq!(report.counts.families, 1);
    assert_eq!(report.counts.events, 0);
    assert_eq!(report.counts.places, 0);
    assert_eq!(report.counts.sources, 0);
    assert_eq!(report.counts.citations, 0);
    assert_eq!(report.counts.repositories, 0);
    assert_eq!(report.counts.media, 0);
    assert_eq!(report.counts.notes, 0);
    assert_eq!(report.counts.tags, 0);

    // Family size distribution: 1 family of size 3
    let mut fam_size = report.family_size_distribution.clone();
    let size3 = fam_size.remove(&3).unwrap_or(0);
    assert_eq!(size3, 1, "expected 1 family of size 3, got {:#?}", report.family_size_distribution);
    // No other family sizes
    assert!(fam_size.is_empty(), "unexpected family sizes: {:#?}", fam_size);

    // Family group distribution: 1 group of size 3
    let mut group_dist = report.family_group_distribution.clone();
    let group3 = group_dist.remove(&3).unwrap_or(0);
    assert_eq!(group3, 1, "expected 1 group of size 3, got {:#?}", report.family_group_distribution);
    assert!(group_dist.is_empty(), "unexpected group sizes: {:#?}", group_dist);

    // All 3 people are in a family
    assert_eq!(report.people_not_in_family, 0);
    // No dangling refs
    assert_eq!(report.dangling_refs, 0);
}

// ---------------------------------------------------------------------------
// Multi-family with people not in a family
// ---------------------------------------------------------------------------

#[test]
fn stats_people_not_in_family() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>John</first><surname>Smith</surname></name></person>
    <person handle="p2"><gender>F</gender><name><first>Jane</first><surname>Smith</surname></name></person>
    <person handle="p3"><gender>M</gender><name><first>Jim</first><surname>Smith</surname></name></person>
    <person handle="p4"><gender>M</gender><name><first>Bob</first><surname>Jones</surname></name></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><mother hlink="p2"/><childref hlink="p3"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    let report = visualize::get_stats(path.to_str().unwrap()).unwrap();

    assert_eq!(report.counts.people, 4);
    assert_eq!(report.counts.families, 1);
    // p4 (Bob Jones) is not in any family
    assert_eq!(report.people_not_in_family, 1);
}

// ---------------------------------------------------------------------------
// Empty file returns StatsReport::default()
// ---------------------------------------------------------------------------

#[test]
fn stats_empty_file() {
    let path = write_gramps_file("");
    let report = visualize::get_stats(path.to_str().unwrap()).unwrap();
    assert_eq!(report, gramps_reader::StatsReport::default());
}

// ---------------------------------------------------------------------------
// File with events and places
// ---------------------------------------------------------------------------

#[test]
fn stats_with_events_and_places() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e1"><eventtype><type>Birth</type></eventtype></event>
    <event handle="e2"><eventtype><type>Death</type></eventtype></event>
  </events>
  <people>
    <person handle="p1"><gender>M</gender><name><first>John</first><surname>Smith</surname></name></person>
  </people>
  <places>
    <placeobj handle="pl1"><name><value>New York</value></name></placeobj>
  </places>
</database>"#;

    let path = write_gramps_file(xml);
    let report = visualize::get_stats(path.to_str().unwrap()).unwrap();

    assert_eq!(report.counts.people, 1);
    assert_eq!(report.counts.events, 2);
    assert_eq!(report.counts.places, 1);
    // p1 is not in any family
    assert_eq!(report.people_not_in_family, 1);
}

// ---------------------------------------------------------------------------
// Round-trip through serde_json
// ---------------------------------------------------------------------------

#[test]
fn stats_serde_roundtrip() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender><name><first>John</first><surname>Smith</surname></name></person>
    <person handle="p2"><gender>F</gender><name><first>Jane</first><surname>Smith</surname></name></person>
  </people>
  <families>
    <family handle="f1"><father hlink="p1"/><mother hlink="p2"/></family>
  </families>
</database>"#;

    let path = write_gramps_file(xml);
    let report = visualize::get_stats(path.to_str().unwrap()).unwrap();

    // Serialize to JSON and back
    let json = serde_json::to_string(&report).unwrap();
    let deserialized: gramps_reader::StatsReport = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.counts.people, 2);
    assert_eq!(deserialized.counts.families, 1);
    assert_eq!(deserialized.people_not_in_family, 0);
}