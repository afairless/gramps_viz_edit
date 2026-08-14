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

/// Create a unique temporary directory (for `--output`).
fn temp_dir(suffix: &str) -> String {
    let dir = temp_path(suffix);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
    let output_dir = temp_dir("dry_run_output");

    write_minimal_family(&input);
    write_selections(&selections, &["p0001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--output",
        &output_dir,
        "--dry-run",
        "--yes",
    ]);
    assert_eq!(code, Some(0), "Dry run failed: {}", stderr);
    // Output file should NOT exist inside the output directory
    let output_file = deleted_1_path(&output_dir, &input);
    assert!(
        !std::path::Path::new(&output_file).exists(),
        "Dry run should not write output"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn e2e_delete_old_output_file_usage_errors() {
    // The old `--output <filename>` usage must fail cleanly: `--output` now
    // takes a directory, so a path naming an existing file is rejected with
    // a clear message (not a raw io error).
    let input = temp_path("oldusage_input.gramps");
    let selections = temp_path("oldusage_selections.json");
    let existing_file = temp_path("oldusage_output.gramps");

    std::fs::write(&existing_file, "i am a file, not a directory").unwrap();
    write_minimal_family(&input);
    write_selections(&selections, &["p0001"]);

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &existing_file,
    ]);
    assert_ne!(
        code,
        Some(0),
        "Old-style -o file usage should fail\nstderr: {}",
        stderr
    );
    assert!(
        stderr.contains("--output"),
        "stderr should mention --output: {}",
        stderr
    );
    assert!(
        stderr.contains("directory"),
        "stderr should explain --output now takes a directory: {}",
        stderr
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_file(&existing_file);
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
    let to_delete: Vec<String> = handles.iter().map(|h| format!("\"{}\"", h)).collect();
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
fn write_v2_reconciled_manifest(path: &str, input_file: &str, deleted: &[&str], pending: &[&str]) {
    let mut entries: Vec<String> = Vec::new();
    for h in deleted {
        entries.push(format!(r#"{{"handle":"{}","status":"deleted"}}"#, h));
    }
    for h in pending {
        entries.push(format!(r#"{{"handle":"{}","status":"pending"}}"#, h));
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
    let output_dir = temp_dir("roundtrip_output");

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
        &output_dir,
    ]);
    assert_eq!(
        code,
        Some(0),
        "Delete round-trip failed: {}\nstderr: {}",
        stderr,
        _stdout
    );

    let output_file = deleted_1_path(&output_dir, &input);

    // Verify the output file exists and is non-empty.
    assert!(
        std::path::Path::new(&output_file).exists(),
        "Output file should exist"
    );
    let content = std::fs::read_to_string(&output_file).expect("Output file should be readable");
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
        let (import_out, import_code) = run_gramps_import(&output_file);
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
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("recon_output");
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
        &output_dir,
        "--save-manifest",
        &manifest_path,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Load the manifest and verify reconciliation.
    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    assert_eq!(manifest["version"], 3);

    // The person should be marked as deleted by Gramps.
    let people = &manifest["plan"]["people"];
    let to_delete = &people["to_delete"];
    assert_eq!(
        to_delete[0]["handle"],
        "a5f0c1a2-4000-4b3d-8000-000000000001"
    );
    assert_eq!(to_delete[0]["status"], "deleted");

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("dbret_output");
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
        &output_dir,
        "--db-dir",
        &db_dir,
        "--retain-db",
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Verify DB directory exists and contains files.
    assert!(
        std::path::Path::new(&db_dir).exists(),
        "DB dir should exist"
    );
    let db_entries: Vec<_> = std::fs::read_dir(&db_dir)
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    assert!(!db_entries.is_empty(), "DB dir should contain files");

    // Clean up DB dir
    let _ = std::fs::remove_dir_all(&db_dir);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("nordb_output");
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
        &output_dir,
        "--db-dir",
        &db_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Verify DB directory was cleaned up (--no-retain-db is now the default).
    assert!(
        !std::path::Path::new(&db_dir).exists(),
        "DB dir should be removed by default"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn e2e_delete_default_db_dir_removed() {
    // Without --db-dir the DB lives in a per-run temp directory; without
    // --retain-db it must be removed after the run (no leak on the happy
    // path). We assert on the CLI's own log line rather than scanning the
    // system temp dir, which would race with parallel test runs.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("dbdef_input.gramps");
    let selections = temp_path("dbdef_selections.json");
    let output_dir = temp_dir("dbdef_output");

    write_uuid_fixture(&input, UUID_FIXTURE_51);
    write_selections(&selections, &["a5f0c1a2-4000-4b3d-8000-000000000001"]);

    let bin = env!("CARGO_BIN_EXE_gramps-gen");
    let output = std::process::Command::new(bin)
        .args([
            "delete",
            &input,
            "--selections",
            &selections,
            "--yes",
            "-o",
            &output_dir,
        ])
        .env("RUST_LOG", "info")
        .output()
        .expect("Failed to run gramps-gen");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(0), "Delete failed: {}", stderr);

    // The default temp DB directory must be cleaned up, not retained.
    assert!(
        stderr.contains("Gramps DB removed (temp dir cleaned up)"),
        "Expected temp DB cleanup log in stderr: {}",
        stderr
    );
    assert!(
        !stderr.contains("Gramps DB retained"),
        "DB should not be retained by default: {}",
        stderr
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("orphan_output");

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
        &output_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    let output_file = deleted_1_path(&output_dir, &input);
    let content = std::fs::read_to_string(&output_file).unwrap();

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
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("v1load_output");

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
        &output_dir,
    ]);
    assert_eq!(code, Some(0), "Load manifest v1 failed: {}", stderr);

    // Verify deletion happened.
    let output_file = deleted_1_path(&output_dir, &input);
    let content = std::fs::read_to_string(&output_file).unwrap();
    assert!(
        !content.contains("a5f0c1a2-4000-4b3d-8000-000000000001"),
        "Deleted person should not appear in output"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&manifest_file);
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("v2load_output");

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
        &output_dir,
    ]);
    assert_eq!(
        code,
        Some(0),
        "Load manifest v2 reconciled failed: {}",
        stderr
    );

    let output_file = deleted_1_path(&output_dir, &input);
    let content = std::fs::read_to_string(&output_file).unwrap();

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
    let _ = std::fs::remove_dir_all(&output_dir);
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

/// Derive the path of a `<stem>-deleted-N.gramps` file inside an output
/// directory, matching delete.rs naming: the stem is the input file's name
/// minus Gramps extensions.
fn deleted_stage_path(out_dir: &str, input: &str, stage: u32) -> String {
    let stem = input.strip_suffix(".gz").unwrap_or(input);
    let stem = match stem.rfind('.') {
        Some(i) => &stem[..i],
        None => stem,
    };
    let stem = std::path::Path::new(stem)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    std::path::Path::new(out_dir)
        .join(format!("{}-deleted-{}.gramps", stem, stage))
        .to_string_lossy()
        .to_string()
}

/// Helper: the `-deleted-1.gramps` file inside the output directory.
fn deleted_1_path(out_dir: &str, input: &str) -> String {
    deleted_stage_path(out_dir, input, 1)
}

/// Helper: the `-deleted-2.gramps` file inside the output directory.
fn no_events_path(out_dir: &str, input: &str) -> String {
    deleted_stage_path(out_dir, input, 2)
}

/// Helper: the `-deleted-3.gramps` file inside the output directory.
fn deleted_3_path(out_dir: &str, input: &str) -> String {
    deleted_stage_path(out_dir, input, 3)
}

/// Helper: the `-deleted-4.gramps` file inside the output directory.
fn deleted_4_path(out_dir: &str, input: &str) -> String {
    deleted_stage_path(out_dir, input, 4)
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
    let output_dir = temp_dir("ec_output");

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
        &output_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Verify -deleted-1 output has the orphaned event (Gramps kept it)
    let output_file = deleted_1_path(&output_dir, &input);
    let cleaned_content = std::fs::read_to_string(&output_file).unwrap();
    assert!(
        cleaned_content.contains("e0000001-4000-4b3d-8000-000000000001"),
        "Orphaned event should survive in -deleted-1 output"
    );

    // Verify -deleted-2 output has the event removed
    let no_events = no_events_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_events).exists(),
        "-deleted-2.gramps file should exist: {}",
        no_events
    );
    let no_events_content = std::fs::read_to_string(&no_events).unwrap();
    assert!(
        !no_events_content.contains("e0000001-4000-4b3d-8000-000000000001"),
        "Orphaned event should be removed from -deleted-2 output"
    );

    // Verify non-event content is preserved in -deleted-2
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
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("nope_output");

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
        &output_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -deleted-1 output should exist
    let output_file = deleted_1_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&output_file).exists(),
        "-deleted-1 output should exist"
    );

    // -deleted-2 should NOT exist (no pending events to clean)
    let no_events = no_events_path(&output_dir, &input);
    assert!(
        !std::path::Path::new(&no_events).exists(),
        "-deleted-2.gramps should NOT be written when there are no pending events"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
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
    let output_dir = temp_dir("mrsac_output");
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
        &output_dir,
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
            entry["status"], "deleted",
            "Event {} should be marked deleted after clean",
            entry["handle"]
        );
    }

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_file(&manifest_path);
}

