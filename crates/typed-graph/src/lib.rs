//! typed-graph — In-memory typed directed multigraph for Gramps genealogy data.
//!
//! This crate provides the core graph model, schema-driven codegen types,
//! validation, and generation capabilities for Gramps family tree datasets.

pub mod schema;

/// Placeholder: re-export the schema module.
/// The schema module is populated by build.rs codegen at compile time.
pub use schema::*;
