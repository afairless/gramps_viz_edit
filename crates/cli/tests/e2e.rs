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
        stdout.contains("Family group size × generation table"),
        "Should contain family group generation table section"
    );
    assert!(
        stdout.contains("# generations"),
        "Should contain generation table header"
    );
    assert!(
        stdout.contains("Family group distribution"),
        "Should contain family group distribution section"
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
        parsed["family_group_generation_table"].is_object(),
        "Should have family_group_generation_table"
    );
    assert!(
        parsed["family_group_distribution"].is_object(),
        "Should have family_group_distribution"
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

// ---------------------------------------------------------------------------
// extract-schema not present in help
// ---------------------------------------------------------------------------

#[test]
fn extract_schema_not_in_help() {
    let (stdout, _stderr, _code) = gramps_gen(&["--help"]);
    assert!(
        !stdout.contains("extract-schema"),
        "extract-schema should not appear in --help"
    );
    assert!(
        !stdout.contains("extract_schema"),
        "extract_schema should not appear in --help"
    );
}

// ---------------------------------------------------------------------------
// visualize subcommand
// ---------------------------------------------------------------------------

#[test]
fn e2e_visualize_nonexistent_file() {
    let (_stdout, stderr, code) = gramps_gen(&["visualize", "/nonexistent/path.gramps"]);
    assert!(code != Some(0), "visualize should fail on nonexistent file");
    assert!(
        stderr.contains("I/O error")
            || stderr.contains("No such file")
            || stderr.contains("Cannot read"),
        "Should mention file not found, got: {}",
        stderr
    );
}

#[test]
fn e2e_visualize_non_gramps_extension() {
    use std::io::Write;
    let path = "/tmp/gramps_gen_e2e_visualize_bad_ext.txt";
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "<database/>").unwrap();

    let (_stdout, stderr, code) = gramps_gen(&["visualize", path]);
    assert!(
        code != Some(0),
        "visualize should fail on non-.gramps extension"
    );
    assert!(
        stderr.contains("gramps") || stderr.contains("extension") || stderr.contains("ConfigError"),
        "Should mention .gramps extension, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn e2e_visualize_gap_out_of_range() {
    use std::io::Write;
    let path = "/tmp/gramps_gen_e2e_visualize_gap_range.gramps";
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "<database/>").unwrap();

    let (_stdout, stderr, code) = gramps_gen(&["visualize", path, "--generation-gap", "200"]);
    assert!(code != Some(0), "visualize should fail on gap out of range");
    assert!(
        stderr.contains("gap") || stderr.contains("1..=100") || stderr.contains("ConfigError"),
        "Should mention gap validation, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn e2e_visualize_gap_zero() {
    use std::io::Write;
    let path = "/tmp/gramps_gen_e2e_visualize_gap_zero.gramps";
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "<database/>").unwrap();

    let (_stdout, stderr, code) = gramps_gen(&["visualize", path, "--generation-gap", "0"]);
    assert!(code != Some(0), "visualize should fail on gap 0");
    assert!(
        stderr.contains("gap") || stderr.contains("1..=100"),
        "Should mention gap validation, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn e2e_visualize_sibling_binary_not_found() {
    // When gramps-gen-visualize is not installed alongside gramps-gen,
    // the visualize command should fail gracefully with a clear error.
    // We use a valid .gramps file to ensure we get past path validation
    // but hit the sibling binary lookup failure.
    //
    // Override the sibling binary path via env var so the test is
    // deterministic regardless of whether the sibling binary exists in
    // the cargo build directory.
    std::env::set_var(
        "GRAMPS_GEN_VISUALIZE_BIN",
        "/nonexistent/gramps-gen-visualize",
    );

    use std::io::Write;
    let path = "/tmp/gramps_gen_e2e_visualize_sibling.gramps";
    let mut f = std::fs::File::create(path).unwrap();
    write!(
        f,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><name><first>Test</first><surname>User</surname></name></person>
  </people>
</database>"#
    )
    .unwrap();

    let (_stdout, stderr, code) = gramps_gen(&["visualize", path]);
    assert!(
        code != Some(0),
        "visualize should fail when sibling binary is missing"
    );
    assert!(
        stderr.contains("gramps-gen-visualize")
            || stderr.contains("sibling")
            || stderr.contains("binary")
            || stderr.contains("No such file"),
        "Should mention missing sibling binary, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(path);
    std::env::remove_var("GRAMPS_GEN_VISUALIZE_BIN");
}

// ---------------------------------------------------------------------------
// Diff subcommand tests
// ---------------------------------------------------------------------------

#[test]
fn e2e_diff_help_shows_subcommand() {
    let (stdout, _, code) = gramps_gen(&["diff", "--help"]);
    assert_eq!(code, Some(0), "diff --help should succeed");
    assert!(stdout.contains("FILE_A"), "Help should show FILE_A arg");
    assert!(stdout.contains("FILE_B"), "Help should show FILE_B arg");
    assert!(
        stdout.contains("threshold"),
        "Help should show --threshold option"
    );
}

#[test]
fn e2e_diff_two_generated_files() {
    let output_a = temp_output_path("diff_a");
    let output_b = temp_output_path("diff_b");

    // Generate two different files
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "5", "--seed", "42", "--output", &output_a,
    ]);
    assert_eq!(code, Some(0), "Generate A failed: {}", stderr);

    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "5", "--seed", "99", "--output", &output_b,
    ]);
    assert_eq!(code, Some(0), "Generate B failed: {}", stderr);

    // Run diff
    let (stdout, stderr, code) = gramps_gen(&["diff", &output_a, &output_b]);
    assert_eq!(code, Some(0), "Diff failed: {}", stderr);

    // Verify output contains expected headings
    assert!(
        stdout.contains("Gramps Diff Report"),
        "Should contain report heading"
    );
    assert!(stdout.contains("Summary"), "Should contain summary");
    assert!(stdout.contains("Total (A)"), "Should show Total (A)");
    assert!(stdout.contains("Total (B)"), "Should show Total (B)");

    // Clean up
    let _ = std::fs::remove_file(&output_a);
    let _ = std::fs::remove_file(&output_b);
}

