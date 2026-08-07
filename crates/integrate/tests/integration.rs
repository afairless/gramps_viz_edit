//! Integration tests for the `integrate_diff_viz` orchestrator.
//!
//! These tests write known diff CSV and selections JSON content to temp
//! files, run the orchestrator, and verify the results.

use std::io::Write;

use integrate::{integrate_diff_viz, IntegrateError};

/// Create a temporary file with the given content and return its path.
fn create_temp_file(content: &str) -> String {
    let mut dir = std::env::temp_dir();
    dir.push(format!("gramps_integrate_orch_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    let mut path = dir.clone();
    path.push(format!("test_{}.tmp", rand::random::<u64>()));

    let mut file = std::fs::File::create(&path).expect("create temp file");
    file.write_all(content.as_bytes())
        .expect("write temp file");
    path.to_string_lossy().to_string()
}

/// Clean up temp files.
fn cleanup_temp_file(path: &str) {
    let _ = std::fs::remove_file(path);
}

/// A minimal diff CSV with Person rows that match the selections.
const DIFF_CSV: &str = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"\n\"MODIFIED\",\"Person\",\"H001\",\"I0001\",\"John Smith\",\"H001\",\"I0001\",\"John Smith\",\"1.00\",\"surname\",\"Text\",\"Smith\",\"Jones\",\"0.50\"\n\"SAME\",\"Person\",\"H002\",\"I0002\",\"Jane Doe\",\"H002\",\"I0002\",\"Jane Doe\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"\n\"MODIFIED\",\"Person\",\"H003\",\"I0003\",\"Bob Brown\",\"H004\",\"I0003\",\"Bob Brown\",\"1.00\",\"given_name\",\"Text\",\"Bob\",\"Robert\",\"0.50\"\n";

/// Selections JSON matching H001, H002, H004.
const SELECTIONS_JSON: &str = r#"{
  "exported_at": "2025-01-15T10:30:00.000Z",
  "file": "selections.json",
  "selections": [
    {"handle": "H001", "name": "John Smith", "birth_date": "1840-07-13", "death_date": "1910-03-22", "gender": "male", "family_group": 3},
    {"handle": "H002", "name": "Jane Doe", "birth_date": null, "death_date": null, "gender": "female", "family_group": 3},
    {"handle": "H004", "name": "Bob Brown", "birth_date": "1850-01-01", "death_date": null, "gender": "male", "family_group": 1}
  ]
}"#;

/// Full pipeline: temp files with known CSV + JSON produce correct matches.
#[test]
fn integrate_diff_viz_matches() {
    let diff_path = create_temp_file(DIFF_CSV);
    let sel_path = create_temp_file(SELECTIONS_JSON);

    let report = integrate_diff_viz(&diff_path, &sel_path).expect("run orchestrator");

    // 3 Person rows: H001 (matches via handle_a), H002 (matches via handle_a),
    // H003 (handle_a not in selections, handle_b=H004 matches)
    assert_eq!(report.matched_count, 3);
    assert_eq!(report.rows.len(), 3);
    assert_eq!(report.diff_path, diff_path);
    assert_eq!(report.sel_path, sel_path);

    // Row 0: H001 matched via handle_a → side "a"
    assert_eq!(report.rows[0].side, "a");
    assert_eq!(report.rows[0].handle_a.as_deref(), Some("H001"));
    assert_eq!(report.rows[0].viz_name, "John Smith");
    assert_eq!(report.rows[0].viz_birth_date.as_deref(), Some("1840-07-13"));
    assert_eq!(report.rows[0].viz_family_group, 3);

    // Row 1: H002 matched via handle_a → side "a"
    assert_eq!(report.rows[1].side, "a");
    assert_eq!(report.rows[1].viz_name, "Jane Doe");
    assert_eq!(report.rows[1].viz_birth_date, None);

    // Row 2: H004 matched via handle_b → side "b"
    assert_eq!(report.rows[2].side, "b");
    assert_eq!(report.rows[2].handle_a.as_deref(), Some("H003"));
    assert_eq!(report.rows[2].handle_b.as_deref(), Some("H004"));
    assert_eq!(report.rows[2].viz_name, "Bob Brown");
    assert_eq!(report.rows[2].viz_family_group, 1);

    cleanup_temp_file(&diff_path);
    cleanup_temp_file(&sel_path);
}

/// Mismatched handles → 0 matches.
#[test]
fn integrate_diff_viz_no_matches() {
    // Diff rows reference H001/H002/H003, selections reference unrelated handles.
    let diff_path = create_temp_file(DIFF_CSV);
    let mismatched_json = r#"{
      "selections": [
        {"handle": "ZZZ1", "name": "Unrelated", "gender": "male", "family_group": 0},
        {"handle": "ZZZ2", "name": "Other", "gender": "female", "family_group": 0}
      ]
    }"#;
    let sel_path = create_temp_file(mismatched_json);

    let report = integrate_diff_viz(&diff_path, &sel_path).expect("run orchestrator");
    assert_eq!(report.matched_count, 0);
    assert!(report.rows.is_empty());

    cleanup_temp_file(&diff_path);
    cleanup_temp_file(&sel_path);
}

/// Missing diff CSV file → DiffReadError.
#[test]
fn integrate_diff_viz_missing_diff() {
    let sel_path = create_temp_file(SELECTIONS_JSON);
    let result = integrate_diff_viz("/nonexistent/diff.csv", &sel_path);
    match result {
        Err(IntegrateError::DiffReadError(_)) => {}
        other => panic!("expected DiffReadError, got {:?}", other),
    }
    cleanup_temp_file(&sel_path);
}

/// Missing selections JSON file → SelectionsReadError.
#[test]
fn integrate_diff_viz_missing_selections() {
    let diff_path = create_temp_file(DIFF_CSV);
    let result = integrate_diff_viz(&diff_path, "/nonexistent/selections.json");
    match result {
        Err(IntegrateError::SelectionsReadError(_)) => {}
        other => panic!("expected SelectionsReadError, got {:?}", other),
    }
    cleanup_temp_file(&diff_path);
}

/// Diff CSV with no Person rows → empty result but not an error.
#[test]
fn integrate_diff_viz_no_person_rows() {
    let no_person_csv = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"\n\"SAME\",\"Family\",\"F001\",\"F0001\",\"Smith Family\",\"F001\",\"F0001\",\"Smith Family\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"\n";
    let diff_path = create_temp_file(no_person_csv);
    let sel_path = create_temp_file(SELECTIONS_JSON);

    let report = integrate_diff_viz(&diff_path, &sel_path).expect("run orchestrator");
    assert_eq!(report.matched_count, 0);
    assert!(report.rows.is_empty());

    cleanup_temp_file(&diff_path);
    cleanup_temp_file(&sel_path);
}