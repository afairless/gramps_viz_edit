//! Streaming counting logic for the `stats` command.
//!
//! `count_gramps_xml` scans a `.gramps` XML document in a single
//! streaming pass and produces a [`StatsReport`] without
//! reconstructing the full typed graph. It is a pure function over
//! `&str` so it can be unit-tested without filesystem access.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::CliError;

/// Statistics collected from a single `.gramps` XML document.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StatsReport {
    /// Path of the analyzed file (filled in by the CLI).
    pub file: String,
    /// Counts per primary object type.
    pub counts: PrimaryTypeCounts,
    /// Family size → number of families, ascending by size.
    pub family_size_distribution: BTreeMap<usize, usize>,
    /// People whose handle never appears in any family.
    pub people_not_in_family: usize,
    /// Family `ref` handles without a matching `<person>`.
    pub dangling_refs: usize,
    /// Non-fatal warnings emitted during the scan.
    pub warnings: Vec<String>,
}

/// Counts for the ten primary object types.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrimaryTypeCounts {
    pub people: usize,
    pub families: usize,
    pub events: usize,
    pub places: usize,
    pub sources: usize,
    pub citations: usize,
    pub repositories: usize,
    pub media: usize,
    pub notes: usize,
    pub tags: usize,
}

/// Strip an optional namespace prefix from an element name.
///
/// `prefix:person` → `"person"`, `person` → `"person"`.
fn strip_prefix(name: &[u8]) -> &[u8] {
    name.iter()
        .rposition(|&b| b == b':')
        .map_or(name, |pos| &name[pos + 1..])
}

