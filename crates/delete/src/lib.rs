//! Deletion cascade engine for gramps-gen.
//!
//! This crate computes which nodes become orphaned when a set of seed nodes
//! (typically people selected for deletion) is removed from a
//! [`typed_graph::Graph`]. It operates read-only on the graph and returns a
//! [`DeletePlan`] with all handles to delete.

pub mod cascade;
pub mod manifest;
pub mod review;
pub mod types;

pub use cascade::cascade;
pub use manifest::{
    build_manifest, check_source_file, load_manifest, load_manifest_v1, save_manifest,
    validate_manifest, ManifestError,
};
pub use review::{run_interactive_review, ReviewAction, ReviewResult};
pub use types::{
    DeleteCandidate, DeleteManifest, DeletePlan, NodeKindLabel, ReviewState, TypePlan,
};
