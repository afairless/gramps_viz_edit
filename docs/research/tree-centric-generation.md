# Design Plan: Tree-Centric Generation

## Problem Statement

The current generation algorithm produces datasets where most people are
isolated or in very small (2-3 person) family units, with only occasional
cross-generation chains. The primary goal is to flip this: **most people
should belong to 1-2 large interconnected trees**, there should optionally be
a medium-sized tree, and only a small minority should be isolated individuals
or tiny nuclear families.

### Root Causes

The existing approach (even after the `family-connection-density.md`
improvements — `--family-ratio`, `--max-parent-roles`, and layer-linking) is
**person-first, then pair**:

1. Create N persons in a flat list distributed evenly across layers.
2. Create M families by greedily pairing eligible parents via random selection.
3. Assign children from remaining eligible persons.
4. Fall through to single-parent childless families when pairing fails.

This fundamentally cannot produce large trees reliably because:

- **No tree-level planning**: The generator doesn't decide up front how many
  trees to build or how big they should be. It just creates persons and
  families independently and hopes they connect.
- **Even layer distribution**: Dividing persons evenly across G layers creates
  shallow pools. With `--count 200 --depth 4`, each layer gets ~50 people
  (25 male, 25 female), making only 25 possible couples per layer. Family
  creation quickly exhausts the eligible pool.
- **Shallow family creation**: `select_parents` prefers same-layer pairs.
  While this creates nuclear families, it doesn't build the parent→child→grandchild
  chains that make a deep tree. Children from family F at layer N must become
  parents in families at layer N-1, but the algorithm doesn't guide this.
- **Family-first, then children**: Families are created first, and children
  are assigned afterward from whoever is eligible. If no eligible children
  are found, the family is childless. There's no mechanism to ensure families
  have children, or that children become parents.
- **Gender balance variance**: With random gender assignment, a layer can
  have a skewed gender ratio, reducing the number of possible parent pairs
  and increasing the rate of single-parent fallback families (which skip
  child assignment entirely).

### Desired Outcome

| Component | Share of Persons | Example (200 persons) |
|-----------|-----------------|----------------------|
| Large tree #1 | 40-60% | 80-120 people |
| Large tree #2 (optional) | 20-40% | 40-80 people |
| Medium tree | 10-20% | 20-40 people |
| Isolated / tiny | 0-10% | 0-20 people |

The user should be able to configure these fractions, with sensible defaults
that bias toward 1-2 dominant trees.

---

## Design Decisions

### Should we replace or complement the existing generator?

We **complement** it. The existing `generate_random` is valuable for uniform
random testing and adversarial generation. We add a new generation entry
point `generate_tree_centric` that produces the desired distribution, with a
new `TreeConfig` struct and a new CLI mode (`--mode tree-centric` or similar).

| Decision | Choice | Rationale |
|----------|--------|-----------|
| New entry point vs. refactor existing | New `generate_tree_centric` + `TreeConfig` | Preserves existing behavior for adversarial testing; clean separation |
| Tree sizing strategy | Configurable weighted buckets | Flexibility without over-parameterization |
| Intra-tree generation algorithm | Two-phase: structure planning → graph commit | Separates concerns; testable intermediate representation |
| Partner sourcing | Partners allocated from the same tree pool (endogamous) | Default: partners come from the same tree to keep it connected |
| Gender balancing within trees | Biased assignment toward parity at each generation | Maximizes pairing success |
| Single-parent families in trees | None in core tree structure; leaf persons can be childless but remain attached to existing two-parent families | Core trees are fully connected with two-parent families |
| CLI interface | New `--mode tree-centric` flag, plus `--tree-sizes` | Backward compatible; `--mode random` (default) preserves existing behavior |
| Adversarial strategies | Not applied inside `generate_tree_centric`; left to CLI pipeline Stage 3 | Follows architecture doc's five-stage pipeline; avoids the double-application bug present in `generate_random` |

> **Note on adversarial double-application:** The existing `generate_random`
> internally calls `apply_adversarial_strategies()`, which means Category B
> strategies are applied twice (once inside the generator, once in the CLI
> pipeline Stage 3). This is a pre-existing bug. The tree-centric generator
> is designed correctly from the start: adversarial transforms are only
> applied by the pipeline. A future cleanup should remove the internal
> application from `generate_random` as well.

