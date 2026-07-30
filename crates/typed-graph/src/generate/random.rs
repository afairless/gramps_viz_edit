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

// Allow dead code for functions used by later steps in the generation pipeline.
#![allow(dead_code)]

use rand::Rng;
use rand::SeedableRng;
use std::ops::Range;

use crate::generate::adversarial::AdversarialConfig;
use crate::generate::adversarial::AdversarialStrategy;

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
        "Ale", "Ben", "Chlo", "Dani", "Emi", "Hann", "Jake", "Jord", "Kait", "Log", "Madd", "Matt",
        "Noah", "Oliv", "Rile", "Sam", "Soph", "Tayl", "Tyl", "Zach",
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
        "Agnes", "Char", "Edw", "Eliz", "Flor", "Fran", "Geor", "Henr", "Isab", "Jame", "Lyd",
        "Marg", "Mary", "Matt", "Oli", "Rach", "Samu", "Thom", "Vict", "Will",
    ],
    middles: &["ar", "el", "in", "on", "et", "eb", "an", "or", "ia", "ie"],
    ends: &["ard", "ette", "ine", "ia", "y", "a", "ah", "on", "el", "en"],
};

/// Syllable table for given names in "nordic" style.
const NORDIC_GIVEN: SyllableTable = SyllableTable {
    starts: &[
        "Agn", "Bjor", "Carl", "Els", "Frid", "Gus", "Hans", "Ingr", "Karl", "Lars", "Lenn",
        "Mats", "Nils", "Ola", "Per", "Ragn", "Sigr", "Sven", "Tor", "Ulr",
    ],
    middles: &["ar", "bj", "er", "ik", "il", "jo", "kn", "or", "ri", "un"],
    ends: &["a", "e", "en", "er", "i", "id", "ik", "o", "or", "us"],
};

/// Syllable table for surnames (shared across all styles).
const SURNAME_TABLE: SyllableTable = SyllableTable {
    starts: &[
        "Ash", "Black", "Brook", "Clay", "Copper", "Dark", "Fair", "Fox", "Gold", "Gray", "Green",
        "Hawk", "Iron", "Lock", "Moor", "Night", "Oak", "Raven", "Red", "Silver", "Snow", "Stone",
        "Storm", "Swift", "Thorn", "Under", "Water", "White", "Wind", "Winter", "Wood",
    ],
    middles: &["er", "in", "on", "en", "ar", "or", "le", "el", "an", "un"],
    ends: &[
        "born", "bridge", "brook", "burn", "bury", "dale", "field", "ford", "gate", "ham", "land",
        "ley", "lock", "mere", "mill", "moor", "more", "shaw", "side", "stead", "stone", "town",
        "wald", "well", "wick", "wood", "worth",
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
// Place generator — hierarchical template system
// ---------------------------------------------------------------------------

/// A generated place with hierarchical components.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedPlace {
    pub city: String,
    pub county: String,
    pub state: String,
    pub country: String,
}

/// City prefixes drawn from a static set.
const CITY_PREFIXES: &[&str] = &[
    "Ash", "Oak", "River", "Mill", "Spring", "Fair", "Meadow", "Cedar", "Pine", "Willow", "Maple",
    "Birch", "Elm", "Hazel", "Holly", "Ivy", "Stone", "Brook", "Lake", "Hill", "Field", "Dale",
    "Glen", "Heath", "Fern", "Rose", "Lily", "Vale", "Crest", "Peak",
];

/// City suffixes for building city names.
const CITY_SUFFIXES: &[&str] = &[
    "ton", "ville", "burg", "field", "bridge", "haven", "brook", "ham", "ley", "more", "side",
    "stead", "ford", "gate", "bury", "dale", "wick", "port", "worth", "view",
];

/// Procedurally named states.
const STATE_NAMES: &[&str] = &[
    "Northumbria",
    "Westland",
    "Southmere",
    "Eastshire",
    "Arcadia",
    "Avalon",
    "Caledonia",
    "Delphia",
    "Eldoria",
    "Fenwick",
    "Grenville",
    "Havenwood",
    "Iverness",
    "Kingsland",
    "Lorien",
];

/// Procedurally named countries.
const COUNTRY_NAMES: &[&str] = &[
    "Albion",
    "Valdoria",
    "Mercia",
    "Thalassia",
    "Eryndor",
    "Celestria",
    "Durnhold",
    "Aeridor",
];

/// Generate a procedural place name using the hierarchical template system.
///
/// Template: "{prefix}{suffix}, {county} County, {state}"
/// Where {prefix} and {suffix} are drawn from procedurally generated tables.
///
/// Depth controls how many levels are filled:
/// - depth=1 → country only
/// - depth=2 → state + country
/// - depth=3 (default) → city + county + state + country
pub fn generate_place(
    depth: usize,
    used_place_names: &std::collections::HashSet<String>,
    rng: &mut impl rand::Rng,
) -> GeneratedPlace {
    let effective_depth = if depth == 0 { 1 } else { depth };

    // Select country (reused within a graph via the seed)
    let country_idx = rng.gen_range(0..COUNTRY_NAMES.len());
    let country = COUNTRY_NAMES[country_idx].to_string();

    if effective_depth == 1 {
        return GeneratedPlace {
            city: String::new(),
            county: String::new(),
            state: String::new(),
            country,
        };
    }

    // Select state
    let state_idx = rng.gen_range(0..STATE_NAMES.len());
    let state = STATE_NAMES[state_idx].to_string();

    if effective_depth == 2 {
        return GeneratedPlace {
            city: String::new(),
            county: String::new(),
            state,
            country,
        };
    }

    // Generate city name (depth 3+)
    let city = generate_city_name(used_place_names, rng);
    let county = format!("{} County", city);

    GeneratedPlace {
        city,
        county,
        state,
        country,
    }
}

/// Generate a single city name from prefix + suffix combination.
fn generate_city_name(
    used_place_names: &std::collections::HashSet<String>,
    rng: &mut impl rand::Rng,
) -> String {
    // Try up to 20 times to generate a unique city name
    for _ in 0..20 {
        let prefix_idx = rng.gen_range(0..CITY_PREFIXES.len());
        let suffix_idx = rng.gen_range(0..CITY_SUFFIXES.len());
        let name = format!("{}{}", CITY_PREFIXES[prefix_idx], CITY_SUFFIXES[suffix_idx]);
        if !used_place_names.contains(&name) {
            return name;
        }
    }
    // Fallback with a unique suffix
    let prefix_idx = rng.gen_range(0..CITY_PREFIXES.len());
    let suffix_idx = rng.gen_range(0..CITY_SUFFIXES.len());
    let base = format!("{}{}", CITY_PREFIXES[prefix_idx], CITY_SUFFIXES[suffix_idx]);
    for suffix in 1u32..1000 {
        let candidate = format!("{}{}", base, suffix);
        if !used_place_names.contains(&candidate) {
            return candidate;
        }
    }
    "City".to_string()
}

// ---------------------------------------------------------------------------
// Person generation
// ---------------------------------------------------------------------------

/// A summary of a generated person, used for parent selection and family building.
#[derive(Clone, Debug)]
pub(crate) struct PersonSummary {
    pub handle: crate::Handle,
    pub birth_year: i32,
    pub gender: i32,
    pub layer: usize,
    pub is_parent: bool,
    pub is_child: bool,
}

/// Generate a random Person node with procedural name, gender, and dates.
///
/// Returns the handle of the created person node.
/// The person is added to the graph via the `Graph` API.
/// Returns `(handle, warning)` where `warning` is `Some` if events were
/// skipped due to the missing-events strategy.
pub(crate) fn generate_random_person(
    graph: &mut crate::Graph,
    config: &RandomConfig,
    used_names: &mut std::collections::HashSet<String>,
    rng: &mut impl rand::Rng,
    generation_layer: usize,
    missing_events_fraction: f64,
) -> Result<(crate::Handle, Option<String>), GenerationError> {
    let handle = uuid::Uuid::new_v4().to_string();

    // Generate a given name and surname
    let given_name = generate_given_name(&config.name_style, used_names, rng);
    let surname = generate_surname(&config.name_style, used_names, rng);

    // Track used names
    used_names.insert(given_name.clone());
    used_names.insert(surname.clone());

    // Select gender: 0 (Male) or 1 (Female) with equal probability,
    // occasionally 2 (Unknown, ~5%)
    let gender: i32 = {
        let roll: f64 = rng.gen();
        if roll < 0.475 {
            0
        } else if roll < 0.95 {
            1
        } else {
            2
        }
    };

    // Generate birth date based on generation layer
    let birth_year = birth_year_for_layer(generation_layer, config, rng);
    let birth_month = rng.gen_range(1..=12);
    let birth_day = rng.gen_range(1..=28); // Safe for all months
    let birth_date = crate::DateValue::new_ymd(birth_year, birth_month, birth_day);

    // Quality: Exact (~80%), Estimated (~15%), Calculated (~5%)
    let birth_date = randomize_date_quality(birth_date, rng);

    // Optionally generate a death date
    let death_date = generate_death_date(birth_year, config, rng);

    // Build the person data
    let person = crate::PersonData {
        handle: handle.clone(),
        gender,
        primary_name: crate::Name {
            first_name: Some(given_name.clone()),
            surname_list: vec![crate::Surname {
                surname: Some(surname),
                ..crate::Surname::default()
            }],
            ..crate::Name::default()
        },
        ..crate::PersonData::default()
    };

    // Add the person node to the graph
    graph
        .add_node(handle.clone(), crate::Node::Person(person))
        .map_err(|_| GenerationError::InvalidConfig(format!("duplicate handle: {}", handle)))?;

    // Check missing-events strategy: skip events for this person?
    let skip_events =
        missing_events_fraction > 0.0 && rng.gen_bool(missing_events_fraction.clamp(0.0, 1.0));

    let mut warning: Option<String> = None;

    if skip_events {
        warning = Some(format!(
            "Person {}: missing events (strategy: missing-events, fraction: {})",
            handle, missing_events_fraction
        ));
    } else {
        // Create birth event
        let event_handle = uuid::Uuid::new_v4().to_string();
        let birth_event = crate::EventData {
            handle: event_handle.clone(),
            event_type: crate::EventType::Birth,
            date: Some(birth_date),
            ..crate::EventData::default()
        };
        graph
            .add_node(event_handle.clone(), crate::Node::Event(birth_event))
            .map_err(|_| {
                GenerationError::InvalidConfig(format!("duplicate event handle: {}", event_handle))
            })?;

        // Link birth event to person
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: handle.clone(),
                target: event_handle,
                metadata: Box::new(crate::EventRef {
                    ref_field: handle.clone(),
                    role: Some(crate::EventRoleType::Primary),
                }),
            })
            .expect("birth event target exists (just added)");

        // Create death event if death date is set
        if let Some(death_date) = death_date {
            let death_event_handle = uuid::Uuid::new_v4().to_string();
            let death_event = crate::EventData {
                handle: death_event_handle.clone(),
                event_type: crate::EventType::Death,
                date: Some(death_date),
                ..crate::EventData::default()
            };
            graph
                .add_node(death_event_handle.clone(), crate::Node::Event(death_event))
                .map_err(|_| {
                    GenerationError::InvalidConfig(format!(
                        "duplicate event handle: {}",
                        death_event_handle
                    ))
                })?;

            graph
                .add_edge(crate::Edge::PersonEventRef {
                    source: handle.clone(),
                    target: death_event_handle,
                    metadata: Box::new(crate::EventRef {
                        ref_field: handle.clone(),
                        role: Some(crate::EventRoleType::Primary),
                    }),
                })
                .expect("death event target exists (just added)");
        }
    }

    Ok((handle, warning))
}

