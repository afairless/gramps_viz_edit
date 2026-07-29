//! Adversarial generation strategies for producing unusual, boundary, or
//! deliberately invalid graph structures for testing downstream tools.
//!
//! # Strategy categories
//!
//! Adversarial strategies fall into two categories:
//!
//! **Category A (generation-time)**: Affect how the random generation
//! algorithm builds the graph. These are gated by config flags on
//! `RandomConfig` and modify the behavior of `generate_random()`.
//! Strategies: one-parent families, missing events, solo persons,
//! many alternate names.
//!
//! **Category B (post-generation transforms)**: Composable pure functions
//! `fn(Graph) -> Graph` applied after generation and the first validation
//! gate. These are applied in sequence by the strategy runner.
//! Strategies: disconnected subgraphs, deep nesting, max ref chains,
//! orphaned references.
//!
//! Category B is further split into:
//!
//! - **Validity-preserving** transforms: Produce graphs that still pass
//!   structural + referential validation after the transform.
//! - **Validity-breaking** transforms: Deliberately produce graphs that
//!   fail validation with known error types.
//!
//! # Pipeline
//!
//! ```text
//! Generate (with Category A flags) → Validate (Gate 1)
//!   → Category B transforms → Validate (Gate 2) → Serialize
//! ```

use crate::Graph;

// ---------------------------------------------------------------------------
// AdversarialStrategy — enumerates all available strategies
// ---------------------------------------------------------------------------

/// Adversarial strategy selector.
///
/// Each variant represents a single composable adversarial strategy.
/// Strategies are divided into two categories:
///
/// - **Category A** (generation-time): Affects how the random generation
///   algorithm builds the graph. Gated by config flags on `RandomConfig`.
/// - **Category B** (post-generation): Pure function transforms applied
///   to a fully generated graph. Applied in sequence.
///
/// Category B is further divided into *validity-preserving* (the graph still
/// passes validation) and *validity-breaking* (expected to fail the second
/// validation gate) sub-categories.
#[derive(Clone, Debug, PartialEq)]
pub enum AdversarialStrategy {
    // ---- Category A: generation-time ----

    /// Skip father or mother assignment for a configurable fraction of families.
    OneParentFamilies(f64),

    /// Skip birth/death events for a configurable fraction of persons.
    MissingEvents(f64),

    /// Create persons with no families, no events, just a name.
    SoloPersons(f64),

    /// Add 5–20 alternate names to a configurable fraction of persons.
    ManyAlternateNames(f64),

    // ---- Category B: post-generation transforms ----

    /// Split the graph into multiple unrelated clusters by deleting
    /// cross-cluster family edges. Validity-preserving.
    DisconnectedSubgraphs,

    /// Replace place hierarchies with 5–10 level deep chains.
    /// Validity-preserving.
    DeepNesting,

    /// Create legal maximum-length reference chains
    /// (Event → Citation → Source → Repository → Note → ...).
    /// Validity-preserving.
    MaxRefChains,

    /// Remove some edges from citation/note/media references while keeping
    /// the target nodes. Validity-breaking (fails second validation gate).
    OrphanedReferences,
}

impl AdversarialStrategy {
    /// Returns `true` if this is a Category A (generation-time) strategy.
    pub fn is_category_a(&self) -> bool {
        matches!(
            self,
            AdversarialStrategy::OneParentFamilies(_)
                | AdversarialStrategy::MissingEvents(_)
                | AdversarialStrategy::SoloPersons(_)
                | AdversarialStrategy::ManyAlternateNames(_)
        )
    }

    /// Returns `true` if this is a Category B (post-generation) strategy.
    pub fn is_category_b(&self) -> bool {
        !self.is_category_a()
    }

    /// Returns `true` if this is a validity-preserving Category B strategy.
    ///
    /// Validity-preserving transforms produce graphs that pass structural
    /// and referential validation.
    pub fn is_validity_preserving(&self) -> bool {
        matches!(
            self,
            AdversarialStrategy::DisconnectedSubgraphs
                | AdversarialStrategy::DeepNesting
                | AdversarialStrategy::MaxRefChains
        )
    }

