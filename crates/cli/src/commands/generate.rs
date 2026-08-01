//! Generate command — wire generation, validation, and serialization.
//!
//! This module implements the `generate` subcommand for the gramps-gen CLI.
//! It follows the five-stage pipeline:
//!
//! Generate → Validate → [Adversarial Transform] → Validate → Serialize

use clap::Args;
use output::GraphXmlWriter;
use output::SerializationMap;
use typed_graph::generate::AdversarialConfig;
use typed_graph::generate::AdversarialStrategy;
use typed_graph::generate::RandomConfig;
use typed_graph::generate::{apply_adversarial_strategies, generate_random};
use typed_graph::Schema;
use typed_graph::ValidationError;

/// Arguments for the `generate` subcommand.
#[derive(Args, Clone, Debug)]
pub struct GenerateArgs {
    /// Number of persons to generate
    #[arg(short = 'n', long, default_value = "200")]
    pub count: usize,

    /// Number of generations
    #[arg(short = 'd', long, default_value = "3")]
    pub depth: usize,

    /// Output .gramps file
    #[arg(short = 'o', long, default_value = "output.gramps")]
    pub output: String,

    /// RNG seed for reproducible generation
    #[arg(long)]
    pub seed: Option<u64>,

    /// Promote plausibility warnings to errors
    #[arg(long)]
    pub strict: bool,

    /// Comma-separated adversarial strategies, or "all"
    #[arg(long)]
    pub adversarial: Option<String>,

    /// How often to report generation progress
    #[arg(long, default_value = "100")]
    pub progress_interval: usize,

    /// YAML scenario file (overrides other options)
    #[arg(short = 'c', long)]
    pub config: Option<String>,

    /// Generate Place nodes
    #[arg(long)]
    pub with_places: bool,

    /// Generate Citation and Source nodes
    #[arg(long)]
    pub with_citations: bool,

    /// Generate Note nodes
    #[arg(long)]
    pub with_notes: bool,

    /// Generate Media objects
    #[arg(long)]
    pub with_media: bool,

    /// Generate Tag nodes
    #[arg(long)]
    pub with_tags: bool,

    /// Schema version to use (e.g., "5.1", "5.2"). Default: highest installed.
    #[arg(long, default_value = "default")]
    pub schema_version: String,
}

/// Run the generate command with the full five-stage pipeline.
pub fn run(args: GenerateArgs) -> Result<(), crate::error::CliError> {
    // Stage 0: Build config
    let (config, adversarial_config, output_path) = build_config(&args)?;

    // Report seed
    let seed_msg = match config.seed {
        Some(s) => format!("{}", s),
        None => "random".to_string(),
    };
    eprintln!("Generation seed: {}", seed_msg);

    // Create progress reporter
    let progress =
        crate::progress::ProgressReporter::new(args.progress_interval, config.person_count);
    eprintln!(
        "Generating {} persons across {} generations...",
        config.person_count, config.generations
    );

    // Stage 0: Resolve schema version
    let schema_version = if args.schema_version == "default" {
        Schema::default_version().to_string()
    } else {
        // Validate that the requested version is available
        if Schema::for_version(&args.schema_version).is_none() {
            return Err(crate::error::CliError::ConfigError(format!(
                "schema version {} is not available in this build.\n  Available versions: {}\n  Rebuild with: cargo build --features schema-{}",
                args.schema_version,
                Schema::available_versions().join(", "),
                args.schema_version.replace('.', "-")
            )));
        }
        args.schema_version.clone()
    };
    let schema = Schema::for_version(&schema_version).expect("schema version was validated above");

    // Map schema version to full Gramps version for XML header
    let gramps_version: String = match schema_version.as_str() {
        "5.0" => "5.0.2".to_string(),
        "5.1" => "5.1.6".to_string(),
        "5.2" => "5.2.0".to_string(),
        "6.0" => "6.0.0".to_string(),
        _ => {
            // For unknown versions, derive a full version
            format!("{}.0", schema_version)
        }
    };

    // Stage 1: Generate
    let mut result = generate_random(&config, &adversarial_config, schema)?;
    progress.finish();
    eprintln!(
        "Generated {} persons, {} families, {} events",
        result.stats.person_count, result.stats.family_count, result.stats.event_count
    );

    // Stage 2: Validation Gate 1
    let errors = result.graph.validate(schema);
    check_validation_errors(&errors, args.strict)?;

    // Stage 3: Adversarial Transform (Category B only)
    if adversarial_config.enabled {
        let adversarial_result = apply_adversarial_strategies(result.graph, &adversarial_config);
        result.graph = adversarial_result.graph;
        for err in &adversarial_result.errors {
            eprintln!("Adversarial transform warning: {}", err);
        }
    }

    // Stage 4: Validation Gate 2
    let errors = result.graph.validate(schema);
    check_validation_errors(&errors, args.strict)?;

    // Stage 5: Serialize
    let map = SerializationMap::new();
    let writer = GraphXmlWriter::new(map, &gramps_version);
    let file = std::fs::File::create(&output_path).map_err(|e| crate::error::CliError::Io {
        path: output_path.clone(),
        source: e,
    })?;
    writer.write(&result.graph, &mut std::io::BufWriter::new(file))?;

    // Report summary
    eprintln!(
        "Generated {} persons, {} families, {} events ({} edges) → {}",
        result.stats.person_count,
        result.stats.family_count,
        result.stats.event_count,
        result.stats.edge_count,
        output_path
    );

    // Report warnings
    for warning in &result.warnings {
        eprintln!("Warning: {}", warning);
    }

    Ok(())
}

