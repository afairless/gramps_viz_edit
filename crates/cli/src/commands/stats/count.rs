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



/// Scan a `.gramps` XML document and produce a [`StatsReport`].
///
/// Returns `Err(CliError::XmlParseError)` when the content is not
/// well-formed XML.
pub fn count_gramps_xml(content: &str) -> Result<StatsReport, CliError> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let report = StatsReport::default();

    loop {
        match reader.read_event() {
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
}