### Alternative: Refactor `generate_random` to accept a tree configuration

We could add tree buckets to `RandomConfig` instead. This is simpler API-wise
but mixes two distinct generation philosophies in one config struct and one
function. The clean separation is better for testing and maintenance.

---

## Shared Configuration Base

To avoid duplicating fields between `RandomConfig` and `TreeConfig`, we
extract a shared struct:

```rust
/// Fields shared between random and tree-centric generation modes.
#[derive(Clone, Debug, PartialEq)]
pub struct BaseGenConfig {
    pub person_count: usize,
    pub start_year: i32,
    pub end_year: i32,
    pub name_style: String,
    pub with_places: bool,
    pub with_citations: bool,
    pub with_notes: bool,
    pub with_media: bool,
    pub with_tags: bool,
    pub place_depth: usize,
    pub seed: Option<u64>,
}
```

Both `RandomConfig` and `TreeConfig` embed a `base: BaseGenConfig` field
and add their mode-specific fields. This keeps new feature flags in one
place and ensures both modes benefit from additions automatically.

---

## Specification

### 1. TreeConfig

```rust
/// Configuration for tree-centric generation.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeConfig {
    /// Shared base configuration (person_count, date range, feature flags, seed).
    pub base: BaseGenConfig,

    /// Maximum genealogical depth for the largest tree.
    ///
    /// This is the number of genealogical generations (founders → children →
    /// grandchildren → ...), NOT temporal layers. A max_depth of 4 means the
    /// tree spans from a founding couple down to their great-grandchildren.
    /// Smaller trees scale proportionally. Default: 4.
    pub max_depth: usize,

    /// Tree size distribution as a list of (label, fraction) pairs.
    ///
    /// The fractions are relative weights. Persons are allocated
    /// proportionally. The sum of fractions is normalized to 1.0.
    ///
    /// Default: [("large_1", 0.5), ("large_2", 0.3), ("medium", 0.15), ("fringe", 0.05)]
    pub tree_buckets: Vec<TreeBucket>,

    /// Children per family range. Default: 1..5.
    ///
    /// At runtime this is clamped by the remaining person budget, so it is
    /// a target, not a hard constraint. Setting a lower minimum (1) allows
    /// the algorithm to squeeze into small remaining budgets.
    pub children_per_family: Range<usize>,

    /// Whether partners come from the same tree (endogamous, default: true)
    /// or can be allocated from the cross-tree partner pool. Endogamous
    /// keeps the tree more connected.
    pub endogamous: bool,

    /// Maximum age difference between parents (default: 20 years).
    pub max_parent_age_diff: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreeBucket {
    /// Display label for diagnostics/logging.
    pub label: String,
    /// Relative fraction of total persons to allocate to this bucket.
    pub fraction: f64,
}

impl Default for TreeConfig {
    fn default() -> Self {
        TreeConfig {
            base: BaseGenConfig {
                person_count: 200,
                start_year: 1850,
                end_year: 2025,
                name_style: "modern".into(),
                with_places: false,
                with_citations: false,
                with_notes: false,
                with_media: false,
                with_tags: false,
                place_depth: 3,
                seed: None,
            },
            max_depth: 4,
            tree_buckets: vec![
                TreeBucket { label: "large_1".into(), fraction: 0.50 },
                TreeBucket { label: "large_2".into(), fraction: 0.30 },
                TreeBucket { label: "medium".into(), fraction: 0.15 },
                TreeBucket { label: "fringe".into(), fraction: 0.05 },
            ],
            children_per_family: 1..5,
            endogamous: true,
            max_parent_age_diff: 20,
        }
    }
}
```

### 2. Tree-centric generation algorithm

The generation proceeds in four phases:

#### Phase A: Allocate persons to trees (ingestion/planning)

1. Compute the integer person count for each tree bucket based on fractions.
2. Round fractions to integers, allocating any remainder to the largest bucket.
3. Validate: if `person_count < 4` with any non-fringe bucket, return `GenerationError::InvalidConfig`.
4. For each tree, compute a target genealogical depth:
   - Large trees get `config.max_depth`.
   - Medium trees get `max(max_depth - 1, 2)`.
   - Fringe gets 1–2 (small nuclear families or isolated persons).
