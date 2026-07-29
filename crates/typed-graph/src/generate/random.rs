//! Random generation engine for producing plausible family tree graphs.
//!
//! This module provides a property-based random generation engine that produces
//! full family tree graphs with procedural names, dates, and places, enforcing
//! genealogical plausibility constraints.
//!
//! # Architecture
//!
//! The generation pipeline follows a staged approach:
//!
//! 1. **Person generation** — Create N persons with procedural names, genders,
//!    and dates distributed across generation layers.
//! 2. **Parent selection** — Pair persons into families with plausible age
//!    differences and generation alignment.
//! 3. **Child assignment** — Assign children to families with genealogical
//!    age constraints (mother 16–45, father 16–70 at childbirth).
//! 4. **Event generation** — Create Birth, Death, and Marriage event nodes
//!    with consistent dates.
//! 5. **Optional features** — Places, sources/citations, notes, media, tags.
//!
//! All RNG operations take an explicit `&mut Rng` parameter, seeded from a
//! configurable seed for reproducibility. Same seed → same graph.
//!
//! The generated graph is NOT automatically validated — callers should run
//! `graph.validate(&schema)` before serialization, following the five-stage
//! pipeline (Generate → Validate → ...).

use std::ops::Range;

// ---------------------------------------------------------------------------
// RandomConfig
// ---------------------------------------------------------------------------

/// Configuration for random graph generation.
///
/// Controls the size, depth, date ranges, and optional features
/// of the generated family tree graph.
#[derive(Clone, Debug, PartialEq)]
pub struct RandomConfig {
    /// Number of Person nodes to generate (default: 200).
    pub person_count: usize,
    /// Number of Family nodes to generate (default: person_count / 2).
    pub family_count: usize,
    /// Number of generations (default: 3).
    /// Used to assign default birth years when no dates are specified.
    pub generations: usize,
    /// Children per family range (default: 1–4).
    pub children_per_family: Range<usize>,
    /// Start year for date ranges (default: 1850).
    pub start_year: i32,
    /// End year for date ranges (default: 2025).
    pub end_year: i32,
    /// Name style for procedural generation (default: "modern").
    pub name_style: String,
    /// Whether to generate Place nodes (default: false).
    pub with_places: bool,
    /// Whether to generate Source nodes and Citation edges (default: false).
    pub with_citations: bool,
    /// Whether to generate Note nodes (default: false).
    pub with_notes: bool,
    /// Whether to generate Media objects (default: false).
    pub with_media: bool,
    /// Whether to generate Tag nodes (default: false).
    pub with_tags: bool,
    /// Optional RNG seed for reproducible generation.
    /// If None, a random seed is generated from OS entropy.
    pub seed: Option<u64>,
    /// Place hierarchy depth (default: 3, used when with_places is true).
    pub place_depth: usize,
}

impl Default for RandomConfig {
    fn default() -> Self {
        RandomConfig {
            person_count: 200,
            family_count: 100,
            generations: 3,
            children_per_family: 1..4,
            start_year: 1850,
            end_year: 2025,
            name_style: "modern".to_string(),
            with_places: false,
            with_citations: false,
            with_notes: false,
            with_media: false,
            with_tags: false,
            seed: None,
            place_depth: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// GenerationError
// ---------------------------------------------------------------------------

/// Errors that can occur during random graph generation.
#[derive(Clone, Debug, PartialEq)]
pub enum GenerationError {
    /// Configuration is invalid (e.g., person_count == 0).
    InvalidConfig(String),
    /// Generation failed due to exhausted constraints (e.g., no eligible parents).
    /// Includes the seed for reproducibility.
    ConstraintExhausted { message: String, seed: u64 },
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerationError::InvalidConfig(msg) => {
                write!(f, "invalid generation config: {}", msg)
            }
            GenerationError::ConstraintExhausted { message, seed } => {
                write!(
                    f,
                    "generation constraint exhausted: {} (seed: {})",
                    message, seed
                )
            }
        }
    }
}

impl std::error::Error for GenerationError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // RandomConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn random_config_defaults() {
        let config = RandomConfig::default();
        assert_eq!(config.person_count, 200);
        assert_eq!(config.generations, 3);
        assert_eq!(config.start_year, 1850);
        assert_eq!(config.end_year, 2025);
        assert_eq!(config.name_style, "modern");
        assert!(!config.with_places);
        assert!(!config.with_citations);
        assert!(!config.with_notes);
        assert!(!config.with_media);
        assert!(!config.with_tags);
        assert!(config.seed.is_none());
        assert_eq!(config.place_depth, 3);
        assert_eq!(config.children_per_family, 1..4);
        assert_eq!(config.family_count, 100);
    }

    #[test]
    fn random_config_custom() {
        let config = RandomConfig {
            person_count: 50,
            family_count: 25,
            generations: 5,
            children_per_family: 2..3,
            start_year: 1900,
            end_year: 2000,
            name_style: "victorian".to_string(),
            with_places: true,
            with_citations: true,
            with_notes: true,
            with_media: true,
            with_tags: true,
            seed: Some(42),
            place_depth: 4,
        };
        assert_eq!(config.person_count, 50);
        assert_eq!(config.family_count, 25);
        assert_eq!(config.generations, 5);
        assert_eq!(config.children_per_family, 2..3);
        assert_eq!(config.start_year, 1900);
        assert_eq!(config.end_year, 2000);
        assert_eq!(config.name_style, "victorian");
        assert!(config.with_places);
        assert!(config.with_citations);
        assert!(config.with_notes);
        assert!(config.with_media);
        assert!(config.with_tags);
        assert_eq!(config.seed, Some(42));
        assert_eq!(config.place_depth, 4);
    }

    // -----------------------------------------------------------------------
    // GenerationError tests
    // -----------------------------------------------------------------------

    #[test]
    fn generation_error_display_invalid_config() {
        let err = GenerationError::InvalidConfig("person_count must be > 0".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("invalid generation config"));
        assert!(msg.contains("person_count must be > 0"));
    }

    #[test]
    fn generation_error_display_constraint_exhausted() {
        let err = GenerationError::ConstraintExhausted {
            message: "no eligible parents found".to_string(),
            seed: 42,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("generation constraint exhausted"));
        assert!(msg.contains("no eligible parents found"));
        assert!(msg.contains("seed: 42"));
    }

    #[test]
    fn generation_error_contains_seed() {
        let err = GenerationError::ConstraintExhausted {
            message: "test".to_string(),
            seed: 12345,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("12345"));
    }

    #[test]
    fn generation_error_is_error() {
        use std::error::Error;
        let err = GenerationError::InvalidConfig("test".to_string());
        assert!(err.source().is_none());
    }
}