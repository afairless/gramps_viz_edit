//! YAML scenario file parsing for generation configuration.
//!
//! This module provides types for loading generation configuration from
//! YAML scenario files, following the schema in design §7.5.

use serde::Deserialize;

/// A scenario configuration loaded from a YAML file.
#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub name: Option<String>,
    pub person_count: Option<usize>,
    pub family_count: Option<usize>,
    pub family_ratio: Option<f64>,
    pub max_parent_roles: Option<usize>,
    pub generations: Option<GenerationsConfig>,
    pub date_range: Option<DateRangeConfig>,
    pub with_citations: Option<bool>,
    pub with_places: Option<bool>,
    pub with_media: Option<bool>,
    pub with_notes: Option<bool>,
    pub with_tags: Option<bool>,
    pub seed: Option<u64>,
    pub adversarial: Option<AdversarialScenarioConfig>,
    pub densify: Option<DensifyScenarioConfig>,
}

/// Generation depth configuration.
#[derive(Debug, Deserialize)]
pub struct GenerationsConfig {
    pub depth: Option<usize>,
    pub children_per_family: Option<ChildrenRange>,
}

/// Range for children per family.
#[derive(Debug, Deserialize)]
pub struct ChildrenRange {
    pub min: usize,
    pub max: usize,
}

/// Date range configuration.
#[derive(Debug, Deserialize)]
pub struct DateRangeConfig {
    pub start: Option<i32>,
    pub end: Option<i32>,
    pub era: Option<String>,
}

/// Adversarial generation configuration.
#[derive(Debug, Deserialize)]
pub struct AdversarialScenarioConfig {
    pub enabled: Option<bool>,
    pub strategies: Option<Vec<String>>,
}

/// Densification configuration in a scenario file.
#[derive(Debug, Deserialize)]
pub struct DensifyScenarioConfig {
    pub enabled: Option<bool>,
    pub target_connectivity: Option<f64>,
    pub max_parent_roles: Option<usize>,
}

/// Errors that can occur during scenario file loading.
#[derive(Debug)]
pub enum ScenarioError {
    /// I/O error reading the file.
    Io {
        path: String,
        source: std::io::Error,
    },
    /// YAML parse error.
    ParseError {
        path: String,
        source: serde_yaml::Error,
    },
}

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioError::Io { path, source } => {
                write!(f, "I/O error reading scenario '{}': {}", path, source)
            }
            ScenarioError::ParseError { path, source } => {
                write!(f, "parse error in scenario '{}': {}", path, source)
            }
        }
    }
}

impl std::error::Error for ScenarioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScenarioError::Io { source, .. } => Some(source),
            ScenarioError::ParseError { source, .. } => Some(source),
        }
    }
}

/// Load a scenario from a YAML file path.
pub fn load_scenario(path: &str) -> Result<Scenario, ScenarioError> {
    let file = std::fs::File::open(path).map_err(|e| ScenarioError::Io {
        path: path.to_string(),
        source: e,
    })?;
    let reader = std::io::BufReader::new(file);
    serde_yaml::from_reader(reader).map_err(|e| ScenarioError::ParseError {
        path: path.to_string(),
        source: e,
    })
}

/// Convert a strategy name string to an `AdversarialStrategy` with default parameters.
impl Scenario {
    /// Convert this scenario to a `RandomConfig`, using defaults for unset fields.
    pub fn to_random_config(&self) -> typed_graph::generate::RandomConfig {
        let base = typed_graph::generate::RandomConfig::default();
        typed_graph::generate::RandomConfig {
            person_count: self.person_count.unwrap_or(base.person_count),
            family_count: self.family_count.unwrap_or(base.family_count),
            family_ratio: self.family_ratio.unwrap_or(base.family_ratio),
            max_parent_roles: self.max_parent_roles.unwrap_or(base.max_parent_roles),
            layer_linking: self
                .generations
                .as_ref()
                .map(|g| g.depth.unwrap_or(base.generations))
                .unwrap_or(base.generations)
                > 1,
            generations: self
                .generations
                .as_ref()
                .map(|g| g.depth.unwrap_or(base.generations))
                .unwrap_or(base.generations),
            children_per_family: self
                .generations
                .as_ref()
                .and_then(|g| g.children_per_family.as_ref())
                .map(|r| r.min..r.max)
                .unwrap_or(base.children_per_family),
            start_year: self
                .date_range
                .as_ref()
                .map(|d| d.start.unwrap_or(base.start_year))
                .unwrap_or(base.start_year),
            end_year: self
                .date_range
                .as_ref()
                .map(|d| d.end.unwrap_or(base.end_year))
                .unwrap_or(base.end_year),
            name_style: self
                .date_range
                .as_ref()
                .and_then(|d| d.era.clone())
                .unwrap_or(base.name_style),
            with_places: self.with_places.unwrap_or(base.with_places),
            with_citations: self.with_citations.unwrap_or(base.with_citations),
            with_notes: self.with_notes.unwrap_or(base.with_notes),
            with_media: self.with_media.unwrap_or(base.with_media),
            with_tags: self.with_tags.unwrap_or(base.with_tags),
            seed: self.seed,
            place_depth: base.place_depth,
        }
    }

