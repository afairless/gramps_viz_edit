//! Library root for the visualize crate.
//!
//! This crate builds the graph data model and date imputation algorithms
//! that power the Tauri-based family-group visualization. When the
//! `visualize` feature is enabled, the `gramps-gen-visualize` binary
//! provides a Tauri desktop shell.
//!
//! Without the `visualize` feature this crate is a plain library —
//! no WebKit2GTK / WebView2 system dependencies are needed.

pub mod args;
pub mod dates;
pub mod graph_data;

pub use args::{parse_cli_args, CliArgs};
pub use graph_data::{
    FamilyGroupMeta, FamilyLink, GraphData, LinkType, PersonNode, SelectedPerson, SelectionExport,
};

/// Combined result of loading a .gramps file: graph data for
/// rendering plus summary statistics for the stats panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LoadedGraph {
    pub graph_data: GraphData,
    pub stats: gramps_reader::StatsReport,
}

/// Load a `.gramps` file and produce the full `GraphData` for visualization.
///
/// This is a pure function (no Tauri dependency) that orchestrates the
/// full pipeline:
///
/// 1. Read the file from disk.
/// 2. Extract persons and families via `gramps-reader`'s streaming parsers.
/// 3. Build the graph data (nodes, links, family groups, generations).
/// 4. Impute birth dates for undated nodes (unless `no_impute` is `true`).
///
/// Returns a user-friendly error string on failure.
///
/// This function delegates to [`load_graph_data_with_stats`] and discards
/// the statistics. Users who need both graph data and statistics should
/// call [`load_graph_data_with_stats`] directly to avoid a second file read.
pub fn load_graph_data(
    path: &str,
    no_impute: bool,
    generation_gap: u32,
) -> Result<GraphData, String> {
    load_graph_data_with_stats(path, no_impute, generation_gap)
        .map(|loaded| loaded.graph_data)
}

/// Load a `.gramps` file and return both graph data and summary statistics.
///
/// This is a pure function (no Tauri dependency) that reads the file **once**
/// and runs both the extraction pipeline and the streaming count pass on the
/// same in-memory content. This eliminates the redundant file I/O that
/// occurred when calling `load_graph_data` and `get_stats` separately.
///
/// Returns a user-friendly error string on failure.
pub fn load_graph_data_with_stats(
    path: &str,
    no_impute: bool,
    generation_gap: u32,
) -> Result<LoadedGraph, String> {
    let content = gramps_reader::read_gramps_file(path)
        .map_err(|e| format!("Cannot read file '{}': {}", path, e))?;

    // Compute stats from the same in-memory content.
    let stats = gramps_reader::count_gramps_xml(&content)
        .map_err(|e| format!("Failed to parse Gramps XML: {}", e))?;

    // Existing extraction pipeline (unchanged).
    let mut persons = gramps_reader::extract_persons(&content)
        .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;
    let events = gramps_reader::extract_events(&content)
        .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;
    let families = gramps_reader::extract_families(&content)
        .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;

    // Resolve event references to populate birth/death dates from
    // separate <event> elements (Gramps 5.x event-reference format).
    gramps_reader::resolve_event_refs(&mut persons, &events);

    if persons.is_empty() {
        return Err("No people found in the Gramps file".to_string());
    }

    let mut gd = graph_data::build_graph_data(&persons, &families);

    // Impute birth dates for undated nodes.
    let imputed = dates::impute_dates(&gd.nodes, &gd.links, generation_gap, no_impute);
    for node in &mut gd.nodes {
        if let Some(Some(year)) = imputed.get(&node.handle) {
            if node.birth_year != Some(*year) {
                node.birth_year = Some(*year);
                node.is_imputed = true;
            }
        }
    }

    Ok(LoadedGraph {
        graph_data: gd,
        stats,
    })
}

