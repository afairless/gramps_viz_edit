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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------


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
fn e2e_delete_zero_percent_overlap_errors() {
    let input = temp_path("no_match.gramps");
    let selections = temp_path("no_match_selections.json");

    write_minimal_family(&input);
    // Write selections with a handle that doesn't exist in the graph
    write_selections(&selections, &["nonexistent_handle"]);

    let (_stdout, stderr, code) =
        gramps_gen(&["delete", &input, "--selections", &selections, "--yes"]);
    // Should fail with non-zero exit code
    assert_ne!(
        code,
        Some(0),
        "Should fail when 0% of selections match\nstderr: {}",
        stderr
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
}

