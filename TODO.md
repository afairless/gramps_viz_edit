# Implementation Plan: Phase 6 — Adversarial Generation

Source: `docs/research/design.md`

## Phase 6 Scope

Phase 6 builds the adversarial generation engine on top of the completed Phases 1–5 (schema extraction/codegen, graph core/validation, GraphBuilder API, XML serializer, random generation). Adversarial strategies produce unusual, boundary, or deliberately invalid graph structures for testing downstream tools.

Two categories of strategies:

- **Category A (generation-time)**: Modify how the random generation algorithm works, gated by config flags — one-parent families, missing events, solo persons, many alternate names.
- **Category B (post-generation transforms)**: Composable pure functions `fn(Graph) -> Graph` applied after generation and the first validation gate — disconnected subgraphs, deep nesting, maximum ref chains, orphaned references.

Category B is further split into:

- **Validity-preserving** (passes the second validation gate): disconnected subgraphs, deep nesting, max ref chains.
- **Validity-breaking** (expected to fail the second validation gate): orphaned references.

**Key design references**: design §7.4 (Adversarial generation), §10 Phase 6, §11 (Pipeline model).

New code lives in `crates/typed-graph/src/generate/adversarial.rs` and its sub-modules.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add adversarial generation module with strategy enums and config` | Adversarial scaffold | `crates/typed-graph/src/generate/adversarial.rs`, `crates/typed-graph/src/generate/mod.rs` | Unit |
| 2 | `feat: implement one-parent families generation-time strategy` | Category A — one-parent | `crates/typed-graph/src/generate/adversarial.rs`, `crates/typed-graph/src/generate/random.rs` | Unit |
| 3 | `feat: implement missing-events generation-time strategy` | Category A — missing events | `crates/typed-graph/src/generate/adversarial.rs`, `crates/typed-graph/src/generate/random.rs` | Unit |
| 4 | `feat: implement solo-persons generation-time strategy` | Category A — solo persons | `crates/typed-graph/src/generate/adversarial.rs`, `crates/typed-graph/src/generate/random.rs` | Unit |
| 5 | `feat: implement many-alternate-names generation-time strategy` | Category A — many names | `crates/typed-graph/src/generate/adversarial.rs`, `crates/typed-graph/src/generate/random.rs` | Unit |
| 6 | `feat: implement disconnected-subgraphs post-generation transform` | Category B — disconnected | `crates/typed-graph/src/generate/adversarial.rs` | Unit |
| 7 | `feat: implement deep-nesting post-generation transform` | Category B — deep nesting | `crates/typed-graph/src/generate/adversarial.rs` | Unit |
| 8 | `feat: implement max-ref-chains post-generation transform (validity-preserving)` | Category B — max ref chains | `crates/typed-graph/src/generate/adversarial.rs` | Unit |
| 9 | `feat: implement orphaned-references post-generation transform (validity-breaking)` | Category B — orphaned refs | `crates/typed-graph/src/generate/adversarial.rs` | Unit |
| 10 | `feat: add adversarial strategy runner and integrate into generation pipeline` | Strategy runner | `crates/typed-graph/src/generate/adversarial.rs`, `crates/typed-graph/src/generate/random.rs`, `crates/typed-graph/src/lib.rs` | Unit, Integration |
| 11 | `feat: add CLI --adversarial flag and YAML config support` | CLI integration | `crates/cli/src/main.rs` (or equivalent CLI crate) | Smoke |
| 12 | `test: add property-based tests for adversarial invariants` | Property tests | `tests/property/adversarial.rs` (or crate-local) | Property-based |

### Step Details

#### Step 1 — Adversarial scaffold

- Create `crates/typed-graph/src/generate/adversarial.rs` with module-level documentation describing the adversarial generation approach (two categories, post-generation pipeline, validity preservation contract).
- Define the strategy enum:

  ```rust
  /// Adversarial strategy selector.
  ///
  /// Each variant represents a single composable adversarial strategy.
  /// Strategies are divided into two categories:
  ///
  /// - **Category A** (generation-time): Affects how the random generation
  ///   algorithm builds the graph. Gated by config flags on `RandomConfig`.
  /// - **Category B** (post-generation): Pure function transforms applied
  ///   to a fully generated graph. Applied in sequence.
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
  ```

- Define `AdversarialConfig`:

  ```rust
  /// Configuration for adversarial generation.
  ///
  /// When `enabled` is false (the default), generation proceeds normally
  /// with no adversarial strategies applied.
  #[derive(Clone, Debug, PartialEq)]
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

  impl Default for AdversarialConfig {
      fn default() -> Self {
          AdversarialConfig {
              enabled: false,
              strategies: vec![],
          }
      }
  }
  ```

- Define `AdversarialError`:

  ```rust
  /// Errors that can occur during adversarial transformation.
  #[derive(Clone, Debug, PartialEq)]
  pub enum AdversarialError {
      /// A transform cannot be applied because the graph is empty or too small.
      TransformNotApplicable(String),
      /// A transform requires features (e.g., places) that are not present.
      MissingRequiredFeature(String),
  }
  ```

  Implement `std::fmt::Display` and `std::error::Error`.

- Define the transform function signature:

  ```rust
  /// A post-generation graph transform.
  ///
  /// Each transform is a pure function that takes a `Graph` and returns
  /// a modified `Graph`. This composable design allows strategies to be
  /// applied in sequence and tested independently.
  pub type GraphTransform = fn(Graph) -> Graph;
  ```

- Update `crates/typed-graph/src/generate/mod.rs`:
  - Add `pub mod adversarial;`
  - Add `pub use adversarial::*;`

- **Tests**:
  - `adversarial_strategy_variants_exist`: All 7 strategy variants can be constructed.
  - `adversarial_strategy_one_parent_holds_fraction`: `OneParentFamilies(0.5)` stores the fraction correctly.
  - `adversarial_config_default_disabled`: `AdversarialConfig::default()` has `enabled == false` and empty strategies.
  - `adversarial_config_explicit`: Construct with `enabled: true` and two strategies, verify fields.
  - `adversarial_error_display_basic`: `TransformNotApplicable` and `MissingRequiredFeature` produce actionable messages.
  - `adversarial_transform_signature`: `GraphTransform` type alias accepts `fn(Graph) -> Graph`.
  - Verify: `cargo build` compiles.

#### Step 2 — Category A: one-parent families

- Modify the random generation algorithm in `random.rs` to support the **one-parent families** strategy:
  - When `AdversarialStrategy::OneParentFamilies(fraction)` is active, for the given fraction of families, skip either father or mother assignment.
  - Modify `select_parents()` and `generate_family()` to accept an optional `one_parent_fraction: f64` parameter or pull it from `RandomConfig`.
  - For one-parent families: randomly choose whether to skip the father (default) or the mother (flip a coin).
  - One-parent families are structurally valid (Gramps allows missing father_handle/mother_handle).
  - Emit a plausibility warning for each one-parent family created.

- **Tests**:
  - `one_parent_families_strategy_zero_fraction`: fraction=0.0 produces no one-parent families.
  - `one_parent_families_strategy_all_single`: fraction=1.0 produces all families with one parent.
  - `one_parent_families_validates_ok`: One-parent families pass structural + referential validation.
  - `one_parent_families_single_produces_warning`: Plausibility warning is emitted for each one-parent family.
  - `one_parent_families_edge_case_one_person_pool`: When only one person is available, one-parent family is created.
  - `one_parent_families_regression_alternating_parents`: Skipped parent alternates between father and mother across families (when fraction >= 0.5 and multiple families exist).

#### Step 3 — Category A: missing events

- Modify the random generation algorithm in `random.rs` to support the **missing events** strategy:
  - When `AdversarialStrategy::MissingEvents(fraction)` is active, for the given fraction of persons, skip birth event generation entirely.
  - This means the person node exists but has no associated events at all.
  - For the same (or a separately configurable) fraction, skip death event generation when the person would normally have a death event.
  - Persons with missing events are structurally valid (events are optional in Gramps).
  - Emit a plausibility warning for each person with missing events.

- **Tests**:
  - `missing_events_zero_fraction`: fraction=0.0 produces normal events for all persons.
  - `missing_events_all_missing`: fraction=1.0 produces no events for any person.
  - `missing_events_validates_ok`: Graphs with missing events pass structural + referential validation.
  - `missing_events_some_missing_some_present`: With fraction=0.5, roughly half of persons miss events (approximate, within statistical bounds).
  - `missing_events_death_only_preserved`: Birth events can be missing while death events still exist.
  - `missing_events_warning_emitted`: Plausibility warning emitted for each person with missing events.

#### Step 4 — Category A: solo persons

- Modify the random generation algorithm in `random.rs` to support the **solo persons** strategy:
  - When `AdversarialStrategy::SoloPersons(fraction)` is active, for the given fraction of persons, do not assign them to any family (as child or parent).
  - Solo persons have a name and optional dates/events, but no family_list, parent_family_list, or child_ref_list.
  - They may still have events (birth, death) — that's controlled by the missing-events strategy, not solo-persons.
  - Solo persons are structurally valid (Gramps allows persons with no family associations).
  - Emit a plausibility warning for each solo person.

- **Tests**:
  - `solo_persons_zero_fraction`: fraction=0.0 produces normal families.
  - `solo_persons_all_solo`: fraction=1.0 produces no families at all (all persons are solo).
  - `solo_persons_some_solo_some_family`: With fraction=0.3, ~30% of persons are solo.
  - `solo_persons_validates_ok`: Graphs with solo persons pass structural + referential validation.
  - `solo_persons_still_have_events`: Solo persons still have birth/death events (unless combined with missing-events).
  - `solo_persons_warning_emitted`: Plausibility warning emitted for each solo person.

#### Step 5 — Category A: many alternate names

- Modify the random generation algorithm in `random.rs` to support the **many alternate names** strategy:
  - When `AdversarialStrategy::ManyAlternateNames(fraction)` is active, for the given fraction of persons, add 5–20 alternate names to their `alternate_names` list.
  - Alternate names follow the same procedural generation as the primary name but use a different style or seed offset to produce distinct names.
  - Each alternate name is a full `Name` struct with `first_name`, `surname`, and `name_type` set to `NameType::AlsoKnownAs` (or similar).
  - Persons with many alternate names are structurally valid.
  - Emit a plausibility warning for each person with 10+ alternate names.

- **Tests**:
  - `many_alternate_names_zero_fraction`: fraction=0.0 produces no alternate names.
  - `many_alternate_names_all_affected`: fraction=1.0 adds alternate names to all persons.
  - `many_alternate_names_count_in_range`: Alternate name count is in 5–20 range.
  - `many_alternate_names_validates_ok`: Graphs with many alternate names pass structural + referential validation.
  - `many_alternate_names_names_distinct_from_primary`: Alternate names differ from the primary name.
  - `many_alternate_names_warning_for_10_plus`: Persons with 10+ alternate names emit a warning.

#### Step 6 — Category B: disconnected subgraphs transform

- In `adversarial.rs`, implement the **disconnected subgraphs** transform:

  ```rust
  /// Split the graph into `k` unrelated clusters by deleting cross-cluster
  /// family edges.
  ///
  /// The graph is partitioned into `k` clusters by dividing the persons
  /// into groups. Family edges that cross cluster boundaries are removed.
  /// All other edges (events, citations, notes, etc.) are preserved.
  ///
  /// This produces `k` disconnected genealogical trees within a single graph.
  /// The transform is validity-preserving: the resulting graph still passes
  /// structural and referential validation.
  ///
  /// # Parameters
  ///
  /// * `k` — number of clusters to create (default: 3, min: 2).
  pub fn disconnected_subgraphs(k: usize) -> GraphTransform { ... }
  ```

  **Algorithm**:
  - Collect all person handles from the graph.
  - Partition persons into `k` roughly equal clusters (round-robin or random assignment based on handle order).
  - For each cluster, identify which families connect persons across clusters (a family where father and mother are in different clusters, or a child is in a different cluster from its parents).
  - Remove cross-cluster family edges (FamilyFather, FamilyMother, FamilyChildRef) and their reverse entries.
  - Remove cross-cluster PersonFamily, PersonParentFamily edges.
  - All other edges (events, citations, notes, media, places, tags) are preserved.
  - Return the modified graph (no nodes are removed, only edges).

- **Tests**:
  - `disconnected_subgraphs_k_2_produces_two_clusters`: With a graph of 10 persons, k=2 produces two disconnected clusters (no family edges between them).
  - `disconnected_subgraphs_k_3_produces_three_clusters`: k=3 produces three clusters.
  - `disconnected_subgraphs_validates_ok`: Transformed graph passes structural + referential validation (validity-preserving).
  - `disconnected_subgraphs_no_nodes_removed`: Node count is unchanged.
  - `disconnected_subgraphs_empty_graph`: Empty graph returns empty graph.
  - `disconnected_subgraphs_k_1_clamps_to_min`: k=1 is treated as k=2 (minimum 2 clusters).
  - `disconnected_subgraphs_single_person`: Graph with one person is unchanged (no cross-cluster edges to remove).
  - `disconnected_subgraphs_all_event_edges_preserved`: Event edges are not affected by the transform.

#### Step 7 — Category B: deep nesting transform

- In `adversarial.rs`, implement the **deep nesting** transform:

  ```rust
  /// Replace place hierarchies with deep nesting chains.
  ///
  /// Creates chains of Place → Place parent references (PlacePlaceRef edges)
  /// with configurable depth (5–10 levels) to test downstream tools with
  /// deeply nested place hierarchies.
  ///
  /// If the graph has no Place nodes, the transform is a no-op
  /// (returns `TransformNotApplicable` error).
  ///
  /// The transform is validity-preserving: all place references remain valid.
  pub fn deep_nesting(depth: usize) -> GraphTransform { ... }
  ```

  **Algorithm**:
  - Collect all Place nodes from the graph.
  - For each place, create a chain of parent places: Place → ParentPlace1 → ParentPlace2 → ... → ParentPlace_N.
  - New parent places are created with procedurally generated names ("Greater {name}", "Upper {name}", etc.) using the existing name generator or a simple prefix scheme.
  - Add PlacePlaceRef edges to link each place to its parent in the chain.
  - Existing PlacePlaceRef edges in the graph are preserved (the new chain extends them).
  - If the graph has no Place nodes, return `Err(TransformNotApplicable)`.

- **Tests**:
  - `deep_nesting_depth_5`: Places have chains of depth 5.
  - `deep_nesting_depth_10`: Places have chains of depth 10.
  - `deep_nesting_preserves_existing_edges`: Existing PlacePlaceRef edges are preserved.
  - `deep_nesting_new_places_have_unique_handles`: Newly created places have valid unique handles.
  - `deep_nesting_validates_ok`: All new place references are valid (validity-preserving).
  - `deep_nesting_empty_graph`: Empty graph unaffected.
  - `deep_nesting_no_places_noop`: Graph with no places returns `Err(TransformNotApplicable)`.
  - `deep_nesting_node_count_increases`: Node count increases by (depth × original place count).

#### Step 8 — Category B: maximum ref chains transform

- In `adversarial.rs`, implement the **maximum ref chains** transform:

  ```rust
  /// Create legal maximum-length reference chains.
  ///
  /// Builds chains of the form:
  ///   Event → Citation → Source → Repository → Note → ...
  ///
  /// Each chain is structurally valid (all handle refs resolve) and tests
  /// downstream tools for stack overflow or O(n²) traversal when following
  /// long reference chains.
  ///
  /// The transform is validity-preserving: all references remain valid.
  pub fn max_ref_chains(chain_length: usize) -> GraphTransform { ... }
  ```

  **Algorithm**:
  - Identify all Event nodes in the graph (or create a small set if none exist).
  - For each event, build a chain of connected primary objects:
    - Event → Citation (new Citation node, linked via event.citation_list)
    - Citation → Source (new Source node, linked via citation.source_handle)
    - Source → Repository (new Repository node, linked via source.repo_ref_list)
    - Optionally extend: Repository → Note, Note → Media, Media → Tag, etc.
  - Each new node is created with default data and a unique UUID v4 handle.
  - Chain length is configurable (default: 5, max: 10 — limited by available type-to-type refs).
  - All handle references are valid (target nodes exist). This is validity-preserving.
  - The transform does NOT create circular references — each chain is linear.
  - If the graph has no Event nodes, create a minimal set of events from persons' birth dates.

- **Tests**:
  - `max_ref_chains_length_3`: Chains of length 3 are created (Event → Citation → Source).
  - `max_ref_chains_length_5`: Chains of length 5 are created (Event → Citation → Source → Repository → Note).
  - `max_ref_chains_all_refs_resolve`: Every handle reference in the chain resolves to an existing node.
  - `max_ref_chains_validates_ok`: All references are valid (validity-preserving).
  - `max_ref_chains_no_circular_refs`: Chains are linear, not circular.
  - `max_ref_chains_non_event_nodes_also_chained`: Non-event nodes that can start a chain (e.g., Person with citations) are also extended.
  - `max_ref_chains_empty_graph`: Create minimal nodes to start chains from.
  - `max_ref_chains_node_count_increases`: New nodes are created for each chain element.
  - `max_ref_chains_regression_long_chain_does_not_stack_overflow`: A chain of length 10 does not cause recursion issues.

#### Step 9 — Category B: orphaned references transform

- In `adversarial.rs`, implement the **orphaned references** transform:

  ```rust
  /// Remove some edges from citation/note/media references while keeping the
  /// target nodes in the graph.
  ///
  /// This produces dangling references: the target node still exists, but the
  /// edge from the source to the target is removed, while the source node still
  /// holds the handle in its field/list.
  ///
  /// This is a **validity-breaking** transform — the resulting graph is
  /// expected to fail the second validation gate with `DanglingReference`
  /// errors.
  pub fn orphaned_references(fraction: f64) -> GraphTransform { ... }
  ```

  **Algorithm**:
  - Collect all edges that are citation/note/media/tag references (CitationRef, NoteRef, MediaRef, TagRef, PersonCitation, PersonNote, etc.).
  - For the given fraction of these edges, remove the edge from the graph's edge lists BUT do NOT remove the target node from the graph.
  - The source node's field/list still contains the handle, creating a dangling reference.
  - The target node remains in the graph (it may be referenced by other edges).
  - Do NOT remove edges that are structural (PersonFamily, FamilyFather, FamilyMother, FamilyChildRef, PersonEventRef) — only remove "soft" reference edges.

- **Tests**:
  - `orphaned_references_zero_fraction`: fraction=0.0 removes no edges.
  - `orphaned_references_all_removed`: fraction=1.0 removes all citation/note/media/tag edges.
  - `orphaned_references_targets_retained`: Target nodes still exist in the graph after edge removal.
  - `orphaned_references_structural_edges_untouched`: Family/person structural edges are not removed.
  - `orphaned_references_fails_validation`: Graph fails Gate 2 validation with `DanglingReference` errors.
  - `orphaned_references_error_count_matches`: Number of `DanglingReference` errors matches number of removed edges.
  - `orphaned_references_empty_graph`: Empty graph unaffected.
  - `orphaned_references_no_reference_edges`: Graph with no citation/note/media edges is unaffected.

#### Step 10 — Adversarial strategy runner and pipeline integration

- In `adversarial.rs`, implement the strategy runner:

  ```rust
  /// Run the adversarial generation pipeline on a graph.
  ///
  /// This function:
  /// 1. Applies each Category B strategy from `config.strategies` in sequence
  ///    to the input graph.
  /// 2. Returns the transformed graph and a list of any
  ///    [`AdversarialError`]s for strategies that could not be applied.
  ///
  /// Category A strategies are NOT applied here — they affect how
  /// `generate_random()` builds the graph and are handled by modifying
  /// `RandomConfig` before calling `generate_random()`.
  pub fn apply_adversarial_strategies(
      graph: Graph,
      config: &AdversarialConfig,
  ) -> (Graph, Vec<AdversarialError>) { ... }
  ```

  **Algorithm**:
  - Filter `config.strategies` to keep only Category B variants.
  - For each Category B strategy (in order):
    - Match on the variant and construct the corresponding `GraphTransform`.
    - Apply the transform: `graph = transform(graph)`.
    - If the transform returns `Err(AdversarialError)`, collect the error and skip that strategy (the graph remains in its pre-transform state for that strategy).
  - Return the final graph and collected errors.
  - Category A strategies are ignored by this function (they're handled in `generate_random`).

- In `random.rs`, integrate adversarial config into `generate_random()`:

  ```rust
  /// Modified signature or new entry point:
  pub fn generate_random(
      config: &RandomConfig,
      adversarial_config: &AdversarialConfig,
      schema: &Schema,
  ) -> Result<GenerationResult, GenerationError> { ... }
  ```

  - At the start of generation, check `adversarial_config.enabled` and apply Category A strategy effects to the generation algorithm (modify parent selection, event generation, etc.).
  - After the graph is generated and before the first validation gate, apply Category B strategies via `apply_adversarial_strategies()`.
  - Return the graph and a list of adversarial errors alongside the normal result.

- Update `lib.rs` re-exports.

- **Tests**:
  - `apply_adversarial_strategies_disabled`: When `config.enabled == false`, graph is returned unchanged with no errors.
  - `apply_adversarial_strategies_empty_list`: Empty strategies list returns graph unchanged.
  - `apply_adversarial_strategies_disconnected_alone`: Running disconnected-subgraphs strategy alone works.
  - `apply_adversarial_strategies_multiple_transforms`: Multiple strategies applied in sequence all take effect.
  - `apply_adversarial_strategies_skip_on_error`: A failing strategy (e.g., deep nesting on a graph with no places) is skipped and others continue.
  - `apply_adversarial_strategies_category_a_ignored`: Category A strategies in the list are silently ignored by `apply_adversarial_strategies`.
  - `generate_random_with_adversarial_enabled_no_strategies`: Adversarial enabled but empty strategies list produces normal graph.
  - `generate_random_with_adversarial_disabled`: Default behavior is unchanged when adversarial is disabled.
  - Integration: `generate_random + adversarial -> validate -> serialize` pipeline works end-to-end.

#### Step 11 — CLI `--adversarial` flag and config support

> **Note**: This step depends on the CLI crate structure. If the CLI crate is not yet created or uses a different structure than described below, adjust paths accordingly.

- In the CLI crate (e.g., `crates/cli/src/main.rs` or equivalent), add the `--adversarial` flag:

  ```
  --adversarial <LIST>     Comma-separated adversarial strategies, or "all"
  ```

  - Parse strategies from comma-separated list: `disconnected,one-parent,deep-nesting`.
  - The `all` keyword enables every available strategy.
  - Map CLI strategy names to `AdversarialStrategy` variants:

    | CLI name | Variant |
    |---|---|
    | `one-parent` | `OneParentFamilies(0.5)` |
    | `missing-events` | `MissingEvents(0.5)` |
    | `solo-persons` | `SoloPersons(0.3)` |
    | `many-names` | `ManyAlternateNames(0.3)` |
    | `disconnected` | `DisconnectedSubgraphs` |
    | `deep-nesting` | `DeepNesting` |
    | `max-ref-chains` | `MaxRefChains` |
    | `orphaned-refs` | `OrphanedReferences` |

  - Construct `AdversarialConfig { enabled: true, strategies }`.
  - Pass it to `generate_random()`.

- If a YAML scenario file is being used, add adversarial config support:

  ```yaml
  adversarial:
    enabled: true
    strategies:
      - disconnected
      - one_parent
  ```

- **Tests**:
  - `cli_adversarial_flag_parses_single`: `--adversarial disconnected` parses correctly.
  - `cli_adversarial_flag_parses_multiple`: `--adversarial disconnected,one-parent` parses both strategies.
  - `cli_adversarial_all_keyword`: `--adversarial all` enables all strategies.
  - `cli_adversarial_unknown_strategy_rejected`: Unknown strategy name returns an error.
  - `cli_adversarial_default_disabled`: Without the flag, adversarial is disabled.
  - Smoke test: `gramps-gen generate --count 10 --adversarial disconnected` runs without crashing.

#### Step 12 — Property-based tests for adversarial invariants

- Add property-based tests for adversarial invariants. These can live in `crates/typed-graph/src/generate/adversarial.rs` under `#[cfg(test)] mod tests`, or in `tests/property/adversarial.rs` for cross-crate tests.

  **Invariant 1: Validity-preserving strategies produce valid graphs**

  ```rust
  #[test]
  fn property_validity_preserving_strategies_produce_valid_graphs() {
      // For any valid RandomConfig and any validity-preserving adversarial
      // strategy, the resulting graph passes structural + referential validation.
      let schema = Schema::new();
      let validity_preserving = vec![
          AdversarialStrategy::DisconnectedSubgraphs,
          AdversarialStrategy::DeepNesting,
          AdversarialStrategy::MaxRefChains,
      ];
      for seed in 0..30 {
          let config = RandomConfig {
              person_count: 10 + (seed % 10) as usize,
              with_places: true,
              with_citations: true,
              with_notes: true,
              seed: Some(seed),
              ..RandomConfig::default()
          };
          for strategy in &validity_preserving {
              let adversarial_config = AdversarialConfig {
                  enabled: true,
                  strategies: vec![strategy.clone()],
              };
              let result = generate_random(&config, &adversarial_config, &schema);
              if let Ok(result) = result {
                  let mut graph = result.graph;
                  let errors = graph.validate(&schema);
                  assert!(
                      errors.is_empty(),
                      "Seed {}, strategy {:?}: validation failed with {} errors",
                      seed, strategy, errors.len()
                  );
              }
          }
      }
  }
  ```

  **Invariant 2: Validity-breaking strategies produce expected errors**

  ```rust
  #[test]
  fn property_validity_breaking_strategies_produce_expected_errors() {
      // For the OrphanedReferences strategy, the graph fails Gate 2
      // validation with DanglingReference errors.
      let schema = Schema::new();
      for seed in 0..30 {
          let config = RandomConfig {
              person_count: 15,
              with_citations: true,
              with_notes: true,
              seed: Some(seed),
              ..RandomConfig::default()
          };
          let adversarial_config = AdversarialConfig {
              enabled: true,
              strategies: vec![AdversarialStrategy::OrphanedReferences(1.0)],
          };
          let result = generate_random(&config, &adversarial_config, &schema);
          if let Ok(result) = result {
              let mut graph = result.graph;
              let errors = graph.validate(&schema);
              assert!(
                  !errors.is_empty(),
                  "Seed {}: OrphanedReferences should produce validation errors",
                  seed
              );
              for error in &errors {
                  assert!(
                      matches!(error, ValidationError::DanglingReference { .. }),
                      "Seed {}: expected DanglingReference, got {:?}",
                      seed, error
                  );
              }
          }
      }
  }
  ```

  **Invariant 3: Adversarial-disabled produces normal output**

  ```rust
  #[test]
  fn property_adversarial_disabled_produces_normal_output() {
      // When adversarial is disabled, the generated graph is the same as
      // the non-adversarial generation (same graph structure, same stats).
      let schema = Schema::new();
      for seed in 0..30 {
          let config = RandomConfig {
              person_count: 15,
              seed: Some(seed),
              ..RandomConfig::default()
          };
          let adversarial_disabled = AdversarialConfig {
              enabled: false,
              strategies: vec![],
          };
          let normal = generate_random(&config, &AdversarialConfig::default(), &schema);
          let disabled = generate_random(&config, &adversarial_disabled, &schema);
          // Both should succeed and produce identical graphs
          if let (Ok(normal), Ok(disabled)) = (normal, disabled) {
              assert_eq!(
                  normal.graph, disabled.graph,
                  "Seed {}: disabled should match normal", seed
              );
          }
      }
  }
  ```

  **Invariant 4: Strategy composition — multiple strategies can be combined**

  ```rust
  #[test]
  fn property_multiple_validity_preserving_strategies_compose() {
      // Applying multiple validity-preserving strategies together should still
      // produce a valid graph (assuming they don't conflict).
      let schema = Schema::new();
      let combined_strategies = vec![
          AdversarialStrategy::DisconnectedSubgraphs,
          AdversarialStrategy::DeepNesting,
      ];
      for seed in 0..20 {
          let config = RandomConfig {
              person_count: 20,
              with_places: true,
              with_citations: true,
              seed: Some(seed),
              ..RandomConfig::default()
          };
          let adversarial_config = AdversarialConfig {
              enabled: true,
              strategies: combined_strategies.clone(),
          };
          let result = generate_random(&config, &adversarial_config, &schema);
          if let Ok(result) = result {
              let mut graph = result.graph;
              let errors = graph.validate(&schema);
              assert!(
                  errors.is_empty(),
                  "Seed {}: combined strategies failed validation: {:?}",
                  seed, errors
              );
          }
      }
  }
  ```

  **Invariant 5: Category A and Category B strategies compose**

  ```rust
  #[test]
  fn property_category_a_and_b_compose() {
      // Combining a Category A (generation-time) strategy with a Category B
      // (post-generation) strategy should work without errors.
      let schema = Schema::new();
      for seed in 0..20 {
          let config = RandomConfig {
              person_count: 15,
              seed: Some(seed),
              ..RandomConfig::default()
          };
          let adversarial_config = AdversarialConfig {
              enabled: true,
              strategies: vec![
                  AdversarialStrategy::OneParentFamilies(0.5),
                  AdversarialStrategy::DisconnectedSubgraphs,
              ],
          };
          let result = generate_random(&config, &adversarial_config, &schema);
          assert!(
              result.is_ok(),
              "Seed {}: combined Category A + B strategies failed: {:?}",
              seed, result
          );
      }
  }
  ```

