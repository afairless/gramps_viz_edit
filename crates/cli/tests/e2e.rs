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