// ---------------------------------------------------------------------------
// Note cleaning integration tests
// ---------------------------------------------------------------------------

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
    let output_dir = temp_dir("nc_output");
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
        &output_dir,
        "--save-manifest",
        &manifest_path,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -deleted-1 output should exist and contain the orphaned note (Gramps
    // Python backend only deletes people, never notes)
    let output_file = deleted_1_path(&output_dir, &input);
    let cleaned_content = std::fs::read_to_string(&output_file).unwrap();
    assert!(
        cleaned_content.contains("n0000001-4000-4b3d-8000-000000000001"),
        "Orphaned note should survive in -deleted-1 output"
    );

    // -deleted-2 may or may not exist (no pending events here, so it's skipped)
    // -deleted-3 should exist and have the note removed
    let no_notes = deleted_3_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_notes).exists(),
        "-deleted-3.gramps file should exist: {}",
        no_notes
    );
    let no_notes_content = std::fs::read_to_string(&no_notes).unwrap();
    assert!(
        !no_notes_content.contains("n0000001-4000-4b3d-8000-000000000001"),
        "Orphaned note should be removed from -deleted-3 output"
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
            entry["status"], "deleted",
            "Note {} should be marked deleted after clean",
            entry["handle"]
        );
    }

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_file(&manifest_path);
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
    let output_dir = temp_dir("nopn_output");

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
        &output_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -deleted-2 should exist (event cleaning runs)
    let no_events = no_events_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_events).exists(),
        "-deleted-2.gramps should exist when pending events are cleaned"
    );

    // -deleted-3 should NOT exist (no pending notes to clean)
    let no_notes = deleted_3_path(&output_dir, &input);
    assert!(
        !std::path::Path::new(&no_notes).exists(),
        "-deleted-3.gramps should NOT be written when there are no pending notes"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
}

