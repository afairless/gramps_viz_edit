//! Output formatters — CSV and JSON serialization of merged diff-viz rows.
//!
//! These functions format a slice of [`MergedRow`]s as CSV or JSON. They do
//! not perform matching themselves — the caller is responsible for producing
//! the merged rows first.

use serde::Serialize;

use crate::csv_reader::DiffRow;
use crate::merge::{MergedRow, RowKind};

/// CSV column headers for merged output.
const CSV_HEADER: &[&str] = &[
    "classification",
    "item_type",
    "handle_a",
    "gramps_id_a",
    "display_name_a",
    "handle_b",
    "gramps_id_b",
    "display_name_b",
    "confidence",
    "field_name",
    "field_kind",
    "old_value",
    "new_value",
    "similarity",
    "side",
    "row_kind",
    "viz_name",
    "viz_birth_date",
    "viz_death_date",
    "viz_gender",
    "viz_family_group",
];

/// Format merged rows as CSV.
///
/// Produces a CSV string with all original diff columns followed by the
/// merge columns (`side`, `viz_name`, `viz_birth_date`, `viz_death_date`,
/// `viz_gender`, `viz_family_group`). An empty input produces a header-only
/// CSV. All cells are quoted per standard CSV rules.
pub fn format_csv(rows: &[MergedRow]) -> String {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());

    // Write header manually
    writer
        .write_record(CSV_HEADER)
        .expect("write CSV header to Vec");

    for row in rows {
        writer.serialize(row).expect("serialize merged row to CSV");
    }

    // Flush and extract the string
    writer.flush().expect("flush CSV writer");
    String::from_utf8(writer.into_inner().expect("into inner CSV writer"))
        .expect("CSV output is valid UTF-8")
}

/// A single match entry in the JSON output.
#[derive(Serialize)]
struct MatchEntry {
    /// The row kind: "matched", "diff_only", or "viz_only".
    row_kind: RowKind,
    /// Which side matched: "a" or "b" (empty for DiffOnly/VizOnly).
    side: String,
    /// The original diff row data.
    diff: DiffRow,
    /// The matching visualizer selection data (absent for DiffOnly rows).
    #[serde(skip_serializing_if = "Option::is_none")]
    selection: Option<SelectionView>,
}

/// The selection portion of a JSON match entry.
#[derive(Serialize)]
struct SelectionView {
    name: String,
    birth_date: Option<String>,
    death_date: Option<String>,
    gender: String,
    family_group: usize,
}

/// Top-level JSON output structure.
#[derive(Serialize)]
struct JsonOutput {
    /// Path of the source diff CSV file.
    diff_file: String,
    /// Path of the source selections JSON file.
    selection_file: String,
    /// Total number of rows (Matched + DiffOnly + VizOnly).
    row_count: usize,
    /// Number of rows with RowKind::Matched.
    matched_count: usize,
    /// The merged rows.
    matches: Vec<MatchEntry>,
}

/// Format merged rows as JSON.
///
/// Produces a pretty-printed JSON object with the input file paths, the
/// row count, matched count, and an array of match entries. Each entry
/// contains the row_kind, side, diff row data, and optionally the
/// selection data (absent for DiffOnly rows).
pub fn format_json(rows: &[MergedRow], diff_path: &str, sel_path: &str) -> String {
    let matched_count = rows
        .iter()
        .filter(|r| r.row_kind == RowKind::Matched)
        .count();

    let matches: Vec<MatchEntry> = rows
        .iter()
        .map(|row| {
            let diff: DiffRow = row.into();
            let selection = match row.row_kind {
                RowKind::DiffOnly => None,
                _ => Some(SelectionView {
                    name: row.viz_name.clone().unwrap_or_default(),
                    birth_date: row.viz_birth_date.clone(),
                    death_date: row.viz_death_date.clone(),
                    gender: row.viz_gender.clone().unwrap_or_default(),
                    family_group: row.viz_family_group.unwrap_or(0),
                }),
            };
            MatchEntry {
                row_kind: row.row_kind.clone(),
                side: row.side.clone(),
                diff,
                selection,
            }
        })
        .collect();

    let output = JsonOutput {
        diff_file: diff_path.to_string(),
        selection_file: sel_path.to_string(),
        row_count: rows.len(),
        matched_count,
        matches,
    };

    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
}

