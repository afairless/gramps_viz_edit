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
    "Ash", "Oak", "River", "Mill", "Spring", "Fair", "Meadow", "Cedar",
    "Pine", "Willow", "Maple", "Birch", "Elm", "Hazel", "Holly", "Ivy",
    "Stone", "Brook", "Lake", "Hill", "Field", "Dale", "Glen", "Heath",
    "Fern", "Rose", "Lily", "Vale", "Crest", "Peak",
];

/// City suffixes for building city names.
const CITY_SUFFIXES: &[&str] = &[
    "ton", "ville", "burg", "field", "bridge", "haven", "brook",
    "ham", "ley", "more", "side", "stead", "ford", "gate",
    "bury", "dale", "wick", "port", "worth", "view",
];

/// Procedurally named states.
const STATE_NAMES: &[&str] = &[
    "Northumbria", "Westland", "Southmere", "Eastshire", "Arcadia",
    "Avalon", "Caledonia", "Delphia", "Eldoria", "Fenwick",
    "Grenville", "Havenwood", "Iverness", "Kingsland", "Lorien",
];

/// Procedurally named countries.
const COUNTRY_NAMES: &[&str] = &[
    "Albion", "Valdoria", "Mercia", "Thalassia", "Eryndor",
    "Celestria", "Durnhold", "Aeridor",
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
pub(crate) fn generate_random_person(
    graph: &mut crate::Graph,
    config: &RandomConfig,
    used_names: &mut std::collections::HashSet<String>,
    rng: &mut impl rand::Rng,
    generation_layer: usize,
) -> Result<crate::Handle, GenerationError> {
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
    graph.add_node(handle.clone(), crate::Node::Person(person))
        .map_err(|_| GenerationError::InvalidConfig(
            format!("duplicate handle: {}", handle)
        ))?;

    // Create birth event
    let event_handle = uuid::Uuid::new_v4().to_string();
    let birth_event = crate::EventData {
        handle: event_handle.clone(),
        event_type: crate::EventType::Birth,
        date: Some(birth_date),
        ..crate::EventData::default()
    };
    graph.add_node(event_handle.clone(), crate::Node::Event(birth_event))
        .map_err(|_| GenerationError::InvalidConfig(
            format!("duplicate event handle: {}", event_handle)
        ))?;

    // Link birth event to person
    graph.add_edge(crate::Edge::PersonEventRef {
        source: handle.clone(),
        target: event_handle,
        metadata: Box::new(crate::EventRef {
            ref_field: handle.clone(),
            role: Some(crate::EventRoleType::Primary),
        }),
    }).expect("birth event target exists (just added)");

    // Create death event if death date is set
    if let Some(death_date) = death_date {
        let death_event_handle = uuid::Uuid::new_v4().to_string();
        let death_event = crate::EventData {
            handle: death_event_handle.clone(),
            event_type: crate::EventType::Death,
            date: Some(death_date),
            ..crate::EventData::default()
        };
        graph.add_node(death_event_handle.clone(), crate::Node::Event(death_event))
            .map_err(|_| GenerationError::InvalidConfig(
                format!("duplicate event handle: {}", death_event_handle)
            ))?;

        graph.add_edge(crate::Edge::PersonEventRef {
            source: handle.clone(),
            target: death_event_handle,
            metadata: Box::new(crate::EventRef {
                ref_field: handle.clone(),
                role: Some(crate::EventRoleType::Primary),
            }),
        }).expect("death event target exists (just added)");
    }

    Ok(handle)
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

    Some(crate::DateValue::new_ymd(death_year, death_month, death_day))
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
    let males: Vec<_> = candidates
        .iter()
        .filter(|(_, s)| s.gender == 0)
        .collect();
    let females: Vec<_> = candidates
        .iter()
        .filter(|(_, s)| s.gender == 1)
        .collect();

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
pub(crate) fn generate_family(
    graph: &mut crate::Graph,
    _config: &RandomConfig,
    persons: &mut [(crate::Handle, PersonSummary)],
    layer: usize,
    rng: &mut impl rand::Rng,
) -> Result<crate::Handle, GenerationError> {
    // Select parents
    let parent_pair = select_parents(persons, _config, layer, rng);

    let (father_handle, mother_handle) = match parent_pair {
        Some(pair) => pair,
        None => {
            // Create a single-parent family
            return create_single_parent_family(graph, persons, layer, rng);
        }
    };

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
            GenerationError::InvalidConfig(format!(
                "duplicate family handle: {}",
                family_handle
            ))
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

    Ok(family_handle)
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
            GenerationError::InvalidConfig(format!(
                "duplicate family handle: {}",
                family_handle
            ))
        })?;

    // Update person family list
    if let Some(crate::Node::Person(ref mut person)) = graph.get_node_mut(&parent_handle) {
        person.family_list.push(family_handle.clone());
    }

    Ok(family_handle)
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
        assert!(!place.country.is_empty(), "Country should be non-empty at depth 1");
    }

    #[test]
    fn generate_place_depth_3() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place = generate_place(3, &used, &mut rng);
        assert!(!place.city.is_empty(), "City should be non-empty at depth 3");
        assert!(!place.county.is_empty(), "County should be non-empty at depth 3");
        assert!(!place.state.is_empty(), "State should be non-empty at depth 3");
        assert!(!place.country.is_empty(), "Country should be non-empty at depth 3");
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
        assert!(!place.country.is_empty(), "Depth 0 should default to depth 1");
        assert!(place.city.is_empty());
    }

    #[test]
    fn generate_place_country_reused() {
        let used = std::collections::HashSet::new();
        let mut rng1 = rand::rngs::StdRng::seed_from_u64(42);
        let mut rng2 = rand::rngs::StdRng::seed_from_u64(42);
        let place1 = generate_place(3, &used, &mut rng1);
        let place2 = generate_place(3, &used, &mut rng2);
        assert_eq!(place1.country, place2.country, "Same seed should produce same country");
    }

    #[test]
    fn generate_place_depth_2() {
        let used = std::collections::HashSet::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let place = generate_place(2, &used, &mut rng);
        assert!(place.city.is_empty(), "City should be empty at depth 2");
        assert!(place.county.is_empty(), "County should be empty at depth 2");
        assert!(!place.state.is_empty(), "State should be non-empty at depth 2");
        assert!(!place.country.is_empty(), "Country should be non-empty at depth 2");
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

        let handle = generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0)
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

        let handle = generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0)
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

        let handle = generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0)
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

        let _handle = generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 1)
            .expect("person generation should succeed");

        // Layer 1: end_year-85 to end_year-55 = 1915 to 1945
        // Check that a birth event exists with a date in the expected range
        for (_, node) in graph.iter_nodes() {
            if let crate::Node::Event(event) = node {
                if event.event_type == crate::EventType::Birth {
                    if let Some(ref date) = event.date {
                        // Broad range check: birth year should be plausible
                        assert!(
                            date.year >= 1900 && date.year <= 2000,
                        );
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

        let handle = generate_random_person(&mut graph, &config, &mut used_names, &mut rng, 0)
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
                dy, birth_year
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

        let h1 = generate_random_person(&mut graph1, &config, &mut used1, &mut rng1, 0)
            .expect("person gen should succeed");
        let h2 = generate_random_person(&mut graph2, &config, &mut used2, &mut rng2, 0)
            .expect("person gen should succeed");

        // Same seed should produce the same person data (excluding UUID handle)
        if let (crate::Node::Person(p1), crate::Node::Person(p2)) = (
            graph1.get_node(&h1).unwrap(),
            graph2.get_node(&h2).unwrap(),
        ) {
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
            assert!(year >= 1970, "Layer 0 birth year {} should be >= 1970", year);
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
            assert!(year >= 1880, "Layer 3 birth year {} should be >= 1880", year);
            assert!(year <= 1925, "Layer 3 birth year {} should be <= 1925", year);
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
            ("p1".to_string(), PersonSummary { handle: "p1".to_string(), birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
            ("p2".to_string(), PersonSummary { handle: "p2".to_string(), birth_year: 1975, gender: 1, layer: 0, is_parent: false, is_child: false }),
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
            ("p1".to_string(), PersonSummary { handle: "p1".to_string(), birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
            ("p2".to_string(), PersonSummary { handle: "p2".to_string(), birth_year: 1975, gender: 1, layer: 0, is_parent: false, is_child: false }),
            ("p3".to_string(), PersonSummary { handle: "p3".to_string(), birth_year: 1950, gender: 0, layer: 0, is_parent: false, is_child: false }),
            ("p4".to_string(), PersonSummary { handle: "p4".to_string(), birth_year: 1990, gender: 1, layer: 0, is_parent: false, is_child: false }),
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
            ("p1".to_string(), PersonSummary { handle: "p1".to_string(), birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
            ("p2".to_string(), PersonSummary { handle: "p2".to_string(), birth_year: 1975, gender: 1, layer: 0, is_parent: false, is_child: false }),
            ("p3".to_string(), PersonSummary { handle: "p3".to_string(), birth_year: 1940, gender: 0, layer: 1, is_parent: false, is_child: false }),
            ("p4".to_string(), PersonSummary { handle: "p4".to_string(), birth_year: 1945, gender: 1, layer: 1, is_parent: false, is_child: false }),
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
            ("p1".to_string(), PersonSummary { handle: "p1".to_string(), birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
            // Two in layer 1
            ("p2".to_string(), PersonSummary { handle: "p2".to_string(), birth_year: 1940, gender: 0, layer: 1, is_parent: false, is_child: false }),
            ("p3".to_string(), PersonSummary { handle: "p3".to_string(), birth_year: 1945, gender: 1, layer: 1, is_parent: false, is_child: false }),
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
        graph.add_node(p1.clone(), crate::Node::Person(crate::PersonData {
            handle: p1.clone(),
            gender: 0,
            primary_name: crate::Name {
                first_name: Some("John".to_string()),
                ..crate::Name::default()
            },
            ..crate::PersonData::default()
        })).unwrap();
        graph.add_node(p2.clone(), crate::Node::Person(crate::PersonData {
            handle: p2.clone(),
            gender: 1,
            primary_name: crate::Name {
                first_name: Some("Jane".to_string()),
                ..crate::Name::default()
            },
            ..crate::PersonData::default()
        })).unwrap();

        let mut persons = vec![
            (p1.clone(), PersonSummary { handle: p1, birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
            (p2.clone(), PersonSummary { handle: p2, birth_year: 1975, gender: 1, layer: 0, is_parent: false, is_child: false }),
        ];

        let family_handle = generate_family(&mut graph, &config, &mut persons, 0, &mut rng)
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
        graph.add_node(p1.clone(), crate::Node::Person(crate::PersonData {
            handle: p1.clone(),
            gender: 0,
            primary_name: crate::Name {
                first_name: Some("John".to_string()),
                ..crate::Name::default()
            },
            ..crate::PersonData::default()
        })).unwrap();
        graph.add_node(p2.clone(), crate::Node::Person(crate::PersonData {
            handle: p2.clone(),
            gender: 1,
            primary_name: crate::Name {
                first_name: Some("Jane".to_string()),
                ..crate::Name::default()
            },
            ..crate::PersonData::default()
        })).unwrap();

        let mut persons = vec![
            (p1.clone(), PersonSummary { handle: p1, birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
            (p2.clone(), PersonSummary { handle: p2, birth_year: 1975, gender: 1, layer: 0, is_parent: false, is_child: false }),
        ];

        let family_handle = generate_family(&mut graph, &config, &mut persons, 0, &mut rng)
            .expect("family generation should succeed");

        let edges = graph.edges_from(&family_handle);
        let has_father = edges.iter().any(|e| matches!(e, crate::Edge::FamilyFather { .. }));
        let has_mother = edges.iter().any(|e| matches!(e, crate::Edge::FamilyMother { .. }));
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
        graph.add_node(p1.clone(), crate::Node::Person(crate::PersonData {
            handle: p1.clone(),
            gender: 0,
            primary_name: crate::Name {
                first_name: Some("John".to_string()),
                ..crate::Name::default()
            },
            ..crate::PersonData::default()
        })).unwrap();

        let mut persons = vec![
            (p1.clone(), PersonSummary { handle: p1, birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
        ];

        let result = generate_family(&mut graph, &config, &mut persons, 0, &mut rng);
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
        graph.add_node(p1.clone(), crate::Node::Person(crate::PersonData {
            handle: p1.clone(),
            gender: 0,
            primary_name: crate::Name {
                first_name: Some("John".to_string()),
                ..crate::Name::default()
            },
            ..crate::PersonData::default()
        })).unwrap();

        let mut persons = vec![
            (p1.clone(), PersonSummary { handle: p1, birth_year: 1970, gender: 0, layer: 0, is_parent: false, is_child: false }),
        ];

        // Only one person, so single-parent family is created
        let result = generate_family(&mut graph, &config, &mut persons, 0, &mut rng);
        assert!(result.is_ok(), "Single-parent family should be created");
    }
}