    /// Convert this scenario to an `AdversarialConfig`.
    pub fn to_adversarial_config(&self) -> typed_graph::generate::AdversarialConfig {
        let base = typed_graph::generate::AdversarialConfig::default();
        if let Some(ref adv) = self.adversarial {
            if adv.enabled.unwrap_or(false) {
                let strategies = adv
                    .strategies
                    .as_ref()
                    .map(|strat_list| {
                        strat_list
                            .iter()
                            .filter_map(|s| typed_graph::generate::AdversarialStrategy::from_name(s))
                            .collect()
                    })
                    .unwrap_or_default();
                return typed_graph::generate::AdversarialConfig {
                    enabled: true,
                    strategies,
                };
            }
        }
        base
    }

    /// Convert this scenario's densify section to a `DensifyConfig`.
    ///
    /// Returns `None` when the `densify` section is absent or has
    /// `enabled: false`.
    pub fn to_densify_config(&self) -> Option<typed_graph::generate::densify::DensifyConfig> {
        let densify = self.densify.as_ref()?;
        if !densify.enabled.unwrap_or(true) {
            return None;
        }
        let base = typed_graph::generate::densify::DensifyConfig::default();
        Some(typed_graph::generate::densify::DensifyConfig {
            target_connectivity: densify
                .target_connectivity
                .unwrap_or(base.target_connectivity),
            max_parent_roles: densify.max_parent_roles.unwrap_or(base.max_parent_roles),
            ..base
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_parse_full_yaml() {
        let yaml = r#"
name: "test"
person_count: 50
family_count: 20
generations:
  depth: 3
  children_per_family: { min: 1, max: 4 }
date_range:
  start: 1850
  end: 2025
  era: modern
with_citations: true
with_places: true
with_media: false
seed: 42
adversarial:
  enabled: true
  strategies:
    - disconnected
"#;
        let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(scenario.person_count, Some(50));
        assert_eq!(scenario.family_count, Some(20));
        assert_eq!(scenario.seed, Some(42));
        assert!(scenario.with_places.unwrap());
        assert!(scenario.adversarial.unwrap().enabled.unwrap());
    }

    #[test]
    fn scenario_parse_minimal_yaml() {
        let yaml = "person_count: 10\n";
        let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(scenario.person_count, Some(10));
        assert!(scenario.with_places.is_none());
    }

    #[test]
    fn scenario_parse_invalid_yaml() {
        let yaml = "person_count: not_a_number\n";
        let result: Result<Scenario, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn scenario_file_not_found() {
        let result = load_scenario("/nonexistent/scenario.yaml");
        match result {
            Err(ScenarioError::Io { .. }) => {} // Expected
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn scenario_to_random_config_full() {
        let yaml = r#"
person_count: 100
family_count: 40
generations:
  depth: 4
  children_per_family: { min: 2, max: 5 }
date_range:
  start: 1800
  end: 2000
  era: victorian
with_citations: true
with_places: true
with_media: true
with_notes: true
with_tags: true
seed: 123
"#;
        let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
        let config = scenario.to_random_config();
        assert_eq!(config.person_count, 100);
        assert_eq!(config.family_count, 40);
        assert_eq!(config.generations, 4);
        assert_eq!(config.children_per_family, 2..5);
        assert_eq!(config.start_year, 1800);
        assert_eq!(config.end_year, 2000);
        assert_eq!(config.name_style, "victorian");
        assert!(config.with_places);
        assert!(config.with_citations);
        assert!(config.with_notes);
        assert!(config.with_media);
        assert!(config.with_tags);
        assert_eq!(config.seed, Some(123));
    }

    #[test]
    fn scenario_to_random_config_defaults() {
        let yaml = "person_count: 10\n";
        let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
        let config = scenario.to_random_config();
        let base = typed_graph::generate::RandomConfig::default();
        // person_count should be overridden
        assert_eq!(config.person_count, 10);
        // Everything else should use defaults
        assert_eq!(config.generations, base.generations);
        assert_eq!(config.start_year, base.start_year);
        assert_eq!(config.end_year, base.end_year);
        assert_eq!(config.with_places, base.with_places);
        assert_eq!(config.family_ratio, base.family_ratio);
        assert_eq!(config.max_parent_roles, base.max_parent_roles);
    }

    #[test]
    fn scenario_to_adversarial_config_enabled() {
        let yaml = r#"
adversarial:
  enabled: true
  strategies:
    - disconnected
    - one_parent
"#;
        let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
        let config = scenario.to_adversarial_config();
        assert!(config.enabled);
    }

    #[test]
    fn scenario_to_adversarial_config_disabled() {
        let yaml = "person_count: 10\n";
        let scenario: Scenario = serde_yaml::from_str(yaml).unwrap();
        let config = scenario.to_adversarial_config();
        assert!(!config.enabled);
    }
}
