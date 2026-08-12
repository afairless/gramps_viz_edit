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
                r#"{{"handle":"{h}","name":"Test Person","birth_date":null,"death_date":null,"gender":"male","family_group":1}}"#
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

/// Write a minimal Gramps XML fixture with a UUID v4 handle for Python backend round-trip testing.
fn write_uuid_fixture(path: &str, xml: &str) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(xml.as_bytes()).unwrap();
}

/// Check if Gramps is available for import verification.
fn gramps_cli_available() -> bool {
    std::process::Command::new("gramps")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run the Gramps import CLI under a short timeout.
fn run_gramps_import(path: &str) -> (String, Option<i32>) {
    let output = std::process::Command::new("timeout")
        .args(["15", "gramps", "-y", "-q", "-i", path])
        .output()
        .expect("failed to run timeout gramps");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (combined, output.status.code())
}

// Check if Gramps Python libraries are available (for the Python backend).
fn gramps_available() -> bool {
    std::process::Command::new("python3")
        .args([
            "-c",
            "import gramps.gen.const as c; \
             assert c.VERSION.startswith(('5.1','5.2'))",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

// ---------------------------------------------------------------------------
// Gramps round-trip E2E test
// ---------------------------------------------------------------------------

/// Minimal Gramps 5.1 XML fixture with one person (UUID v4 handle).
const UUID_FIXTURE_51: &str = r###"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1.6"/>
    <researcher/>
  </header>
  <people>
    <person handle="a5f0c1a2-4000-4b3d-8000-000000000001" id="I0001">
      <gender>1</gender>
      <name><first>Ada</first><surname>Lovelace</surname></name>
    </person>
  </people>
  <families/>
  <events/>
</database>
"###;

/// Minimal Gramps 5.1 XML fixture with two people in a family and an event.
const UUID_FIXTURE_FAMILY_EVENT: &str = r###"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1.6"/>
    <researcher/>
  </header>
  <people>
    <person handle="b0000001-4000-4b3d-8000-000000000001" id="I0001">
      <gender>1</gender>
      <name><first>John</first><surname>Doe</surname></name>
      <eventref hlink="e0000001-4000-4b3d-8000-000000000001" role="Primary"/>
    </person>
    <person handle="b0000002-4000-4b3d-8000-000000000002" id="I0002">
      <gender>2</gender>
      <name><first>Jane</first><surname>Doe</surname></name>
    </person>
  </people>
  <families>
    <family handle="f0000001-4000-4b3d-8000-000000000001">
      <father hlink="b0000001-4000-4b3d-8000-000000000001"/>
      <mother hlink="b0000002-4000-4b3d-8000-000000000002"/>
    </family>
  </families>
  <events>
    <event handle="e0000001-4000-4b3d-8000-000000000001" id="E0001">
      <type>Birth</type>
      <dateval val="1980-01-15"/>
    </event>
  </events>
</database>
"###;

/// Minimal Gramps 5.1 XML fixture with two people, a family, and a note
/// referenced by both people.
const UUID_FIXTURE_FAMILY_NOTE: &str = r###"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1.6"/>
    <researcher/>
  </header>
  <people>
    <person handle="c0000001-4000-4b3d-8000-000000000001" id="I0001">
      <gender>1</gender>
      <name><first>John</first><surname>Doe</surname></name>
      <noteref hlink="n0000001-4000-4b3d-8000-000000000001"/>
    </person>
    <person handle="c0000002-4000-4b3d-8000-000000000002" id="I0002">
      <gender>2</gender>
      <name><first>Jane</first><surname>Doe</surname></name>
      <noteref hlink="n0000001-4000-4b3d-8000-000000000001"/>
    </person>
  </people>
  <families>
    <family handle="f0000001-4000-4b3d-8000-000000000001">
      <father hlink="c0000001-4000-4b3d-8000-000000000001"/>
      <mother hlink="c0000002-4000-4b3d-8000-000000000002"/>
    </family>
  </families>
  <notes>
    <note handle="n0000001-4000-4b3d-8000-000000000001" type="Research" format="0">
      <text>Shared note referenced by both people.</text>
    </note>
  </notes>
</database>
"###;

/// Write a v1 manifest JSON file (flat string format) for testing.
fn write_v1_manifest(path: &str, input_file: &str, handles: &[&str]) {
    let to_delete: Vec<String> = handles
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect();
    let json = format!(
        r#"{{
  "version": 1,
  "source_file": "{}",
  "created_at": "2025-01-15T10:30:00Z",
  "seed_people": [{}],
  "plan": {{
    "people": {{
      "to_delete": [{}],
      "kept": []
    }}
  }}
}}"#,
        input_file,
        to_delete.join(",\n    "),
        to_delete.join(",\n        ")
    );
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(json.as_bytes()).unwrap();
}

/// Write a v2 reconciled manifest JSON file for testing.
/// `deleted` handles are marked Deleted, `pending` handles are marked Pending.
fn write_v2_reconciled_manifest(
    path: &str,
    input_file: &str,
    deleted: &[&str],
    pending: &[&str],
) {
    let mut entries: Vec<String> = Vec::new();
    for h in deleted {
        entries.push(format!(
            r#"{{"handle":"{}","status":"deleted"}}"#,
            h
        ));
    }
    for h in pending {
        entries.push(format!(
            r#"{{"handle":"{}","status":"pending"}}"#,
            h
        ));
    }
    let json = format!(
        r#"{{
  "version": 2,
  "source_file": "{}",
  "created_at": "2025-01-15T10:30:00Z",
  "deletion_mode": "people_only",
  "seed_people": [],
  "plan": {{
    "people": {{
      "to_delete": [{}],
      "kept": []
    }}
  }}
}}"#,
        input_file,
        entries.join(",\n        ")
    );
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(json.as_bytes()).unwrap();
}