### Key design references

- **Adversarial strategy catalog**: design §7.4 — Two categories (A: generation-time, B: post-generation), each with 4 strategies. Category B split into validity-preserving and validity-breaking sub-categories.
- **Pipeline model**: design §11 — Generate → Validate (Gate 1) → Adversarial Transform → Validate (Gate 2) → Serialize. Category A strategies affect the generation stage. Category B strategies are the adversarial transform stage.
- **Strategy categories**: design §7.4 — Category A affects how generation works (one-parent families, missing events, solo persons, many alternate names). Category B are pure function transforms (disconnected subgraphs, deep nesting, max ref chains, orphaned references).
- **Validity contract**: design §7.4 — Validity-preserving strategies produce graphs that pass Gate 2. Validity-breaking strategies produce graphs expected to fail Gate 2 with known error types.
- **Configuration schema**: design §7.5 — `adversarial.enabled` flag, `adversarial.strategies` list.
- **CLI interface**: design §9 — `--adversarial <LIST>` flag, comma-separated strategy names, `"all"` keyword.
- **Data contracts**: design §7.1 — All generated strings are UTF-8, Latin script, non-empty. Generators must not panic on valid input. Adversarial errors are collected and reported, not panicked.
- **Error handling**: design §12 — Collected errors (not fail-fast), reported with context. Adversarial errors include the strategy name and affected handles.
- **Module map**: design §3 — All adversarial code lives in `crates/typed-graph/src/generate/adversarial.rs`.