/// Scan a `.gramps` XML document and produce a [`StatsReport`].
///
/// Returns `Err(CliError::XmlParseError)` when the content is not
/// well-formed XML.
pub fn count_gramps_xml(content: &str) -> Result<StatsReport, CliError> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut report = StatsReport::default();
    let mut all_handles: HashSet<String> = HashSet::new();
    let mut ref_handles: HashSet<String> = HashSet::new();
    let mut family_members: Vec<HashSet<String>> = Vec::new();
    let mut histogram: HashMap<usize, usize> = HashMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = e.name().as_ref().to_vec();
                let name = strip_prefix(&name);
                match name {
                    b"person" => {
                        report.counts.people += 1;
                        if let Some(h) = read_handle_attr(e) {
                            all_handles.insert(h);
                        }
                    }
                    b"family" => {
                        report.counts.families += 1;
                        family_members.push(HashSet::new());
                    }
                    b"event" => report.counts.events += 1,
                    b"placeobj" => report.counts.places += 1,
                    b"source" => report.counts.sources += 1,
                    b"citation" => report.counts.citations += 1,
                    b"repository" => report.counts.repositories += 1,
                    b"object" => report.counts.media += 1,
                    b"note" => report.counts.notes += 1,
                    b"tag" => report.counts.tags += 1,
                    b"father" | b"mother" | b"childref" => {
                        if let Some(ref_handle) = read_hlink_attr(e) {
                            if let Some(current) = family_members.last_mut() {
                                current.insert(ref_handle.clone());
                            }
                            ref_handles.insert(ref_handle);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name().as_ref().to_vec();
                let name = strip_prefix(&name);
                match name {
                    b"person" => {
                        report.counts.people += 1;
                        if let Some(h) = read_handle_attr(e) {
                            all_handles.insert(h);
                        }
                    }
                    b"family" => {
                        report.counts.families += 1;
                        // Self-closing family: size 0
                        *histogram.entry(0).or_insert(0) += 1;
                    }
                    b"event" => report.counts.events += 1,
                    b"placeobj" => report.counts.places += 1,
                    b"source" => report.counts.sources += 1,
                    b"citation" => report.counts.citations += 1,
                    b"repository" => report.counts.repositories += 1,
                    b"object" => report.counts.media += 1,
                    b"note" => report.counts.notes += 1,
                    b"tag" => report.counts.tags += 1,
                    b"father" | b"mother" | b"childref" => {
                        if let Some(ref_handle) = read_hlink_attr(e) {
                            if let Some(current) = family_members.last_mut() {
                                current.insert(ref_handle.clone());
                            }
                            ref_handles.insert(ref_handle);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name().as_ref().to_vec();
                let name = strip_prefix(&name);
                if name == b"family" {
                    if let Some(members) = family_members.pop() {
                        let size = members.len();
                        *histogram.entry(size).or_insert(0) += 1;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(CliError::XmlParseError {
                    message: format!("{} at byte {}", e, reader.error_position()),
                });
            }
        }
    }

    // Build sorted histogram
    let mut sizes: Vec<usize> = histogram.keys().copied().collect();
    sizes.sort();
    for size in sizes {
        report.family_size_distribution.insert(size, histogram[&size]);
    }

    // Compute people_not_in_family and dangling_refs
    report.people_not_in_family = all_handles.len().saturating_sub(
        ref_handles.intersection(&all_handles).count(),
    );
    report.dangling_refs = ref_handles.len() - ref_handles.intersection(&all_handles).count();

    Ok(report)
}

/// Read the `handle` attribute from an element.
fn read_handle_attr(e: &quick_xml::events::BytesStart) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"handle" || key.ends_with(b":handle") || key.ends_with(b"\"handle") {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}

/// Read the `hlink` attribute from an element.
fn read_hlink_attr(e: &quick_xml::events::BytesStart) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"hlink" || key.ends_with(b":hlink") || key.ends_with(b"\"hlink") {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_empty_content_returns_zeroed_report() {
        let report = count_gramps_xml("").unwrap();
        assert_eq!(report, StatsReport::default());
        assert_eq!(report.counts.people, 0);
        assert_eq!(report.counts.families, 0);
        assert!(report.family_size_distribution.is_empty());
        assert_eq!(report.people_not_in_family, 0);
        assert_eq!(report.dangling_refs, 0);
    }

    #[test]
    fn count_malformed_xml_returns_xml_parse_error() {
        let result = count_gramps_xml("<database><person></database>");
        match result {
            Err(CliError::XmlParseError { .. }) => {}
            other => panic!("Expected XmlParseError, got: {:?}", other),
        }
    }

    #[test]
    fn count_all_ten_types() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <tags><tag handle="t1"/><tag handle="t2"/></tags>
  <events><event handle="e1"/><event handle="e2"/><event handle="e3"/></events>
  <people>
    <person handle="p1"/>
    <person handle="p2"/>
    <person handle="p3"/>
    <person handle="p4"/>
  </people>
  <families>
    <family handle="f1"/>
    <family handle="f2"/>
  </families>
  <citations>
    <citation handle="c1"/>
  </citations>
  <sources>
    <source handle="s1"/>
    <source handle="s2"/>
    <source handle="s3"/>
  </sources>
  <places>
    <placeobj handle="pl1"/>
  </places>
  <objects>
    <object handle="o1"/>
    <object handle="o2"/>
    <object handle="o3"/>
    <object handle="o4"/>
  </objects>
  <repositories>
    <repository handle="r1"/>
  </repositories>
  <notes>
    <note handle="n1"/>
    <note handle="n2"/>
  </notes>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();
        assert_eq!(report.counts.people, 4);
        assert_eq!(report.counts.families, 2);
        assert_eq!(report.counts.events, 3);
        assert_eq!(report.counts.places, 1);
        assert_eq!(report.counts.sources, 3);
        assert_eq!(report.counts.citations, 1);
        assert_eq!(report.counts.repositories, 1);
        assert_eq!(report.counts.media, 4);
        assert_eq!(report.counts.notes, 2);
        assert_eq!(report.counts.tags, 2);
    }

    #[test]
    fn count_empty_database_returns_zero() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();
        let zero = PrimaryTypeCounts::default();
        assert_eq!(report.counts, zero);
    }

    #[test]
    fn count_self_closing_elements() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"/>
  </people>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();
        assert_eq!(report.counts.people, 1);
    }

    #[test]
    fn count_namespace_prefixed_input() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:header><ns:created date="2025-01-01" version="5.2"/></ns:header>
  <ns:people>
    <ns:person handle="p1"/>
    <ns:person handle="p2"/>
  </ns:people>
  <ns:families>
    <ns:family handle="f1"/>
  </ns:families>
</ns:database>"#;
        let report = count_gramps_xml(xml).unwrap();
        assert_eq!(report.counts.people, 2);
        assert_eq!(report.counts.families, 1);
    }

    #[test]
    fn count_family_histogram_example_scenario() {
        // 7 families: sizes 10, 10, 3, 3, 3, 3, 3
        // 15 isolated people (not in any family)
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people>
    <person handle="p01"/><person handle="p02"/><person handle="p03"/>
    <person handle="p04"/><person handle="p05"/><person handle="p06"/>
    <person handle="p07"/><person handle="p08"/><person handle="p09"/>
    <person handle="p10"/><person handle="p11"/><person handle="p12"/>
    <person handle="p13"/><person handle="p14"/><person handle="p15"/>
    <person handle="p16"/><person handle="p17"/><person handle="p18"/>
    <person handle="p19"/><person handle="p20"/><person handle="p21"/>
    <person handle="p22"/><person handle="p23"/><person handle="p24"/>
    <person handle="p25"/><person handle="p26"/><person handle="p27"/>
    <person handle="p28"/><person handle="p29"/><person handle="p30"/>
    <person handle="p31"/><person handle="p32"/><person handle="p33"/>
    <person handle="p34"/><person handle="p35"/><person handle="p36"/>
    <person handle="p37"/><person handle="p38"/><person handle="p39"/>
    <person handle="p40"/><person handle="p41"/><person handle="p42"/>
    <person handle="p43"/><person handle="p44"/><person handle="p45"/>
    <person handle="p46"/><person handle="p47"/><person handle="p48"/>
    <person handle="p49"/><person handle="p50"/>
  </people>
  <families>
    <!-- Family size 10 -->
    <family handle="f01">
      <father hlink="p01"/><mother hlink="p02"/>
      <childref hlink="p03"/><childref hlink="p04"/><childref hlink="p05"/>
      <childref hlink="p06"/><childref hlink="p07"/><childref hlink="p08"/>
      <childref hlink="p09"/><childref hlink="p10"/>
    </family>
    <!-- Family size 10 -->
    <family handle="f02">
      <father hlink="p11"/><mother hlink="p12"/>
      <childref hlink="p13"/><childref hlink="p14"/><childref hlink="p15"/>
      <childref hlink="p16"/><childref hlink="p17"/><childref hlink="p18"/>
      <childref hlink="p19"/><childref hlink="p20"/>
    </family>
    <!-- Family size 3 -->
    <family handle="f03"><father hlink="p21"/><mother hlink="p22"/><childref hlink="p23"/></family>
    <!-- Family size 3 -->
    <family handle="f04"><father hlink="p24"/><mother hlink="p25"/><childref hlink="p26"/></family>
    <!-- Family size 3 -->
    <family handle="f05"><father hlink="p27"/><mother hlink="p28"/><childref hlink="p29"/></family>
    <!-- Family size 3 -->
    <family handle="f06"><father hlink="p30"/><mother hlink="p31"/><childref hlink="p32"/></family>
    <!-- Family size 3 -->
    <family handle="f07"><father hlink="p33"/><mother hlink="p34"/><childref hlink="p35"/></family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        // 35 people in families (p01-p35), 15 isolated (p36-p50)
        assert_eq!(report.counts.people, 50);
        assert_eq!(report.counts.families, 7);

        // Histogram: size 10: 2 families, size 3: 5 families
        assert_eq!(report.family_size_distribution.len(), 2);
        assert_eq!(
            report.family_size_distribution.get(&3),
            Some(&5)
        );
        assert_eq!(
            report.family_size_distribution.get(&10),
            Some(&2)
        );

        // 15 people not in any family (p36-p50)
        assert_eq!(report.people_not_in_family, 15);
        assert_eq!(report.dangling_refs, 0);
    }

    #[test]
    fn count_family_duplicate_refs_counted_once() {
        // Same handle appears as both father and childref
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people>
    <person handle="p1"/><person handle="p2"/><person handle="p3"/>
  </people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p1"/>
      <childref hlink="p1"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        // Size 3: p1, p2, p3 (p1 counted once)
        assert_eq!(report.family_size_distribution.len(), 1);
        assert_eq!(report.family_size_distribution.get(&3), Some(&1));

        // All 3 people are in families
        assert_eq!(report.people_not_in_family, 0);
        assert_eq!(report.dangling_refs, 0);
    }

    #[test]
    fn count_empty_family_size_zero() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people><person handle="p1"/></people>
  <families>
    <!-- Empty family with no refs -->
    <family handle="f1">
    </family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        assert_eq!(report.family_size_distribution.len(), 1);
        assert_eq!(report.family_size_distribution.get(&0), Some(&1));

        // p1 is not in any family
        assert_eq!(report.people_not_in_family, 1);
    }

    #[test]
    fn count_dangling_refs_reported() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people><person handle="p1"/></people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        // p1 is in a family, p2 and p3 are dangling
        assert_eq!(report.people_not_in_family, 0);
        assert_eq!(report.dangling_refs, 2);

        // Family size is 3 (p1, p2, p3) even though p2/p3 are dangling
        assert_eq!(report.family_size_distribution.get(&3), Some(&1));
    }
}