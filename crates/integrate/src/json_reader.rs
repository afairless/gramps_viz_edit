//! Selections JSON parser — reads the visualizer selection export format.
//!
//! The visualizer exports selected persons as a JSON file with this shape:
//!
//! ```json
//! {
//!   "exported_at": "2025-01-15T10:30:00.000Z",
//!   "file": "selections.json",
//!   "selections": [
//!     { "handle": "...", "name": "John Smith", "birth_date": "...",
//!       "death_date": "...", "gender": "male", "family_group": 3 }
//!   ]
//! }
//! ```
//!
//! Only the `selections` array is extracted; the `exported_at` and `file`
//! metadata fields are ignored.

use serde::{Deserialize, Serialize};

use crate::IntegrateError;

/// A single person selected in the visualizer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    /// Gramps UUID handle — the join key against diff CSV handles.
    pub handle: String,
    /// Person's full name from the visualizer.
    pub name: String,
    /// Birth date as a display string.
    pub birth_date: Option<String>,
    /// Death date as a display string.
    pub death_date: Option<String>,
    /// Gender: "male", "female", or "unknown".
    pub gender: String,
    /// DSU family group ID (connected component index).
    pub family_group: usize,
}

/// Export payload produced by the visualizer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct SelectionExport {
    /// The selected persons.
    selections: Vec<Selection>,
}

/// Parse a visualizer selections JSON file into a vector of [`Selection`]s.
///
/// Returns an error if the file cannot be read, the JSON is malformed, or
/// a required field is missing.
///
/// # Errors
///
/// Returns [`IntegrateError::SelectionsReadError`] if:
/// - The file cannot be read (does not exist, permissions, etc.)
/// - The JSON is not valid
/// - A `selections` entry is missing a required field (e.g. `handle`)
pub fn parse_selections_json(path: &str) -> Result<Vec<Selection>, IntegrateError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        IntegrateError::SelectionsReadError(format!("cannot read '{}': {}", path, e))
    })?;
    parse_selections_json_str(&content)
}

/// Internal helper: parse selections JSON from a string — used by both the
/// public function and unit tests to avoid temp-file overhead.
fn parse_selections_json_str(content: &str) -> Result<Vec<Selection>, IntegrateError> {
    let export: SelectionExport = serde_json::from_str(content)
        .map_err(|e| IntegrateError::SelectionsReadError(e.to_string()))?;
    Ok(export.selections)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a selections JSON string from a selections array body.
    fn to_json(selections_body: &str) -> String {
        format!(
            "{{\"exported_at\":\"2025-01-15T10:30:00.000Z\",\"file\":\"selections.json\",\"selections\":{selections_body}}}"
        )
    }

    /// Parse valid selections JSON with 3 people.
    #[test]
    fn parse_valid_selections() {
        let json = to_json(
            r#"[
                {"handle":"abc-1","name":"John Smith","birth_date":"1840-07-13","death_date":"1910-03-22","gender":"male","family_group":3},
                {"handle":"abc-2","name":"Jane Doe","birth_date":null,"death_date":null,"gender":"female","family_group":3},
                {"handle":"abc-3","name":"Unknown Person","birth_date":null,"death_date":null,"gender":"unknown","family_group":0}
            ]"#,
        );
        let selections = parse_selections_json_str(&json).expect("parse valid selections");
        assert_eq!(selections.len(), 3);

        // First: fully populated
        assert_eq!(selections[0].handle, "abc-1");
        assert_eq!(selections[0].name, "John Smith");
        assert_eq!(selections[0].birth_date.as_deref(), Some("1840-07-13"));
        assert_eq!(selections[0].death_date.as_deref(), Some("1910-03-22"));
        assert_eq!(selections[0].gender, "male");
        assert_eq!(selections[0].family_group, 3);

        // Second: null dates
        assert_eq!(selections[1].birth_date, None);
        assert_eq!(selections[1].death_date, None);
        assert_eq!(selections[1].gender, "female");

        // Third: unknown gender
        assert_eq!(selections[2].gender, "unknown");
        assert_eq!(selections[2].family_group, 0);
    }

    /// Empty selections array produces an empty vec.
    #[test]
    fn parse_empty_selections() {
        let json = to_json("[]");
        let selections = parse_selections_json_str(&json).expect("parse empty selections");
        assert!(selections.is_empty());
    }

    /// Invalid JSON returns a SelectionsReadError.
    #[test]
    fn parse_invalid_json() {
        let result = parse_selections_json_str("{\"selections\": [not valid json");
        match result {
            Err(IntegrateError::SelectionsReadError(msg)) => {
                assert!(!msg.is_empty());
            }
            _ => panic!("expected SelectionsReadError"),
        }
    }

    /// Missing required `handle` field returns a SelectionsReadError.
    #[test]
    fn parse_missing_handle() {
        let json = to_json(
            r#"[
                {"name":"John Smith","gender":"male","family_group":1}
            ]"#,
        );
        let result = parse_selections_json_str(&json);
        match result {
            Err(IntegrateError::SelectionsReadError(msg)) => {
                assert!(msg.contains("handle"), "error should mention handle: {msg}");
            }
            _ => panic!("expected SelectionsReadError for missing handle"),
        }
    }

    /// Missing `selections` key entirely returns an error.
    #[test]
    fn parse_missing_selections_key() {
        let json = "{\"exported_at\":\"2025-01-15T10:30:00.000Z\"}";
        let result = parse_selections_json_str(json);
        assert!(result.is_err());
        match result {
            Err(IntegrateError::SelectionsReadError(_)) => {}
            _ => panic!("expected SelectionsReadError"),
        }
    }

    /// A bare array (not wrapped in an object) returns an error.
    #[test]
    fn parse_bare_array() {
        let json = r#"[{"handle":"abc-1","name":"John","gender":"male","family_group":1}]"#;
        let result = parse_selections_json_str(json);
        assert!(result.is_err(), "bare array should be rejected");
    }
}