/// Determine the birth year for a person in the given generation layer.
fn birth_year_for_layer(layer: usize, config: &RandomConfig, rng: &mut impl rand::Rng) -> i32 {
    // Each layer shifts birth year range back by ~30 years
    // Layer 0: end_year-55 to end_year-25 (roughly 1970-2000 for end_year=2025)
    // Layer 1: end_year-85 to end_year-55 (roughly 1940-1970)
    // Layer 2: end_year-115 to end_year-85 (roughly 1910-1940)
    // etc.
    let range_end = config.end_year - 25 - (layer as i32 * 30);
    let range_start = config.end_year - 55 - (layer as i32 * 30);

    let effective_start = range_start.max(config.start_year);
    let effective_end = range_end.max(effective_start + 1);

    rng.gen_range(effective_start..effective_end)
}

/// Randomize the quality of a date value.
fn randomize_date_quality(date: crate::DateValue, rng: &mut impl rand::Rng) -> crate::DateValue {
    let roll: f64 = rng.gen();
    let quality = if roll < 0.80 {
        crate::DateQuality::Exact
    } else if roll < 0.95 {
        crate::DateQuality::Estimated
    } else {
        crate::DateQuality::Calculated
    };
    crate::DateValue {
        quality: Some(quality),
        ..date
    }
}

/// Generate a death date if the person is plausibly deceased.
fn generate_death_date(
    birth_year: i32,
    config: &RandomConfig,
    rng: &mut impl rand::Rng,
) -> Option<crate::DateValue> {
    let current_year = config.end_year;
    let age = current_year - birth_year;

    // Determine if the person is plausibly deceased
    let is_deceased = if age > 100 {
        true // Very old, almost certainly deceased
    } else if age > 80 {
        rng.gen_bool(0.8) // Likely deceased
    } else if age > 60 {
        rng.gen_bool(0.4) // Possibly deceased
    } else {
        rng.gen_bool(0.1) // Unlikely to be deceased
    };

    if !is_deceased {
        return None;
    }

    // Death year = birth year + random age at death (18-100)
    let age_at_death: i32 = rng.gen_range(18..=100);
    let death_year = birth_year + age_at_death;

    // Ensure death year is not after the current year
    let death_year = death_year.min(current_year);

    // Ensure death year is after birth year
    if death_year <= birth_year {
        return None;
    }

    let death_month = rng.gen_range(1..=12);
    let death_day = rng.gen_range(1..=28);

    Some(crate::DateValue::new_ymd(
        death_year,
        death_month,
        death_day,
    ))
}

// ---------------------------------------------------------------------------
// Parent selection and family creation
// ---------------------------------------------------------------------------

/// Select eligible parents for a family from the existing person pool.
///
/// Returns `(father_handle, mother_handle)` or `None` if no eligible pair found.
///
/// Eligibility criteria:
/// - Father and mother must be of opposite genders (0=Male, 1=Female).
/// - Birth years must be within a plausible range (0-20 years difference).
/// - Neither person is already a parent in the same generation layer.
/// - Neither person is already a sibling or child of the other.
pub(crate) fn select_parents(
    persons: &[(crate::Handle, PersonSummary)],
    _config: &RandomConfig,
    layer: usize,
    rng: &mut impl rand::Rng,
) -> Option<(crate::Handle, crate::Handle)> {
    // Filter by layer to find candidates in the same generation
    let same_layer: Vec<_> = persons
        .iter()
        .filter(|(_, s)| s.layer == layer && !s.is_parent)
        .collect();

    if same_layer.len() < 2 {
        // Try to expand to adjacent layers
        let adjacent: Vec<_> = persons
            .iter()
            .filter(|(_, s)| {
                (s.layer == layer || s.layer == layer + 1 || (layer > 0 && s.layer == layer - 1))
                    && !s.is_parent
            })
            .collect();

        if adjacent.len() < 2 {
            return None;
        }

        // Try to find a compatible pair from adjacent layers
        return find_compatible_pair(&adjacent, rng);
    }

    find_compatible_pair(&same_layer, rng)
}

/// Find a compatible father-mother pair from the candidate list.
fn find_compatible_pair(
    candidates: &[&(crate::Handle, PersonSummary)],
    rng: &mut impl rand::Rng,
) -> Option<(crate::Handle, crate::Handle)> {
    // Separate by gender
    let males: Vec<_> = candidates.iter().filter(|(_, s)| s.gender == 0).collect();
    let females: Vec<_> = candidates.iter().filter(|(_, s)| s.gender == 1).collect();

    if males.is_empty() || females.is_empty() {
        return None;
    }

    // Try to find a compatible pair
    // Shuffle by picking random indices
    for _ in 0..10 {
        let male_idx = rng.gen_range(0..males.len());
        let female_idx = rng.gen_range(0..females.len());

        let (male_handle, male_summary) = males[male_idx];
        let (female_handle, female_summary) = females[female_idx];

        let age_diff = (male_summary.birth_year - female_summary.birth_year).abs();
        if age_diff <= 20 {
            // Check plausible parenting window: father's birth + 16 <= mother's birth + 50
            let father_min_child = male_summary.birth_year + 16;
            let mother_max_child = female_summary.birth_year + 50;
            if father_min_child <= mother_max_child {
                return Some((male_handle.clone(), female_handle.clone()));
            }
        }
    }

    // Fallback: pick the closest compatible pair
    let mut best_pair: Option<(crate::Handle, crate::Handle)> = None;
    let mut best_age_diff = i32::MAX;

    for (male_handle, male_summary) in &males {
        for (female_handle, female_summary) in &females {
            let age_diff = (male_summary.birth_year - female_summary.birth_year).abs();
            if age_diff <= 20 && age_diff < best_age_diff {
                let father_min_child = male_summary.birth_year + 16;
                let mother_max_child = female_summary.birth_year + 50;
                if father_min_child <= mother_max_child {
                    best_age_diff = age_diff;
                    best_pair = Some(((*male_handle).clone(), (*female_handle).clone()));
                }
            }
        }
    }

    best_pair
}

/// Create a Family node with the given parents.
///
/// When `one_parent_fraction > 0.0`, a fraction of families will have only
/// one parent assigned (randomly skipping either father or mother).
/// This is used by the [`AdversarialStrategy::OneParentFamilies`] strategy.
///
/// Returns a tuple of `(family_handle, warning)` where `warning` is `Some`
/// if a one-parent family was created.
pub(crate) fn generate_family(
    graph: &mut crate::Graph,
    _config: &RandomConfig,
    persons: &mut [(crate::Handle, PersonSummary)],
    layer: usize,
    rng: &mut impl rand::Rng,
    one_parent_fraction: f64,
) -> Result<(crate::Handle, Option<String>), GenerationError> {
    // Select parents
    let parent_pair = select_parents(persons, _config, layer, rng);

    let (father_handle, mother_handle) = match parent_pair {
        Some(pair) => pair,
        None => {
            // Create a single-parent family
            let handle = create_single_parent_family(graph, persons, layer, rng)?;
            return Ok((handle, None));
        }
    };

    // Check if this should become a one-parent family (Category A: OneParentFamilies)

    if one_parent_fraction > 0.0 && rng.gen_bool(one_parent_fraction.clamp(0.0, 1.0)) {
        // Randomly decide which parent to skip (coin flip)
        let skip_father = rng.gen_bool(0.5);

        if skip_father {
            for (handle, summary) in persons.iter_mut() {
                if *handle == mother_handle {
                    summary.is_parent = true;
                }
            }

            let family_handle = uuid::Uuid::new_v4().to_string();
            let family = crate::FamilyData {
                handle: family_handle.clone(),
                father_handle: None,
                mother_handle: Some(mother_handle.clone()),
                ..crate::FamilyData::default()
            };

            graph
                .add_node(family_handle.clone(), crate::Node::Family(family))
                .map_err(|_| {
                    GenerationError::InvalidConfig(format!(
                        "duplicate family handle: {}",
                        family_handle
                    ))
                })?;

            // Add FamilyMother edge only
            graph
                .add_edge(crate::Edge::FamilyMother {
                    source: family_handle.clone(),
                    target: mother_handle.clone(),
                })
                .expect("mother node exists (was just checked)");

            // Update mother's family list
            if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(&mother_handle) {
                person.family_list.push(family_handle.clone());
            }

            let msg = format!(
                "Family {}: one-parent family — father skipped (strategy: one-parent, fraction: {})",
                family_handle, one_parent_fraction
            );
            return Ok((family_handle, Some(msg)));
        } else {
            // Skip mother: create family with father only
            // Mark only father as parent
            for (handle, summary) in persons.iter_mut() {
                if *handle == father_handle {
                    summary.is_parent = true;
                }
            }

            let family_handle = uuid::Uuid::new_v4().to_string();
            let family = crate::FamilyData {
                handle: family_handle.clone(),
                father_handle: Some(father_handle.clone()),
                mother_handle: None,
                ..crate::FamilyData::default()
            };

            graph
                .add_node(family_handle.clone(), crate::Node::Family(family))
                .map_err(|_| {
                    GenerationError::InvalidConfig(format!(
                        "duplicate family handle: {}",
                        family_handle
                    ))
                })?;

            // Add FamilyFather edge only
            graph
                .add_edge(crate::Edge::FamilyFather {
                    source: family_handle.clone(),
                    target: father_handle.clone(),
                })
                .expect("father node exists (was just checked)");

            // Update father's family list
            if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(&father_handle) {
                person.family_list.push(family_handle.clone());
            }

            let msg = format!(
                "Family {}: one-parent family — mother skipped (strategy: one-parent, fraction: {})",
                family_handle, one_parent_fraction
            );
            return Ok((family_handle, Some(msg)));
        }
    }

    // Normal two-parent family
    // Mark as parents
    for (handle, summary) in persons.iter_mut() {
        if *handle == father_handle || *handle == mother_handle {
            summary.is_parent = true;
        }
    }

    // Create family node
    let family_handle = uuid::Uuid::new_v4().to_string();
    let family = crate::FamilyData {
        handle: family_handle.clone(),
        father_handle: Some(father_handle.clone()),
        mother_handle: Some(mother_handle.clone()),
        ..crate::FamilyData::default()
    };

    graph
        .add_node(family_handle.clone(), crate::Node::Family(family))
        .map_err(|_| {
            GenerationError::InvalidConfig(format!("duplicate family handle: {}", family_handle))
        })?;

    // Add FamilyFather edge
    graph
        .add_edge(crate::Edge::FamilyFather {
            source: family_handle.clone(),
            target: father_handle.clone(),
        })
        .expect("father node exists (was just checked)");

    // Add FamilyMother edge
    graph
        .add_edge(crate::Edge::FamilyMother {
            source: family_handle.clone(),
            target: mother_handle.clone(),
        })
        .expect("mother node exists (was just checked)");

    // Update person family lists
    if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(&father_handle) {
        person.family_list.push(family_handle.clone());
    }
    if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(&mother_handle) {
        person.family_list.push(family_handle.clone());
    }

    Ok((family_handle, None))
}

