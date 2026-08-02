//! End-to-end tests for the gramps-gen CLI.
//!
//! These tests run the CLI binary as a subprocess and verify its output.
//! They test the full pipeline: generate → validate → serialize.
//!
//! To run these tests:
//! ```bash
//! cargo test -p cli --test e2e
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
fn temp_output_path(test_name: &str) -> String {
    std::format!(
        "/tmp/gramps_gen_e2e_{}_{}.gramps",
        test_name,
        std::process::id()
    )
}

#[test]
fn e2e_generate_basic() {
    let output = temp_output_path("basic");
    let (_stdout, stderr, code) = gramps_gen(&["generate", "--count", "10", "--output", &output]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);

    // Verify the output file exists and is non-empty XML
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(!content.is_empty(), "Output file should not be empty");
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

    // Clean up
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_with_seed() {
    let output = temp_output_path("seed");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "10", "--seed", "42", "--output", &output,
    ]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);
    assert!(stderr.contains("42"), "Should report seed 42");

    // Verify the output file is valid XML
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.contains("<database"),
        "Output should contain <database>"
    );

    // Clean up
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_with_adversarial() {
    let output = temp_output_path("adversarial");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count",
        "20",
        "--adversarial",
        "disconnected",
        "--output",
        &output,
    ]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);

    // Verify output is valid XML
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("<database"));
    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_with_all_adversarial() {
    let output = temp_output_path("all_adv");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count",
        "30",
        "--adversarial",
        "all",
        "--output",
        &output,
    ]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_with_all_features() {
    let output = temp_output_path("features");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count",
        "30",
        "--with-places",
        "--with-citations",
        "--with-notes",
        "--with-media",
        "--with-tags",
        "--output",
        &output,
    ]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.contains("<placeobj"),
        "Should contain place elements"
    );
    assert!(
        content.contains("<citation"),
        "Should contain citation elements"
    );
    // Note: notes, media, and tags feature flags are defined in the config
    // but not yet fully implemented in the generation engine. Assertions are
    // omitted for those elements.

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_validate_roundtrip() {
    let output = temp_output_path("roundtrip");
    let (_stdout, gen_stderr, gen_code) =
        gramps_gen(&["generate", "--count", "10", "--output", &output]);
    assert_eq!(gen_code, Some(0), "Generate failed: {}", gen_stderr);

    // Validate the generated file
    let (_stdout, val_stderr, val_code) = gramps_gen(&["validate", &output]);
    assert_eq!(val_code, Some(0), "Validate failed: {}", val_stderr);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_zero_count_fails() {
    let output = temp_output_path("zero");
    let (_stdout, stderr, code) = gramps_gen(&["generate", "--count", "0", "--output", &output]);
    assert_eq!(code, Some(1), "Zero count should fail");
    assert!(!stderr.is_empty(), "Should produce error output");

    // File should not exist
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn e2e_generate_large() {
    let output = temp_output_path("large");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "100", "--depth", "5", "--output", &output,
    ]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.len() > 1000,
        "Large generation should produce substantial output"
    );

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_validate_invalid_file() {
    let invalid_path = "/tmp/gramps_gen_e2e_invalid.gramps";
    std::fs::write(invalid_path, "<invalid>not valid grammar</invalid>").unwrap();

    let (_stdout, stderr, code) = gramps_gen(&["validate", invalid_path]);
    assert_eq!(code, Some(1), "Invalid file should fail validation");
    assert!(
        stderr.contains("database"),
        "Should mention missing <database>"
    );

    let _ = std::fs::remove_file(invalid_path);
}

#[test]
fn e2e_validate_nonexistent_file() {
    let (_stdout, stderr, code) = gramps_gen(&["validate", "/nonexistent/file.gramps"]);
    assert_eq!(code, Some(1), "Non-existent file should fail");
    assert!(!stderr.is_empty(), "Should produce error message");
}

#[test]
fn e2e_help_output() {
    let (stdout, stderr, code) = gramps_gen(&["--help"]);
    assert_eq!(code, Some(0), "Help should succeed");
    // clap prints help to stdout
    let combined = stdout + &stderr;
    assert!(
        combined.contains("gramps-gen"),
        "Help should mention binary name"
    );
    assert!(
        combined.contains("generate"),
        "Help should mention generate"
    );
    assert!(
        combined.contains("validate"),
        "Help should mention validate"
    );
}

#[test]
fn e2e_version_output() {
    let (stdout, stderr, code) = gramps_gen(&["--version"]);
    assert_eq!(code, Some(0), "Version should succeed");
    let combined = stdout + &stderr;
    assert!(!combined.is_empty(), "Should output version info");
}

#[test]
fn e2e_generate_with_scenario_yaml() {
    // Create a temporary YAML scenario file
    let scenario_path = "/tmp/gramps_gen_e2e_scenario.yaml";
    let mut file = std::fs::File::create(scenario_path).unwrap();
    write!(
        file,
        r#"
person_count: 15
with_places: true
with_citations: true
seed: 100
"#
    )
    .unwrap();
    file.flush().unwrap();

    let output = temp_output_path("scenario");
    let (_stdout, stderr, code) =
        gramps_gen(&["generate", "--config", scenario_path, "--output", &output]);
    assert_eq!(code, Some(0), "Generate with scenario failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.contains("<database"),
        "Output should contain <database>"
    );

    let _ = std::fs::remove_file(&output);
    let _ = std::fs::remove_file(scenario_path);
}

#[test]
fn e2e_validate_valid_file() {
    // Create a minimal valid .gramps file
    let valid_path = "/tmp/gramps_gen_e2e_valid.gramps";
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-01" version="5.2.0"/>
    <researcher><resname>Test</resname></researcher>
  </header>
</database>"#;
    std::fs::write(valid_path, xml).unwrap();

    let (_stdout, stderr, code) = gramps_gen(&["validate", valid_path]);
    assert_eq!(
        code,
        Some(0),
        "Valid file should pass validation: {}",
        stderr
    );

    let _ = std::fs::remove_file(valid_path);
}

#[test]
fn e2e_schema_list_output() {
    let (stdout, stderr, code) = gramps_gen(&["schema", "list"]);
    assert_eq!(code, Some(0), "Schema list should succeed: {}", stderr);
    let combined = stdout + &stderr;
    assert!(
        combined.contains("Local schemas"),
        "Should show local schemas"
    );
    // Note: this assertion assumes default features (schema-5-2).
    // Single-version builds (e.g. --features schema-5-1) need separate test jobs.
    assert!(combined.contains("5.2"), "Should mention version 5.2");
}

#[test]
fn e2e_generate_with_schema_version_52() {
    let output = temp_output_path("schema_52");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count",
        "5",
        "--seed",
        "42",
        "--schema-version",
        "5.2",
        "--output",
        &output,
    ]);
    assert_eq!(
        code,
        Some(0),
        "Generate with schema-version 5.2 failed: {}",
        stderr
    );

    let content = std::fs::read_to_string(&output).unwrap();
    // Note: this assertion assumes default features (schema-5-2).
    // Single-version builds (e.g. --features schema-5-1) need separate test jobs.
    assert!(
        content.contains(r#"version="5.2.0""#),
        "XML should have version 5.2.0"
    );

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_with_schema_version_51() {
    // This test only works when schema-5-1 is compiled in.
    // The binary is built with default features (schema-5-2), so this test
    // requires: cargo test --all-features
    let output = temp_output_path("schema_51");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count",
        "5",
        "--seed",
        "42",
        "--schema-version",
        "5.1",
        "--output",
        &output,
    ]);
    if code != Some(0) {
        // If 5.1 is not available, the command should fail with a clear error.
        // This is expected when only schema-5-2 is compiled in.
        assert!(
            stderr.contains("not available"),
            "Unexpected error: {}",
            stderr
        );
        eprintln!("Skipping: schema-5-1 not compiled in this build");
        return;
    }

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(
        content.contains(r#"version="5.1.6""#),
        "XML should have version 5.1.6"
    );

    let _ = std::fs::remove_file(&output);
}