5. When `endogamous: false`, compute a cross-tree partner pool budget:
   each non-largest tree may borrow partners from trees that have surplus
   unassigned persons after their own pairing. Validate that the total
   partner deficit across all trees does not exceed the available pool.

#### Phase B: Plan tree structure (transformation — no graph mutations)

For each tree in decreasing size order, build an intermediate
`TreePlan` that describes the genealogical structure:

1. **Create founding couple (generation 0)**:
   - Plan two persons of opposite gender with birth years in the oldest range.
   - Plan a Family with both as parents.

2. **Recursively plan descendant generations**:
   For each family at genealogical generation `g` (where `g < depth - 1`):
   - Determine number of children via `distribute_children()`.
   - For each child:
     a. Plan a person with birth year in layer `g+1`.
     b. If the child is designated as a parent (to continue the tree),
        plan a partner for them. Source the partner from:
        - The same tree's unassigned pool (endogamous), or
        - The cross-tree partner pool (non-endogamous, see Phase C.1).
     c. Plan a Family where the child and their partner are parents.
        This family becomes part of generation `g+1`.
   - Designate which children are partnered vs. leaf children.

3. **Gender balance**: If a generation has too few males/females to form
   pairs, bias the gender assignment of remaining planned persons in that
   generation toward the underrepresented gender (see §4).

4. **Wrap up**: Any remaining person slots that couldn't be placed as
   spouses become **leaf children** attached to existing two-parent families
   in the last generation. These are additional children of intact families,
   NOT single-parent units. This is consistent with the design decision that
   core trees contain only two-parent families.

5. **Validate the plan** before committing: check that total planned persons
   ≤ budget, all families have valid parent pairs, and gender balance is
   within acceptable range.

#### Phase B-prime: Commit tree structure to graph (output)

Walk each `TreePlan` and materialize nodes and edges in the `Graph`:

1. Create each planned Person node via `GraphBuilder`.
2. Create each planned Family node via `GraphBuilder`.
3. Create edges (father/mother → family, family → child, person → event).
4. Generate Birth, Death, and Marriage events with consistent dates and
   date quality using the same patterns as `generate_events` in `random.rs`
   (birth years from layer ranges, life expectancy for death years, date
   quality randomized: ~80% Exact, ~15% Estimated, ~5% Calculated).
5. Generate optional features if enabled in `BaseGenConfig`:
   - Places (reuse `generate_place` from `random.rs`)
   - Sources and Citations (reuse existing generation patterns)
   - Notes, Media, Tags (reuse existing generation patterns)

#### Phase C: Cross-tree partner pool and fringe

##### C.1 — Cross-tree partner pool (when `endogamous: false`)

Before any tree is built, allocate a shared "partner pool" budget:

- After computing per-tree person budgets, reserve a small fraction (5–10%)
  of each non-fringe tree's budget for partners that may be claimed by other
  trees.
- During Phase B, when a tree needs an exogamous partner, it draws from the
  partner pool. The pool tracks which tree each reserved slot belongs to,
  so the partner's birth year can be set appropriately for the donor tree's
  generation.
- If the partner pool is exhausted before all trees are built, fall back
  to creating a new person within the requesting tree (endogamous fallback)
  and emit a warning.

##### C.2 — Fringe / isolated persons

- The "fringe" bucket is the last to be processed.
- Persons are created as isolated individuals (no families, no children) or
  as small single-parent families with 0–1 children.
- This is analogous to the existing solo-persons behavior.

### 3. The `distribute_children` Algorithm

This is the critical function that controls tree shape and prevents
exponential explosion.

