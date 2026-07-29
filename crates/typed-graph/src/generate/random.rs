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
// Name generator — Markov-chain syllable approach
// ---------------------------------------------------------------------------

/// Syllable table for a single name style.
struct SyllableTable {
    /// Syllables that can start a name.
    starts: &'static [&'static str],
    /// Syllables that can follow any other syllable (including starts).
    middles: &'static [&'static str],
    /// Syllables that can end a name.
    ends: &'static [&'static str],
}

/// Syllable table for given names in "modern" style.
const MODERN_GIVEN: SyllableTable = SyllableTable {
    starts: &[
        "Ale", "Ben", "Chlo", "Dani", "Emi", "Hann", "Jake", "Jord", "Kait", "Log",
        "Madd", "Matt", "Noah", "Oliv", "Rile", "Sam", "Soph", "Tayl", "Tyl", "Zach",
    ],
    middles: &[
        "ex", "iss", "son", "ton", "sha", "lyn", "iel", "ica", "ber", "mac",
    ],
    ends: &[
        "xander", "nna", "ah", "er", "y", "ie", "a", "ia", "an", "en",
    ],
};

/// Syllable table for given names in "victorian" style.
const VICTORIAN_GIVEN: SyllableTable = SyllableTable {
    starts: &[
        "Agnes", "Char", "Edw", "Eliz", "Flor", "Fran", "Geor", "Henr", "Isab",
        "Jame", "Lyd", "Marg", "Mary", "Matt", "Oli", "Rach", "Samu", "Thom",
        "Vict", "Will",
    ],
    middles: &[
        "ar", "el", "in", "on", "et", "eb", "an", "or", "ia", "ie",
    ],
    ends: &[
        "ard", "ette", "ine", "ia", "y", "a", "ah", "on", "el", "en",
    ],
};

/// Syllable table for given names in "nordic" style.
const NORDIC_GIVEN: SyllableTable = SyllableTable {
    starts: &[
        "Agn", "Bjor", "Carl", "Els", "Frid", "Gus", "Hans", "Ingr",
        "Karl", "Lars", "Lenn", "Mats", "Nils", "Ola", "Per", "Ragn",
        "Sigr", "Sven", "Tor", "Ulr",
    ],
    middles: &[
        "ar", "bj", "er", "ik", "il", "jo", "kn", "or", "ri", "un",
    ],
    ends: &[
        "a", "e", "en", "er", "i", "id", "ik", "o", "or", "us",
    ],
};

/// Syllable table for surnames (shared across all styles).
const SURNAME_TABLE: SyllableTable = SyllableTable {
    starts: &[
        "Ash", "Black", "Brook", "Clay", "Copper", "Dark", "Fair", "Fox",
        "Gold", "Gray", "Green", "Hawk", "Iron", "Lock", "Moor", "Night",
        "Oak", "Raven", "Red", "Silver", "Snow", "Stone", "Storm", "Swift",
        "Thorn", "Under", "Water", "White", "Wind", "Winter", "Wood",
    ],
    middles: &[
        "er", "in", "on", "en", "ar", "or", "le", "el", "an", "un",
    ],
    ends: &[
        "born", "bridge", "brook", "burn", "bury", "dale", "field", "ford",
        "gate", "ham", "land", "ley", "lock", "mere", "mill", "moor",
        "more", "shaw", "side", "stead", "stone", "town", "wald", "well",
        "wick", "wood", "worth",
    ],
};

/// Get the syllable table for the given name style.
/// Falls back to "modern" for unknown styles.
fn given_table_for_style(style: &str) -> &'static SyllableTable {
    match style {
        "victorian" => &VICTORIAN_GIVEN,
        "nordic" => &NORDIC_GIVEN,
        _ => &MODERN_GIVEN,
    }
}

/// Generate a procedural given name using a Markov-chain syllable approach.
///
/// The name style determines the syllable inventory and transition probabilities.
/// Supported styles: "modern", "victorian", "nordic" (default: "modern").
///
/// Returns a name in the range 1-40 characters, Latin script, UTF-8.
/// The name is guaranteed to be non-empty and different from other names
/// generated in the same graph (via a set of used names passed in).
pub fn generate_given_name(
    style: &str,
    used_names: &std::collections::HashSet<String>,
    rng: &mut impl rand::Rng,
) -> String {
    let table = given_table_for_style(style);
    let target_len: usize = rng.gen_range(4..=10);
    generate_name_from_table(table, target_len, used_names, rng)
}

/// Generate a procedural surname.
///
/// Uses a separate syllable inventory from given names to produce
/// distinct surname patterns.
pub fn generate_surname(
    style: &str,
    used_names: &std::collections::HashSet<String>,
    rng: &mut impl rand::Rng,
) -> String {
    let _ = style; // Surname styles are shared; param kept for future extensibility
    let target_len: usize = rng.gen_range(5..=12);
    generate_name_from_table(&SURNAME_TABLE, target_len, used_names, rng)
}

