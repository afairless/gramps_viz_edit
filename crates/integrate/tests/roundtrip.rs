//! Integration tests for the diff CSV parser round-trip.
//!
//! These tests verify that CSV output from `diff::output::format_csv()`
//! can be parsed back by `integrate::csv_reader::parse_diff_csv()`.

use std::io::Write;

use diff::output::format_csv;
use diff::report::{Classification, DiffReport, DiffSummary, FieldChange, FieldKind, ItemDiff};
use integrate::csv_reader::parse_diff_csv;

/// Create a temporary file with the given content and return its path.
fn create_temp_csv(content: &str) -> String {
    let mut dir = std::env::temp_dir();
    dir.push(format!("gramps_integrate_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    let mut path = dir.clone();
    path.push(format!("test_{}.csv", rand::random::<u64>()));

    let mut file = std::fs::File::create(&path).expect("create temp file");
    file.write_all(content.as_bytes()).expect("write temp file");
    path.to_string_lossy().to_string()
}

/// Clean up temp files.
fn cleanup_temp_file(path: &str) {
    let _ = std::fs::remove_file(path);
}

/// Helper: build a field change.
fn field_change(
    kind: FieldKind,
    name: &str,
    old: Option<&str>,
    new: Option<&str>,
    sim: f64,
) -> FieldChange {
    FieldChange {
        field_kind: kind,
        field_name: name.to_string(),
        old_value: old.map(String::from),
        new_value: new.map(String::from),
        similarity: sim,
    }
}

/// Helper: build an item diff.
fn item(
    handle_a: Option<&str>,
    handle_b: Option<&str>,
    item_type: &str,
    class: Classification,
    changes: Vec<FieldChange>,
) -> ItemDiff {
    ItemDiff {
        handle_a: handle_a.map(String::from),
        handle_b: handle_b.map(String::from),
        gramps_id_a: None,
        gramps_id_b: None,
        display_name_a: None,
        display_name_b: None,
        item_type: item_type.to_string(),
        classification: class,
        field_changes: changes,
        confidence: 1.0,
    }
}

/// Round-trip: format a DiffReport as CSV, then parse it back.
#[test]
fn roundtrip_diff_csv() {
    let report = DiffReport {
        summary: DiffSummary::default(),
        items: vec![
            item(
                Some("A001"),
                Some("B001"),
                "Person",
                Classification::Same,
                vec![],
            ),
            item(
                Some("A002"),
                Some("B002"),
                "Person",
                Classification::Modified,
                vec![field_change(
                    FieldKind::Text,
                    "surname",
                    Some("Smith"),
                    Some("Jones"),
                    0.5,
                )],
            ),
            item(None, Some("B003"), "Person", Classification::Added, vec![]),
            item(
                Some("A004"),
                None,
                "Person",
                Classification::Removed,
                vec![],
            ),
        ],
        ambiguous_cases: vec![],
    };

    let csv = format_csv(&report, true);
    let path = create_temp_csv(&csv);

    let result = parse_diff_csv(&path);
    assert!(result.is_ok(), "parse_diff_csv failed: {:?}", result.err());
    let rows = result.unwrap();

    // 4 items: Same (1) + Modified (1 field change → 1 row) + Added (1) + Removed (1) = 4 rows
    assert_eq!(rows.len(), 4, "expected 4 rows from round-trip");

    // Row 0: Same
    assert_eq!(rows[0].classification, "SAME");
    assert_eq!(rows[0].item_type, "Person");
    assert_eq!(rows[0].handle_a.as_deref(), Some("A001"));
    assert_eq!(rows[0].handle_b.as_deref(), Some("B001"));
    assert_eq!(rows[0].field_name, "");
    assert!((rows[0].confidence - 1.0).abs() < 1e-10);

    // Row 1: Modified (surname)
    assert_eq!(rows[1].classification, "MODIFIED");
    assert_eq!(rows[1].handle_a.as_deref(), Some("A002"));
    assert_eq!(rows[1].field_name, "surname");
    assert_eq!(rows[1].old_value, "Smith");
    assert_eq!(rows[1].new_value, "Jones");
    assert!((rows[1].similarity - 0.5).abs() < 1e-10);

    // Row 2: Added
    assert_eq!(rows[2].classification, "ADDED");
    assert_eq!(rows[2].handle_a, None);
    assert_eq!(rows[2].handle_b.as_deref(), Some("B003"));

    // Row 3: Removed
    assert_eq!(rows[3].classification, "REMOVED");
    assert_eq!(rows[3].handle_a.as_deref(), Some("A004"));
    assert_eq!(rows[3].handle_b, None);

    cleanup_temp_file(&path);
}

/// Round-trip with special characters: quotes, commas, newlines.
#[test]
fn roundtrip_special_characters() {
    let report = DiffReport {
        summary: DiffSummary::default(),
        items: vec![item_with_meta(
            Some("A001"),
            Some("B001"),
            Some("Smith, John \"The Great\""),
            Some("Jones, Jane"),
            "Person",
            Classification::Modified,
            vec![field_change(
                FieldKind::Text,
                "surname",
                Some("Smith, O'Brien"),
                Some("Jones\nDoe"),
                0.5,
            )],
        )],
        ambiguous_cases: vec![],
    };

    let csv = format_csv(&report, true);
    let path = create_temp_csv(&csv);

    let rows = parse_diff_csv(&path).expect("parse special characters CSV");
    assert_eq!(rows.len(), 1);

    // Display names should round-trip with commas and quotes preserved
    assert_eq!(
        rows[0].display_name_a.as_deref(),
        Some("Smith, John \"The Great\"")
    );
    assert_eq!(rows[0].display_name_b.as_deref(), Some("Jones, Jane"));

    // old_value preserves commas
    assert_eq!(rows[0].old_value, "Smith, O'Brien");
    // new_value: newlines are replaced with spaces by format_csv
    assert_eq!(rows[0].new_value, "Jones Doe");

    cleanup_temp_file(&path);
}

/// Helper: build an item diff with display names.
fn item_with_meta(
    handle_a: Option<&str>,
    handle_b: Option<&str>,
    display_name_a: Option<&str>,
    display_name_b: Option<&str>,
    item_type: &str,
    class: Classification,
    changes: Vec<FieldChange>,
) -> ItemDiff {
    ItemDiff {
        handle_a: handle_a.map(String::from),
        handle_b: handle_b.map(String::from),
        gramps_id_a: None,
        gramps_id_b: None,
        display_name_a: display_name_a.map(String::from),
        display_name_b: display_name_b.map(String::from),
        item_type: item_type.to_string(),
        classification: class,
        field_changes: changes,
        confidence: 1.0,
    }
}