    /// Returns `true` if this is a validity-breaking Category B strategy.
    ///
    /// Validity-breaking transforms produce graphs expected to fail the
    /// second validation gate.
    pub fn is_validity_breaking(&self) -> bool {
        self.is_category_b() && !self.is_validity_preserving()
    }
}

// ---------------------------------------------------------------------------
// AdversarialConfig
// ---------------------------------------------------------------------------

/// Configuration for adversarial generation.
///
/// When `enabled` is false (the default), generation proceeds normally
/// with no adversarial strategies applied.
#[derive(Clone, Debug, PartialEq)]
#[derive(Default)]
pub struct AdversarialConfig {
    /// Whether adversarial generation is enabled.
    pub enabled: bool,

    /// List of adversarial strategies to apply.
    ///
    /// Category A strategies are applied during generation (they modify
    /// how `generate_random` behaves). Category B strategies are applied
    /// as post-generation transforms on a known-valid graph.
    pub strategies: Vec<AdversarialStrategy>,
}


// ---------------------------------------------------------------------------
// AdversarialError
// ---------------------------------------------------------------------------

/// Errors that can occur during adversarial transformation.
#[derive(Clone, Debug, PartialEq)]
pub enum AdversarialError {
    /// A transform cannot be applied because the graph is empty or too small.
    TransformNotApplicable(String),

    /// A transform requires features (e.g., places) that are not present.
    MissingRequiredFeature(String),
}

impl std::fmt::Display for AdversarialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdversarialError::TransformNotApplicable(msg) => {
                write!(f, "transform not applicable: {}", msg)
            }
            AdversarialError::MissingRequiredFeature(msg) => {
                write!(f, "missing required feature: {}", msg)
            }
        }
    }
}

impl std::error::Error for AdversarialError {}

// ---------------------------------------------------------------------------
// GraphTransform — composable post-generation transforms
// ---------------------------------------------------------------------------