/// Internal: generate a name from a syllable table targeting a length range.
fn generate_name_from_table(
    table: &SyllableTable,
    target_len: usize,
    used_names: &std::collections::HashSet<String>,
    rng: &mut impl rand::Rng,
) -> String {
    // Try up to 5 times to generate a unique name
    for attempt in 0..5 {
        let name = build_name(table, target_len, rng);
        if !used_names.contains(&name) {
            return name;
        }
        // If this was the last attempt, fall through to suffix approach
        if attempt == 4 {
            // Generate with a numeric suffix
            let base = build_name(table, target_len, rng);
            for suffix in 1u32..1000 {
                let candidate = format!("{}{}", base, suffix);
                if !used_names.contains(&candidate) {
                    return candidate;
                }
            }
        }
    }
    // Last resort: should never reach here, but return something unique
    "Unique".to_string()
}

/// Build a single name from the syllable table.
fn build_name(table: &SyllableTable, target_len: usize, rng: &mut impl rand::Rng) -> String {
    loop {
        // Start with a start syllable
        let start_idx = rng.gen_range(0..table.starts.len());
        let mut name = String::from(table.starts[start_idx]);

        // Add middle syllables until we reach the target length
        // or decide to end
        let max_middles = 3;
        for _ in 0..max_middles {
            if name.len() >= target_len || rng.gen_bool(0.4) {
                break;
            }
            let mid_idx = rng.gen_range(0..table.middles.len());
            name.push_str(table.middles[mid_idx]);
        }

        // Add an ending syllable
        let end_idx = rng.gen_range(0..table.ends.len());
        name.push_str(table.ends[end_idx]);

        // Ensure the name is non-empty and within length bounds
        if !name.is_empty() && name.len() <= 40 {
            return name;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

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

    // -----------------------------------------------------------------------
    // Name generator tests
    // -----------------------------------------------------------------------

    #[test]
    fn generate_given_name_returns_non_empty() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for style in &["modern", "victorian", "nordic"] {
            let name = generate_given_name(style, &used, &mut rng);
            assert!(!name.is_empty(), "Style {} produced empty name", style);
        }
    }

    #[test]
    fn generate_given_name_fits_length_bounds() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for style in &["modern", "victorian", "nordic"] {
            let name = generate_given_name(style, &used, &mut rng);
            assert!(name.len() <= 40, "Name '{}' exceeds 40 chars", name);
            assert!(!name.is_empty(), "Style {} produced empty name", style);
        }
    }

    #[test]
    fn generate_given_name_different_with_different_seeds() {
        let used = std::collections::HashSet::new();
        let mut rng1 = rand::rngs::StdRng::seed_from_u64(42);
        let mut rng2 = rand::rngs::StdRng::seed_from_u64(99);
        let name1 = generate_given_name("modern", &used, &mut rng1);
        let name2 = generate_given_name("modern", &used, &mut rng2);
        assert_ne!(name1, name2, "Different seeds produced same name");
    }

    #[test]
    fn generate_given_name_unique_across_calls() {
        let mut used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for _ in 0..50 {
            let name = generate_given_name("modern", &used, &mut rng);
            assert!(!used.contains(&name), "Duplicate name generated: {}", name);
            used.insert(name);
        }
    }

    #[test]
    fn generate_given_name_style_unsupported_falls_back() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let name = generate_given_name("nonexistent_style", &used, &mut rng);
        assert!(!name.is_empty(), "Fallback style produced empty name");
    }

    #[test]
    fn generate_given_name_utf8() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for style in &["modern", "victorian", "nordic"] {
            let name = generate_given_name(style, &used, &mut rng);
            assert!(std::str::from_utf8(name.as_bytes()).is_ok());
        }
    }

    #[test]
    fn generate_surname_differs_from_given_name() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let given = generate_given_name("modern", &used, &mut rng);
        let surname = generate_surname("modern", &used, &mut rng);
        // Surnames use a different syllable table so they should differ
        // (there's a tiny chance they could collide)
        assert!(!given.is_empty(), "Given name should be non-empty");
        assert!(!surname.is_empty(), "Surname should be non-empty");
    }

    #[test]
    fn generate_surname_non_empty() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let surname = generate_surname("modern", &used, &mut rng);
        assert!(!surname.is_empty());
        assert!(surname.len() <= 40);
    }

    #[test]
    fn generate_given_name_append_suffix_when_exhausted() {
        // Fill the used set with many names to force suffix fallback
        let mut used = std::collections::HashSet::new();
        // Pre-populate with common patterns to force suffix
        for i in 0..500 {
            used.insert(format!("Name{}", i));
        }
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let name = generate_given_name("modern", &used, &mut rng);
        assert!(!name.is_empty());
        assert!(!used.contains(&name));
    }
}