// ---------------------------------------------------------------------------
// Place cleaning integration tests
// ---------------------------------------------------------------------------

/// Minimal Gramps 5.1 XML fixture with two people, a family, an event, a note,
/// and a place referenced by the event.
const UUID_FIXTURE_FAMILY_EVENT_NOTE_PLACE: &str = r###"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1.6"/>
    <researcher/>
  </header>
  <people>
    <person handle="d0000001-4000-4b3d-8000-000000000001" id="I0001">
      <gender>1</gender>
      <name><first>John</first><surname>Doe</surname></name>
      <eventref hlink="d0000001-4000-4b3d-8000-000000000003" role="Primary"/>
      <noteref hlink="d0000001-4000-4b3d-8000-000000000004"/>
    </person>
    <person handle="d0000002-4000-4b3d-8000-000000000002" id="I0002">
      <gender>2</gender>
      <name><first>Jane</first><surname>Doe</surname></name>
      <eventref hlink="d0000001-4000-4b3d-8000-000000000003" role="Primary"/>
      <noteref hlink="d0000001-4000-4b3d-8000-000000000004"/>
    </person>
  </people>
  <families>
    <family handle="f0000001-4000-4b3d-8000-000000000001">
      <father hlink="d0000001-4000-4b3d-8000-000000000001"/>
      <mother hlink="d0000002-4000-4b3d-8000-000000000002"/>
    </family>
  </families>
  <events>
    <event handle="d0000001-4000-4b3d-8000-000000000003" id="E0001">
      <type>Birth</type>
      <dateval val="1980-01-15"/>
      <place hlink="d0000001-4000-4b3d-8000-000000000005"/>
    </event>
  </events>
  <notes>
    <note handle="d0000001-4000-4b3d-8000-000000000004" type="Research" format="0">
      <text>Shared note referenced by both people.</text>
    </note>
  </notes>
  <places>
    <placeobj handle="d0000001-4000-4b3d-8000-000000000005" id="P0001">
      <ptitle>New York City</ptitle>
      <coord lat="40.7128" long="-74.0060"/>
    </placeobj>
  </places>
