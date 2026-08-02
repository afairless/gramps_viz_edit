//! gramps-reader — shared `.gramps` XML parsing for gramps-gen.
//!
//! Extracts streaming statistics and person/family detail records from
//! Gramps XML documents. Shared by the `cli` and `visualize` crates so
//! that library code never depends on a binary crate.
//!
//! Modules:
//!
//! - [`xml`] — low-level streaming helpers (`strip_prefix`,
//!   `read_handle_attr`, `read_hlink_attr`) and the streaming extractors
//!   (`count`, `extract`).
//! - [`graph`] — disjoint-set union, connected components, and
//!   generation layering over the person graph.
//! - [`types`] — shared data records (`FamilyRecord`, `ParsedPerson`,
//!   `ParsedFamily`).
//! - [`error`] — the crate-wide [`Error`] type.

#![deny(deprecated)]

pub mod error;
pub mod graph;
pub mod types;
pub mod xml;

pub use error::Error;
pub use graph::{compute_generation_table, FamilyGroupGenerationTable, MAX_GENERATION};
pub use types::{FamilyRecord, ParsedFamily, ParsedPerson};
pub use xml::count::{count_gramps_xml, PrimaryTypeCounts, StatsReport};
pub use xml::extract::{extract_families, extract_persons};
pub use xml::{read_hlink_attr, read_handle_attr, strip_prefix};