#[test]
fn e2e_diff_json_output() {
    let output_a = temp_output_path("diff_json_a");
    let output_b = temp_output_path("diff_json_b");

    // Generate two files with same seed (identical)
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "5", "--seed", "42", "--output", &output_a,
    ]);
    assert_eq!(code, Some(0), "Generate A failed: {}", stderr);

    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "5", "--seed", "42", "--output", &output_b,
    ]);
    assert_eq!(code, Some(0), "Generate B failed: {}", stderr);

    // Run diff with JSON output
    let (stdout, stderr, code) = gramps_gen(&["diff", &output_a, &output_b, "--output", "json"]);
    assert_eq!(code, Some(0), "Diff JSON failed: {}", stderr);

    // Verify JSON output parses
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("Diff JSON output should be valid JSON");
    assert!(
        report.get("summary").is_some(),
        "JSON should contain summary"
    );

    // Clean up
    let _ = std::fs::remove_file(&output_a);
    let _ = std::fs::remove_file(&output_b);
}

#[test]
fn e2e_diff_csv_output() {
    let output_a = temp_output_path("diff_csv_a");
    let output_b = temp_output_path("diff_csv_b");

    // Generate two files with different seeds to produce some diffs
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "5", "--seed", "42", "--output", &output_a,
    ]);
    assert_eq!(code, Some(0), "Generate A failed: {}", stderr);

    let (_stdout, stderr, code) = gramps_gen(&[
        "generate", "--count", "5", "--seed", "99", "--output", &output_b,
    ]);
    assert_eq!(code, Some(0), "Generate B failed: {}", stderr);

    // Run diff with CSV output
    let (stdout, stderr, code) = gramps_gen(&["diff", &output_a, &output_b, "--output", "csv"]);
    assert_eq!(code, Some(0), "Diff CSV failed: {}", stderr);

    // Verify CSV header is present
    assert!(
        stdout.contains("\"classification\""),
        "CSV should have classification header"
    );
    assert!(
        stdout.contains("\"item_type\""),
        "CSV should have item_type header"
    );
    assert!(
        stdout.contains("\"handle_a\""),
        "CSV should have handle_a header"
    );
    assert!(
        stdout.contains("\"field_name\""),
        "CSV should have field_name header"
    );
    assert!(
        stdout.contains("\"old_value\""),
        "CSV should have old_value header"
    );
    assert!(
        stdout.contains("\"new_value\""),
        "CSV should have new_value header"
    );

    // Verify at least one data row exists
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.len() > 1,
        "CSV should have header plus data rows, got {} lines",
        lines.len()
    );

    // Clean up
    let _ = std::fs::remove_file(&output_a);
    let _ = std::fs::remove_file(&output_b);
}

