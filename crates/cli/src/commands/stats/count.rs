//! Re-exports of the shared streaming stats logic.
//!
//! The implementation lives in the `gramps-reader` crate; this module keeps
//! the historical `cli::commands::stats::count` import path working for
//! callers that reference it directly (e.g. integration tests).

pub use gramps_reader::{
    compute_generation_table, count_gramps_xml, FamilyGroupGenerationTable, FamilyRecord,
    PrimaryTypeCounts, StatsReport,
};