/// A post-generation graph transform.
///
/// Each transform is a pure function that takes a `Graph` and returns
/// a modified `Graph`. This composable design allows strategies to be
/// applied in sequence and tested independently.
pub type GraphTransform = fn(Graph) -> Graph;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =======================================================================
    // AdversarialStrategy tests
    // =======================================================================

    #[test]
    fn adversarial_strategy_variants_exist() {
        // Verify all 7 strategy variants can be constructed
        let _ = AdversarialStrategy::OneParentFamilies(0.5);
        let _ = AdversarialStrategy::MissingEvents(0.5);
        let _ = AdversarialStrategy::SoloPersons(0.3);
        let _ = AdversarialStrategy::ManyAlternateNames(0.3);
        let _ = AdversarialStrategy::DisconnectedSubgraphs;
        let _ = AdversarialStrategy::DeepNesting;
        let _ = AdversarialStrategy::MaxRefChains;
        let _ = AdversarialStrategy::OrphanedReferences;
    }

    #[test]
    fn adversarial_strategy_one_parent_holds_fraction() {
        let s = AdversarialStrategy::OneParentFamilies(0.5);
        assert_eq!(s, AdversarialStrategy::OneParentFamilies(0.5));
        assert_ne!(s, AdversarialStrategy::OneParentFamilies(0.3));
    }

    #[test]
    fn adversarial_strategy_category_classification() {
        // Category A
        assert!(AdversarialStrategy::OneParentFamilies(0.5).is_category_a());
        assert!(AdversarialStrategy::MissingEvents(0.5).is_category_a());
        assert!(AdversarialStrategy::SoloPersons(0.3).is_category_a());
        assert!(AdversarialStrategy::ManyAlternateNames(0.3).is_category_a());

        // Category B
        assert!(AdversarialStrategy::DisconnectedSubgraphs.is_category_b());
        assert!(AdversarialStrategy::DeepNesting.is_category_b());
        assert!(AdversarialStrategy::MaxRefChains.is_category_b());
        assert!(AdversarialStrategy::OrphanedReferences.is_category_b());

        // Category A are not Category B
        assert!(!AdversarialStrategy::OneParentFamilies(0.5).is_category_b());
        assert!(!AdversarialStrategy::MissingEvents(0.5).is_category_b());
    }

    #[test]
    fn adversarial_strategy_validity_classification() {
        // Validity-preserving
        assert!(AdversarialStrategy::DisconnectedSubgraphs.is_validity_preserving());
        assert!(AdversarialStrategy::DeepNesting.is_validity_preserving());
        assert!(AdversarialStrategy::MaxRefChains.is_validity_preserving());

        // Validity-breaking
        assert!(AdversarialStrategy::OrphanedReferences.is_validity_breaking());

        // Category A are neither
        assert!(!AdversarialStrategy::OneParentFamilies(0.5).is_validity_preserving());
        assert!(!AdversarialStrategy::OneParentFamilies(0.5).is_validity_breaking());
    }

    #[test]
    fn adversarial_strategy_clone_debug_partialeq() {
        let s1 = AdversarialStrategy::OneParentFamilies(0.5);
        let s2 = s1.clone();
        assert_eq!(s1, s2);
        let _ = format!("{:?}", s1);

        let s3 = AdversarialStrategy::DisconnectedSubgraphs;
        let s4 = s3.clone();
        assert_eq!(s3, s4);
        let _ = format!("{:?}", s3);
    }

    // =======================================================================
    // AdversarialConfig tests
    // =======================================================================

    #[test]
    fn adversarial_config_default_disabled() {
        let config = AdversarialConfig::default();
        assert!(!config.enabled);
        assert!(config.strategies.is_empty());
    }

    #[test]
    fn adversarial_config_explicit() {
        let config = AdversarialConfig {
            enabled: true,
            strategies: vec![
                AdversarialStrategy::DisconnectedSubgraphs,
                AdversarialStrategy::OneParentFamilies(0.5),
            ],
        };
        assert!(config.enabled);
        assert_eq!(config.strategies.len(), 2);
        assert_eq!(
            config.strategies[0],
            AdversarialStrategy::DisconnectedSubgraphs
        );
        assert_eq!(
            config.strategies[1],
            AdversarialStrategy::OneParentFamilies(0.5)
        );
    }

    #[test]
    fn adversarial_config_clone_debug_partialeq() {
        let c1 = AdversarialConfig::default();
        let c2 = c1.clone();
        assert_eq!(c1, c2);
        let _ = format!("{:?}", c1);

        let c3 = AdversarialConfig {
            enabled: true,
            strategies: vec![AdversarialStrategy::DeepNesting],
        };
        let c4 = c3.clone();
        assert_eq!(c3, c4);
    }

    // =======================================================================
    // AdversarialError tests
    // =======================================================================

    #[test]
    fn adversarial_error_display_basic() {
        let err = AdversarialError::TransformNotApplicable(
            "graph has no persons to partition".to_string(),
        );
        let msg = format!("{}", err);
        assert!(msg.contains("transform not applicable"));
        assert!(msg.contains("no persons"));

        let err2 =
            AdversarialError::MissingRequiredFeature("no place nodes in graph".to_string());
        let msg2 = format!("{}", err2);
        assert!(msg2.contains("missing required feature"));
        assert!(msg2.contains("no place nodes"));
    }

    #[test]
    fn adversarial_error_clone_debug_partialeq() {
        let e1 = AdversarialError::TransformNotApplicable("test".to_string());
        let e2 = e1.clone();
        assert_eq!(e1, e2);
        let _ = format!("{:?}", e1);

        let e3 = AdversarialError::MissingRequiredFeature("test".to_string());
        let e4 = e3.clone();
        assert_eq!(e3, e4);
    }

    #[test]
    fn adversarial_error_is_std_error() {
        let err = AdversarialError::TransformNotApplicable("test".to_string());
        // Verify it implements std::error::Error
        let _: &dyn std::error::Error = &err;

        // Verify Display is accessible via Error
        let msg = format!("{}", err);
        assert!(!msg.is_empty());
    }

    // =======================================================================
    // GraphTransform type alias tests
    // =======================================================================

    #[test]
    fn adversarial_transform_signature() {
        // Verify that a function matching the GraphTransform signature can be
        // assigned to the type alias.
        fn identity_transform(g: Graph) -> Graph {
            g
        }

        let _t: GraphTransform = identity_transform;

        // Verify it works with a closure-like function
        fn noop(g: Graph) -> Graph {
            g
        }

        let transform: GraphTransform = noop;
        let graph = Graph::new();
        let result = transform(graph);
        assert_eq!(result.node_count(), 0);
        assert_eq!(result.edge_count(), 0);
    }
}