</database>
"###;

#[test]
fn e2e_delete_with_place_clean() {
    // Verify the full pipeline: deleting both people orphans the event, note,
    // and place; event cleaning runs, note cleaning runs, and place cleaning
    // removes the orphaned place from -deleted_4.gramps.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("pc_input.gramps");
    let selections = temp_path("pc_selections.json");
    let output_dir = temp_dir("pc_output");
    let manifest_path = temp_path("pc_manifest.json");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT_NOTE_PLACE);
    // Delete both people — the event, note, and place become orphaned
    write_selections(
        &selections,
        &[
            "d0000001-4000-4b3d-8000-000000000001",
            "d0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output_dir,
        "--save-manifest",
        &manifest_path,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -deleted-1 output should exist and contain the orphaned place (Gramps
    // Python backend only deletes people, never places)
    let output_file = deleted_1_path(&output_dir, &input);
    let cleaned_content = std::fs::read_to_string(&output_file).unwrap();
    assert!(
        cleaned_content.contains("d0000001-4000-4b3d-8000-000000000005"),
        "Orphaned place should survive in -deleted-1 output"
    );

    // -deleted-2 should exist (event cleaning runs) and still contain the place
    let no_events = no_events_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_events).exists(),
        "-deleted-2.gramps file should exist: {}",
        no_events
    );
    let no_events_content = std::fs::read_to_string(&no_events).unwrap();
    assert!(
        no_events_content.contains("d0000001-4000-4b3d-8000-000000000005"),
        "Place should survive in -deleted-2 (event cleaning only removes events)"
    );

    // -deleted-3 should exist (note cleaning runs) and still contain the place
    let no_notes = deleted_3_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_notes).exists(),
        "-deleted-3.gramps file should exist: {}",
        no_notes
    );
    let no_notes_content = std::fs::read_to_string(&no_notes).unwrap();
    assert!(
        no_notes_content.contains("d0000001-4000-4b3d-8000-000000000005"),
        "Place should survive in -deleted-3 (note cleaning only removes notes)"
    );

    // -deleted-4 should exist and have the place removed
    let no_places = deleted_4_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_places).exists(),
        "-deleted-4.gramps file should exist: {}",
        no_places
    );
    let no_places_content = std::fs::read_to_string(&no_places).unwrap();
    assert!(
        !no_places_content.contains("d0000001-4000-4b3d-8000-000000000005"),
        "Orphaned place should be removed from -deleted-4 output"
    );
    assert!(
        no_places_content.contains("<?xml"),
        "XML declaration should be preserved"
    );
    assert!(
        no_places_content.contains("<database"),
        "Database root element should be preserved"
    );

    // Manifest should mark the place as deleted (not pending)
    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let places_plan = &manifest["plan"]["places"];
    let to_delete = &places_plan["to_delete"];
    assert!(
        to_delete.as_array().is_some_and(|a| !a.is_empty()),
        "Places to_delete should not be empty"
    );
    for entry in to_delete.as_array().unwrap() {
        assert_eq!(
            entry["status"], "deleted",
            "Place {} should be marked deleted after clean",
            entry["handle"]
        );
    }

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_file(&manifest_path);
}

