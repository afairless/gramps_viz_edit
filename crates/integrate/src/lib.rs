//! Integrate crate — merge diff CSV results with visualizer selections.
//!
//! This crate provides tools to combine a Gramps diff report (CSV format)
//! with a visualizer selection export (JSON format), matching people by
//! handle and producing a unified output table.

pub mod csv_reader;
pub mod json_reader;
pub mod merge;
pub mod output;

use std::fmt;

/// Errors that can occur during integration.
#[derive(Debug, Clone, PartialEq)]
pub enum IntegrateError {
    /// The diff CSV file could not be read or parsed.
    DiffReadError(String),
    /// The selections JSON file could not be read or parsed.
    SelectionsReadError(String),
    /// Output write error.
    OutputError(String),
}

impl fmt::Display for IntegrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntegrateError::DiffReadError(msg) => write!(f, "diff CSV read error: {msg}"),
            IntegrateError::SelectionsReadError(msg) => write!(f, "selections JSON read error: {msg}"),
            IntegrateError::OutputError(msg) => write!(f, "output error: {msg}"),
        }
    }
}

impl std::error::Error for IntegrateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrate_error_debug() {
        let err = IntegrateError::DiffReadError("oops".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("DiffReadError"));
    }

    #[test]
    fn integrate_error_display() {
        let err = IntegrateError::DiffReadError("bad file".to_string());
        assert_eq!(err.to_string(), "diff CSV read error: bad file");

        let err = IntegrateError::SelectionsReadError("bad json".to_string());
        assert_eq!(err.to_string(), "selections JSON read error: bad json");

        let err = IntegrateError::OutputError("write failed".to_string());
        assert_eq!(err.to_string(), "output error: write failed");
    }

    #[test]
    fn integrate_error_is_std_error() {
        fn takes_error(_e: &dyn std::error::Error) {}
        takes_error(&IntegrateError::DiffReadError("x".to_string()));
    }

    #[test]
    fn integrate_error_clone() {
        let a = IntegrateError::DiffReadError("x".to_string());
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn integrate_error_partialeq() {
        assert_eq!(
            IntegrateError::DiffReadError("x".to_string()),
            IntegrateError::DiffReadError("x".to_string()),
        );
        assert_ne!(
            IntegrateError::DiffReadError("x".to_string()),
            IntegrateError::DiffReadError("y".to_string()),
        );
    }
}