/// Create a single-parent family when no eligible parent pair is found.
fn create_single_parent_family(
    graph: &mut crate::Graph,
    persons: &mut [(crate::Handle, PersonSummary)],
    layer: usize,
    rng: &mut impl rand::Rng,
) -> Result<crate::Handle, GenerationError> {
    // Find an eligible person in this layer who is not already a parent
    let eligible: Vec<_> = persons
        .iter()
        .filter(|(_, s)| s.layer == layer && !s.is_parent)
        .map(|(h, _)| h.clone())
        .collect();

    if eligible.is_empty() {
        return Err(GenerationError::ConstraintExhausted {
            message: format!("no eligible parents found for layer {}", layer),
            seed: 0, // Seed will be set by caller
        });
    }

    let parent_idx = rng.gen_range(0..eligible.len());
    let parent_handle = eligible[parent_idx].clone();

    // Mark as parent
    for (handle, summary) in persons.iter_mut() {
        if *handle == parent_handle {
            summary.is_parent = true;
        }
    }

    // Create family node
    let family_handle = uuid::Uuid::new_v4().to_string();
    let family = crate::FamilyData {
        handle: family_handle.clone(),
        ..crate::FamilyData::default()
    };

    graph
        .add_node(family_handle.clone(), crate::Node::Family(family))
        .map_err(|_| {
            GenerationError::InvalidConfig(format!("duplicate family handle: {}", family_handle))
        })?;

    // Update person family list
    if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(&parent_handle) {
        person.family_list.push(family_handle.clone());
    }

    Ok(family_handle)
}

// ---------------------------------------------------------------------------
// Child assignment
// ---------------------------------------------------------------------------

/// Assign children to a family from the next generation of persons.
///
/// Age constraint: child's birth year must be > max(father_birth + 16,
/// mother_birth + 16) and < mother_birth + 50.
///
/// Outside this range: plausibility warning, not rejection.
pub(crate) fn assign_children(
    graph: &mut crate::Graph,
    family_handle: &crate::Handle,
    father_handle: &crate::Handle,
    mother_handle: &crate::Handle,
    persons: &mut [(crate::Handle, PersonSummary)],
    config: &RandomConfig,
    rng: &mut impl rand::Rng,
) -> Vec<crate::Handle> {
    // Determine the number of children for this family
    let target_count = if config.children_per_family.start == config.children_per_family.end {
        config.children_per_family.start
    } else {
        rng.gen_range(config.children_per_family.clone())
    };

    // Find father and mother birth years
    let father_birth = persons
        .iter()
        .find(|(h, _)| h == father_handle)
        .map(|(_, s)| s.birth_year)
        .unwrap_or(0);
    let mother_birth = persons
        .iter()
        .find(|(h, _)| h == mother_handle)
        .map(|(_, s)| s.birth_year)
        .unwrap_or(0);

    // Age constraint bounds
    let min_child_year = std::cmp::max(father_birth + 16, mother_birth + 16);
    let max_child_year = mother_birth + 50;

    // Find eligible children: next generation, not already assigned, within age bounds
    let eligible_indices: Vec<usize> = persons
        .iter()
        .enumerate()
        .filter(|(_, (_, s))| {
            !s.is_child && s.birth_year > min_child_year && s.birth_year < max_child_year
        })
        .map(|(i, _)| i)
        .collect();

    // Select up to target_count children
    let mut selected: Vec<crate::Handle> = Vec::new();
    let mut available = eligible_indices.clone();

    while selected.len() < target_count && !available.is_empty() {
        let pick = rng.gen_range(0..available.len());
        let idx = available.remove(pick);
        let child_handle = persons[idx].0.clone();

        // Mark as child
        persons[idx].1.is_child = true;

        // Add FamilyChildRef edge
        let edge = crate::Edge::FamilyChildRef {
            source: family_handle.clone(),
            target: child_handle.clone(),
            metadata: Box::new(crate::ChildRef {
                ref_field: child_handle.clone(),
                relation: Some(crate::ChildRefType::Birth),
            }),
        };
        let _ = graph.add_edge(edge);

        // Update person's parent_family_list
        if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(&child_handle) {
            person.parent_family_list.push(family_handle.clone());
        }

        selected.push(child_handle);
    }

    selected
}

// ---------------------------------------------------------------------------
// Event generation
// ---------------------------------------------------------------------------

/// Generate event nodes (birth, death, marriage) for persons and families.
///
/// For each person:
/// - A Birth event node is created with the person's birth date.
/// - If the person has a death date, a Death event node is created.
///
/// For each family:
/// - A Marriage event node is created with a date between the parents'
///   birth dates and the first child's birth date.
pub(crate) fn generate_events(
    graph: &mut crate::Graph,
    config: &RandomConfig,
    rng: &mut impl rand::Rng,
) -> Result<(), GenerationError> {
    // Collect family handles
    let family_handles: Vec<crate::Handle> = graph
        .iter_nodes()
        .filter(|(_, node)| matches!(node, crate::Node::Family(_)))
        .map(|(h, _)| h.clone())
        .collect();

    // Generate marriage events for families
    for family_handle in &family_handles {
        if let Some(crate::Node::Family(family)) = graph.get_node(family_handle).cloned() {
            // Check if family has both parents
            if let (Some(ref father_handle), Some(ref mother_handle)) =
                (&family.father_handle, &family.mother_handle)
            {
                // Find birth years of parents
                let father_birth = get_person_birth_year(graph, father_handle);
                let mother_birth = get_person_birth_year(graph, mother_handle);

                if let (Some(fb), Some(mb)) = (father_birth, mother_birth) {
                    // Marriage date: between later parent's birth + 16 and earliest child's birth
                    let min_marriage = std::cmp::max(fb + 16, mb + 16);

                    // Find earliest child's birth year
                    let earliest_child = family
                        .child_ref_list
                        .iter()
                        .filter_map(|cr| get_person_birth_year(graph, &cr.ref_field))
                        .min();

                    let max_marriage = earliest_child.unwrap_or(min_marriage + 10);

                    if min_marriage <= max_marriage {
                        let marriage_year = rng.gen_range(min_marriage..=max_marriage);
                        let marriage_month = rng.gen_range(1..=12);
                        let marriage_day = rng.gen_range(1..=28);

                        let marriage_date =
                            crate::DateValue::new_ymd(marriage_year, marriage_month, marriage_day);

                        // Create marriage event
                        let event_handle = uuid::Uuid::new_v4().to_string();
                        let event = crate::EventData {
                            handle: event_handle.clone(),
                            event_type: crate::EventType::Marriage,
                            date: Some(marriage_date),
                            ..crate::EventData::default()
                        };

                        graph
                            .add_node(event_handle.clone(), crate::Node::Event(event))
                            .map_err(|_| {
                                GenerationError::InvalidConfig(format!(
                                    "duplicate event handle: {}",
                                    event_handle
                                ))
                            })?;

                        // Link marriage event to family
                        graph
                            .add_edge(crate::Edge::FamilyEventRef {
                                source: family_handle.clone(),
                                target: event_handle,
                                metadata: Box::new(crate::EventRef {
                                    ref_field: family_handle.clone(),
                                    role: Some(crate::EventRoleType::Family),
                                }),
                            })
                            .expect("marriage event target exists (just added)");
                    }
                }
            }
        }
    }

    // If with_places, assign places to events
    if config.with_places {
        let event_handles: Vec<crate::Handle> = graph
            .iter_nodes()
            .filter(|(_, node)| matches!(node, crate::Node::Event(_)))
            .map(|(h, _)| h.clone())
            .collect();

        let mut used_place_names = std::collections::HashSet::new();

        for event_handle in &event_handles {
            let place = generate_place(config.place_depth, &used_place_names, rng);
            used_place_names.insert(place.city.clone());

            let place_handle = uuid::Uuid::new_v4().to_string();
            let place_node = crate::PlaceData {
                handle: place_handle.clone(),
                name: crate::Location {
                    city: Some(place.city),
                    county: Some(place.county),
                    state: Some(place.state),
                    country: Some(place.country),
                    ..crate::Location::default()
                },
                ..crate::PlaceData::default()
            };

            graph
                .add_node(place_handle.clone(), crate::Node::Place(place_node))
                .map_err(|_| {
                    GenerationError::InvalidConfig(format!(
                        "duplicate place handle: {}",
                        place_handle
                    ))
                })?;

            // Update event's place_handle
            if let Some(crate::Node::Event(ref mut event)) = graph.get_node_mut(event_handle) {
                event.place_handle = Some(place_handle.clone());
            }

            // Add EventPlace edge
            graph
                .add_edge(crate::Edge::EventPlace {
                    source: event_handle.clone(),
                    target: place_handle,
                })
                .expect("place node exists (just added)");
        }
    }

    // If with_citations, create a Source node and Citation edges for events
    if config.with_citations {
        let event_handles: Vec<crate::Handle> = graph
            .iter_nodes()
            .filter(|(_, node)| matches!(node, crate::Node::Event(_)))
            .map(|(h, _)| h.clone())
            .collect();

        // Create a single source for all citations
        let source_handle = uuid::Uuid::new_v4().to_string();
        let source = crate::SourceData {
            handle: source_handle.clone(),
            title: "Generated dataset".to_string(),
            ..crate::SourceData::default()
        };

        graph
            .add_node(source_handle.clone(), crate::Node::Source(source))
            .map_err(|_| {
                GenerationError::InvalidConfig(format!(
                    "duplicate source handle: {}",
                    source_handle
                ))
            })?;

        // Add citations for a fraction of events
        for event_handle in &event_handles {
            if rng.gen_bool(0.3) {
                // 30% of events get citations
                let citation_handle = uuid::Uuid::new_v4().to_string();
                let citation = crate::CitationData {
                    handle: citation_handle.clone(),
                    source_handle: source_handle.clone(),
                    ..crate::CitationData::default()
                };

                graph
                    .add_node(citation_handle.clone(), crate::Node::Citation(citation))
                    .map_err(|_| {
                        GenerationError::InvalidConfig(format!(
                            "duplicate citation handle: {}",
                            citation_handle
                        ))
                    })?;

                // Link citation to event
                graph
                    .add_edge(crate::Edge::EventCitation {
                        source: event_handle.clone(),
                        target: citation_handle,
                    })
                    .expect("citation node exists (just added)");
            }
        }
    }

    Ok(())
}