impl From<&MergedRow> for DiffRow {
    fn from(row: &MergedRow) -> Self {
        DiffRow {
            classification: row.classification.clone(),
            item_type: row.item_type.clone(),
            handle_a: row.handle_a.clone(),
            gramps_id_a: row.gramps_id_a.clone(),
            display_name_a: row.display_name_a.clone(),
            handle_b: row.handle_b.clone(),
            gramps_id_b: row.gramps_id_b.clone(),
            display_name_b: row.display_name_b.clone(),
            confidence: row.confidence,
            field_name: row.field_name.clone(),
            field_kind: row.field_kind.clone(),
            old_value: row.old_value.clone(),
            new_value: row.new_value.clone(),
            similarity: row.similarity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::{MergedRow, RowKind};

    /// Helper: build a fully-populated MergedRow.
    fn full_row() -> MergedRow {
        MergedRow {
            classification: "Modified".to_string(),
            item_type: "Person".to_string(),
            handle_a: Some("H001".to_string()),
            gramps_id_a: Some("I0001".to_string()),
            display_name_a: Some("Old Name".to_string()),
            handle_b: Some("H002".to_string()),
            gramps_id_b: Some("I0002".to_string()),
            display_name_b: Some("New Name".to_string()),
            confidence: 0.95,
            field_name: "surname".to_string(),
            field_kind: "Text".to_string(),
            old_value: "Smith".to_string(),
            new_value: "Jones".to_string(),
            similarity: 0.5,
            side: "a".to_string(),
            row_kind: RowKind::Matched,
            viz_name: Some("John Smith".to_string()),
            viz_birth_date: Some("1840-07-13".to_string()),
            viz_death_date: Some("1910-03-22".to_string()),
            viz_gender: Some("male".to_string()),
            viz_family_group: Some(3),
        }
    }

    /// Helper: build a MergedRow with None fields.
    fn sparse_row() -> MergedRow {
        MergedRow {
            classification: "Added".to_string(),
            item_type: "Person".to_string(),
            handle_a: None,
            gramps_id_a: None,
            display_name_a: None,
            handle_b: Some("B001".to_string()),
            gramps_id_b: None,
            display_name_b: None,
            confidence: 1.0,
            field_name: String::new(),
            field_kind: String::new(),
            old_value: String::new(),
            new_value: String::new(),
            similarity: 0.0,
            side: "b".to_string(),
            row_kind: RowKind::Matched,
            viz_name: Some("Jane Doe".to_string()),
            viz_birth_date: None,
            viz_death_date: None,
            viz_gender: Some("female".to_string()),
            viz_family_group: Some(0),
        }
    }

    /// A DiffOnly row for testing JSON output (no selection field).
    fn diff_only_row() -> MergedRow {
        MergedRow {
            classification: "Modified".to_string(),
            item_type: "Person".to_string(),
            handle_a: Some("H001".to_string()),
            gramps_id_a: Some("I0001".to_string()),
            display_name_a: Some("Old Name".to_string()),
            handle_b: Some("H002".to_string()),
            gramps_id_b: Some("I0002".to_string()),
            display_name_b: Some("New Name".to_string()),
            confidence: 0.95,
            field_name: "surname".to_string(),
            field_kind: "Text".to_string(),
            old_value: "Smith".to_string(),
            new_value: "Jones".to_string(),
            similarity: 0.5,
            side: String::new(),
            row_kind: RowKind::DiffOnly,
            viz_name: None,
            viz_birth_date: None,
            viz_death_date: None,
            viz_gender: None,
            viz_family_group: None,
        }
    }

    /// A VizOnly row for testing JSON output (default diff fields).
    fn viz_only_row() -> MergedRow {
        MergedRow {
            classification: String::new(),
            item_type: "Person".to_string(),
            handle_a: None,
            gramps_id_a: None,
            display_name_a: None,
            handle_b: None,
            gramps_id_b: None,
            display_name_b: None,
            confidence: 0.0,
            field_name: String::new(),
            field_kind: String::new(),
            old_value: String::new(),
            new_value: String::new(),
            similarity: 0.0,
            side: String::new(),
            row_kind: RowKind::VizOnly,
            viz_name: Some("Viz Person".to_string()),
            viz_birth_date: None,
            viz_death_date: None,
            viz_gender: Some("male".to_string()),
            viz_family_group: Some(5),
        }
    }

    // -----------------------------------------------------------------------
    // CSV tests
    // -----------------------------------------------------------------------

    /// Helper: parse a CSV string back into MergedRows.
    fn parse_csv(csv: &str) -> Vec<MergedRow> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(csv.as_bytes());
        reader
            .deserialize()
            .map(|r| r.expect("parse merged row"))
            .collect()
    }

    /// CSV header row contains all column names.
    #[test]
    fn csv_header() {
        let csv = format_csv(&[]);
        let header = csv.lines().next().expect("should have header");
        for col in CSV_HEADER {
            assert!(header.contains(col), "header should contain {col}");
        }
        // Header should have exactly 21 columns
        let cols: Vec<&str> = header.split(',').collect();
        assert_eq!(cols.len(), 21);
    }

    /// CSV: one row with all fields populated round-trips.
    #[test]
    fn csv_one_row_full() {
        let rows = vec![full_row()];
        let csv = format_csv(&rows);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 row

        let parsed = parse_csv(&csv);
        assert_eq!(parsed, rows);
    }

    /// CSV: empty input → only header row.
    #[test]
    fn csv_empty() {
        let csv = format_csv(&[]);
        assert_eq!(csv.lines().count(), 1);
        assert!(csv.starts_with("classification"));
    }

    /// CSV: special characters are escaped and round-trip.
    #[test]
    fn csv_special_characters() {
        let mut row = full_row();
        row.viz_name = Some("Smith, John \"The Great\"".to_string());
        row.viz_birth_date = Some("1840-07-13 / note, with comma".to_string());
        row.old_value = "Smith, O'Brien".to_string();

        let csv = format_csv(&[row.clone()]);
        // The value with quotes and commas should round-trip exactly
        let parsed = parse_csv(&csv);
        assert_eq!(parsed, vec![row]);
    }

    /// CSV: None fields produce empty cells and round-trip to None.
    #[test]
    fn csv_none_fields_empty() {
        let rows = vec![sparse_row()];
        let csv = format_csv(&rows);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);

        let parsed = parse_csv(&csv);
        assert_eq!(parsed, rows);
    }

