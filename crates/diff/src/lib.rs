//! Gramps diff analyzer — compare two Gramps family trees.
//!
//! This crate provides utilities for comparing two Gramps XML files and
//! producing a structured diff report. Planned modules:
//!
//! - `similarity`: Text similarity scoring (Levenshtein, Jaccard, etc.)
//! - `normalize`: Text normalization utilities
//! - `report`: Diff report data types

use std::error::Error;
use std::fmt;

/// Errors that can occur during diff analysis.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffError {
    /// One or both input files could not be parsed.
    ParseError(String),
    /// The two files use incompatible schema versions.
    SchemaMismatch(String),
    /// One or both graphs are empty.
    EmptyGraph(String),
    /// An internal error occurred.
    InternalError(String),
    /// The requested operation is not yet implemented.
    Unimplemented,
}

impl fmt::Display for DiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffError::ParseError(msg) => write!(f, "parse error: {msg}"),
            DiffError::SchemaMismatch(msg) => write!(f, "schema mismatch: {msg}"),
            DiffError::EmptyGraph(msg) => write!(f, "empty graph: {msg}"),
            DiffError::InternalError(msg) => write!(f, "internal error: {msg}"),
            DiffError::Unimplemented => write!(f, "not yet implemented"),
        }
    }
}

impl Error for DiffError {}

/// Run the full diff pipeline between two Gramps XML files.
///
/// # Errors
///
/// Returns [`DiffError::Unimplemented`] until the pipeline is implemented.
pub fn run_diff(_file_a: &str, _file_b: &str) -> Result<(), DiffError> {
    Err(DiffError::Unimplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_error_debug() {
        let err = DiffError::Unimplemented;
        assert!(format!("{err:?}").contains("Unimplemented"));
    }

    #[test]
    fn diff_error_display() {
        assert_eq!(
            DiffError::Unimplemented.to_string(),
            "not yet implemented"
        );
        assert_eq!(
            DiffError::ParseError("bad file".into()).to_string(),
            "parse error: bad file"
        );
        assert_eq!(
            DiffError::SchemaMismatch("v5.1 vs v5.2".into()).to_string(),
            "schema mismatch: v5.1 vs v5.2"
        );
        assert_eq!(
            DiffError::EmptyGraph("file_a".into()).to_string(),
            "empty graph: file_a"
        );
        assert_eq!(
            DiffError::InternalError("oops".into()).to_string(),
            "internal error: oops"
        );
    }

    #[test]
    fn diff_error_is_std_error() {
        fn takes_error(_e: &dyn Error) {}
        takes_error(&DiffError::Unimplemented);
    }

    #[test]
    fn diff_error_clone() {
        let a = DiffError::ParseError("x".into());
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn run_diff_returns_unimplemented() {
        let result = run_diff("a.gramps", "b.gramps");
        assert!(matches!(result, Err(DiffError::Unimplemented)));
    }
}