/// Check validation errors and return an error if they are blocking.
fn check_validation_errors(
    errors: &[ValidationError],
    strict: bool,
) -> Result<(), crate::error::CliError> {
    if errors.is_empty() {
        return Ok(());
    }

    if strict {
        // In strict mode, ALL errors are blocking
        for error in errors {
            eprintln!("{}", error);
        }
        return Err(crate::error::CliError::ValidationFailed(errors.to_vec()));
    }

    // In non-strict mode, only structural/referential errors are blocking
    let blocking_errors: Vec<ValidationError> = errors
        .iter()
        .filter(|e| !matches!(e, ValidationError::PlausibilityWarning { .. }))
        .cloned()
        .collect();

    if !blocking_errors.is_empty() {
        for error in &blocking_errors {
            eprintln!("{}", error);
        }
        return Err(crate::error::CliError::ValidationFailed(blocking_errors));
    }

    // Plausibility warnings are reported but non-blocking
    for error in errors {
        if matches!(error, ValidationError::PlausibilityWarning { .. }) {
            eprintln!("Warning: {}", error);
        }
    }

    Ok(())
}

/// Build configuration from CLI args or scenario file.
fn build_config(
    args: &GenerateArgs,
) -> Result<(RandomConfig, AdversarialConfig, String), crate::error::CliError> {
    // If a config file is specified, load from YAML
    if let Some(ref config_path) = args.config {
        let scenario = crate::scenario::load_scenario(config_path)?;
        let output_path = args.output.clone();
        return Ok((
            scenario.to_random_config(),
            scenario.to_adversarial_config(),
            output_path,
        ));
    }

    // Build from CLI args
    let config = RandomConfig {
        person_count: args.count,
        family_count: args.count / 2,
        generations: args.depth,
        children_per_family: 1..4,
        start_year: 1850,
        end_year: 2025,
        name_style: "modern".to_string(),
        with_places: args.with_places,
        with_citations: args.with_citations,
        with_notes: args.with_notes,
        with_media: args.with_media,
        with_tags: args.with_tags,
        seed: args.seed,
        place_depth: 3,
    };

    // Parse adversarial flag
    let adversarial_config = parse_adversarial_flag(&args.adversarial)?;

    Ok((config, adversarial_config, args.output.clone()))
}

/// Parse the `--adversarial` flag value into an `AdversarialConfig`.
fn parse_adversarial_flag(
    flag: &Option<String>,
) -> Result<AdversarialConfig, crate::error::CliError> {
    let flag = match flag {
        Some(f) => f,
        None => {
            return Ok(AdversarialConfig::default());
        }
    };

    if flag == "all" {
        return Ok(AdversarialConfig {
            enabled: true,
            strategies: vec![
                AdversarialStrategy::OneParentFamilies(0.5),
                AdversarialStrategy::MissingEvents(0.3),
                AdversarialStrategy::SoloPersons(0.2),
                AdversarialStrategy::ManyAlternateNames(0.3),
                AdversarialStrategy::DisconnectedSubgraphs,
                AdversarialStrategy::DeepNesting,
                AdversarialStrategy::MaxRefChains,
                AdversarialStrategy::OrphanedReferences,
                AdversarialStrategy::DoubleGender(0.2),
            ],
        });
    }

    let strategies: Result<Vec<AdversarialStrategy>, _> = flag
        .split(',')
        .map(|s| {
            let s = s.trim();
            strategy_from_name(s).ok_or_else(|| {
                crate::error::CliError::ConfigError(format!(
                    "unknown adversarial strategy: '{}'. Valid strategies: one-parent, missing-events, \
                     solo, many-names, disconnected, deep-nesting, max-ref-chains, orphaned, double-gender",
                    s
                ))
            })
        })
        .collect();

    let strategies = strategies?;
    if strategies.is_empty() {
        return Err(crate::error::CliError::ConfigError(
            "adversarial flag is set but no valid strategies were specified".to_string(),
        ));
    }

    Ok(AdversarialConfig {
        enabled: true,
        strategies,
    })
}