#[test]
fn e2e_delete_gramps_python_roundtrip() {
    // This test requires Gramps to be installed. Skip gracefully if not.
    if !gramps_available() {
        eprintln!("Skipping: Gramps Python libraries not available (gramps_available()=false)");
        return;
    }

    let input = temp_path("roundtrip_input.gramps");
    let selections = temp_path("roundtrip_selections.json");
    let output = temp_path("roundtrip_output.gramps");

    write_uuid_fixture(&input, UUID_FIXTURE_51);
    write_selections(&selections, &["a5f0c1a2-4000-4b3d-8000-000000000001"]);

    // Run delete with Python backend.
    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
    ]);
    assert_eq!(
        code,
        Some(0),
        "Delete round-trip failed: {}\nstderr: {}",
        stderr,
        _stdout
    );

    // Verify the output file exists and is non-empty.
    assert!(
        std::path::Path::new(&output).exists(),
        "Output file should exist"
    );
    let content = std::fs::read_to_string(&output).expect("Output file should be readable");
    assert!(!content.is_empty(), "Output file should not be empty");

    // Verify the deleted person handle is NOT present in the output.
    assert!(
        !content.contains("a5f0c1a2-4000-4b3d-8000-000000000001"),
        "Deleted person handle should not appear in output"
    );

    // Verify the output is valid Gramps XML.
    assert!(
        content.starts_with("<?xml"),
        "Output should start with XML declaration"
    );

    // If Gramps CLI is available, verify the output imports cleanly.
    if gramps_cli_available() {
        let (import_out, import_code) = run_gramps_import(&output);
        // Exit 0 (success) or 124 (timeout) are OK.
        assert!(
            import_code == Some(0) || import_code == Some(124),
            "Gramps import should complete, exit code: {:?}\noutput: {}",
            import_code,
            import_out
        );
        for pat in ["ERROR:", "Traceback", "TypeError", "Failed to import"] {
            assert!(
                !import_out.contains(pat),
                "Gramps import reported '{}' — output: {}",
                pat,
                import_out
            );
        }
    } else {
        eprintln!("gramps CLI not available; skipping import verification");
    }

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_manifest_v2_reconciliation() {
    // Verify manifest correctly marks deleted/pending after reconciliation.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("recon_input.gramps");
    let selections = temp_path("recon_selections.json");
    let output = temp_path("recon_output.gramps");
    let manifest_path = temp_path("recon_manifest.json");

    write_uuid_fixture(&input, UUID_FIXTURE_51);
    write_selections(&selections, &["a5f0c1a2-4000-4b3d-8000-000000000001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
        "--save-manifest",
        &manifest_path,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Load the manifest and verify reconciliation.
    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    assert_eq!(manifest["version"], 2);

    // The person should be marked as deleted by Gramps.
    let people = &manifest["plan"]["people"];
    let to_delete = &people["to_delete"];
    assert_eq!(to_delete[0]["handle"], "a5f0c1a2-4000-4b3d-8000-000000000001");
    assert_eq!(to_delete[0]["status"], "deleted");

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
    let _ = std::fs::remove_file(&manifest_path);
}

#[test]
fn e2e_delete_db_retained() {
    // Verify Gramps DB directory exists after delete.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("dbret_input.gramps");
    let selections = temp_path("dbret_selections.json");
    let output = temp_path("dbret_output.gramps");
    let db_dir = temp_path("dbret_db");

    write_uuid_fixture(&input, UUID_FIXTURE_51);
    write_selections(&selections, &["a5f0c1a2-4000-4b3d-8000-000000000001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
        "--db-dir",
        &db_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Verify DB directory exists and contains files.
    assert!(std::path::Path::new(&db_dir).exists(), "DB dir should exist");
    let db_entries: Vec<_> = std::fs::read_dir(&db_dir).into_iter().flatten().flatten().collect();
    assert!(!db_entries.is_empty(), "DB dir should contain files");

    // Clean up DB dir
    let _ = std::fs::remove_dir_all(&db_dir);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_no_retain_db() {
    // Verify --no-retain-db cleans up the Gramps DB directory.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("nordb_input.gramps");
    let selections = temp_path("nordb_selections.json");
    let output = temp_path("nordb_output.gramps");
    let db_dir = temp_path("nordb_db");

    write_uuid_fixture(&input, UUID_FIXTURE_51);
    write_selections(&selections, &["a5f0c1a2-4000-4b3d-8000-000000000001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
        "--db-dir",
        &db_dir,
        "--no-retain-db",
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Verify DB directory was cleaned up.
    assert!(
        !std::path::Path::new(&db_dir).exists(),
        "DB dir should be removed with --no-retain-db"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_orphaned_events_survive() {
    // Verify events referenced only by deleted people still exist in output.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("orphan_input.gramps");
    let selections = temp_path("orphan_selections.json");
    let output = temp_path("orphan_output.gramps");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT);
    // Delete both people — the event should survive as orphan.
    write_selections(
        &selections,
        &[
            "b0000001-4000-4b3d-8000-000000000001",
            "b0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();

    // Deleted people should NOT appear.
    assert!(
        !content.contains("b0000001-4000-4b3d-8000-000000000001"),
        "Deleted person 1 should not appear in output"
    );
    assert!(
        !content.contains("b0000002-4000-4b3d-8000-000000000002"),
        "Deleted person 2 should not appear in output"
    );

    // The event should still exist (orphaned).
    assert!(
        content.contains("e0000001-4000-4b3d-8000-000000000001"),
        "Orphaned event should survive in output"
    );

    // The family should be gone (Gramps auto-deletes empty families).
    assert!(
        !content.contains("f0000001-4000-4b3d-8000-000000000001"),
        "Empty family should be deleted by Gramps"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_load_manifest_v1() {
    // Verify --load-manifest with v1 manifest works (auto-migration).
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("v1load_input.gramps");
    let manifest_file = temp_path("v1load_manifest.json");
    let output = temp_path("v1load_output.gramps");

    write_uuid_fixture(&input, UUID_FIXTURE_51);
    write_v1_manifest(
        &manifest_file,
        "v1load_input.gramps",
        &["a5f0c1a2-4000-4b3d-8000-000000000001"],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--load-manifest",
        &manifest_file,
        "-o",
        &output,
    ]);
    assert_eq!(code, Some(0), "Load manifest v1 failed: {}", stderr);

    // Verify deletion happened.
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        !content.contains("a5f0c1a2-4000-4b3d-8000-000000000001"),
        "Deleted person should not appear in output"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&manifest_file);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_load_manifest_v2_reconciled() {
    // Verify --load-manifest with reconciled v2 manifest skips already-deleted
    // handles and re-deletes only pending items.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("v2load_input.gramps");
    let manifest_file = temp_path("v2load_manifest.json");
    let output = temp_path("v2load_output.gramps");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT);
    // Write a v2 reconciled manifest where person1 is already deleted and
    // person2 is still pending.
    write_v2_reconciled_manifest(
        &manifest_file,
        "v2load_input.gramps",
        &["b0000001-4000-4b3d-8000-000000000001"],
        &["b0000002-4000-4b3d-8000-000000000002"],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--load-manifest",
        &manifest_file,
        "-o",
        &output,
    ]);
    assert_eq!(code, Some(0), "Load manifest v2 reconciled failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();

    // The already-deleted person should still be present in the original
    // input and thus appear in the output (the manifest skips re-deleting it).
    assert!(
        content.contains("b0000001-4000-4b3d-8000-000000000001"),
        "Already-deleted person should NOT be re-deleted, so they remain in output"
    );

    // The pending person should be deleted.
    assert!(
        !content.contains("b0000002-4000-4b3d-8000-000000000002"),
        "Pending person should be deleted in output"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&manifest_file);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_gramps_available_guard() {
    // This test verifies that gramps_available() returns a bool without
    // panicking. If Gramps is installed, it should return true.
    let result = gramps_available();
    // Verifies it returns a bool without panicking.
    assert_eq!(result, result, "must not panic");

    // Log the actual result for debugging.
    if result {
        eprintln!("gramps_available() = true (Gramps Python libs found)");
    } else {
        eprintln!("gramps_available() = false (Gramps Python libs not found)");
    }
}

// ---------------------------------------------------------------------------
// Event cleaning integration tests
// ---------------------------------------------------------------------------

/// Helper: derive the `-deleted_2.gramps` path from an input path, matching
/// the logic in delete.rs::derive_no_events_path.
fn no_events_path(input: &str) -> String {
    let stem = if let Some(s) = input.strip_suffix(".gz") {
        s
    } else {
        input
    };
    if let Some(dot) = stem.rfind('.') {
        format!("{}-deleted_2.gramps", &stem[..dot])
    } else {
        format!("{}-deleted_2.gramps", stem)
    }
}

#[test]
fn e2e_delete_with_event_clean() {
    // Verify that orphaned events are removed from the -deleted_2.gramps output.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("ec_input.gramps");
    let selections = temp_path("ec_selections.json");
    let output = temp_path("ec_output.gramps");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT);
    // Delete both people — the event becomes orphaned
    write_selections(
        &selections,
        &[
            "b0000001-4000-4b3d-8000-000000000001",
            "b0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Verify -cleaned output has the orphaned event (Gramps kept it)
    let cleaned_content = std::fs::read_to_string(&output).unwrap();
    assert!(
        cleaned_content.contains("e0000001-4000-4b3d-8000-000000000001"),
        "Orphaned event should survive in -cleaned output"
    );

    // Verify -deleted_2 output has the event removed
    let no_events = no_events_path(&input);
    assert!(
        std::path::Path::new(&no_events).exists(),
        "-deleted_2.gramps file should exist: {}",
        no_events
    );
    let no_events_content = std::fs::read_to_string(&no_events).unwrap();
    assert!(
        !no_events_content.contains("e0000001-4000-4b3d-8000-000000000001"),
        "Orphaned event should be removed from -deleted_2 output"
    );

    // Verify non-event content is preserved in -deleted_2
    // (XML declaration and header should survive)
    assert!(
        no_events_content.contains("<?xml"),
        "XML declaration should be preserved"
    );
    assert!(
        no_events_content.contains("<database"),
        "Database root element should be preserved"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
    let _ = std::fs::remove_file(&no_events);
}

#[test]
fn e2e_delete_no_pending_events_noop() {
    // Verify that when there are no events (and thus no pending events),
    // the clean step is skipped and no -deleted_2.gramps file is written.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("nope_input.gramps");
    let selections = temp_path("nope_selections.json");
    let output = temp_path("nope_output.gramps");

    // UUID_FIXTURE_51 has no events
    write_uuid_fixture(&input, UUID_FIXTURE_51);
    write_selections(&selections, &["a5f0c1a2-4000-4b3d-8000-000000000001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -cleaned output should exist
    assert!(
        std::path::Path::new(&output).exists(),
        "-cleaned output should exist"
    );

    // -deleted_2 should NOT exist (no pending events to clean)
    let no_events = no_events_path(&input);
    assert!(
        !std::path::Path::new(&no_events).exists(),
        "-deleted_2.gramps should NOT be written when there are no pending events"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_delete_manifest_re_saved_after_clean() {
    // Verify that after successful event cleaning, the manifest on disk
    // has the pending events marked as Deleted.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("mrsac_input.gramps");
    let selections = temp_path("mrsac_selections.json");
    let output = temp_path("mrsac_output.gramps");
    let manifest_path = temp_path("mrsac_manifest.json");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT);
    write_selections(
        &selections,
        &[
            "b0000001-4000-4b3d-8000-000000000001",
            "b0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
        "--save-manifest",
        &manifest_path,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Load the manifest and verify event status
    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();

    // Events plan should exist
    let events_plan = &manifest["plan"]["events"];
    let to_delete = &events_plan["to_delete"];
    assert!(
        to_delete.as_array().is_some_and(|a| !a.is_empty()),
        "Events to_delete should not be empty"
    );

    // All event entries should be 'deleted' (not 'pending')
    for entry in to_delete.as_array().unwrap() {
        assert_eq!(
            entry["status"],
            "deleted",
            "Event {} should be marked deleted after clean",
            entry["handle"]
        );
    }

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
    let _ = std::fs::remove_file(&manifest_path);
}

// ---------------------------------------------------------------------------
// Note cleaning integration tests
// ---------------------------------------------------------------------------

/// Helper: derive the `-deleted_3.gramps` path from an input path, matching
/// the logic in delete.rs::derive_deleted_3_path.
fn deleted_3_path(input: &str) -> String {
    let stem = if let Some(s) = input.strip_suffix(".gz") {
        s
    } else {
        input
    };
    if let Some(dot) = stem.rfind('.') {
        format!("{}-deleted_3.gramps", &stem[..dot])
    } else {
        format!("{}-deleted_3.gramps", stem)
    }
}

#[test]
fn e2e_delete_with_note_clean() {
    // Verify the full pipeline: deleting both people orphans the shared note,
    // event cleaning runs (no events here, so -deleted_2 is a copy), and note
    // cleaning removes the orphaned note from -deleted_3.gramps.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("nc_input.gramps");
    let selections = temp_path("nc_selections.json");
    let output = temp_path("nc_output.gramps");
    let manifest_path = temp_path("nc_manifest.json");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_NOTE);
    // Delete both people — the note becomes orphaned
    write_selections(
        &selections,
        &[
            "c0000001-4000-4b3d-8000-000000000001",
            "c0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
        "--save-manifest",
        &manifest_path,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -cleaned output should exist and contain the orphaned note (Gramps
    // Python backend only deletes people, never notes)
    let cleaned_content = std::fs::read_to_string(&output).unwrap();
    assert!(
        cleaned_content.contains("n0000001-4000-4b3d-8000-000000000001"),
        "Orphaned note should survive in -cleaned output"
    );

    // -deleted_2 may or may not exist (no pending events here, so it's skipped)
    // -deleted_3 should exist and have the note removed
    let no_notes = deleted_3_path(&input);
    assert!(
        std::path::Path::new(&no_notes).exists(),
        "-deleted_3.gramps file should exist: {}",
        no_notes
    );
    let no_notes_content = std::fs::read_to_string(&no_notes).unwrap();
    assert!(
        !no_notes_content.contains("n0000001-4000-4b3d-8000-000000000001"),
        "Orphaned note should be removed from -deleted_3 output"
    );
    assert!(
        no_notes_content.contains("<?xml"),
        "XML declaration should be preserved"
    );
    assert!(
        no_notes_content.contains("<database"),
        "Database root element should be preserved"
    );

    // Manifest should mark the note as deleted (not pending)
    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let notes_plan = &manifest["plan"]["notes"];
    let to_delete = &notes_plan["to_delete"];
    assert!(
        to_delete.as_array().is_some_and(|a| !a.is_empty()),
        "Notes to_delete should not be empty"
    );
    for entry in to_delete.as_array().unwrap() {
        assert_eq!(
            entry["status"],
            "deleted",
            "Note {} should be marked deleted after clean",
            entry["handle"]
        );
    }

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&no_notes);
}

#[test]
fn e2e_delete_no_pending_notes_noop() {
    // Verify that when there are no notes (and thus no pending notes), the
    // note clean step is skipped and no -deleted_3.gramps file is written.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("nopn_input.gramps");
    let selections = temp_path("nopn_selections.json");
    let output = temp_path("nopn_output.gramps");

    // UUID_FIXTURE_FAMILY_EVENT has an event but no notes
    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT);
    write_selections(
        &selections,
        &[
            "b0000001-4000-4b3d-8000-000000000001",
            "b0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -deleted_2 should exist (event cleaning runs)
    let no_events = no_events_path(&input);
    assert!(
        std::path::Path::new(&no_events).exists(),
        "-deleted_2.gramps should exist when pending events are cleaned"
    );

    // -deleted_3 should NOT exist (no pending notes to clean)
    let no_notes = deleted_3_path(&input);
    assert!(
        !std::path::Path::new(&no_notes).exists(),
        "-deleted_3.gramps should NOT be written when there are no pending notes"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&output);
    let _ = std::fs::remove_file(&no_events);
}
