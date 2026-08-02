//! Streaming counting logic for the `stats` command.
//!
//! `count_gramps_xml` scans a `.gramps` XML document in a single
//! streaming pass and produces a [`StatsReport`] without
//! reconstructing the full typed graph. It is a pure function over
//! `&str` so it can be unit-tested without filesystem access.

use crate::error::CliError;

/// One histogram bucket: how many families of a given size exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilySizeBucket {
    /// Number of families of this size.
    pub families: usize,
    /// Total people across those families (size × families).
    pub people: usize,
}

/// Statistics collected from a single `.gramps` XML document.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub struct StatsReport {
    /// Counts per primary object type.
    pub counts: PrimaryTypeCounts,
    /// Families grouped by member count, ascending by size.
    pub family_size_distribution: Vec<(usize, FamilySizeBucket)>,
    /// People whose handle never appears in any family.
    pub people_not_in_family: usize,
    /// Family `ref` handles without a matching `<person>`.
    pub dangling_refs: usize,
    /// Non-fatal warnings emitted during the scan.
    pub warnings: Vec<String>,
}

/// Counts for the ten primary object types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
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

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = e.name().as_ref().to_vec();
                let name = strip_prefix(&name);
                match name {
                    b"person" => report.counts.people += 1,
                    b"family" => report.counts.families += 1,
                    b"event" => report.counts.events += 1,
                    b"placeobj" => report.counts.places += 1,
                    b"source" => report.counts.sources += 1,
                    b"citation" => report.counts.citations += 1,
                    b"repository" => report.counts.repositories += 1,
                    b"object" => report.counts.media += 1,
                    b"note" => report.counts.notes += 1,
                    b"tag" => report.counts.tags += 1,
                    _ => {}
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

    Ok(report)
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
}