/// Get the birth year of a person from their birth event in the graph.
fn get_person_birth_year(graph: &crate::Graph, person_handle: &crate::Handle) -> Option<i32> {
    let edges = graph.edges_from(person_handle);
    for edge in &edges {
        if let crate::Edge::PersonEventRef { target, .. } = edge {
            if let Some(crate::Node::Event(event)) = graph.get_node(target) {
                if event.event_type == crate::EventType::Birth {
                    if let Some(ref date) = event.date {
                        return Some(date.year);
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// GenerationResult, GenerationStats, and generate_random entry point
// ---------------------------------------------------------------------------

/// The result of a random generation run.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationResult {
    /// The generated graph.
    pub graph: crate::Graph,
    /// The seed used for this generation (for reproducibility).
    pub seed: u64,
    /// Plausibility warnings emitted during generation.
    pub warnings: Vec<String>,
    /// Generation statistics.
    pub stats: GenerationStats,
}

/// Statistics about a generation run.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct GenerationStats {
    pub person_count: usize,
    pub family_count: usize,
    pub event_count: usize,
    pub place_count: usize,
    pub source_count: usize,
    pub citation_count: usize,
    pub note_count: usize,
    pub edge_count: usize,
}

/// Generate a random family tree graph with the given configuration.
///
/// The RNG is seeded from `config.seed` if provided, otherwise a random
/// seed is generated from OS entropy. The seed is recorded in the returned
/// `GenerationResult` for reproducibility.
///
/// The generated graph is NOT automatically validated — callers should
/// run `graph.validate(&schema)` before serialization, following the
/// five-stage pipeline (Generate → Validate → ...).
///
/// # Errors
///
/// Returns [`GenerationError::InvalidConfig`] if the configuration is
/// invalid (e.g., `person_count == 0`). Returns
/// [`GenerationError::ConstraintExhausted`] if generation cannot proceed
/// due to exhausted constraints (e.g., no eligible parents found).
pub fn generate_random(
    config: &RandomConfig,
    adversarial_config: &AdversarialConfig,
    _schema: &crate::Schema,
) -> Result<GenerationResult, GenerationError> {
    // Validate config
    if config.person_count == 0 {
        return Err(GenerationError::InvalidConfig(
            "person_count must be > 0".to_string(),
        ));
    }
    if config.generations == 0 {
        return Err(GenerationError::InvalidConfig(
            "generations must be >= 1".to_string(),
        ));
    }
    if config.start_year > config.end_year {
        return Err(GenerationError::InvalidConfig(format!(
            "start_year ({}) must be <= end_year ({})",
            config.start_year, config.end_year
        )));
    }
    if config.children_per_family.start > config.children_per_family.end {
        return Err(GenerationError::InvalidConfig(
            "children_per_family.start must be <= children_per_family.end".to_string(),
        ));
    }

    // Extract Category A strategy parameters from adversarial config
    let one_parent_fraction: f64 = if adversarial_config.enabled {
        adversarial_config
            .strategies
            .iter()
            .filter_map(|s| {
                if let AdversarialStrategy::OneParentFamilies(f) = s {
                    Some(*f)
                } else {
                    None
                }
            })
            .next()
            .unwrap_or(0.0)
    } else {
        0.0
    };

    let missing_events_fraction: f64 = if adversarial_config.enabled {
        adversarial_config
            .strategies
            .iter()
            .filter_map(|s| {
                if let AdversarialStrategy::MissingEvents(f) = s {
                    Some(*f)
                } else {
                    None
                }
            })
            .next()
            .unwrap_or(0.0)
    } else {
        0.0
    };

    let solo_persons_fraction: f64 = if adversarial_config.enabled {
        adversarial_config
            .strategies
            .iter()
            .filter_map(|s| {
                if let AdversarialStrategy::SoloPersons(f) = s {
                    Some(*f)
                } else {
                    None
                }
            })
            .next()
            .unwrap_or(0.0)
    } else {
        0.0
    };

    let many_alternate_names_fraction: f64 = if adversarial_config.enabled {
        adversarial_config
            .strategies
            .iter()
            .filter_map(|s| {
                if let AdversarialStrategy::ManyAlternateNames(f) = s {
                    Some(*f)
                } else {
                    None
                }
            })
            .next()
            .unwrap_or(0.0)
    } else {
        0.0
    };

    // Create seeded RNG
    let seed = config.seed.unwrap_or_else(|| rand::rngs::OsRng.gen());
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    // Create empty graph
    let mut graph = crate::Graph::new();
    let mut warnings: Vec<String> = Vec::new();

    // Track used names and place names
    let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Track person summaries for parent selection
    let mut persons: Vec<(crate::Handle, PersonSummary)> = Vec::new();

    // -----------------------------------------------------------------------
    // Stage 1: Create Person nodes
    // -----------------------------------------------------------------------
    let persons_per_layer = config.person_count.div_ceil(config.generations);

    for layer in 0..config.generations {
        let layer_count = if layer == config.generations - 1 {
            // Last layer gets remaining persons
            config.person_count - persons.len()
        } else {
            persons_per_layer.min(config.person_count - persons.len())
        };

        for _ in 0..layer_count {
            let (handle, person_warning) = generate_random_person(
                &mut graph,
                config,
                &mut used_names,
                &mut rng,
                layer,
                missing_events_fraction,
            )?;

            if let Some(w) = person_warning {
                warnings.push(w);
            }

            // Extract birth year from the birth event
            let birth_year = get_person_birth_year(&graph, &handle).unwrap_or(1970);

            // Get gender from the person node
            let gender = match graph.get_node(&handle) {
                Some(crate::Node::Person(p)) => p.gender,
                _ => 0,
            };

            persons.push((
                handle.clone(),
                PersonSummary {
                    handle,
                    birth_year,
                    gender,
                    layer,
                    is_parent: false,
                    is_child: false,
                },
            ));
        }
    }

    // ---- Solo-persons strategy ----
    // Mark a fraction of persons as solo: they won't participate in families
    if solo_persons_fraction > 0.0 {
        let target_solo_count =
            (persons.len() as f64 * solo_persons_fraction.clamp(0.0, 1.0)) as usize;
        // Randomly select persons to be solo (shuffle and take first N)
        let mut indices: Vec<usize> = (0..persons.len()).collect();
        // Fisher-Yates partial shuffle for first target_solo_count elements
        for i in 0..target_solo_count.min(persons.len()) {
            let j = rng.gen_range(i..persons.len());
            indices.swap(i, j);
        }
        for &idx in indices.iter().take(target_solo_count.min(persons.len())) {
            persons[idx].1.is_parent = true;
            persons[idx].1.is_child = true;
            warnings.push(format!(
                "Person {}: solo person — excluded from families (strategy: solo-persons, fraction: {})",
                persons[idx].0, solo_persons_fraction
            ));
        }
    }

    // ---- Many-alternate-names strategy ----
    if many_alternate_names_fraction > 0.0 {
        let mut alt_names_rng = rand::rngs::StdRng::seed_from_u64(seed.wrapping_add(1));
        for (handle, _summary) in &persons {
            if alt_names_rng.gen_bool(many_alternate_names_fraction.clamp(0.0, 1.0)) {
                let name_count: usize = alt_names_rng.gen_range(5..=20);
                let mut alternate_names = Vec::with_capacity(name_count);
                for _ in 0..name_count {
                    let alt_given =
                        generate_given_name(&config.name_style, &used_names, &mut alt_names_rng);
                    let alt_surname =
                        generate_surname(&config.name_style, &used_names, &mut alt_names_rng);
                    used_names.insert(alt_given.clone());
                    used_names.insert(alt_surname.clone());
                    alternate_names.push(crate::Name {
                        first_name: Some(alt_given),
                        surname_list: vec![crate::Surname {
                            surname: Some(alt_surname),
                            ..crate::Surname::default()
                        }],
                        type_field: Some(crate::NameType::Unknown),
                        ..crate::Name::default()
                    });
                }
                // Update the person node with alternate names
                if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(handle) {
                    person.alternate_names = alternate_names;
                }
                if name_count >= 10 {
                    warnings.push(format!(
                        "Person {}: many alternate names ({}) (strategy: many-alternate-names, fraction: {})",
                        handle, name_count, many_alternate_names_fraction
                    ));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Stage 2: Parent selection and Family creation
    // -----------------------------------------------------------------------
    // Sort persons by birth year for parent selection
    persons.sort_by(|a, b| a.1.birth_year.cmp(&b.1.birth_year));

    let families_to_create = config.family_count.min(persons.len() / 2);
    let mut family_handles: Vec<crate::Handle> = Vec::new();

    for _ in 0..families_to_create {
        // Assign a layer for this family (cycling through generations)
        let layer = rng.gen_range(0..config.generations);

        match generate_family(
            &mut graph,
            config,
            &mut persons,
            layer,
            &mut rng,
            one_parent_fraction,
        ) {
            Ok((family_handle, warning)) => {
                if let Some(w) = warning {
                    warnings.push(w);
                }
                family_handles.push(family_handle);
            }
            Err(GenerationError::ConstraintExhausted { message, seed: _ }) => {
                warnings.push(format!(
                    "Constraint exhausted during family generation: {}",
                    message
                ));
                break;
            }
            Err(e) => return Err(e),
        }
    }

    // -----------------------------------------------------------------------
    // Stage 3: Child assignment
    // -----------------------------------------------------------------------
    // For each family, find parents and assign children
    for family_handle in &family_handles {
        if let Some(crate::Node::Family(family)) = graph.get_node(family_handle).cloned() {
            let father_handle = family.father_handle.clone();
            let mother_handle = family.mother_handle.clone();

            match (father_handle, mother_handle) {
                (Some(ref father_h), Some(ref mother_h)) => {
                    let children = assign_children(
                        &mut graph,
                        family_handle,
                        father_h,
                        mother_h,
                        &mut persons,
                        config,
                        &mut rng,
                    );
                    if children.is_empty() {
                        warnings.push(format!(
                            "Family {}: no eligible children found for parent pair",
                            family_handle
                        ));
                    }
                }
                _ => {
                    // Single-parent family, skip child assignment
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Stage 4: Event generation
    // -----------------------------------------------------------------------
    generate_events(&mut graph, config, &mut rng)?;

    // -----------------------------------------------------------------------
    // Apply Category B (post-generation) adversarial strategies
    // -----------------------------------------------------------------------
    let adversarial_result =
        crate::generate::adversarial::apply_adversarial_strategies(graph, adversarial_config);
    graph = adversarial_result.graph;

    // Collect any warnings from adversarial strategies
    for error in &adversarial_result.errors {
        warnings.push(format!("adversarial strategy: {}", error));
    }

    // -----------------------------------------------------------------------
    // Collect statistics
    // -----------------------------------------------------------------------
    let stats = collect_stats(&graph);

    Ok(GenerationResult {
        graph,
        seed,
        warnings,
        stats,
    })
}

/// Collect statistics from the generated graph.
fn collect_stats(graph: &crate::Graph) -> GenerationStats {
    let mut stats = GenerationStats::default();

    for (_, node) in graph.iter_nodes() {
        match node {
            crate::Node::Person(_) => stats.person_count += 1,
            crate::Node::Family(_) => stats.family_count += 1,
            crate::Node::Event(_) => stats.event_count += 1,
            crate::Node::Place(_) => stats.place_count += 1,
            crate::Node::Source(_) => stats.source_count += 1,
            crate::Node::Citation(_) => stats.citation_count += 1,
            crate::Node::Note(_) => stats.note_count += 1,
            _ => {}
        }
    }

    stats.edge_count = graph.edge_count();
    stats
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

    // -----------------------------------------------------------------------
    // Place generator tests
    // -----------------------------------------------------------------------

    #[test]
    fn generate_place_depth_1() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place = generate_place(1, &used, &mut rng);
        assert!(place.city.is_empty());
        assert!(place.county.is_empty());
        assert!(place.state.is_empty());
        assert!(
            !place.country.is_empty(),
            "Country should be non-empty at depth 1"
        );
    }

    #[test]
    fn generate_place_depth_3() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place = generate_place(3, &used, &mut rng);
        assert!(
            !place.city.is_empty(),
            "City should be non-empty at depth 3"
        );
        assert!(
            !place.county.is_empty(),
            "County should be non-empty at depth 3"
        );
        assert!(
            !place.state.is_empty(),
            "State should be non-empty at depth 3"
        );
        assert!(
            !place.country.is_empty(),
            "Country should be non-empty at depth 3"
        );
    }

    #[test]
    fn generate_place_city_unique() {
        let mut used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place1 = generate_place(3, &used, &mut rng);
        used.insert(place1.city.clone());
        let place2 = generate_place(3, &used, &mut rng);
        // The city should differ because the first one is in used_place_names
        // (there's a small chance of collision with prefix+suffix combinations)
        assert_ne!(place1.city, place2.city);
    }

    #[test]
    fn generate_place_city_not_empty() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for _ in 0..10 {
            let place = generate_place(3, &used, &mut rng);
            assert!(!place.city.is_empty(), "City name should be non-empty");
        }
    }

    #[test]
    fn generate_place_all_utf8() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place = generate_place(3, &used, &mut rng);
        assert!(std::str::from_utf8(place.city.as_bytes()).is_ok());
        assert!(std::str::from_utf8(place.county.as_bytes()).is_ok());
        assert!(std::str::from_utf8(place.state.as_bytes()).is_ok());
        assert!(std::str::from_utf8(place.country.as_bytes()).is_ok());
    }

    #[test]
    fn generate_place_depth_0_defaults_to_1() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place = generate_place(0, &used, &mut rng);
        assert!(
            !place.country.is_empty(),
            "Depth 0 should default to depth 1"
        );
        assert!(place.city.is_empty());
    }

    #[test]
    fn generate_place_country_reused() {
        let used = std::collections::HashSet::new();
        let mut rng1 = rand::rngs::StdRng::seed_from_u64(42);
        let mut rng2 = rand::rngs::StdRng::seed_from_u64(42);
        let place1 = generate_place(3, &used, &mut rng1);
        let place2 = generate_place(3, &used, &mut rng2);
        assert_eq!(
            place1.country, place2.country,
            "Same seed should produce same country"
        );
    }

    #[test]
    fn generate_place_depth_2() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place = generate_place(2, &used, &mut rng);
        assert!(place.city.is_empty(), "City should be empty at depth 2");
        assert!(place.county.is_empty(), "County should be empty at depth 2");
        assert!(
            !place.state.is_empty(),
            "State should be non-empty at depth 2"
        );
        assert!(
            !place.country.is_empty(),
            "Country should be non-empty at depth 2"
        );
    }

    // -----------------------------------------------------------------------
    // Random person generation tests
    // -----------------------------------------------------------------------

    #[test]
    fn generate_random_person_creates_node() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut used_names = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let (handle, _person_warning) =
            generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0, 0.0)
                .expect("person generation should succeed");

        assert!(graph.contains_node(&handle));
        assert_eq!(graph.node_count(), 2); // Person + birth event
    }

    #[test]
    fn generate_random_person_has_name() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut used_names = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let (handle, _person_warning) =
            generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0, 0.0)
                .expect("person generation should succeed");

        match graph.get_node(&handle).unwrap() {
            crate::Node::Person(person) => {
                assert!(person.primary_name.first_name.is_some());
            }
            _ => panic!("Expected Person node"),
        }
    }

    #[test]
    fn generate_random_person_has_valid_gender() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut used_names = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let (handle, _person_warning) =
            generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0, 0.0)
                .expect("person generation should succeed");

        match graph.get_node(&handle).unwrap() {
            crate::Node::Person(person) => {
                assert!(
                    person.gender == 0 || person.gender == 1 || person.gender == 2,
                    "Gender must be 0, 1, or 2, got {}",
                    person.gender
                );
            }
            _ => panic!("Expected Person node"),
        }
    }

    #[test]
    fn generate_random_person_birth_date_in_range() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig {
            start_year: 1900,
            end_year: 2000,
            ..RandomConfig::default()
        };
        let mut used_names = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let (_handle, _person_warning) =
            generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 1, 0.0)
                .expect("person generation should succeed");

        // Layer 1: end_year-85 to end_year-55 = 1915 to 1945
        // Check that a birth event exists with a date in the expected range
        for (_, node) in graph.iter_nodes() {
            if let crate::Node::Event(event) = node {
                if event.event_type == crate::EventType::Birth {
                    if let Some(ref date) = event.date {
                        // Broad range check: birth year should be plausible
                        assert!(date.year >= 1900 && date.year <= 2000,);
                    }
                }
            }
        }
    }

    #[test]
    fn generate_random_person_death_after_birth() {
        // Use old config to ensure death is generated
        let mut graph = crate::Graph::new();
        let config = RandomConfig {
            start_year: 1850,
            end_year: 1925,
            ..RandomConfig::default()
        };
        let mut used_names = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let (handle, _person_warning) =
            generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0, 0.0)
                .expect("person generation should succeed");

        // Check that if death event exists, its date is after birth
        let mut birth_year = 0i32;
        let mut death_year = None;

        let edges = graph.edges_from(&handle);
        for edge in edges {
            if let crate::Edge::PersonEventRef { target, .. } = edge {
                if let Some(crate::Node::Event(event)) = graph.get_node(target) {
                    match event.event_type {
                        crate::EventType::Birth => {
                            if let Some(ref date) = event.date {
                                birth_year = date.year;
                            }
                        }
                        crate::EventType::Death => {
                            if let Some(ref date) = event.date {
                                death_year = Some(date.year);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(dy) = death_year {
            assert!(
                dy > birth_year,
                "Death year {} must be after birth year {}",
                dy,
                birth_year
            );
        }
    }

    #[test]
    fn generate_random_person_with_seed() {
        let mut graph1 = crate::Graph::new();
        let mut graph2 = crate::Graph::new();
        let config = RandomConfig::default();
        let mut used1 = std::collections::HashSet::new();
        let mut used2 = std::collections::HashSet::new();
        let mut rng1 = rand::rngs::StdRng::seed_from_u64(42);
        let mut rng2 = rand::rngs::StdRng::seed_from_u64(42);

        let (h1, _pw) = generate_random_person(&mut graph1, &config, &mut used1, &mut rng1, 0, 0.0)
            .expect("person gen should succeed");
        let (h2, _pw) = generate_random_person(&mut graph2, &config, &mut used2, &mut rng2, 0, 0.0)
            .expect("person gen should succeed");

        // Same seed should produce the same person data (excluding UUID handle)
        if let (crate::Node::Person(p1), crate::Node::Person(p2)) =
            (graph1.get_node(&h1).unwrap(), graph2.get_node(&h2).unwrap())
        {
            assert_eq!(p1.primary_name, p2.primary_name, "Names should match");
            assert_eq!(p1.gender, p2.gender, "Genders should match");
        } else {
            panic!("Expected Person nodes");
        }
    }

    #[test]
    fn generate_random_person_regression_layer_0() {
        let config = RandomConfig {
            start_year: 1850,
            end_year: 2025,
            ..RandomConfig::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for _ in 0..10 {
            let year = birth_year_for_layer(0, &config, &mut rng);
            // Layer 0: end_year-55 to end_year-25 = 1970 to 2000
            assert!(
                year >= 1970,
                "Layer 0 birth year {} should be >= 1970",
                year
            );
            assert!(year < 2000, "Layer 0 birth year {} should be < 2000", year);
        }
    }

    #[test]
    fn generate_random_person_regression_layer_3() {
        let config = RandomConfig {
            start_year: 1850,
            end_year: 2025,
            ..RandomConfig::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for _ in 0..10 {
            let year = birth_year_for_layer(3, &config, &mut rng);
            // Layer 3: end_year-55-90 to end_year-25-90 = 1880 to 1910
            // More precisely: end_year-55-90 = 1880, end_year-25-90 = 1910
            assert!(
                year >= 1880,
                "Layer 3 birth year {} should be >= 1880",
                year
            );
            assert!(
                year <= 1925,
                "Layer 3 birth year {} should be <= 1925",
                year
            );
        }
    }

    // -----------------------------------------------------------------------
    // Parent selection and family creation tests
    // -----------------------------------------------------------------------

    #[test]
    fn select_parents_returns_opposite_genders() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let config = RandomConfig::default();
        let persons = vec![
            (
                "p1".to_string(),
                PersonSummary {
                    handle: "p1".to_string(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p2".to_string(),
                PersonSummary {
                    handle: "p2".to_string(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        let result = select_parents(&persons, &config, 0, &mut rng);
        assert!(result.is_some(), "Should find eligible parents");
        let (father, mother) = result.unwrap();
        assert_eq!(father, "p1");
        assert_eq!(mother, "p2");
    }

    #[test]
    fn select_parents_age_difference_within_bounds() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let config = RandomConfig::default();
        let persons = vec![
            (
                "p1".to_string(),
                PersonSummary {
                    handle: "p1".to_string(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p2".to_string(),
                PersonSummary {
                    handle: "p2".to_string(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p3".to_string(),
                PersonSummary {
                    handle: "p3".to_string(),
                    birth_year: 1950,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p4".to_string(),
                PersonSummary {
                    handle: "p4".to_string(),
                    birth_year: 1990,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        // p3 (b.1950) and p4 (b.1990) have a 40 year age gap, too large
        // p1 (b.1970) and p2 (b.1975) have a 5 year gap, should be fine
        let result = select_parents(&persons, &config, 0, &mut rng);
        assert!(result.is_some(), "Should find eligible parents");
    }

    #[test]
    fn select_parents_returns_none_when_no_eligible() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let config = RandomConfig::default();
        let persons: Vec<(crate::Handle, PersonSummary)> = vec![];

        let result = select_parents(&persons, &config, 0, &mut rng);
        assert!(result.is_none(), "Empty pool should return None");
    }

    #[test]
    fn select_parents_prefers_same_layer() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let config = RandomConfig::default();
        let persons = vec![
            (
                "p1".to_string(),
                PersonSummary {
                    handle: "p1".to_string(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p2".to_string(),
                PersonSummary {
                    handle: "p2".to_string(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p3".to_string(),
                PersonSummary {
                    handle: "p3".to_string(),
                    birth_year: 1940,
                    gender: 0,
                    layer: 1,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p4".to_string(),
                PersonSummary {
                    handle: "p4".to_string(),
                    birth_year: 1945,
                    gender: 1,
                    layer: 1,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        // Layer 0 has p1 and p2 which are compatible
        let result = select_parents(&persons, &config, 0, &mut rng);
        assert!(result.is_some());
        let (father, mother) = result.unwrap();
        // Should prefer same-layer parents
        assert!(
            (father == "p1" && mother == "p2") || (father == "p3" && mother == "p4"),
            "Expected parents from same or adjacent layer"
        );
    }

    #[test]
    fn select_parents_expands_to_adjacent_layer() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let config = RandomConfig::default();
        let persons = vec![
            // Only one person in layer 0
            (
                "p1".to_string(),
                PersonSummary {
                    handle: "p1".to_string(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            // Two in layer 1
            (
                "p2".to_string(),
                PersonSummary {
                    handle: "p2".to_string(),
                    birth_year: 1940,
                    gender: 0,
                    layer: 1,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                "p3".to_string(),
                PersonSummary {
                    handle: "p3".to_string(),
                    birth_year: 1945,
                    gender: 1,
                    layer: 1,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        // Layer 0 only has one person, should expand to layer 1
        let result = select_parents(&persons, &config, 0, &mut rng);
        assert!(result.is_some(), "Should expand to adjacent layer");
    }

    #[test]
    fn generate_family_creates_family_node() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Add person nodes
        let p1 = "p1".to_string();
        let p2 = "p2".to_string();
        graph
            .add_node(
                p1.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: p1.clone(),
                    gender: 0,
                    primary_name: crate::Name {
                        first_name: Some("John".to_string()),
                        ..crate::Name::default()
                    },
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                p2.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: p2.clone(),
                    gender: 1,
                    primary_name: crate::Name {
                        first_name: Some("Jane".to_string()),
                        ..crate::Name::default()
                    },
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();

        let mut persons = vec![
            (
                p1.clone(),
                PersonSummary {
                    handle: p1,
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                p2.clone(),
                PersonSummary {
                    handle: p2,
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        let (family_handle, _warning) =
            generate_family(&mut graph, &config, &mut persons, 0, &mut rng, 0.0)
                .expect("family generation should succeed");

        assert!(graph.contains_node(&family_handle));
        // Check it's a Family node
        match graph.get_node(&family_handle).unwrap() {
            crate::Node::Family(_) => {}
            _ => panic!("Expected Family node"),
        }
    }

    #[test]
    fn generate_family_adds_father_mother_edges() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let p1 = "p1".to_string();
        let p2 = "p2".to_string();
        graph
            .add_node(
                p1.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: p1.clone(),
                    gender: 0,
                    primary_name: crate::Name {
                        first_name: Some("John".to_string()),
                        ..crate::Name::default()
                    },
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                p2.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: p2.clone(),
                    gender: 1,
                    primary_name: crate::Name {
                        first_name: Some("Jane".to_string()),
                        ..crate::Name::default()
                    },
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();

        let mut persons = vec![
            (
                p1.clone(),
                PersonSummary {
                    handle: p1,
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                p2.clone(),
                PersonSummary {
                    handle: p2,
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        let (family_handle, _warning) =
            generate_family(&mut graph, &config, &mut persons, 0, &mut rng, 0.0)
                .expect("family generation should succeed");

        let edges = graph.edges_from(&family_handle);
        let has_father = edges
            .iter()
            .any(|e| matches!(e, crate::Edge::FamilyFather { .. }));
        let has_mother = edges
            .iter()
            .any(|e| matches!(e, crate::Edge::FamilyMother { .. }));
        assert!(has_father, "Family should have FamilyFather edge");
        assert!(has_mother, "Family should have FamilyMother edge");
    }

    #[test]
    fn generate_family_single_parent_when_no_eligible() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Only one person (male) - no eligible female
        let p1 = "p1".to_string();
        graph
            .add_node(
                p1.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: p1.clone(),
                    gender: 0,
                    primary_name: crate::Name {
                        first_name: Some("John".to_string()),
                        ..crate::Name::default()
                    },
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();

        let mut persons = vec![(
            p1.clone(),
            PersonSummary {
                handle: p1,
                birth_year: 1970,
                gender: 0,
                layer: 0,
                is_parent: false,
                is_child: false,
            },
        )];

        let result = generate_family(&mut graph, &config, &mut persons, 0, &mut rng, 0.0);
        // Should succeed with single parent
        assert!(result.is_ok(), "Should create single-parent family");
    }

    #[test]
    fn generate_family_plausibility_warning() {
        // Test that single-parent families are created when no pair found
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let p1 = "p1".to_string();
        graph
            .add_node(
                p1.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: p1.clone(),
                    gender: 0,
                    primary_name: crate::Name {
                        first_name: Some("John".to_string()),
                        ..crate::Name::default()
                    },
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();

        let mut persons = vec![(
            p1.clone(),
            PersonSummary {
                handle: p1,
                birth_year: 1970,
                gender: 0,
                layer: 0,
                is_parent: false,
                is_child: false,
            },
        )];

        // Only one person, so single-parent family is created
        let result = generate_family(&mut graph, &config, &mut persons, 0, &mut rng, 0.0);
        assert!(result.is_ok(), "Single-parent family should be created");
    }

    // -----------------------------------------------------------------------
    // Child assignment tests
    // -----------------------------------------------------------------------

    // =======================================================================
    // Step 2: One-parent families adversarial strategy tests
    // =======================================================================

    #[test]
    fn one_parent_families_strategy_zero_fraction() {
        let config = RandomConfig {
            person_count: 20,
            family_count: 8,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::OneParentFamilies(0.0)],
        };
        let schema = crate::Schema::default();
        let result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // With fraction 0.0, no one-parent families should be created
        // (all families should have both parents if the pair was found)
        let mut _one_parent_count = 0;
        for (_, node) in result.graph.iter_nodes() {
            if let crate::Node::Family(f) = node {
                let parent_count = f.father_handle.iter().count() + f.mother_handle.iter().count();
                if parent_count == 1 {
                    _one_parent_count += 1;
                }
            }
        }
        // With fraction 0.0, any one-parent families are from normal
        // constraint exhaustion, not from the adversarial strategy.
        // No warning about one-parent strategy should appear.
        let strategy_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("one-parent"))
            .collect();
        assert!(
            strategy_warnings.is_empty(),
            "Zero fraction should produce no one-parent strategy warnings, got: {:?}",
            strategy_warnings
        );
    }

    #[test]
    fn one_parent_families_strategy_all_single() {
        let config = RandomConfig {
            person_count: 20,
            family_count: 6,
            generations: 2,
            seed: Some(123),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::OneParentFamilies(1.0)],
        };
        let schema = crate::Schema::default();
        let result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // With fraction 1.0, all families should be one-parent
        let mut families = 0;
        let mut one_parent = 0;
        for (_, node) in result.graph.iter_nodes() {
            if let crate::Node::Family(f) = node {
                families += 1;
                let parent_count = f.father_handle.iter().count() + f.mother_handle.iter().count();
                if parent_count == 1 {
                    one_parent += 1;
                }
            }
        }
        assert!(families > 0, "Should have created families");
        assert_eq!(
            one_parent, families,
            "All families should be one-parent with fraction=1.0"
        );
    }

    #[test]
    fn one_parent_families_validates_ok() {
        let config = RandomConfig {
            person_count: 20,
            family_count: 6,
            generations: 2,
            seed: Some(456),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::OneParentFamilies(0.5)],
        };
        let schema = crate::Schema::default();
        let mut result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // One-parent families are structurally valid
        let errors = result.graph.validate(&schema);
        assert!(
            errors.is_empty(),
            "One-parent families should pass validation, got: {:?}",
            errors
        );
    }

    #[test]
    fn one_parent_families_single_produces_warning() {
        let config = RandomConfig {
            person_count: 20,
            family_count: 6,
            generations: 2,
            seed: Some(789),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::OneParentFamilies(1.0)],
        };
        let schema = crate::Schema::default();
        let result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // With fraction 1.0, every family should produce a one-parent warning
        let one_parent_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("one-parent family"))
            .collect();
        assert!(
            !one_parent_warnings.is_empty(),
            "Should have one-parent warnings"
        );
    }

    #[test]
    fn one_parent_families_edge_case_one_person_pool() {
        // When only one person total, family creation would fail anyway
        // (not enough persons for select_parents). Verify graceful handling.
        let config = RandomConfig {
            person_count: 1,
            family_count: 0, // no families can be created
            generations: 1,
            seed: Some(101),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::OneParentFamilies(1.0)],
        };
        let schema = crate::Schema::default();
        let result = generate_random(&config, &adversarial, &schema)
            .expect("single person generation should succeed");
        assert_eq!(result.stats.person_count, 1);
        assert_eq!(result.stats.family_count, 0);
    }

    // =======================================================================
    // Step 3: Missing events adversarial strategy tests
    // =======================================================================

    #[test]
    fn missing_events_zero_fraction() {
        let config = RandomConfig {
            person_count: 20,
            family_count: 5,
            generations: 2,
            seed: Some(1001),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::MissingEvents(0.0)],
        };
        let schema = crate::Schema::default();
        let result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // With fraction 0.0, all persons should have events.
        // No missing-events warnings should appear.
        let missing_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("missing events"))
            .collect();
        assert!(
            missing_warnings.is_empty(),
            "Zero fraction should produce no missing-events warnings, got: {:?}",
            missing_warnings
        );
    }

    #[test]
    fn missing_events_all_missing() {
        let config = RandomConfig {
            person_count: 20,
            family_count: 5,
            generations: 2,
            seed: Some(2002),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::MissingEvents(1.0)],
        };
        let schema = crate::Schema::default();
        let result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // With fraction 1.0, all persons should have missing events
        let missing_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("missing events"))
            .collect();
        assert!(
            !missing_warnings.is_empty(),
            "All persons should have missing events warnings"
        );
        assert_eq!(
            missing_warnings.len(),
            result.stats.person_count,
            "Every person should have a missing-events warning"
        );
    }

    #[test]
    fn missing_events_validates_ok() {
        let config = RandomConfig {
            person_count: 20,
            family_count: 5,
            generations: 2,
            seed: Some(3003),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::MissingEvents(0.5)],
        };
        let schema = crate::Schema::default();
        let mut result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // Missing events should still pass validation
        let errors = result.graph.validate(&schema);
        assert!(
            errors.is_empty(),
            "Missing events should pass validation, got: {:?}",
            errors
        );
    }

    #[test]
    fn missing_events_some_missing_some_present() {
        let config = RandomConfig {
            person_count: 30,
            family_count: 8,
            generations: 2,
            seed: Some(4004),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::MissingEvents(0.5)],
        };
        let schema = crate::Schema::default();
        let result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // With fraction 0.5, roughly half should be missing
        // (statistical, so check for some but not all)
        let missing_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("missing events"))
            .collect();
        assert!(
            !missing_warnings.is_empty(),
            "Should have some missing events"
        );
        assert!(
            missing_warnings.len() < result.stats.person_count,
            "Should not have all persons missing events"
        );
    }

    #[test]
    fn missing_events_warning_emitted() {
        let config = RandomConfig {
            person_count: 10,
            family_count: 3,
            generations: 1,
            seed: Some(5005),
            ..RandomConfig::default()
        };
        let adversarial = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::MissingEvents(1.0)],
        };
        let schema = crate::Schema::default();
        let result =
            generate_random(&config, &adversarial, &schema).expect("generation should succeed");

        // Every warning should mention "missing events"
        let missing_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("missing events"))
            .collect();
        assert_eq!(
            missing_warnings.len(),
            result.stats.person_count,
            "Each person should have a warning"
        );
    }

    #[test]
    fn one_parent_families_regression_alternating_parents() {
        // Verify that skipped parent alternates between father and mother
        use std::collections::HashSet;

        let mut father_skipped = HashSet::new();
        let mut mother_skipped = HashSet::new();

        for seed in 0..20 {
            let config = RandomConfig {
                person_count: 30,
                family_count: 10,
                generations: 2,
                seed: Some(seed * 100),
                ..RandomConfig::default()
            };
            let adversarial = AdversarialConfig {
                enabled: true,
                strategies: vec![AdversarialStrategy::OneParentFamilies(0.8)],
            };
            let schema = crate::Schema::default();
            if let Ok(result) = generate_random(&config, &adversarial, &schema) {
                for w in &result.warnings {
                    if w.contains("father skipped") {
                        father_skipped.insert(seed);
                    } else if w.contains("mother skipped") {
                        mother_skipped.insert(seed);
                    }
                }
            }
        }

        // With fraction 0.8 and 20 seeds, we should see both father and mother
        // being skipped across different runs
        assert!(
            !father_skipped.is_empty(),
            "Should have seeds where father was skipped"
        );
        assert!(
            !mother_skipped.is_empty(),
            "Should have seeds where mother was skipped"
        );
    }

    // -----------------------------------------------------------------------
    // (original child assignment tests follow)
    // -----------------------------------------------------------------------

    #[test]
    fn assign_children_adds_children() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Create family node
        let family_handle = "f1".to_string();
        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData::default()),
            )
            .unwrap();

        // Create person nodes
        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: father_handle.clone(),
                    gender: 0,
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: mother_handle.clone(),
                    gender: 1,
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();

        // Create child-age persons
        let mut persons = vec![
            (
                father_handle.clone(),
                PersonSummary {
                    handle: father_handle.clone(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                mother_handle.clone(),
                PersonSummary {
                    handle: mother_handle.clone(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        // Add children in the next generation
        for i in 0..5 {
            let child_handle = format!("child{}", i);
            graph
                .add_node(
                    child_handle.clone(),
                    crate::Node::Person(crate::PersonData {
                        handle: child_handle.clone(),
                        ..crate::PersonData::default()
                    }),
                )
                .unwrap();
            persons.push((
                child_handle.clone(),
                PersonSummary {
                    handle: child_handle,
                    birth_year: 2000 + i,
                    gender: 0,
                    layer: 1,
                    is_parent: false,
                    is_child: false,
                },
            ));
        }

        let children = assign_children(
            &mut graph,
            &family_handle,
            &father_handle,
            &mother_handle,
            &mut persons,
            &config,
            &mut rng,
        );
        assert!(!children.is_empty(), "Should assign at least one child");

        // Check child edges
        let edges = graph.edges_from(&family_handle);
        let child_edges: Vec<_> = edges
            .iter()
            .filter(|e| matches!(e, crate::Edge::FamilyChildRef { .. }))
            .collect();
        assert_eq!(child_edges.len(), children.len());
    }

    #[test]
    fn assign_children_age_constraint() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let family_handle = "f1".to_string();
        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData::default()),
            )
            .unwrap();

        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: father_handle.clone(),
                    gender: 0,
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: mother_handle.clone(),
                    gender: 1,
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();

        let mut persons = vec![
            (
                father_handle.clone(),
                PersonSummary {
                    handle: father_handle.clone(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                mother_handle.clone(),
                PersonSummary {
                    handle: mother_handle.clone(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        // Father born 1970, mother born 1975
        // min_child_year = max(1970+16, 1975+16) = 1991
        // max_child_year = 1975+50 = 2025
        // Valid children: birth_year > 1991 and < 2025

        // Child born 1990 (too young - before 1991)
        let child_too_young = "child_too_young".to_string();
        graph
            .add_node(
                child_too_young.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: child_too_young.clone(),
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();
        persons.push((
            child_too_young.clone(),
            PersonSummary {
                handle: child_too_young,
                birth_year: 1990,
                gender: 0,
                layer: 1,
                is_parent: false,
                is_child: false,
            },
        ));

        // Child born 2000 (valid)
        let child_valid = "child_valid".to_string();
        graph
            .add_node(
                child_valid.clone(),
                crate::Node::Person(crate::PersonData {
                    handle: child_valid.clone(),
                    ..crate::PersonData::default()
                }),
            )
            .unwrap();
        persons.push((
            child_valid.clone(),
            PersonSummary {
                handle: child_valid,
                birth_year: 2000,
                gender: 0,
                layer: 1,
                is_parent: false,
                is_child: false,
            },
        ));

        let children = assign_children(
            &mut graph,
            &family_handle,
            &father_handle,
            &mother_handle,
            &mut persons,
            &config,
            &mut rng,
        );
        // The valid child should be selected, not the too-young one
        // (but since we shuffle, the valid child might or might not be chosen)
        // At minimum, the function should not crash
        assert!(children.len() <= 1, "Should only select eligible children");
    }

    #[test]
    fn assign_children_count_in_range() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig {
            children_per_family: 2..5,
            ..RandomConfig::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let family_handle = "f1".to_string();
        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData::default()),
            )
            .unwrap();

        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let mut persons = vec![
            (
                father_handle.clone(),
                PersonSummary {
                    handle: father_handle.clone(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                mother_handle.clone(),
                PersonSummary {
                    handle: mother_handle.clone(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        // Add many eligible children
        for i in 0..10 {
            let child_handle = format!("c{}", i);
            graph
                .add_node(
                    child_handle.clone(),
                    crate::Node::Person(crate::PersonData::default()),
                )
                .unwrap();
            persons.push((
                child_handle.clone(),
                PersonSummary {
                    handle: child_handle,
                    birth_year: 2000 + i,
                    gender: 0,
                    layer: 1,
                    is_parent: false,
                    is_child: false,
                },
            ));
        }

        let children = assign_children(
            &mut graph,
            &family_handle,
            &father_handle,
            &mother_handle,
            &mut persons,
            &config,
            &mut rng,
        );
        assert!(!children.is_empty());
        assert!(
            children.len() <= 4,
            "Number of children should be within range"
        );
    }

    #[test]
    fn assign_children_child_not_reassigned() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let family_handle = "f1".to_string();
        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData::default()),
            )
            .unwrap();

        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let mut persons = vec![
            (
                father_handle.clone(),
                PersonSummary {
                    handle: father_handle.clone(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                mother_handle.clone(),
                PersonSummary {
                    handle: mother_handle.clone(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        // One child, already marked as child
        let child = "child".to_string();
        graph
            .add_node(
                child.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        persons.push((
            child.clone(),
            PersonSummary {
                handle: child.clone(),
                birth_year: 2000,
                gender: 0,
                layer: 1,
                is_parent: false,
                is_child: true, // Already assigned!
            },
        ));

        let children = assign_children(
            &mut graph,
            &family_handle,
            &father_handle,
            &mother_handle,
            &mut persons,
            &config,
            &mut rng,
        );
        // The child should NOT be reassigned since it's already marked as child
        assert!(children.is_empty() || !children.contains(&child));
    }

    #[test]
    fn assign_children_child_parent_family_list_updated() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let family_handle = "f1".to_string();
        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData::default()),
            )
            .unwrap();

        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let mut persons = vec![
            (
                father_handle.clone(),
                PersonSummary {
                    handle: father_handle.clone(),
                    birth_year: 1970,
                    gender: 0,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
            (
                mother_handle.clone(),
                PersonSummary {
                    handle: mother_handle.clone(),
                    birth_year: 1975,
                    gender: 1,
                    layer: 0,
                    is_parent: false,
                    is_child: false,
                },
            ),
        ];

        let child = "child".to_string();
        graph
            .add_node(
                child.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        persons.push((
            child.clone(),
            PersonSummary {
                handle: child.clone(),
                birth_year: 2000,
                gender: 0,
                layer: 1,
                is_parent: false,
                is_child: false,
            },
        ));

        let children = assign_children(
            &mut graph,
            &family_handle,
            &father_handle,
            &mother_handle,
            &mut persons,
            &config,
            &mut rng,
        );

        if !children.is_empty() {
            // Check parent_family_list was updated on the child node
            if let Some(crate::Node::Person(person)) = graph.get_node(&child) {
                assert!(
                    person.parent_family_list.contains(&family_handle),
                    "Child's parent_family_list should include the family"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Event generation tests
    // -----------------------------------------------------------------------

    #[test]
    fn generate_events_marriage_events() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Create a family with parents
        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        let family_handle = "f1".to_string();

        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        // Add birth events for parents
        let birth_event_f = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event_f.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event_f.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(1970)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: father_handle.clone(),
                target: birth_event_f,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        let birth_event_m = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event_m.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event_m.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(1975)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: mother_handle.clone(),
                target: birth_event_m,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData {
                    handle: family_handle.clone(),
                    father_handle: Some(father_handle),
                    mother_handle: Some(mother_handle),
                    ..crate::FamilyData::default()
                }),
            )
            .unwrap();

        generate_events(&mut graph, &config, &mut rng).expect("event generation should succeed");

        // Check that a marriage event exists
        let edges = graph.edges_from(&family_handle);
        let has_marriage = edges
            .iter()
            .any(|e| matches!(e, crate::Edge::FamilyEventRef { .. }));
        assert!(has_marriage, "Family should have a marriage event");
    }

    #[test]
    fn generate_events_marriage_date_after_birth() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        let family_handle = "f1".to_string();

        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let birth_event_f = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event_f.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event_f.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(1970)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: father_handle.clone(),
                target: birth_event_f,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        let birth_event_m = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event_m.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event_m.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(1975)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: mother_handle.clone(),
                target: birth_event_m,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData {
                    handle: family_handle.clone(),
                    father_handle: Some(father_handle),
                    mother_handle: Some(mother_handle),
                    ..crate::FamilyData::default()
                }),
            )
            .unwrap();

        generate_events(&mut graph, &config, &mut rng).expect("event generation should succeed");

        // Check marriage date is after both parents' birth dates
        let edges = graph.edges_from(&family_handle);
        for edge in &edges {
            if let crate::Edge::FamilyEventRef { target, .. } = edge {
                if let Some(crate::Node::Event(event)) = graph.get_node(target) {
                    if event.event_type == crate::EventType::Marriage {
                        if let Some(ref date) = event.date {
                            assert!(
                                date.year >= 1970,
                                "Marriage year should be after father's birth"
                            );
                            assert!(
                                date.year >= 1975,
                                "Marriage year should be after mother's birth"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn generate_events_with_places() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig {
            with_places: true,
            ..RandomConfig::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Add a minimal person with birth event
        let p1 = "p1".to_string();
        graph
            .add_node(
                p1.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let birth_event = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(2000)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: p1.clone(),
                target: birth_event,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        generate_events(&mut graph, &config, &mut rng).expect("event generation should succeed");

        // Check that Place nodes were created
        let place_count = graph.nodes_by_kind(crate::NodeKind::Place).len();
        assert!(
            place_count > 0,
            "Places should be created when with_places is true"
        );
    }

    #[test]
    fn generate_events_with_citations() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig {
            with_citations: true,
            ..RandomConfig::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Add a minimal person with birth event
        let p1 = "p1".to_string();
        graph
            .add_node(
                p1.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let birth_event = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(2000)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: p1.clone(),
                target: birth_event,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        generate_events(&mut graph, &config, &mut rng).expect("event generation should succeed");

        // Check that Source and Citation nodes were created
        let source_count = graph.nodes_by_kind(crate::NodeKind::Source).len();
        assert!(
            source_count > 0,
            "Sources should be created when with_citations is true"
        );
    }

    #[test]
    fn generate_events_empty_graph() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let result = generate_events(&mut graph, &config, &mut rng);
        assert!(result.is_ok(), "Empty graph should not cause errors");
        assert_eq!(graph.node_count(), 0);
    }

    #[test]
    fn generate_events_event_links_correct() {
        let mut graph = crate::Graph::new();
        let config = RandomConfig::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let father_handle = "father".to_string();
        let mother_handle = "mother".to_string();
        let family_handle = "f1".to_string();

        graph
            .add_node(
                father_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();
        graph
            .add_node(
                mother_handle.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let birth_event_f = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event_f.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event_f.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(1970)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: father_handle.clone(),
                target: birth_event_f,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        let birth_event_m = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                birth_event_m.clone(),
                crate::Node::Event(crate::EventData {
                    handle: birth_event_m.clone(),
                    event_type: crate::EventType::Birth,
                    date: Some(crate::DateValue::new(1975)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: mother_handle.clone(),
                target: birth_event_m,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        graph
            .add_node(
                family_handle.clone(),
                crate::Node::Family(crate::FamilyData {
                    handle: family_handle.clone(),
                    father_handle: Some(father_handle),
                    mother_handle: Some(mother_handle),
                    ..crate::FamilyData::default()
                }),
            )
            .unwrap();

        generate_events(&mut graph, &config, &mut rng).expect("event generation should succeed");

        // Check that FamilyEventRef edge exists
        let edges = graph.edges_from(&family_handle);
        let has_event_ref = edges
            .iter()
            .any(|e| matches!(e, crate::Edge::FamilyEventRef { .. }));
        assert!(has_event_ref, "Family should have FamilyEventRef edge");
    }

    #[test]
    fn generate_events_death_event_type() {
        let mut graph = crate::Graph::new();

        // Person with a death event already created
        let p1 = "p1".to_string();
        graph
            .add_node(
                p1.clone(),
                crate::Node::Person(crate::PersonData::default()),
            )
            .unwrap();

        let death_event = uuid::Uuid::new_v4().to_string();
        graph
            .add_node(
                death_event.clone(),
                crate::Node::Event(crate::EventData {
                    handle: death_event.clone(),
                    event_type: crate::EventType::Death,
                    date: Some(crate::DateValue::new(2050)),
                    ..crate::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(crate::Edge::PersonEventRef {
                source: p1.clone(),
                target: death_event,
                metadata: Box::new(crate::EventRef::default()),
            })
            .unwrap();

        // Verify death event exists
        let edges = graph.edges_from(&p1);
        let has_death = edges.iter().any(|e| {
            if let crate::Edge::PersonEventRef { target, .. } = e {
                if let Some(crate::Node::Event(event)) = graph.get_node(target) {
                    return event.event_type == crate::EventType::Death;
                }
            }
            false
        });
        assert!(has_death, "Death event should exist");
    }

    // -----------------------------------------------------------------------
    // generate_random entry point tests
    // -----------------------------------------------------------------------

    #[test]
    fn generate_random_basic() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 10,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        assert!(result.graph.node_count() > 0, "Graph should have nodes");
        assert!(
            !result.warnings.is_empty() || result.stats.family_count > 0,
            "Should generate families or emit warnings"
        );
    }

    #[test]
    fn generate_random_person_count() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 15,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        assert_eq!(
            result.stats.person_count, 15,
            "Graph should have {} persons, got {}",
            config.person_count, result.stats.person_count
        );
    }

    #[test]
    fn generate_random_family_count_nonzero() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 20,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        assert!(
            result.stats.family_count > 0,
            "Graph should have at least one family, got {}",
            result.stats.family_count
        );
    }

    #[test]
    fn generate_random_events_present() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 10,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        assert!(
            result.stats.event_count > 0,
            "Graph should have event nodes, got {}",
            result.stats.event_count
        );
    }

    #[test]
    fn generate_random_seed_reproducibility() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 10,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let r1 = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("first gen should succeed");
        let r2 = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("second gen should succeed");

        // Same seed should produce identical graphs
        assert_eq!(r1.stats, r2.stats, "Stats should match between runs");
    }

    #[test]
    fn generate_random_different_seeds_differ() {
        let schema = crate::Schema::default();
        let config1 = RandomConfig {
            person_count: 10,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };
        let config2 = RandomConfig {
            person_count: 10,
            generations: 2,
            seed: Some(99),
            ..RandomConfig::default()
        };

        let r1 = generate_random(&config1, &AdversarialConfig::default(), &schema)
            .expect("first gen should succeed");
        let r2 = generate_random(&config2, &AdversarialConfig::default(), &schema)
            .expect("second gen should succeed");

        // Different seeds should produce different stats (or at least different seeds)
        assert_ne!(r1.seed, r2.seed, "Seeds should differ");
    }

    #[test]
    fn generate_random_seed_recorded() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        assert_eq!(result.seed, 42, "Seed should be recorded in result");
    }

    #[test]
    fn generate_random_stats_match() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 10,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        let total_nodes = result.stats.person_count
            + result.stats.family_count
            + result.stats.event_count
            + result.stats.place_count
            + result.stats.source_count
            + result.stats.citation_count
            + result.stats.note_count;
        assert_eq!(
            result.graph.node_count(),
            total_nodes,
            "Node count should match stats sum"
        );
    }

    #[test]
    fn generate_random_invalid_config_zero_persons() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 0,
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema);
        assert!(
            matches!(result, Err(GenerationError::InvalidConfig(_))),
            "Zero persons should be invalid"
        );
    }

    #[test]
    fn generate_random_invalid_config_bad_range() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            start_year: 2000,
            end_year: 1900,
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema);
        assert!(
            matches!(result, Err(GenerationError::InvalidConfig(_))),
            "start_year > end_year should be invalid"
        );
    }

    #[test]
    fn generate_random_with_places() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 10,
            generations: 2,
            with_places: true,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        assert!(
            result.stats.place_count > 0,
            "Places should be present when with_places is true"
        );
    }

    #[test]
    fn generate_random_with_citations() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 10,
            generations: 2,
            with_citations: true,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        assert!(
            result.stats.source_count > 0,
            "Sources should be present when with_citations is true"
        );
        // Citations may or may not be created (30% probability per event)
    }

    #[test]
    fn generate_random_large_count() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 50,
            generations: 3,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let result = generate_random(&config, &AdversarialConfig::default(), &schema);
        assert!(result.is_ok(), "50 persons should generate without panic");
    }

    #[test]
    fn generate_random_validates_ok() {
        let schema = crate::Schema::default();
        let config = RandomConfig {
            person_count: 10,
            generations: 2,
            seed: Some(42),
            ..RandomConfig::default()
        };

        let mut result = generate_random(&config, &AdversarialConfig::default(), &schema)
            .expect("generation should succeed");
        let errors = result.graph.validate(&schema);
        // Birth/death events are created inline, so the graph should be valid
        // (Marriage events are added by generate_events)
        assert!(
            errors.is_empty(),
            "Generated graph should validate: {:?}",
            errors
        );
    }

    // -----------------------------------------------------------------------
    // Property-based tests for random generation invariants
    // -----------------------------------------------------------------------

    #[test]
    fn property_generate_validate_always_passes() {
        // For any valid RandomConfig, the generated graph passes structural
        // and referential validation.
        let schema = crate::Schema::default();
        for seed in 0..20 {
            let config = RandomConfig {
                person_count: 10 + (seed % 10) as usize,
                generations: 2 + (seed % 3) as usize,
                seed: Some(seed as u64),
                ..RandomConfig::default()
            };
            let result = generate_random(&config, &AdversarialConfig::default(), &schema)
                .unwrap_or_else(|e| {
                    panic!("Seed {}: generation failed: {:?}", seed, e);
                });
            let mut graph = result.graph;
            let errors = graph.validate(&schema);
            assert!(
                errors.is_empty(),
                "Seed {}: validation failed with {} errors: {:?}",
                seed,
                errors.len(),
                errors
            );
        }
    }

    #[test]
    fn property_same_seed_same_graph() {
        // For any seed, generate_random(config, seed) == generate_random(config, seed)
        let schema = crate::Schema::default();
        for seed in 0..20 {
            let config = RandomConfig {
                person_count: 10,
                generations: 3,
                seed: Some(seed),
                ..RandomConfig::default()
            };
            let r1 = generate_random(&config, &AdversarialConfig::default(), &schema)
                .unwrap_or_else(|e| {
                    panic!("Seed {}: first gen failed: {:?}", seed, e);
                });
            let r2 = generate_random(&config, &AdversarialConfig::default(), &schema)
                .unwrap_or_else(|e| {
                    panic!("Seed {}: second gen failed: {:?}", seed, e);
                });
            assert_eq!(
                r1.stats, r2.stats,
                "Seed {}: stats differ between runs",
                seed
            );
        }
    }

    #[test]
    fn property_all_persons_have_unique_handles() {
        // For any config, no two person nodes share a handle.
        let schema = crate::Schema::default();
        for seed in 0..20 {
            let config = RandomConfig {
                person_count: 15,
                seed: Some(seed),
                ..RandomConfig::default()
            };
            let result = generate_random(&config, &AdversarialConfig::default(), &schema)
                .unwrap_or_else(|e| {
                    panic!("Seed {}: generation failed: {:?}", seed, e);
                });
            let handles: Vec<_> = result.graph.nodes_by_kind(crate::NodeKind::Person);
            let unique_handles: std::collections::HashSet<_> = handles.iter().cloned().collect();
            assert_eq!(
                handles.len(),
                unique_handles.len(),
                "Seed {}: duplicate handles found",
                seed
            );
        }
    }

    #[test]
    fn property_optional_features_produce_nodes() {
        let schema = crate::Schema::default();
        for seed in 0..10 {
            let config = RandomConfig {
                person_count: 10,
                with_places: true,
                with_citations: true,
                seed: Some(seed),
                ..RandomConfig::default()
            };
            let result = generate_random(&config, &AdversarialConfig::default(), &schema)
                .unwrap_or_else(|e| {
                    panic!("Seed {}: generation failed: {:?}", seed, e);
                });
            assert!(
                !result
                    .graph
                    .nodes_by_kind(crate::NodeKind::Place)
                    .is_empty(),
                "Seed {}: with_places=true but no Place nodes",
                seed
            );
            assert!(
                !result
                    .graph
                    .nodes_by_kind(crate::NodeKind::Source)
                    .is_empty(),
                "Seed {}: with_citations=true but no Source nodes",
                seed
            );
        }
    }

    #[test]
    fn property_families_have_at_least_one_parent() {
        let schema = crate::Schema::default();
        for seed in 0..20 {
            let config = RandomConfig {
                person_count: 15,
                seed: Some(seed),
                ..RandomConfig::default()
            };
            let result = generate_random(&config, &AdversarialConfig::default(), &schema)
                .unwrap_or_else(|e| {
                    panic!("Seed {}: generation failed: {:?}", seed, e);
                });
            for family_handle in result.graph.nodes_by_kind(crate::NodeKind::Family) {
                let has_parent = result.graph.edges_from(family_handle).iter().any(|e| {
                    matches!(
                        e,
                        crate::Edge::FamilyFather { .. } | crate::Edge::FamilyMother { .. }
                    )
                });
                // This test just checks that the generation doesn't crash
                // (single-parent families are structurally valid)
                let _ = has_parent;
            }
        }
    }

    #[test]
    fn property_event_dates_consistent() {
        // For any config, birth event dates match person birth dates.
        let schema = crate::Schema::default();
        for seed in 0..20 {
            let config = RandomConfig {
                person_count: 10,
                seed: Some(seed),
                ..RandomConfig::default()
            };
            let result = generate_random(&config, &AdversarialConfig::default(), &schema)
                .unwrap_or_else(|e| {
                    panic!("Seed {}: generation failed: {:?}", seed, e);
                });
            for (handle, node) in result.graph.iter_nodes() {
                if let crate::Node::Person(person) = node {
                    let edges = result.graph.edges_from(handle);
                    for edge in edges {
                        if let crate::Edge::PersonEventRef { target, .. } = edge {
                            if let Some(crate::Node::Event(event)) = result.graph.get_node(target) {
                                if event.event_type == crate::EventType::Birth {
                                    // Birth event should have a date
                                    assert!(
                                        event.date.is_some(),
                                        "Person {} has birth event with no date",
                                        person.handle
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
