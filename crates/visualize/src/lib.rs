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

pub use args::{CliArgs, parse_cli_args};
pub use graph_data::{
    FamilyGroupMeta, FamilyLink, GraphData, LinkType, PersonNode, SelectedPerson, SelectionExport,
};

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
pub fn load_graph_data(
    path: &str,
    no_impute: bool,
    generation_gap: u32,
) -> Result<GraphData, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Cannot read file '{}': {}", path, e))?;

    let persons = gramps_reader::extract_persons(&content)
        .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;
    let families = gramps_reader::extract_families(&content)
        .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;

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

    Ok(gd)
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
}
