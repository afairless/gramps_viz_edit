//! Diff CSV parser — reads the CSV output from `gramps-gen diff --output csv`.
//!
//! The CSV format uses header-name-based deserialization via the `csv` crate,
//! making the parser robust against column reordering.

use serde::{Deserialize, Deserializer, Serialize};

use crate::IntegrateError;

/// A single row from the diff CSV file.
///
/// Matches the column layout of `gramps-gen diff --output csv`.
/// All fields are `pub` and match the CSV header names for
/// header-driven deserialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DiffRow {
    /// Classification: Same, Modified, Added, Removed, NeedsReview, ExtrinsicOnly
    pub classification: String,
    /// Type of primary item (e.g., "Person", "Family", "Event")
    pub item_type: String,
    /// Handle in the first graph (A). Empty for Added items.
    pub handle_a: Option<String>,
    /// Gramps database ID in the first graph (e.g. "I0002")
    pub gramps_id_a: Option<String>,
    /// Display name for the item in graph A (e.g. "John Smith")
    pub display_name_a: Option<String>,
    /// Handle in the second graph (B). Empty for Removed items.
    pub handle_b: Option<String>,
    /// Gramps database ID in the second graph (e.g. "I0002")
    pub gramps_id_b: Option<String>,
    /// Display name for the item in graph B
    pub display_name_b: Option<String>,
    /// Confidence score for the match (0.0–1.0)
    pub confidence: f64,
    /// Field name that changed (empty for Same/Added/Removed)
    pub field_name: String,
    /// Kind of field (Text, Date, HandleRef, etc.)
    pub field_kind: String,
    /// Old value in graph A (empty for Added)
    pub old_value: String,
    /// New value in graph B (empty for Removed)
    pub new_value: String,
    /// Similarity score between old and new values (0.0–1.0)
    #[serde(deserialize_with = "deserialize_f64_empty_as_zero")]
    pub similarity: f64,
}

/// Parse a diff CSV file into a vector of [`DiffRow`]s.
///
/// All rows are returned — no filtering by `item_type` is performed at
/// this stage. Returns an error if the file cannot be read or parsed.
///
/// # Errors
///
/// Returns [`IntegrateError::DiffReadError`] if:
/// - The file cannot be read (does not exist, permissions, etc.)
/// - The CSV content is malformed or has incorrect column types
pub fn parse_diff_csv(path: &str) -> Result<Vec<DiffRow>, IntegrateError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| IntegrateError::DiffReadError(format!("cannot read '{}': {}", path, e)))?;
    parse_diff_csv_str(&content)
}

/// Deserialize an f64, treating an empty string as 0.0.
pub(crate) fn deserialize_f64_empty_as_zero<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        return Ok(0.0);
    }
    s.parse::<f64>().map_err(D::Error::custom)
}