    /// CSV: round-trips through the csv crate into MergedRow.
    #[test]
    fn csv_roundtrip_parse() {
        let rows = vec![full_row(), sparse_row()];
        let csv = format_csv(&rows);
        let parsed = parse_csv(&csv);
        assert_eq!(parsed, rows);
    }

    // -----------------------------------------------------------------------
    // JSON tests
    // -----------------------------------------------------------------------

    /// JSON: valid structure with row count and matched count.
    #[test]
    fn json_valid() {
        let rows = vec![full_row()];
        let json = format_json(&rows, "diff.csv", "selections.json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(value["diff_file"], "diff.csv");
        assert_eq!(value["selection_file"], "selections.json");
        assert_eq!(value["row_count"], 1);
        assert_eq!(value["matched_count"], 1);
        assert_eq!(value["matches"].as_array().unwrap().len(), 1);

        let m = &value["matches"][0];
        assert_eq!(m["row_kind"], "matched");
        assert_eq!(m["side"], "a");
        assert!(m["selection"].is_object());
        assert_eq!(m["diff"]["classification"], "Modified");
        assert_eq!(m["diff"]["handle_a"], "H001");
        assert_eq!(m["diff"]["field_name"], "surname");
        assert_eq!(m["diff"]["old_value"], "Smith");
        assert_eq!(m["diff"]["new_value"], "Jones");
        assert_eq!(m["selection"]["name"], "John Smith");
        assert_eq!(m["selection"]["birth_date"], "1840-07-13");
        assert_eq!(m["selection"]["death_date"], "1910-03-22");
        assert_eq!(m["selection"]["gender"], "male");
        assert_eq!(m["selection"]["family_group"], 3);
    }

    /// JSON: empty rows → "matches": [], row_count 0, matched_count 0.
    #[test]
    fn json_empty() {
        let json = format_json(&[], "diff.csv", "selections.json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["row_count"], 0);
        assert_eq!(value["matched_count"], 0);
        assert_eq!(value["matches"].as_array().unwrap().len(), 0);
    }

    /// JSON: None fields serialize as null, selection omitted for DiffOnly.
    #[test]
    fn json_none_fields_null() {
        let json = format_json(&[sparse_row()], "d.csv", "s.json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let m = &value["matches"][0];
        assert_eq!(m["row_kind"], "matched");
        assert_eq!(m["side"], "b");
        assert_eq!(m["diff"]["handle_a"], serde_json::Value::Null);
        assert_eq!(m["selection"]["birth_date"], serde_json::Value::Null);
        assert_eq!(m["selection"]["death_date"], serde_json::Value::Null);
    }

    /// JSON: DiffOnly row has row_kind "diff_only" and no selection field.
    #[test]
    fn json_diff_only() {
        let json = format_json(&[diff_only_row()], "d.csv", "s.json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(value["row_count"], 1);
        assert_eq!(value["matched_count"], 0);

        let m = &value["matches"][0];
        assert_eq!(m["row_kind"], "diff_only");
        assert_eq!(m["side"], "");
        assert_eq!(m["diff"]["classification"], "Modified");
        assert_eq!(m["diff"]["handle_a"], "H001");
        // selection field should be absent for DiffOnly rows
        assert_eq!(m.get("selection"), None);
    }

    /// JSON: VizOnly row has row_kind "viz_only", default diff fields, and selection present.
    #[test]
    fn json_viz_only() {
        let json = format_json(&[viz_only_row()], "d.csv", "s.json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(value["row_count"], 1);
        assert_eq!(value["matched_count"], 0);

        let m = &value["matches"][0];
        assert_eq!(m["row_kind"], "viz_only");
        assert_eq!(m["side"], "");
        // Diff fields should be default
        assert_eq!(m["diff"]["classification"], "");
        assert_eq!(m["diff"]["handle_a"], serde_json::Value::Null);
        assert_eq!(m["diff"]["confidence"], 0.0);
        // Selection should be present
        assert_eq!(m["selection"]["name"], "Viz Person");
        assert_eq!(m["selection"]["family_group"], 5);
    }

    /// JSON: mixed rows produce correct row_count vs matched_count.
    #[test]
    fn json_mixed_counts() {
        let rows = vec![full_row(), diff_only_row(), viz_only_row()];
        let json = format_json(&rows, "d.csv", "s.json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(value["row_count"], 3);
        assert_eq!(value["matched_count"], 1);
        assert_eq!(value["matches"].as_array().unwrap().len(), 3);

        let ms = value["matches"].as_array().unwrap();
        assert_eq!(ms[0]["row_kind"], "matched");
        assert!(ms[0]["selection"].is_object());
        assert_eq!(ms[1]["row_kind"], "diff_only");
        assert_eq!(ms[1].get("selection"), None);
        assert_eq!(ms[2]["row_kind"], "viz_only");
        assert!(ms[2]["selection"].is_object());
    }

    // -----------------------------------------------------------------------
    // Property-based round-trip tests
    // -----------------------------------------------------------------------

    use proptest::prelude::*;
    use proptest_derive::Arbitrary;

    /// A row shape for arbitrary generation — maps to MergedRow.
    #[derive(Arbitrary, Debug, Clone)]
    struct PropRow {
        classification: String,
        item_type: String,
        #[proptest(strategy = "opt_nonempty_str()")]
        handle_a: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        gramps_id_a: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        display_name_a: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        handle_b: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        gramps_id_b: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        display_name_b: Option<String>,
        #[proptest(strategy = "(0..=10000u32).prop_map(|n| n as f64 / 100.0)")]
        confidence: f64,
        field_name: String,
        field_kind: String,
        old_value: String,
        new_value: String,
        #[proptest(strategy = "(0..=10000u32).prop_map(|n| n as f64 / 100.0)")]
        similarity: f64,
        side: String,
        #[proptest(strategy = "row_kind_strategy()")]
        row_kind: RowKind,
        #[proptest(strategy = "opt_nonempty_str()")]
        viz_name: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        viz_birth_date: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        viz_death_date: Option<String>,
        #[proptest(strategy = "opt_nonempty_str()")]
        viz_gender: Option<String>,
        #[proptest(strategy = "opt_usize()")]
        viz_family_group: Option<usize>,
    }

    /// Strategy for optional strings that never produces Some("").
    fn opt_nonempty_str() -> impl Strategy<Value = Option<String>> {
        prop::option::weighted(0.8, "[a-zA-Z0-9 ._,!?@/-]+")
    }

    /// Strategy for optional usize values.
    fn opt_usize() -> impl Strategy<Value = Option<usize>> {
        prop::option::weighted(0.8, 0..=1000usize)
    }

    /// Strategy for generating any RowKind variant.
    fn row_kind_strategy() -> impl Strategy<Value = RowKind> {
        prop::sample::select(vec![RowKind::Matched, RowKind::DiffOnly, RowKind::VizOnly])
    }

    impl From<PropRow> for MergedRow {
        fn from(p: PropRow) -> Self {
            MergedRow {
                classification: p.classification,
                item_type: p.item_type,
                handle_a: p.handle_a,
                gramps_id_a: p.gramps_id_a,
                display_name_a: p.display_name_a,
                handle_b: p.handle_b,
                gramps_id_b: p.gramps_id_b,
                display_name_b: p.display_name_b,
                confidence: p.confidence,
                field_name: p.field_name,
                field_kind: p.field_kind,
                old_value: p.old_value,
                new_value: p.new_value,
                similarity: p.similarity,
                side: p.side,
                row_kind: p.row_kind,
                viz_name: p.viz_name,
                viz_birth_date: p.viz_birth_date,
                viz_death_date: p.viz_death_date,
                viz_gender: p.viz_gender,
                viz_family_group: p.viz_family_group,
            }
        }
    }

    proptest! {
        /// Property: CSV round-trip — format_csv then parse back produces the same rows.
        #[test]
        fn prop_csv_roundtrip(ref rows in proptest::collection::vec(any::<PropRow>().prop_map(MergedRow::from), 0..=10)) {
            let csv = format_csv(rows);
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv.as_bytes());
            let parsed: Result<Vec<MergedRow>, _> = reader.deserialize().collect();
            let parsed = parsed.expect("parse CSV output");
            assert_eq!(parsed, *rows);
        }

        /// Property: JSON round-trip — format_json produces valid JSON with correct structure.
        #[test]
        fn prop_json_structure(ref rows in proptest::collection::vec(any::<PropRow>().prop_map(MergedRow::from), 0..=10)) {
            let json = format_json(rows, "diff.csv", "selections.json");
            let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
            assert_eq!(value["diff_file"], "diff.csv");
            assert_eq!(value["selection_file"], "selections.json");
            assert_eq!(value["row_count"].as_u64().unwrap(), rows.len() as u64);
            let matched_count = rows
                .iter()
                .filter(|r| r.row_kind == crate::merge::RowKind::Matched)
                .count();
            assert_eq!(
                value["matched_count"].as_u64().unwrap(),
                matched_count as u64
            );
            let matches = value["matches"].as_array().unwrap();
            assert_eq!(matches.len(), rows.len());

            for (i, m) in matches.iter().enumerate() {
                let row = &rows[i];
                match row.row_kind {
                    crate::merge::RowKind::Matched => {
                        assert_eq!(m["row_kind"], "matched");
                        assert!(m.get("selection").is_some());
                    }
                    crate::merge::RowKind::DiffOnly => {
                        assert_eq!(m["row_kind"], "diff_only");
                        assert_eq!(m.get("selection"), None);
                    }
                    crate::merge::RowKind::VizOnly => {
                        assert_eq!(m["row_kind"], "viz_only");
                        assert!(m.get("selection").is_some());
                    }
                }
                // Check side
                assert_eq!(m["side"].as_str().unwrap(), row.side);
                // Check diff object has all expected keys
                let diff = m["diff"].as_object().unwrap();
                assert!(diff.contains_key("classification"));
                assert!(diff.contains_key("handle_a"));
                assert!(diff.contains_key("handle_b"));
                assert!(diff.contains_key("field_name"));
                assert!(diff.contains_key("old_value"));
                assert!(diff.contains_key("new_value"));
                // Check selection is present for Matched/VizOnly, absent for DiffOnly
                if row.row_kind != crate::merge::RowKind::DiffOnly {
                    let sel = m["selection"].as_object().unwrap();
                    assert!(sel.contains_key("name"));
                    assert!(sel.contains_key("birth_date"));
                    assert!(sel.contains_key("death_date"));
                    assert!(sel.contains_key("gender"));
                    assert!(sel.contains_key("family_group"));
                }
            }
        }
    }
}