#[test]
fn e2e_diff_missing_file_shows_error() {
    let (_stdout, stderr, code) = gramps_gen(&[
        "diff",
        "/tmp/nonexistent_a.gramps",
        "/tmp/nonexistent_b.gramps",
    ]);
    assert!(code != Some(0), "Diff should fail when files don't exist");
    assert!(
        stderr.contains("parse error") || stderr.contains("error") || stderr.contains("failed"),
        "Should show error message, got: {}",
        stderr
    );
}

/// Smoke test: `integrate diff-viz --help` works.
#[test]
fn e2e_integrate_diff_viz_help() {
    let (stdout, _stderr, code) = gramps_gen(&["integrate", "diff-viz", "--help"]);
    assert_eq!(code, Some(0), "integrate diff-viz --help should succeed");
    assert!(stdout.contains("diff-viz"));
    assert!(stdout.contains("--diff"));
    assert!(stdout.contains("--selections"));
    assert!(stdout.contains("--format"));
}

/// Integration test: run `integrate diff-viz` with fixture files and verify CSV output.
#[test]
fn e2e_integrate_diff_viz_csv_output() {
    // Create fixture diff CSV
    let diff_csv = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"
\"MODIFIED\",\"Person\",\"H001\",\"I0001\",\"John Smith\",\"H001\",\"I0001\",\"John Smith\",\"1.00\",\"surname\",\"Text\",\"Smith\",\"Jones\",\"0.50\"
\"SAME\",\"Person\",\"H002\",\"I0002\",\"Jane Doe\",\"H002\",\"I0002\",\"Jane Doe\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"
";

    let selections_json = r#"{
  "selections": [
    {"handle": "H001", "name": "John Smith", "birth_date": "1840-07-13", "death_date": "1910-03-22", "gender": "male", "family_group": 3},
    {"handle": "H002", "name": "Jane Doe", "birth_date": null, "death_date": null, "gender": "female", "family_group": 3}
  ]
}"#;

    let diff_path = format!("/tmp/gramps_gen_e2e_diff_{}.csv", std::process::id());
    let sel_path = format!("/tmp/gramps_gen_e2e_sel_{}.json", std::process::id());
    let out_path = format!("/tmp/gramps_gen_e2e_out_{}.csv", std::process::id());

    std::fs::write(&diff_path, diff_csv).unwrap_or(());
    std::fs::write(&sel_path, selections_json).unwrap_or(());

    let (_stdout, stderr, code) = gramps_gen(&[
        "integrate",
        "diff-viz",
        "--diff",
        &diff_path,
        "--selections",
        &sel_path,
        "--output",
        &out_path,
    ]);
    assert_eq!(
        code,
        Some(0),
        "integrate diff-viz should succeed, stderr: {}",
        stderr
    );

    // Verify output file exists and has expected content
    let content = std::fs::read_to_string(&out_path).unwrap_or_default();
    assert!(!content.is_empty(), "Output file should not be empty");
    assert!(content.contains("viz_name"));
    assert!(content.contains("John Smith"));
    assert!(content.contains("Jane Doe"));
    assert!(content.contains("male"));

    // Verify integrated columns header
    assert!(content.contains("side"));
    assert!(content.contains("row_kind"));
    assert!(content.contains("viz_family_group"));

    // Clean up
    let _ = std::fs::remove_file(&diff_path);
    let _ = std::fs::remove_file(&sel_path);
    let _ = std::fs::remove_file(&out_path);
}