```rust
/// Distribute the remaining person budget across families in the current
/// generation, respecting the per-family child range and the total budget.
///
/// Returns a `Vec<usize>` with one entry per family, giving the number of
/// children (and whether each is partnered to continue the tree).
fn distribute_children(
    num_families: usize,
    remaining_budget: usize,
    child_range: &Range<usize>,
    is_last_generation: bool,
    rng: &mut impl Rng,
) -> Vec<ChildAllocation> {
    // 1. Compute base allocation: each family gets at least child_range.start
    //    children, clamped to the remaining budget divided by families.
    let min_per_family = child_range.start.min(
        remaining_budget / num_families.max(1)
    );
    let base_total = min_per_family * num_families;

    // 2. Distribute extra children randomly up to child_range.end,
    //    without exceeding remaining_budget.
    let extra_budget = remaining_budget.saturating_sub(base_total);
    let mut allocations = vec![min_per_family; num_families];
    let max_extra_per_family = child_range.end.saturating_sub(min_per_family);

    // Weighted random distribution of extra children
    let mut extra_remaining = extra_budget;
    let mut family_indices: Vec<usize> = (0..num_families).collect();
    // Shuffle to randomize which families get extras
    for i in (0..num_families).rev() {
        let j = rng.gen_range(0..=i);
        family_indices.swap(i, j);
    }

    for &idx in &family_indices {
        if extra_remaining == 0 { break; }
        let extra = rng.gen_range(0..=max_extra_per_family.min(extra_remaining));
        allocations[idx] += extra;
        extra_remaining -= extra;
    }

    // 3. Decide which children partner (continue the tree).
    //    In non-last generations, target ~50% of children partnering.
    //    In the last generation, no children partner.
    allocations.into_iter().enumerate().map(|(i, count)| {
        let partnered = if is_last_generation {
            0
        } else {
            // At least 0, at most count, biased toward 1-2 for continuity
            let max_partnered = count.min(2);
            if max_partnered == 0 { 0 }
            else { rng.gen_range(0..=max_partnered) }
        };
        ChildAllocation { total_children: count, partnered_children: partnered }
    }).collect()
}

struct ChildAllocation {
    total_children: usize,
    partnered_children: usize,  // of the children, how many get spouses
}
```

**Key invariants:**

- `sum(alloc.total_children) ≤ remaining_budget`
- Each `alloc.total_children` is in `[child_range.start, child_range.end]` (or
  clamped lower if budget is tight)
- `alloc.partnered_children ≤ alloc.total_children`
- The total persons consumed by a family = `total_children + partnered_children`
  (each partnered child needs a spouse, consuming one extra person slot)

### 4. Gender balancing

During tree structure planning, maintain a running tally of males vs. females
at each generation. When planning a person whose gender is not yet constrained
by pairing needs, bias toward the underrepresented gender.

All gender values use the project's version-specific helpers (`gender_value`,
`into_gender_field`, etc.) to support multi-version schemas where gender may
be `i32` (schema 5.2) or `Option<i32>` (merged schema).

```rust
fn biased_gender(gen_males: usize, gen_females: usize, rng: &mut impl Rng) -> i32 {
    if gen_males < gen_females {
        if rng.gen_bool(0.8) { 0 } else { 1 }   // 80% male
    } else if gen_females < gen_males {
        if rng.gen_bool(0.8) { 1 } else { 0 }   // 80% female
    } else {
        if rng.gen_bool(0.5) { 0 } else { 1 }   // 50/50
    }
}

fn opposite_gender(g: i32) -> i32 {
    if g == 0 { 1 } else { 0 }
}
```

> **Note on gender values**: Gramps 5.2 uses `0` = Male, `1` = Female.
> The tree builder always works with `i32` internally and converts via
> `into_gender_field` when constructing `PersonData` nodes.

### 5. Family distribution across generations

To avoid a "binary tree explosion" (2 → 4 → 8 → 16 families), the number of
partnered children per family is controlled by `distribute_children`:

- Generation 1 (children of founders): 2–4 children, 1–2 of whom partner.
- Generation 2+: 1–3 children per family, 0–1 of whom partner.
- The exact counts are derived from the remaining person budget by
  `distribute_children`, not hard-coded.

This creates a more realistic genealogical shape where some branches die out
and others continue.

### 6. CLI Integration

Add a `--mode` flag to the `generate` command:

```
gramps-gen generate --mode tree-centric --count 200 --depth 4
gramps-gen generate --mode tree-centric --tree-sizes "0.5,0.3,0.15,0.05"
gramps-gen generate --mode random    # existing behavior (default)
```

