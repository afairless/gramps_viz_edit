//! gramps-reader — shared Gramps XML parsing for gramps-gen.
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
pub mod io;
pub mod types;
pub mod xml;

pub use error::Error;
pub use graph::{
    compute_generation_table, compute_generations, Dsu, FamilyGroupGenerationTable, MAX_GENERATION,
};
pub use io::read_gramps_file;
pub use types::{FamilyRecord, ParsedEvent, ParsedFamily, ParsedPerson};
pub use xml::count::{count_gramps_xml, PrimaryTypeCounts, StatsReport};
pub use xml::extract::{extract_events, extract_families, extract_persons, resolve_event_refs};
pub use xml::graph::parse_gramps_xml;
pub use xml::{read_handle_attr, read_hlink_attr, strip_prefix};