/// Integration test: missing diff file shows error.
#[test]
fn e2e_integrate_diff_viz_missing_diff() {
    let (_stdout, stderr, code) = gramps_gen(&[
        "integrate",
        "diff-viz",
        "--diff",
        "/tmp/nonexistent_diff.csv",
        "--selections",
        "/tmp/nonexistent_sel.json",
    ]);
    assert!(
        code != Some(0),
        "integrate diff-viz should fail when diff file is missing"
    );
    assert!(
        stderr.contains("error") || stderr.contains("failed"),
        "Should show error message, got: {}",
        stderr
    );
}

/// E2E test: unmatched diff row still appears as DiffOnly row in output.
#[test]
fn e2e_integrate_diff_viz_unmatched() {
    // Create a diff CSV where one handle has no matching selection
    let diff_csv = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"
\"MODIFIED\",\"Person\",\"H001\",\"I0001\",\"John Smith\",\"H001\",\"I0001\",\"John Smith\",\"1.00\",\"surname\",\"Text\",\"Smith\",\"Jones\",\"0.50\"
\"ADDED\",\"Person\",\"\",\"\",\"\",\"H002\",\"I0002\",\"Jane Doe\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"
";

    // Only H001 is in selections — H002 has no matching selection
    let selections_json = r#"{
      "selections": [
        {"handle": "H001", "name": "John Smith", "birth_date": "1840-07-13", "death_date": "1910-03-22", "gender": "male", "family_group": 3}
      ]
    }"#;

    let diff_path = format!(
        "/tmp/gramps_gen_e2e_unmatched_diff_{}.csv",
        std::process::id()
    );
    let sel_path = format!(
        "/tmp/gramps_gen_e2e_unmatched_sel_{}.json",
        std::process::id()
    );

    std::fs::write(&diff_path, diff_csv).unwrap();
    std::fs::write(&sel_path, selections_json).unwrap();

    // Test CSV output
    let out_path = format!(
        "/tmp/gramps_gen_e2e_unmatched_out_{}.csv",
        std::process::id()
    );
    let (_stdout, stderr, code) = gramps_gen(&[
        "integrate",
        "diff-viz",
        "--diff",
        &diff_path,
        "--selections",
        &sel_path,
        "--output",
        &out_path,
    ]);
    assert_eq!(
        code,
        Some(0),
        "integrate diff-viz should succeed with unmatched row, stderr: {}",
        stderr
    );

    let content = std::fs::read_to_string(&out_path).unwrap_or_default();
    // Both H001 and H002 should appear
    assert!(
        content.contains("H001"),
        "output should contain H001 (matched)"
    );
    assert!(
        content.contains("H002"),
        "output should contain H002 (DiffOnly)"
    );
    // H002 row should have empty viz fields and side
    assert!(content.contains("row_kind"));

    // Test JSON output
    let (json_stdout, json_stderr, json_code) = gramps_gen(&[
        "integrate",
        "diff-viz",
        "--diff",
        &diff_path,
        "--selections",
        &sel_path,
        "--format",
        "json",
    ]);
    assert_eq!(
        json_code,
        Some(0),
        "JSON output should succeed, stderr: {}",
        json_stderr
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&json_stdout).expect("should be valid JSON");
    assert_eq!(
        parsed["row_count"].as_i64(),
        Some(2),
        "row_count should be 2"
    );
    assert_eq!(
        parsed["matched_count"].as_i64(),
        Some(1),
        "matched_count should be 1"
    );

    // Verify row kinds
    let matches = parsed["matches"].as_array().unwrap();
    assert_eq!(matches[0]["row_kind"], "matched");
    assert_eq!(matches[1]["row_kind"], "diff_only");
    // DiffOnly row should have no selection field
    assert_eq!(matches[1].get("selection"), None);

    // Clean up
    let _ = std::fs::remove_file(&diff_path);
    let _ = std::fs::remove_file(&sel_path);
    let _ = std::fs::remove_file(&out_path);
}
/// selections verifies the wrapped format produced by the fixed export_selections
/// command is correctly consumed by integrate diff-viz.
#[test]
fn e2e_integrate_diff_viz_wrapped_envelope() {
    // Create a diff CSV with 3 Person rows (abc-3 will be DiffOnly)
    let diff_csv = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"
\"MODIFIED\",\"Person\",\"abc-1\",\"I0001\",\"John Smith\",\"abc-1\",\"I0001\",\"John Smith\",\"1.00\",\"name\",\"Text\",\"John\",\"John Smith\",\"0.80\"
\"SAME\",\"Person\",\"abc-2\",\"I0002\",\"Jane Doe\",\"abc-2\",\"I0002\",\"Jane Doe\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"
\"ADDED\",\"Person\",\"abc-3\",\"I0003\",\"Bob Brown\",\"abc-3\",\"I0003\",\"Bob Brown\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"
";

    // Create a selections JSON with only abc-1 and abc-2 selected.
    let selections_json = r#"{
  "exported_at": "2025-01-15T10:30:00.000Z",
  "file": "selections.json",
  "selections": [
    {"handle":"abc-1","name":"John Smith","birth_date":"1840-07-13","death_date":"1910-03-22","gender":"male","family_group":3},
    {"handle":"abc-2","name":"Jane Doe","birth_date":null,"death_date":null,"gender":"female","family_group":3}
  ]
}"#;

    let diff_path = format!(
        "/tmp/gramps_gen_e2e_wrapped_diff_{}.csv",
        std::process::id()
    );
    let sel_path = format!(
        "/tmp/gramps_gen_e2e_wrapped_sel_{}.json",
        std::process::id()
    );
    let out_path = format!("/tmp/gramps_gen_e2e_wrapped_out_{}.csv", std::process::id());

    std::fs::write(&diff_path, diff_csv).unwrap();
    std::fs::write(&sel_path, selections_json).unwrap();

    // Run integrate diff-viz with CSV output
    let (_stdout, stderr, code) = gramps_gen(&[
        "integrate",
        "diff-viz",
        "--diff",
        &diff_path,
        "--selections",
        &sel_path,
        "--output",
        &out_path,
    ]);
    assert_eq!(
        code,
        Some(0),
        "integrate diff-viz with wrapped envelope should succeed, stderr: {}",
        stderr
    );

    // Verify output file exists and has merged rows
    let content = std::fs::read_to_string(&out_path).expect("output file should exist");
    assert!(
        content.contains("abc-1"),
        "output should contain abc-1 (matched)"
    );
    assert!(
        content.contains("abc-2"),
        "output should contain abc-2 (matched)"
    );
    // With full outer join, abc-3 appears as a DiffOnly row
    assert!(
        content.contains("abc-3"),
        "output should contain abc-3 as DiffOnly row"
    );
    assert!(content.contains("John Smith"));
    assert!(content.contains("Jane Doe"));
    assert!(content.contains("male"));
    assert!(content.contains("female"));
    assert!(content.contains("viz_name"));
    assert!(content.contains("viz_family_group"));
    assert!(content.contains("row_kind"));

    // Also test --format json output to verify structured output
    let (json_stdout, json_stderr, json_code) = gramps_gen(&[
        "integrate",
        "diff-viz",
        "--diff",
        &diff_path,
        "--selections",
        &sel_path,
        "--format",
        "json",
    ]);
    assert_eq!(
        json_code,
        Some(0),
        "integrate diff-viz JSON format should succeed, stderr: {}",
        json_stderr
    );

    // Verify JSON output
    let parsed: serde_json::Value =
        serde_json::from_str(&json_stdout).expect("should be valid JSON");
    let matches = parsed["matches"]
        .as_array()
        .expect("should have matches array");
    // 2 Matched + 1 DiffOnly = 3 total rows
    assert_eq!(
        matches.len(),
        3,
        "should have 3 rows: 2 matched + 1 diff-only"
    );
    assert_eq!(
        parsed["matched_count"].as_i64(),
        Some(2),
        "matched_count should be 2"
    );
    assert_eq!(
        parsed["row_count"].as_i64(),
        Some(3),
        "row_count should be 3"
    );

    // Verify row kinds
    assert_eq!(matches[0]["row_kind"], "matched");
    assert_eq!(matches[1]["row_kind"], "matched");
    assert_eq!(matches[2]["row_kind"], "diff_only");

    // Verify matched handles
    let handles: Vec<&str> = matches
        .iter()
        .filter(|m| m["row_kind"] == "matched")
        .filter_map(|m| m["diff"]["handle_a"].as_str())
        .collect();
    assert!(handles.contains(&"abc-1"), "missing abc-1");
    assert!(handles.contains(&"abc-2"), "missing abc-2");
    assert_eq!(handles.len(), 2, "should have exactly 2 matched handles");

    // Verify diff-only row has no selection field
    assert_eq!(matches[2].get("selection"), None);

    // Clean up
    let _ = std::fs::remove_file(&diff_path);
    let _ = std::fs::remove_file(&sel_path);
    let _ = std::fs::remove_file(&out_path);
}