/// Internal helper: parse CSV from a string — used by both the public
/// function and unit tests to avoid temp-file overhead.
fn parse_diff_csv_str(content: &str) -> Result<Vec<DiffRow>, IntegrateError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(content.as_bytes());

    let mut rows = Vec::new();
    for result in reader.deserialize() {
        let row: DiffRow = result.map_err(|e| IntegrateError::DiffReadError(e.to_string()))?;
        rows.push(row);
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a CSV string from rows for testing.
    fn to_csv(rows: &[&str]) -> String {
        let header = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"";
        let mut out = header.to_string();
        out.push('\n');
        for row in rows {
            out.push_str(row);
            out.push('\n');
        }
        out
    }

    /// Parse a valid CSV with 3 Person items:
    /// - Same (no field changes, 1 row)
    /// - Modified with 2 field changes (surname, first_name) → 2 rows
    /// - Added (handle_a empty, 1 row)
    ///   Total: 4 rows
    #[test]
    fn parse_valid_csv() {
        let csv = to_csv(&[
            "\"Same\",\"Person\",\"A001\",\"I0001\",\"John Smith\",\"B001\",\"I0001\",\"John Smith\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"",
            "\"Modified\",\"Person\",\"A002\",\"I0002\",\"John Smith\",\"B002\",\"I0002\",\"John Jones\",\"1.00\",\"surname\",\"Text\",\"Smith\",\"Jones\",\"0.50\"",
            "\"Modified\",\"Person\",\"A002\",\"I0002\",\"John Smith\",\"B002\",\"I0002\",\"John Jones\",\"1.00\",\"first_name\",\"Text\",\"John\",\"James\",\"0.50\"",
            "\"Added\",\"Person\",\"\",\"\",\"\",\"B003\",\"I0003\",\"Jane Doe\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"",
        ]);
        let rows = parse_diff_csv_str(&csv).expect("parse valid CSV");
        assert_eq!(
            rows.len(),
            4,
            "expected 4 rows: 1 Same + 2 Modified + 1 Added"
        );

        // Row 0: Same
        assert_eq!(rows[0].classification, "Same");
        assert_eq!(rows[0].item_type, "Person");
        assert_eq!(rows[0].handle_a.as_deref(), Some("A001"));
        assert_eq!(rows[0].handle_b.as_deref(), Some("B001"));
        assert_eq!(rows[0].field_name, "");
        assert!((rows[0].similarity - 0.0).abs() < 1e-10);

        // Row 1: Modified (surname)
        assert_eq!(rows[1].classification, "Modified");
        assert_eq!(rows[1].field_name, "surname");
        assert_eq!(rows[1].old_value, "Smith");
        assert_eq!(rows[1].new_value, "Jones");
        assert!((rows[1].similarity - 0.5).abs() < 1e-10);

        // Row 2: Modified (first_name)
        assert_eq!(rows[2].classification, "Modified");
        assert_eq!(rows[2].field_name, "first_name");
        assert_eq!(rows[2].old_value, "John");
        assert_eq!(rows[2].new_value, "James");

        // Row 3: Added
        assert_eq!(rows[3].classification, "Added");
        assert_eq!(rows[3].handle_a, None);
        assert_eq!(rows[3].handle_b.as_deref(), Some("B003"));
    }

    /// Empty CSV (header only) produces an empty vec.
    #[test]
    fn parse_empty_csv() {
        let csv = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"\n";
        let rows = parse_diff_csv_str(csv).expect("parse empty CSV");
        assert!(rows.is_empty(), "header-only CSV should produce 0 rows");
    }

    /// CSV with non-Person rows is accepted — no item_type filtering at parse stage.
    #[test]
    fn parse_non_person_rows() {
        let csv = to_csv(&[
            "\"Same\",\"Family\",\"F001\",\"F0001\",\"Smith Family\",\"F001\",\"F0001\",\"Smith Family\",\"1.00\",\"\",\"\",\"\",\"\",\"0.00\"",
            "\"Modified\",\"Event\",\"E001\",\"E0001\",\"Birth\",\"E002\",\"E0001\",\"Birth\",\"1.00\",\"date\",\"Date\",\"1840\",\"1850\",\"0.80\"",
        ]);
        let rows = parse_diff_csv_str(&csv).expect("parse non-Person CSV");
        assert_eq!(rows.len(), 2, "should return all rows, no filtering");
        assert_eq!(rows[0].item_type, "Family");
        assert_eq!(rows[1].item_type, "Event");
    }

    /// CSV with special characters (commas, quotes) round-trips correctly.
    #[test]
    fn parse_special_characters() {
        // CSV escaping: embedded quotes are doubled (\"\")
        // The value \"Smith, John \"The Great\"\" becomes:
        // \"Smith, John \"\"The Great\"\"\" in CSV
        let csv = to_csv(&[
            "\"Modified\",\"Person\",\"A001\",\"I0001\",\"Smith, John \"\"The Great\"\"\",\"B001\",\"I0001\",\"Jones, Jane\",\"1.00\",\"surname\",\"Text\",\"Smith, John\",\"Jones Newline\",\"0.50\"",
        ]);
        let rows = parse_diff_csv_str(&csv).expect("parse special characters CSV");
        assert_eq!(rows.len(), 1);
        // Display names with commas and quotes
        assert_eq!(
            rows[0].display_name_a.as_deref(),
            Some("Smith, John \"The Great\"")
        );
        assert_eq!(rows[0].display_name_b.as_deref(), Some("Jones, Jane"));
        // Values with commas
        assert_eq!(rows[0].old_value, "Smith, John");
        assert_eq!(rows[0].new_value, "Jones Newline");
    }

    /// Invalid CSV (wrong number of columns) returns an error.
    #[test]
    fn parse_invalid_csv() {
        // Missing a column (only 3 columns instead of 14)
        let csv = "\"classification\",\"item_type\",\"handle_a\"\n\"Same\",\"Person\",\"A001\"\n";
        let result = parse_diff_csv_str(csv);
        assert!(result.is_err(), "malformed CSV should return DiffReadError");
        match result {
            Err(IntegrateError::DiffReadError(msg)) => {
                assert!(!msg.is_empty(), "error message should not be empty");
            }
            _ => panic!("expected DiffReadError"),
        }
    }

    /// CSV with a row that has wrong types (non-numeric confidence) returns an error.
    #[test]
    fn parse_invalid_type_csv() {
        let csv = "\"classification\",\"item_type\",\"handle_a\",\"gramps_id_a\",\"display_name_a\",\"handle_b\",\"gramps_id_b\",\"display_name_b\",\"confidence\",\"field_name\",\"field_kind\",\"old_value\",\"new_value\",\"similarity\"\n\"Same\",\"Person\",\"A001\",\"I0001\",\"John\",\"B001\",\"I0001\",\"John\",\"not-a-number\",\"\",\"\",\"\",\"\",\"0.00\"\n";
        let result = parse_diff_csv_str(csv);
        assert!(
            result.is_err(),
            "CSV with non-numeric confidence should return DiffReadError"
        );
    }
}