#[test]
fn e2e_delete_no_pending_places_noop() {
    // Verify that when there are no pending places, the place clean step is
    // skipped and no -deleted_4.gramps file is written.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("nopp_input.gramps");
    let selections = temp_path("nopp_selections.json");
    let output_dir = temp_dir("nopp_output");

    // UUID_FIXTURE_FAMILY_EVENT has an event but no places and no notes
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
        &output_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -deleted-2 should exist (event cleaning runs)
    let no_events = no_events_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_events).exists(),
        "-deleted-2.gramps should exist when pending events are cleaned"
    );

    // -deleted-4 should NOT exist (no pending places to clean)
    let no_places = deleted_4_path(&output_dir, &input);
    assert!(
        !std::path::Path::new(&no_places).exists(),
        "-deleted-4.gramps should NOT be written when there are no pending places"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn e2e_delete_deleted_2_and_3_unchanged() {
    // Verify that -deleted_2.gramps and -deleted_3.gramps still contain the
    // orphaned place; only -deleted_4.gramps drops it.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("dc_input.gramps");
    let selections = temp_path("dc_selections.json");
    let output_dir = temp_dir("dc_output");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT_NOTE_PLACE);
    write_selections(
        &selections,
        &[
            "d0000001-4000-4b3d-8000-000000000001",
            "d0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output_dir,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // -deleted-2 still contains the place
    let no_events = no_events_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_events).exists(),
        "-deleted-2.gramps should exist"
    );
    let no_events_content = std::fs::read_to_string(&no_events).unwrap();
    assert!(
        no_events_content.contains("d0000001-4000-4b3d-8000-000000000005"),
        "Place should survive in -deleted-2"
    );

    // -deleted-3 still contains the place
    let no_notes = deleted_3_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_notes).exists(),
        "-deleted-3.gramps should exist"
    );
    let no_notes_content = std::fs::read_to_string(&no_notes).unwrap();
    assert!(
        no_notes_content.contains("d0000001-4000-4b3d-8000-000000000005"),
        "Place should survive in -deleted-3"
    );

    // -deleted-4 drops the place
    let no_places = deleted_4_path(&output_dir, &input);
    assert!(
        std::path::Path::new(&no_places).exists(),
        "-deleted-4.gramps should exist"
    );
    let no_places_content = std::fs::read_to_string(&no_places).unwrap();
    assert!(
        !no_places_content.contains("d0000001-4000-4b3d-8000-000000000005"),
        "Place should be removed from -deleted-4"
    );

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn e2e_delete_manifest_places_marked_deleted() {
    // Verify that after successful place cleaning, the manifest on disk has
    // the pending places marked as Deleted.
    if !gramps_available() {
        eprintln!("Skipping: Gramps not available");
        return;
    }

    let input = temp_path("mpmd_input.gramps");
    let selections = temp_path("mpmd_selections.json");
    let output_dir = temp_dir("mpmd_output");
    let manifest_path = temp_path("mpmd_manifest.json");

    write_uuid_fixture(&input, UUID_FIXTURE_FAMILY_EVENT_NOTE_PLACE);
    write_selections(
        &selections,
        &[
            "d0000001-4000-4b3d-8000-000000000001",
            "d0000002-4000-4b3d-8000-000000000002",
        ],
    );

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input,
        "--selections",
        &selections,
        "--yes",
        "-o",
        &output_dir,
        "--save-manifest",
        &manifest_path,
    ]);
    assert_eq!(code, Some(0), "Delete failed: {}", stderr);

    // Load the manifest and verify place status
    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();

    // Places plan should exist
    let places_plan = &manifest["plan"]["places"];
    let to_delete = &places_plan["to_delete"];
    assert!(
        to_delete.as_array().is_some_and(|a| !a.is_empty()),
        "Places to_delete should not be empty"
    );

    // All place entries should be 'deleted' (not 'pending')
    for entry in to_delete.as_array().unwrap() {
        assert_eq!(
            entry["status"], "deleted",
            "Place {} should be marked deleted after clean",
            entry["handle"]
        );
    }

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&selections);
    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_file(&manifest_path);
}
