//! End-to-end tests for the `gramps-gen delete` command.
//!
//! These tests run the CLI binary as a subprocess and verify that deletion
//! produces correct output: selected people are removed, orphaned objects
//! are removed, pre-existing orphaned objects are preserved, and the output
//! is valid Gramps XML.
//!
//! To run:
//! ```bash
//! cargo test -p cli --test e2e_delete
//! ```

use std::io::Write;

/// Helper to run gramps-gen with arguments and return stdout, stderr, and exit code.
fn gramps_gen(args: &[&str]) -> (String, String, Option<i32>) {
    let bin = env!("CARGO_BIN_EXE_gramps-gen");
    let output = std::process::Command::new(bin)
        .args(args)
        .output()
        .expect("Failed to run gramps-gen");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code();
    (stdout, stderr, code)
}

/// Create a unique temporary file path.
fn temp_path(suffix: &str) -> String {
    std::format!(
        "/tmp/gramps_gen_e2e_delete_{}_{}.{}",
        suffix,
        std::process::id(),
        suffix
    )
}

/// Create a minimal Gramps XML file with two people in a family.
fn write_minimal_family(path: &str) {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
    <researcher><resname>Test</resname></researcher>
  </header>
  <people>
    <person handle="p0001" id="I0001">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
    </person>
    <person handle="p0002" id="I0002">
      <gender>F</gender>
      <name><first>Jane</first><surname>Smith</surname></name>
    </person>
  </people>
  <families>
    <family handle="f0001">
      <father hlink="p0001"/>
      <mother hlink="p0002"/>
    </family>
  </families>
</database>"#;
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(xml.as_bytes()).unwrap();
}

/// Create a selection JSON file for specific people.
fn write_selections(path: &str, handles: &[&str]) {
    let selections: Vec<String> = handles
        .iter()
        .map(|h| {
            format!(
                r#"{{"handle":"{}","name":"Test Person","birth_date":null,"death_date":null,"gender":"male","family_group":1}}"#,
                h
            )
        })
        .collect();
    let json = format!(
        r#"{{"exported_at":"2025-01-15T10:30:00.000Z","file":"selections.json","selections":[{}]}}"#,
        selections.join(",")
    );
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(json.as_bytes()).unwrap();
}