**New CLI flags for tree-centric mode:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--mode` | enum | `random` | `random` or `tree-centric` |
| `--tree-sizes` | string | `"0.5,0.3,0.15,0.05"` | Comma-separated fractions for tree buckets |
| `--endogamous` | bool | `true` | Whether partners come from within the same tree |

The `--tree-sizes` flag accepts a comma-separated list of fractions. The
number of values determines the number of trees. Values are normalized to
sum to 1.0.

Examples:

- `"0.5,0.3,0.15,0.05"` → 1 large (50%), 1 large (30%), 1 medium (15%), fringe (5%)
- `"0.7,0.2,0.1"` → 1 large (70%), 1 medium (20%), fringe (10%)
- `"0.6,0.4"` → 1 large (60%), 1 medium (40%), no fringe

An empty `--tree-sizes ""` is rejected with a validation error ("must specify
at least one tree bucket").

#### `build_config` dispatch

The `build_config()` function in `cli/src/commands/generate.rs` returns a
new `GenerationMode` enum:

```rust
enum GenerationMode {
    Random(RandomConfig, AdversarialConfig),
    TreeCentric(TreeConfig),
}
```

The `run()` function matches on `GenerationMode` to dispatch to either
`generate_random` or `generate_tree_centric`. When `--mode tree-centric`,
the `--adversarial` flag is ignored with a warning (tree-centric mode does
not support adversarial transforms in v1).

Tree-centric mode does not use `ProgressReporter` in v1; the progress
reporting is limited to a start/finish message emitted by the CLI layer.

### 7. Scenario YAML Integration

Tree-centric config can be specified in scenario files alongside or instead of
random config:

```yaml
mode: tree-centric
person_count: 200
max_depth: 4
tree_buckets:
  - label: main_tree
    fraction: 0.5
  - label: secondary_tree
    fraction: 0.3
  - label: medium_tree
    fraction: 0.15
  - label: fringe
    fraction: 0.05
children_per_family: { min: 2, max: 5 }
endogamous: true
date_range:
  start: 1850
  end: 2025
seed: 42
```

The `Scenario` struct gains:

- `mode: Option<String>` — `"random"` (default when absent) or `"tree-centric"`
- `tree_buckets: Option<Vec<TreeBucketYaml>>`
- `endogamous: Option<bool>`

The `Scenario` methods become:

- `to_random_config() -> RandomConfig` (unchanged)
- `to_tree_config() -> TreeConfig` (returns actual config, not Option — panics
  if mode is not tree-centric)
- `to_generation_mode() -> GenerationMode` — dispatches based on `mode` field

### 8. Output metadata

The `GenerationResult` gains a `tree_stats: Option<Vec<TreeStat>>` field
(`None` for random mode, `Some` for tree-centric mode):

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct TreeStat {
    pub label: String,
    pub target_persons: usize,
    pub actual_persons: usize,
    pub actual_families: usize,
    pub actual_generations: usize,
}
```

This lets callers verify that the generated tree matches the requested distribution.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `person_count < 4` with tree-centric | Error: need at least 4 persons for a founding couple + 1 child + partner |
| `max_depth < 2` | Degrade gracefully: create a single-generation tree (just nuclear families) |
| Tree exhausts its person budget mid-generation | Wrap up the current generation; attach remaining budget as leaf children of existing families |
| Gender skew makes pairing impossible | Bias gender assignment, then (as last resort) create single-parent families for the remaining unpaired persons (only in fringe bucket; in core trees, reduce the tree depth instead) |
| `tree_buckets` sum rounds to < person_count | Distribute remainder to the largest tree |
| `tree_buckets` empty | Error: must specify at least one tree bucket |
| Cross-tree partner pool exhausted | Fall back to endogamous pairing within the requesting tree; emit warning |
| `--mode tree-centric` + `--adversarial` | Warning: adversarial transforms ignored in tree-centric mode (v1 limitation) |

---

## Implementation Plan

### Step 1: Extract `BaseGenConfig` and define `TreeConfig` + supporting types

**Files:** `crates/typed-graph/src/generate/base.rs` (new), `crates/typed-graph/src/generate/tree.rs` (new)

- Define `BaseGenConfig` with shared fields from `RandomConfig`.
- Update `RandomConfig` to embed `base: BaseGenConfig` (update all field
  accesses from `config.field` to `config.base.field` in `random.rs` and
  consumers).