/// Convert a strategy name string to an `AdversarialStrategy`.
fn strategy_from_name(name: &str) -> Option<AdversarialStrategy> {
    match name {
        "one_parent" | "one-parent" | "one_parent_families" => {
            Some(AdversarialStrategy::OneParentFamilies(0.5))
        }
        "missing_events" | "missing-events" => Some(AdversarialStrategy::MissingEvents(0.3)),
        "solo" | "solo_persons" | "solo-persons" => Some(AdversarialStrategy::SoloPersons(0.2)),
        "many_names" | "many-names" | "many_alternate_names" => {
            Some(AdversarialStrategy::ManyAlternateNames(0.3))
        }
        "disconnected" | "disconnected_subgraphs" | "disconnected-subgraphs" => {
            Some(AdversarialStrategy::DisconnectedSubgraphs)
        }
        "deep_nesting" | "deep-nesting" => Some(AdversarialStrategy::DeepNesting),
        "max_ref_chains" | "max-ref-chains" => Some(AdversarialStrategy::MaxRefChains),
        "orphaned" | "orphaned_references" | "orphaned-references" => {
            Some(AdversarialStrategy::OrphanedReferences)
        }
        "double_gender" | "double-gender" => Some(AdversarialStrategy::DoubleGender(0.2)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typed_graph::generate::GenerationError;

    #[test]
    fn generate_command_build_config_from_args() {
        let args = GenerateArgs {
            count: 50,
            depth: 3,
            output: "test.gramps".to_string(),
            seed: Some(42),
            strict: false,
            adversarial: None,
            progress_interval: 100,
            config: None,
            with_places: true,
            with_citations: false,
            with_notes: false,
            with_media: false,
            with_tags: false,
            schema_version: "default".to_string(),
        };
        let (config, adv_config, output) = build_config(&args).unwrap();
        assert_eq!(config.person_count, 50);
        assert_eq!(config.generations, 3);
        assert_eq!(config.seed, Some(42));
        assert!(config.with_places);
        assert!(!config.with_citations);
        assert!(!adv_config.enabled);
        assert_eq!(output, "test.gramps");
    }

    #[test]
    fn generate_command_adversarial_flag_parses_all() {
        let config = parse_adversarial_flag(&Some("all".to_string())).unwrap();
        assert!(config.enabled);
        assert!(!config.strategies.is_empty());
    }

    #[test]
    fn generate_command_adversarial_flag_parses_list() {
        let config = parse_adversarial_flag(&Some("disconnected,one-parent".to_string())).unwrap();
        assert!(config.enabled);
        assert_eq!(config.strategies.len(), 2);
    }

    #[test]
    fn generate_command_adversarial_flag_unknown_rejected() {
        let result = parse_adversarial_flag(&Some("unknown_strategy".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn generate_command_empty_output_path() {
        let args = GenerateArgs {
            count: 50,
            depth: 3,
            output: "".to_string(),
            seed: None,
            strict: false,
            adversarial: None,
            progress_interval: 100,
            config: None,
            with_places: false,
            with_citations: false,
            with_notes: false,
            with_media: false,
            with_tags: false,
            schema_version: "default".to_string(),
        };
        // build_config should succeed even with empty output (it's just a string)
        let result = build_config(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn generate_command_zero_count_rejected() {
        let args = GenerateArgs {
            count: 0,
            depth: 3,
            output: "test.gramps".to_string(),
            seed: None,
            strict: false,
            adversarial: None,
            progress_interval: 100,
            config: None,
            with_places: false,
            with_citations: false,
            with_notes: false,
            with_media: false,
            with_tags: false,
            schema_version: "default".to_string(),
        };
        let (config, _, _) = build_config(&args).unwrap();
        // person_count is 0, generation should fail
        let schema = Schema::default();
        let adv_config = AdversarialConfig::default();
        let result = generate_random(&config, &adv_config, &schema);
        assert!(result.is_err());
        match result {
            Err(GenerationError::InvalidConfig(_)) => {} // Expected
            _ => panic!("Expected InvalidConfig error"),
        }
    }
}