/// Whether the `gramps` binary is available on PATH.
///
/// Used to gate E2E tests that import real Gramps output. When Gramps is
/// not installed the test is skipped rather than failed, so headless CI
/// runners without Gramps still pass.
fn gramps_installed() -> bool {
    std::process::Command::new("gramps")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run the Gramps import CLI under a short timeout and return combined
/// stdout+stderr plus the exit code.
///
/// Gramps stays open after a successful import, so the command is run under
/// `timeout`; the timeout exit code (124) is treated as success since the
/// import itself has already completed by then.
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

#[test]
fn e2e_delete_notes_import_roundtrip() {
    // This E2E test exercises the fixed note type/format serialization.
    // It requires a real Gramps install to import the output; when Gramps
    // is absent the non-Gramps assertions still run and the import step is
    // skipped.
    let pid = std::process::id();
    let input_path = format!("/tmp/gramps_gen_e2e_notes_in_{}.gramps", pid);
    let sel_path = format!("/tmp/gramps_gen_e2e_notes_sel_{}.json", pid);
    let out_path = format!("/tmp/gramps_gen_e2e_notes_out_{}.gramps", pid);

    // Input fixture with two notes carrying type and format attributes.
    let fixture = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2024-01-01" version="5.2"/></header>
  <people>
    <person handle="P0001">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>
          <surname>Smith</surname>
          <primary>1</primary>
        </surname>
      </name>
    </person>
    <person handle="P0002">
      <gender>F</gender>
      <name>
        <first>Jane</first>
        <surname>
          <surname>Doe</surname>
          <primary>1</primary>
        </surname>
      </name>
    </person>
  </people>
  <notes>
    <note handle="N0001" type="General" format="1">
      <text>Research note about John.</text>
    </note>
    <note handle="N0002" type="To Do" format="0">
      <text>Follow up.</text>
    </note>
  </notes>
</database>
"#;
    // Delete P0001, keeping P0002 and both notes.
    let selections = r#"{
  "exported_at": "2025-01-15T10:30:00.000Z",
  "file": "selections.json",
  "selections": [
    {"handle":"P0001","name":"John Smith","birth_date":null,"death_date":null,"gender":"male","family_group":0}
  ]
}"#;

    std::fs::write(&input_path, fixture).unwrap();
    std::fs::write(&sel_path, selections).unwrap();

    // Run delete.
    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input_path,
        "--selections",
        &sel_path,
        "--yes",
        "--output",
        &out_path,
    ]);
    assert_eq!(code, Some(0), "delete should succeed, stderr: {}", stderr);

    let output = std::fs::read_to_string(&out_path).expect("output file should exist");

    // Key regression: every <note> element must carry a type attribute.
    // Match the opening tag only (not <notes>, </note>, or <noteref>).
    let notes: Vec<&str> = output
        .lines()
        .filter(|l| l.trim_start().starts_with("<note ") || l.trim_start().starts_with("<note>"))
        .collect();
    assert!(
        !notes.is_empty(),
        "output should contain note elements, got: {}",
        output
    );
    for line in &notes {
        assert!(
            line.contains("type="),
            "every <note> must carry a type attribute, offending line: {}",
            line
        );
    }
    // Note types must survive the round-trip losslessly.
    assert!(
        output.contains("type=\"General\""),
        "expected General note type"
    );
    assert!(
        output.contains("type=\"To Do\""),
        "expected To Do note type"
    );
    assert!(output.contains("format=\"1\""), "expected format 1");
    assert!(output.contains("format=\"0\""), "expected format 0");

    // Import into Gramps if it is installed.
    if gramps_installed() {
        let (import_out, import_code) = run_gramps_import(&out_path);
        // Exit 0 (success) or 124 (timeout because Gramps stays open) are OK.
        assert!(
            import_code == Some(0) || import_code == Some(124),
            "gramps import should complete after timeout, exit code: {:?}",
            import_code
        );
        for pat in ["ERROR:", "Traceback", "TypeError", "Failed to import"] {
            assert!(
                !import_out.contains(pat),
                "gramps import reported '{}' — bug regression, output:\n{}",
                pat,
                import_out
            );
        }
    } else {
        eprintln!("gramps not installed; skipping real import step");
    }

    // Clean up
    let _ = std::fs::remove_file(&input_path);
    let _ = std::fs::remove_file(&sel_path);
    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn e2e_delete_51_namespace_emits_flat_event_type() {
    // A 5.1-namespace input must round-trip with the same namespace and 5.1-flat
    // event-type serialization (<type>Birth</type>), not the 5.2 nested form.
    let pid = std::process::id();
    let input_path = format!("/tmp/gramps_gen_e2e_51_in_{}.gramps", pid);
    let sel_path = format!("/tmp/gramps_gen_e2e_51_sel_{}.json", pid);
    let out_path = format!("/tmp/gramps_gen_e2e_51_out_{}.gramps", pid);

    let fixture = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header><created date="2024-01-01" version="5.1"/></header>
  <people>
    <person handle="P0001">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>
          <surname>Smith</surname>
          <primary>1</primary>
        </surname>
      </name>
    </person>
  </people>
  <events>
    <event handle="E0001">
      <type>Birth</type>
    </event>
  </events>
</database>
"#;
    // Delete P0001 only; the standalone event is unrelated and must survive.
    let selections = r#"{
  "selections": [
    {"handle":"P0001","name":"John Smith","birth_date":null,"death_date":null,"gender":"male","family_group":0}
  ]
}"#;

    std::fs::write(&input_path, fixture).unwrap();
    std::fs::write(&sel_path, selections).unwrap();

    let (_stdout, stderr, code) = gramps_gen(&[
        "delete",
        &input_path,
        "--selections",
        &sel_path,
        "--yes",
        "--output",
        &out_path,
    ]);
    assert_eq!(code, Some(0), "delete should succeed, stderr: {}", stderr);

    let output = std::fs::read_to_string(&out_path).expect("output file should exist");

    // The preserved 5.1 namespace must remain.
    assert!(
        output.contains("http://gramps-project.org/xml/1.7.1/"),
        "5.1 namespace should be preserved, got: {}",
        output
    );
    // 5.1 flat event type, with no 5.2 nested <eventtype> wrapper.
    assert!(
        output.contains("<type>Birth</type>"),
        "5.1 event type should serialize flat, got: {}",
        output
    );
    assert!(
        !output.contains("<eventtype>"),
        "5.1 output must not contain nested <eventtype>, got: {}",
        output
    );

    let _ = std::fs::remove_file(&input_path);
    let _ = std::fs::remove_file(&sel_path);
    let _ = std::fs::remove_file(&out_path);
}