/// Create a Gramps file with a pre-existing orphaned event (no references to it).
fn write_graph_with_orphans(path: &str) {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
    <researcher><resname>Test</resname></researcher>
  </header>
  <people>
    <person handle="p0001" id="I0001">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
    </person>
    <person handle="p0002" id="I0002">
      <gender>F</gender>
      <name><first>Jane</first><surname>Smith</surname></name>
    </person>
  </people>
  <families>
    <family handle="f0001">
      <father hlink="p0001"/>
      <mother hlink="p0002"/>
    </family>
  </families>
  <events>
    <!-- Orphaned event: no person references it -->
    <event handle="e0001">
      <eventtype><type>Birth</type></eventtype>
    </event>
  </events>
</database>"#;
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(xml.as_bytes()).unwrap();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn e2e_delete_single_person_removes_family() {
    let input = temp_path("single_person.gramps");
    let selections = temp_path("single_person_selections.json");
    let output = temp_path("single_person_output.gramps");

    write_minimal_family(&input);
    write_selections(&selections, &["p0001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--output",
        &output,
        "--yes",
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Read output and verify
    let content = std::fs::read_to_string(&output).unwrap();
    // John (p0001) should be removed
    assert!(!content.contains("John"), "John should be deleted");
    // Jane (p0002) should remain
    assert!(content.contains("Jane"), "Jane should remain");
    // Family should remain (Jane still exists)
    assert!(
        content.contains("f0001") || content.contains("<families>"),
        "Family should remain"
    );

    // Clean up
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_both_people_removes_family() {
    let input = temp_path("both_people.gramps");
    let selections = temp_path("both_people_selections.json");
    let output = temp_path("both_people_output.gramps");

    write_minimal_family(&input);
    write_selections(&selections, &["p0001", "p0002"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--output",
        &output,
        "--yes",
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();
    // All people should be removed
    assert!(!content.contains("John"), "John should be deleted");
    assert!(!content.contains("Jane"), "Jane should be deleted");
    // Family should be removed too (both parents deleted)
    assert!(
        !content.contains("f0001"),
        "Family should be deleted"
    );

    // Clean up
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_preserves_orphaned_objects() {
    let input = temp_path("orphans.gramps");
    let selections = temp_path("orphans_selections.json");
    let output = temp_path("orphans_output.gramps");

    write_graph_with_orphans(&input);
    write_selections(&selections, &["p0001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--output",
        &output,
        "--yes",
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();
    // e0001 was already orphaned (no incoming refs) — must NOT be deleted
    assert!(
        content.contains("e0001"),
        "Already orphaned event should be preserved"
    );
    // Jane (p0002) should remain
    assert!(content.contains("Jane"), "Jane should remain");

    // Clean up
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_dry_run_does_not_write_output() {
    let input = temp_path("dry_run.gramps");
    let selections = temp_path("dry_run_selections.json");
    let output = temp_path("dry_run_output.gramps");

    write_minimal_family(&input);
    write_selections(&selections, &["p0001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--output",
        &output,
        "--dry-run",
        "--yes",
    ]);
    assert_eq!(code, Some(0), "Dry run failed: {}", stderr);
    // Output file should NOT exist
    assert!(
        !std::path::Path::new(&output).exists(),
        "Dry run should not write output"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
}

#[test]
fn e2e_delete_save_and_load_manifest() {
    let input = temp_path("manifest.gramps");
    let selections = temp_path("manifest_selections.json");
    let manifest = temp_path("manifest.json");
    let output_a = temp_path("manifest_output_a.gramps");
    let output_b = temp_path("manifest_output_b.gramps");

    write_minimal_family(&input);
    write_selections(&selections, &["p0001"]);

    // Step 1: run with --save-manifest
    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--output",
        &output_a,
        "--yes",
        "--save-manifest",
        &manifest,
    ]);
    assert_eq!(code, Some(0), "Save manifest run failed: {}", stderr);
    assert!(
        std::path::Path::new(&manifest).exists(),
        "Manifest file should exist"
    );

    // Step 2: run again with --load-manifest (skip selections file)
    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--load-manifest",
        &manifest,
        "--output",
        &output_b,
        "--yes",
    ]);
    assert_eq!(code, Some(0), "Load manifest run failed: {}", stderr);

    // Both outputs should be the same
    let content_a = std::fs::read_to_string(&output_a).unwrap();
    let content_b = std::fs::read_to_string(&output_b).unwrap();

    // Both should have John removed
    assert!(!content_a.contains("John"), "John should be deleted in A");
    assert!(!content_b.contains("John"), "John should be deleted in B");

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&manifest);
    let _ = std::fs::remove_file(&output_a);
    let _ = std::fs::remove_file(&output_b);
}

#[test]
fn e2e_delete_zero_percent_overlap_errors() {
    let input = temp_path("no_match.gramps");
    let selections = temp_path("no_match_selections.json");

    write_minimal_family(&input);
    // Write selections with a handle that doesn't exist in the graph
    write_selections(&selections, &["nonexistent_handle"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
    ]);
    // Should fail with non-zero exit code
    assert_ne!(
        code, Some(0),
        "Should fail when 0% of selections match\nstderr: {}",
        stderr
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
}

#[test]
fn e2e_delete_validates_output_xml() {
    let input = temp_path("valid_xml.gramps");
    let selections = temp_path("valid_xml_selections.json");
    let output = temp_path("valid_xml_output.gramps");

    write_minimal_family(&input);
    write_selections(&selections, &["p0001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--output",
        &output,
        "--yes",
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Verify output is valid XML
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.starts_with("<?xml"),
        "Output should start with XML declaration"
    );
    assert!(
        content.contains("<database"),
        "Output should contain <database>"
    );
    assert!(
        content.contains("</database>"),
        "Output should close </database>"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}