/// Generate with schema-5.1, 100 persons, fixed seed, and verify that the
/// output contains a mix of genders (at least 5% female). This test guards
/// against the regression where all persons were assigned gender 0 (male)
/// due to the missing Gender enum in the 5.1 schema conversion.
#[test]
fn e2e_51_gender_distribution() {
    // This test only works when schema-5-1 is compiled in.
    let output = temp_output_path("gender_51");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count",
        "100",
        "--seed",
        "2026",
        "--schema-version",
        "5.1",
        "--output",
        &output,
    ]);
    if code != Some(0) {
        assert!(
            stderr.contains("not available"),
            "Unexpected error: {}",
            stderr
        );
        eprintln!("Skipping: schema-5-1 not compiled in this build");
        return;
    }

    let content = std::fs::read_to_string(&output).unwrap();

    // Count genders in the output XML (child element format)
    let female_count = content.matches("<gender>F</gender>").count();
    let total_count = content.matches("<gender>").count();
    let female_pct = female_count as f64 / total_count as f64 * 100.0;

    assert!(
        female_count > 0,
        "No female persons found in 5.1 generation. All {} persons are male/unknown.",
        total_count
    );
    assert!(
        female_pct >= 5.0,
        "Female proportion too low: {:.1}% ({}/{}). Expected at least 5%.",
        female_pct,
        female_count,
        total_count
    );

    eprintln!(
        "5.1 gender distribution: {:.1}% female ({}/{} persons)",
        female_pct, female_count, total_count
    );

    // Verify families have parent edges
    let family_count = content.matches("<family ").count();
    if family_count > 0 {
        // Count families with father or mother elements
        let parent_elements =
            content.matches("<father").count() + content.matches("<mother").count();
        assert!(
            parent_elements >= family_count,
            "Expected at least {} parent elements for {} families, got {}",
            family_count,
            family_count,
            parent_elements
        );
    }

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_generate_with_schema_version_unknown() {
    let output = temp_output_path("schema_unknown");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count",
        "5",
        "--schema-version",
        "99.99",
        "--output",
        &output,
    ]);
    assert_eq!(code, Some(1), "Unknown schema version should fail");
    assert!(
        stderr.contains("not available"),
        "Should mention not available"
    );

    // File should not exist
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn e2e_stats_text_output() {
    // Generate a file first
    let output = temp_output_path("stats_text");
    let (_stdout, stderr, code) = gramps_gen(&["generate", "--count", "20", "--output", &output]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);

    // Run stats in text mode
    let (stdout, stderr, code) = gramps_gen(&["stats", &output]);
    assert_eq!(code, Some(0), "Stats failed: {}", stderr);

    // Verify text output contains expected sections
    assert!(stdout.contains("File:"), "Should contain File: header");
    assert!(
        stdout.contains("Object counts"),
        "Should contain Object counts"
    );
    assert!(
        stdout.contains("Family size distribution"),
        "Should contain family size distribution"
    );
    assert!(
        stdout.contains("Family size × generation table"),
        "Should contain generation table section"
    );
    assert!(
        stdout.contains("# generations"),
        "Should contain generation table header"
    );
    assert!(
        stdout.contains("People not in any family"),
        "Should contain people not in family"
    );
    assert!(
        stdout.contains("Dangling family refs"),
        "Should contain dangling refs"
    );
    assert!(stdout.contains("People:"), "Should contain People count");
    assert!(
        stdout.contains("Families:"),
        "Should contain Families count"
    );

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_stats_json_output() {
    // Generate a file first
    let output = temp_output_path("stats_json");
    let (_stdout, stderr, code) = gramps_gen(&["generate", "--count", "20", "--output", &output]);
    assert_eq!(code, Some(0), "Generate failed: {}", stderr);

    // Run stats with --json
    let (stdout, stderr, code) = gramps_gen(&["stats", "--json", &output]);
    assert_eq!(code, Some(0), "Stats --json failed: {}", stderr);

    // Verify JSON output
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");
    assert!(parsed["file"].is_string(), "Should have file field");
    assert!(parsed["counts"].is_object(), "Should have counts field");
    assert!(
        parsed["counts"]["people"].as_i64().unwrap_or(0) > 0,
        "Should have people > 0"
    );
    assert!(
        parsed["family_size_distribution"].is_object(),
        "Should have family_size_distribution"
    );
    assert!(
        parsed["family_generation_table"].is_object(),
        "Should have family_generation_table"
    );
    assert!(
        parsed["people_not_in_family"].is_i64(),
        "Should have people_not_in_family"
    );
    assert!(
        parsed["dangling_refs"].is_i64(),
        "Should have dangling_refs"
    );
    assert!(parsed["warnings"].is_array(), "Should have warnings array");

    let _ = std::fs::remove_file(&output);
}

#[test]
fn e2e_stats_malformed_xml_fails() {
    use std::io::Write;
    let path = "/tmp/gramps_gen_e2e_stats_malformed.xml";
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "<database><person></database>").unwrap();

    let (_stdout, stderr, code) = gramps_gen(&["stats", path]);
    assert!(code != Some(0), "Stats should fail on malformed XML");
    assert!(
        stderr.contains("parse error") || stderr.contains("XmlParseError"),
        "Should mention parse error, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn e2e_stats_nonexistent_file_fails() {
    let (_stdout, stderr, code) = gramps_gen(&["stats", "/nonexistent/file.gramps"]);
    assert!(code != Some(0), "Stats should fail on nonexistent file");
    assert!(
        stderr.contains("I/O error") || stderr.contains("No such file"),
        "Should mention I/O error, got: {}",
        stderr
    );
}