/// Read a `.gramps` file and return summary statistics.
///
/// This is a pure function (no Tauri dependency) that re-reads the file
/// from disk and runs the streaming `count_gramps_xml` pass. The file
/// content is typically already cached by the OS after the initial
/// `load_graph_data` call, so the second read is fast.
///
/// Returns a user-friendly error string on failure.
pub fn get_stats(path: &str) -> Result<gramps_reader::StatsReport, String> {
    let content = gramps_reader::read_gramps_file(path)
        .map_err(|e| format!("Cannot read file '{}': {}", path, e))?;
    gramps_reader::count_gramps_xml(&content)
        .map_err(|e| format!("Failed to parse Gramps XML: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_graph_data_nonexistent_file() {
        let result = load_graph_data("/nonexistent/path.gramps", false, 25);
        match result {
            Err(msg) => assert!(msg.contains("Cannot read file"), "got: {}", msg),
            Ok(_) => panic!("expected error for nonexistent file"),
        }
    }

    #[test]
    fn load_graph_data_malformed_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database><person></database>").unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let result = load_graph_data(path.to_str().unwrap(), false, 25);
        match result {
            Err(msg) => assert!(msg.contains("Gramps XML"), "got: {}", msg),
            Ok(_) => panic!("expected error for malformed XML"),
        }
    }

    #[test]
    fn load_graph_data_empty_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "").unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let result = load_graph_data(path.to_str().unwrap(), false, 25);
        match result {
            Err(msg) => assert!(msg.contains("No people found"), "got: {}", msg),
            Ok(_) => panic!("expected error for empty file"),
        }
    }

    #[test]
    fn load_graph_data_xml_extension_accepted() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <birth><dateval val="1850-03-15"/></birth>
    </person>
  </people>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("xml");
        std::fs::rename(tmp.path(), &path).unwrap();

        let gd = load_graph_data(path.to_str().unwrap(), false, 25).unwrap();
        assert_eq!(gd.nodes.len(), 1);
        assert_eq!(gd.nodes[0].handle, "p1");
    }

    #[test]
    fn get_stats_xml_extension_accepted() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"/>
  </people>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("xml");
        std::fs::rename(tmp.path(), &path).unwrap();

        let report = get_stats(path.to_str().unwrap()).unwrap();
        assert_eq!(report.counts.people, 1);
    }

    #[test]
    fn load_graph_data_valid_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <birth><dateval val="1850-03-15"/></birth>
    </person>
    <person handle="p2">
      <gender>F</gender>
      <name>
        <first>Jane</first>
        <surname>Smith</surname>
      </name>
      <birth><dateval val="1855-06-01"/></birth>
    </person>
    <person handle="p3">
      <name><first>Jim</first><surname>Smith</surname></name>
    </person>
  </people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let gd = load_graph_data(path.to_str().unwrap(), false, 25).unwrap();
        assert_eq!(gd.nodes.len(), 3);
        assert_eq!(gd.links.len(), 3); // 1 spouse + 2 parent-child
        assert_eq!(gd.family_groups.len(), 1);

        // John, Jane are gen 0, Jim is gen 1
        let p1 = gd.nodes.iter().find(|n| n.handle == "p1").unwrap();
        let p3 = gd.nodes.iter().find(|n| n.handle == "p3").unwrap();
        assert_eq!(p1.generation, 0);
        assert_eq!(p3.generation, 1);
        // Jim's birth year should be imputed from both parents
        assert!(p3.is_imputed);
        // p1 (1850) + p2 (1855) averaged: (1875 + 1880) / 2 = 1877
        assert_eq!(p3.birth_year, Some(1877));
    }

    #[test]
    fn load_graph_data_no_impute() {
        use std::io::Write;
        use tempfile::NamedTempFile;

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

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let gd = load_graph_data(path.to_str().unwrap(), true, 25).unwrap();
        // Jim's birth year stays None (no imputation)
        let p2 = gd.nodes.iter().find(|n| n.handle == "p2").unwrap();
        assert!(p2.birth_year.is_none());
        assert!(!p2.is_imputed);
    }

    #[test]
    fn load_graph_data_generation_gap_validation() {
        // generation_gap is validated by the CLI before calling this function,
        // but the function should handle any value gracefully.
        use std::io::Write;
        use tempfile::NamedTempFile;

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

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let gd = load_graph_data(path.to_str().unwrap(), false, 50).unwrap();
        let p2 = gd.nodes.iter().find(|n| n.handle == "p2").unwrap();
        // 1850 + (1-0)*50 = 1900
        assert_eq!(p2.birth_year, Some(1900));
    }

    #[test]
    fn load_graph_data_event_ref_format() {
        // Full pipeline with the Gramps 5.x event-reference format: dates
        // live in separate <event> elements referenced via <eventref>.
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e-birth-1">
      <eventtype><type>Birth</type></eventtype>
      <dateval val="1850-07-13"/>
    </event>
    <event handle="e-birth-2">
      <eventtype><type>Birth</type></eventtype>
      <dateval val="1855-06-01"/>
    </event>
    <event handle="e-death-1">
      <eventtype><type>Death</type></eventtype>
      <dateval val="1920-03-01"/>
    </event>
  </events>
  <people>
    <person handle="p1">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
      <eventref hlink="e-birth-1"><role>Primary</role></eventref>
      <eventref hlink="e-death-1"><role>Primary</role></eventref>
    </person>
    <person handle="p2">
      <gender>F</gender>
      <name><first>Jane</first><surname>Smith</surname></name>
      <eventref hlink="e-birth-2"><role>Primary</role></eventref>
    </person>
    <person handle="p3">
      <name><first>Jim</first><surname>Smith</surname></name>
    </person>
  </people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let gd = load_graph_data(path.to_str().unwrap(), false, 25).unwrap();
        assert_eq!(gd.nodes.len(), 3);
        assert_eq!(gd.links.len(), 3); // 1 spouse + 2 parent-child
        assert_eq!(gd.family_groups.len(), 1);

        // John and Jane get birth_year from event reference resolution.
        let p1 = gd.nodes.iter().find(|n| n.handle == "p1").unwrap();
        let p2 = gd.nodes.iter().find(|n| n.handle == "p2").unwrap();
        assert_eq!(p1.birth_year, Some(1850), "John birth_year from eventref");
        assert_eq!(
            p1.death_date.as_deref(),
            Some("1920-03-01"),
            "John death_date from eventref"
        );
        assert_eq!(p2.birth_year, Some(1855), "Jane birth_year from eventref");
        assert!(!p1.is_imputed);
        assert!(!p2.is_imputed);

        // Jim's birth year is imputed from his parents (same as the
        // equivalent inline-birth test): (1875 + 1880) / 2 = 1877.
        let p3 = gd.nodes.iter().find(|n| n.handle == "p3").unwrap();
        assert!(p3.is_imputed);
        assert_eq!(p3.birth_year, Some(1877));
    }

    #[test]
    fn load_graph_data_mixed_inline_and_eventrefs() {
        // A person with both inline <birth> and eventrefs: inline takes
        // precedence, eventrefs fill in what inline doesn't.
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e-birth">
      <eventtype><type>Birth</type></eventtype>
      <dateval val="1850-07-13"/>
    </event>
    <event handle="e-death">
      <eventtype><type>Death</type></eventtype>
      <dateval val="1920-03-01"/>
    </event>
  </events>
  <people>
    <person handle="p1">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
      <birth><dateval val="1845-01-01"/></birth>
      <eventref hlink="e-birth"><role>Primary</role></eventref>
      <eventref hlink="e-death"><role>Primary</role></eventref>
    </person>
  </people>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let gd = load_graph_data(path.to_str().unwrap(), true, 25).unwrap();
        assert_eq!(gd.nodes.len(), 1);

        let p1 = gd.nodes.iter().find(|n| n.handle == "p1").unwrap();
        // Inline birth takes precedence over the Birth eventref.
        assert_eq!(p1.birth_year, Some(1845));
        assert_eq!(p1.birth_date.as_deref(), Some("1845-01-01"));
        // Death comes only from the eventref (no inline death).
        assert_eq!(p1.death_date.as_deref(), Some("1920-03-01"));
    }

    // ------------------------------------------------------------------
    // get_stats tests
    // ------------------------------------------------------------------

    #[test]
    fn get_stats_nonexistent_file() {
        let result = get_stats("/nonexistent/path.gramps");
        match result {
            Err(msg) => assert!(msg.contains("Cannot read file"), "got: {}", msg),
            Ok(_) => panic!("expected error for nonexistent file"),
        }
    }

    #[test]
    fn get_stats_malformed_xml() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database><person></database>").unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let result = get_stats(path.to_str().unwrap());
        match result {
            Err(msg) => assert!(msg.contains("Failed to parse Gramps XML"), "got: {}", msg),
            Ok(_) => panic!("expected error for malformed XML"),
        }
    }

    #[test]
    fn get_stats_valid_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"/>
  </people>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let report = get_stats(path.to_str().unwrap()).unwrap();
        assert_eq!(report.counts.people, 1);
        // No families, so people-not-in-family should be 1
        assert_eq!(report.people_not_in_family, 1);
    }

    #[test]
    fn get_stats_empty_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "").unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let report = get_stats(path.to_str().unwrap()).unwrap();
        assert_eq!(report, gramps_reader::StatsReport::default());
    }

    // ------------------------------------------------------------------
    // load_graph_data_with_stats tests
    // ------------------------------------------------------------------

    #[test]
    fn load_graph_data_with_stats_valid_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <birth><dateval val="1850-03-15"/></birth>
    </person>
    <person handle="p2">
      <gender>F</gender>
      <name>
        <first>Jane</first>
        <surname>Smith</surname>
      </name>
      <birth><dateval val="1855-06-01"/></birth>
    </person>
  </people>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let loaded = load_graph_data_with_stats(path.to_str().unwrap(), false, 25).unwrap();
        assert_eq!(loaded.graph_data.nodes.len(), 2);
        assert!(loaded.stats.counts.people > 0);
        assert_eq!(loaded.stats.counts.people, 2);
    }

    #[test]
    fn load_graph_data_with_stats_nonexistent_file() {
        let result = load_graph_data_with_stats("/nonexistent/path.gramps", false, 25);
        match result {
            Err(msg) => assert!(msg.contains("Cannot read file"), "got: {}", msg),
            Ok(_) => panic!("expected error for nonexistent file"),
        }
    }

    #[test]
    fn load_graph_data_with_stats_malformed_xml() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "<database><person></database>").unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let result = load_graph_data_with_stats(path.to_str().unwrap(), false, 25);
        match result {
            Err(msg) => assert!(msg.contains("Failed to parse Gramps XML"), "got: {}", msg),
            Ok(_) => panic!("expected error for malformed XML"),
        }
    }

    #[test]
    fn load_graph_data_with_stats_empty_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "").unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let result = load_graph_data_with_stats(path.to_str().unwrap(), false, 25);
        match result {
            Err(msg) => assert!(msg.contains("No people found"), "got: {}", msg),
            Ok(_) => panic!("expected error for empty file"),
        }
    }

    #[test]
    fn load_graph_data_delegates_to_with_stats() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <birth><dateval val="1850-03-15"/></birth>
    </person>
    <person handle="p2">
      <gender>F</gender>
      <name>
        <first>Jane</first>
        <surname>Smith</surname>
      </name>
      <birth><dateval val="1855-06-01"/></birth>
    </person>
    <person handle="p3">
      <name><first>Jim</first><surname>Smith</surname></name>
    </person>
  </people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", xml).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let from_old = load_graph_data(path.to_str().unwrap(), false, 25).unwrap();
        let from_new = load_graph_data_with_stats(path.to_str().unwrap(), false, 25).unwrap();
        assert_eq!(from_old.nodes.len(), from_new.graph_data.nodes.len());
        assert_eq!(from_old.links.len(), from_new.graph_data.links.len());
        assert_eq!(from_old.family_groups.len(), from_new.graph_data.family_groups.len());
    }

    #[test]
    fn load_graph_data_gzip_compressed_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
      <birth><dateval val="1850-03-15"/></birth>
    </person>
    <person handle="p2">
      <gender>F</gender>
      <name><first>Jane</first><surname>Smith</surname></name>
      <birth><dateval val="1855-06-01"/></birth>
    </person>
  </people>
</database>"#;

        // Gzip-compress the XML.
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(xml.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&compressed).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let gd = load_graph_data(path.to_str().unwrap(), false, 25).unwrap();
        assert_eq!(gd.nodes.len(), 2);

        let p1 = gd.nodes.iter().find(|n| n.handle == "p1").unwrap();
        assert_eq!(p1.birth_year, Some(1850));
    }

    #[test]
    fn get_stats_gzip_compressed_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><name><first>Test</first></name></person>
  </people>
</database>"#;

        // Gzip-compress the XML.
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(xml.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&compressed).unwrap();
        let path = tmp.path().with_extension("gramps");
        std::fs::rename(tmp.path(), &path).unwrap();

        let report = get_stats(path.to_str().unwrap()).unwrap();
        assert_eq!(report.counts.people, 1);
    }
}
