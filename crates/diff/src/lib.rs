//! Gramps diff analyzer — compare two Gramps family trees.
//!
//! This crate provides utilities for comparing two Gramps XML files and
//! producing a structured diff report.

pub mod cascading;
pub mod compare;
pub mod matcher;
pub mod normalize;
pub mod output;
pub mod report;
pub mod similarity;
pub mod visualizer_index;

#[cfg(feature = "resolve")]
pub mod resolve;

pub use cascading::resolve_extrinsic;
pub use matcher::match_graphs;
pub use report::*;

#[cfg(feature = "resolve")]
pub use resolve::{run_interactive_resolution, ResolvedMatches};

use std::error::Error;
use std::fmt;

use gramps_reader::io::read_gramps_file;
use gramps_reader::xml::parse::parse_graph;

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
}

impl fmt::Display for DiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffError::ParseError(msg) => write!(f, "parse error: {msg}"),
            DiffError::SchemaMismatch(msg) => write!(f, "schema mismatch: {msg}"),
            DiffError::EmptyGraph(msg) => write!(f, "empty graph: {msg}"),
            DiffError::InternalError(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl Error for DiffError {}

/// Configuration for the diff pipeline.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffConfig {
    /// Minimum similarity threshold (0.0–1.0) for text field matches.
    /// Items with all field-level similarities at or above this threshold
    /// are considered to have acceptable differences.
    pub threshold: f64,
    /// Whether to include extrinsic-only items in the report output.
    pub include_extrinsic: bool,
    /// Whether to apply text normalization before comparison.
    pub normalize_enabled: bool,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            threshold: 0.8,
            include_extrinsic: true,
            normalize_enabled: true,
        }
    }
}

/// Run the full diff pipeline between two Gramps XML files.
///
/// Orchestrates: parse both files → match (Pass 1) → cascade (Pass 2) →
/// build and return report.
///
/// # Errors
///
/// Returns [`DiffError::ParseError`] if either file cannot be parsed.
/// Returns [`DiffError::EmptyGraph`] if both files are empty.
/// Returns [`DiffError::SchemaMismatch`] if the two files use incompatible
/// schema versions (only returned when a schema version is not compiled in).
pub fn run_diff(file_a: &str, file_b: &str, _config: &DiffConfig) -> Result<DiffReport, DiffError> {
    // Read both files
    let content_a = read_gramps_file(file_a).map_err(|e| DiffError::ParseError(e.to_string()))?;
    let content_b = read_gramps_file(file_b).map_err(|e| DiffError::ParseError(e.to_string()))?;

    // Parse both graphs
    let graph_a = parse_graph(&content_a).map_err(|e| DiffError::ParseError(e.to_string()))?;
    let graph_b = parse_graph(&content_b).map_err(|e| DiffError::ParseError(e.to_string()))?;

    // Check for empty graphs
    if graph_a.node_count() == 0 && graph_b.node_count() == 0 {
        return Err(DiffError::EmptyGraph("both files are empty".to_string()));
    }

    // Pass 1: Match
    let match_result = match_graphs(&graph_a, &graph_b);

    // Pass 2: Cascade (extrinsic resolution)
    let item_diffs = resolve_extrinsic(match_result.item_diffs, &match_result.handle_map);

    // Build summary
    let mut summary = DiffSummary {
        total_a: graph_a.node_count(),
        total_b: graph_b.node_count(),
        ..Default::default()
    };

    for item in &item_diffs {
        match item.classification {
            Classification::Same => summary.same += 1,
            Classification::Modified => summary.modified += 1,
            Classification::Added => summary.added += 1,
            Classification::Removed => summary.removed += 1,
            Classification::NeedsReview => summary.needs_review += 1,
            Classification::ExtrinsicOnly => summary.extrinsic_only += 1,
        }
    }

    Ok(DiffReport {
        summary,
        items: item_diffs,
        ambiguous_cases: match_result.ambiguous_cases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_error_debug() {
        let err = DiffError::InternalError("oops".to_string());
        assert!(format!("{err:?}").contains("InternalError"));
    }

    #[test]
    fn diff_error_display() {
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
        takes_error(&DiffError::InternalError("x".to_string()));
    }

    #[test]
    fn diff_error_clone() {
        let a = DiffError::ParseError("x".into());
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn diff_config_default() {
        let config = DiffConfig::default();
        assert!((config.threshold - 0.8).abs() < f64::EPSILON);
        assert!(config.include_extrinsic);
        assert!(config.normalize_enabled);
    }

    #[test]
    fn diff_config_debug() {
        let config = DiffConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("threshold"));
        assert!(debug.contains("include_extrinsic"));
    }
}