- Define `TreeConfig`, `TreeBucket`, `TreeStat`, `TreeBuildResult`,
  `ChildAllocation`, `TreePlan` (intermediate representation).
- Implement `Default` for `TreeConfig`.
- Add unit tests for defaults, validation, and fraction normalization.
- Extend `GenerationResult` with `tree_stats: Option<Vec<TreeStat>>`.
- Re-export from `crates/typed-graph/src/generate/mod.rs`.

### Step 2: Make shared utilities `pub(crate)`

**Files:** `crates/typed-graph/src/generate/random.rs`

- Make `generate_given_name`, `generate_surname`, `generate_place`,
  `generate_events`, `birth_year_for_layer` `pub(crate)` so the tree
  builder can reuse them.
- Add `generate_person_with_gender` — generates a person node with an
  explicit gender parameter (wraps the existing `generate_random_person`
  logic but with an optional gender override).
- Make `PersonSummary` and its fields `pub(crate)`.

### Step 3: Implement the tree-centric generator

**Files:** `crates/typed-graph/src/generate/tree.rs`

- Implement `generate_tree_centric(config: &TreeConfig, schema: &Schema) -> Result<GenerationResult, GenerationError>`.
  The `schema` parameter is used for constructing nodes with correct field
  shapes, enum value conversions, and version-specific optional field handling
  (e.g., `DateValue` differences across schema versions).
- Implement `allocate_tree_buckets(...)` — Phase A.
- Implement `plan_tree(...)` — Phase B (structure planning, returns `TreePlan`).
- Implement `commit_tree(...)` — Phase B-prime (materializes `TreePlan` into
  `&mut Graph` via `GraphBuilder`).
- Implement `build_fringe(...)` — Phase C.2.
- Implement `build_partner_pool(...)` — Phase C.1 (cross-tree partner pool).
- Implement `distribute_children(...)` — as specified in §3 above.
- Implement `biased_gender(...)` and `opposite_gender(...)` — as specified in §4.
- All gender handling uses `gender_value` / `into_gender_field` helpers for
  multi-version schema compatibility.

> **Note on pseudocode:** The pseudocode in §3-§4 is conceptual. The actual
> implementation adapts these signatures to the real `GraphBuilder` API
> (`builder.add_person(handle).with_name(...).with_gender(...).build()`),
> the real `FamilyBuilder`, and the existing `assign_children` / `generate_events`
> patterns from `random.rs`.

### Step 4: Add `--mode` flag, CLI dispatch, and scenario YAML support

**Files:** `crates/cli/src/commands/generate.rs`, `crates/cli/src/main.rs`,
`crates/cli/src/scenario.rs`

- Add `--mode` enum (`random` / `tree-centric`), default `random`.
- Add `--tree-sizes`, `--endogamous` flags (only active in tree-centric mode).
- Introduce `GenerationMode` enum in `generate.rs`.
- Update `build_config()` to return `GenerationMode`.
- In `run()`, dispatch to `generate_tree_centric` when mode is tree-centric.
- Add validation: `--mode tree-centric` + `--adversarial` → warning.
- Add `mode: Option<String>`, `tree_buckets: Option<Vec<TreeBucketYaml>>`,
  `endogamous: Option<bool>` fields to `Scenario`.
- Implement `Scenario::to_tree_config() -> TreeConfig`.
- Implement `Scenario::to_generation_mode() -> GenerationMode`.
- Add progress start/finish messages for tree-centric mode.

### Step 5: Add tests

- **Unit tests** (`tree.rs`):
  - `tree_config_defaults` — verify default values.
  - `base_gen_config_shared_fields` — verify `BaseGenConfig` fields match across modes.
  - `allocate_tree_buckets_normalizes` — fractions sum to 1, integer rounding.
  - `allocate_tree_buckets_single_bucket` — 100% in one bucket.
  - `allocate_tree_buckets_remainder_to_largest` — remainder distribution.
  - `allocate_tree_buckets_equal_fractions` — 0.25 each × 4, rounding behavior.
  - `allocate_tree_buckets_rejects_empty` — empty buckets → error.
  - `allocate_tree_buckets_too_few_persons` — person_count < 4 → error.
  - `biased_gender_statistical` — 1000 samples, verify ~80% bias when imbalanced.
  - `opposite_gender_maps_correctly` — 0→1, 1→0.
  - `distribute_children_respects_budget` — sum ≤ remaining_budget.
  - `distribute_children_respects_range` — each count in [start, end].
  - `distribute_children_zero_budget` — all zeros returned.
  - `distribute_children_more_families_than_budget` — some families get 0 children.
  - `build_tree_produces_connected_graph` — BFS/DFS confirms single component.
  - `build_tree_respects_person_budget` — person count matches target.
  - `build_tree_max_depth_matches_config` — genealogical depth check.
  - `build_tree_children_per_family_in_range` — all families have 1–5 children.
  - `build_tree_gender_balanced` — gender ratio is within 45–55%.
  - `build_tree_endogamous_true_all_partners_in_tree` — no external persons.
  - `build_tree_tree_stats_populated` — `tree_stats` has correct labels and counts.
  - `build_fringe_creates_isolated_or_small_families` — fringe persons have ≤1 connection.
  - `generate_tree_centric_empty_buckets_error` — validation.
  - `generate_tree_centric_too_few_persons_error` — person_count < 4.

- **Property-based tests** (`tree.rs` — NOT deferred to future extensions):
  - `prop_tree_graph_validates` — for random valid configs, generated graph
    passes `graph.validate(&schema)` without structural or referential errors.
  - `prop_tree_person_budget_invariant` — `sum(tree_stats.actual_persons) ≤ config.person_count`.
  - `prop_tree_connected_endogamous` — for endogamous trees, BFS from any
    person reaches all other persons in the same tree.

- **Integration tests** (`generate.rs`):
  - `tree_centric_produces_large_connected_component` — 200 persons, largest
    component ≥ 100.
  - `tree_centric_default_distribution` — verify stats match expected buckets.
  - `tree_centric_seed_reproducibility` — same seed → same graph.
  - `tree_centric_mode_dispatch` — `GenerationMode::TreeCentric` dispatched
    correctly from CLI args.

- **E2E tests** (`e2e.rs`):
  - `e2e_generate_tree_centric_basic` — run CLI with `--mode tree-centric`.
  - `e2e_generate_tree_centric_custom_sizes` — run with `--tree-sizes`.

### Step 6: Run full test suite and manual verification

```bash
# All tests
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings

# Manual smoke tests
cargo run -- generate --mode tree-centric --count 200 --depth 4 --seed 42 -o /tmp/tree.gramps
cargo run -- generate --mode tree-centric --count 50 --depth 3 --tree-sizes "0.7,0.2,0.1" -o /tmp/tree2.gramps

# Verify connectedness: inspect XML manually or write a quick script to
# count connected components by following Person→Family→Person edges.
```

### Step 7: Update AGENTS.md

Add a note in AGENTS.md about the two generation modes and when to use each.

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Tree builder adds significant complexity to the codebase | Keep it in a separate `tree.rs` module; don't touch the existing random generator |
| Gender balancing creates unrealistic gender ratios in small trees | The 80/20 bias is only applied when there's a measurable imbalance; in large trees it should be undetectable |
| Endogamous trees don't look like real genealogy (real families have external marriages) | Make `endogamous` configurable (default `true`); when `false`, partners are allocated from the cross-tree partner pool, creating cross-tree connections |
| Binary explosion of tree size | The `distribute_children` function caps growth based on remaining budget, not exponential expansion |
| Tree-centric + adversarial strategies conflict | Document as unsupported in v1; tree-centric mode ignores `--adversarial` |
| `BaseGenConfig` extraction is a noisy refactor touching many files | Do it as Step 1 before any tree logic exists; verify all existing tests pass after the refactor before proceeding |

---

## Future Extensions (not in scope)

- **Inter-tree marriages**: When `endogamous: false`, partners can come from
  other trees, creating a super-component that connects multiple trees.
- **Immigration events**: Periodically introduce a new person from outside
  the tree as a spouse, simulating real-world migration patterns.
- **Tree age profiles**: Control how "old" each tree is (how many generations
  have passed since the founding couple).
- **Surname propagation**: Track and propagate surnames through male-line
  descent, creating realistic surname clusters within trees.
- **Visualization of tree structure**: ASCII-art or DOT output of the tree
  structure for debugging generation quality.
