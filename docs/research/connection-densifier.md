# Design Plan: Connection Densifier

## Problem Statement

The current generation algorithm produces datasets where most people are
isolated or in very small (2-3 person) family units, with only occasional
cross-generation chains. The primary goal is to flip this: **most people
should belong to 1-2 large interconnected trees**, there should optionally be
a medium-sized tree, and only a small minority should be isolated individuals
or tiny nuclear families.

### Root Causes

The existing `generate_random` is structurally incapable of producing large
trees reliably because:

- **No tree-level planning**: Persons and families are created independently,
  with no intent for them to connect into deep genealogical chains.
- **Even layer distribution**: Persons are divided evenly across temporal
  layers, creating shallow pools (e.g., 25 males and 25 females per layer
  with 200 people / 4 layers). `select_parents` quickly exhausts these pools.
- **`select_parents` prefers same-layer pairs**: Only falls back to adjacent
  layers when same-layer pairing fails. This creates flat nuclear families,
  not parent→child→grandchild chains.
- **Children are an afterthought**: Families are created first; children are
  assigned afterward from whoever happens to be eligible. No mechanism
  ensures families have children or that children become parents.
- **`max_parent_roles` defaults to 1**: No remarriage, limiting connection
  density.
- **Gender imbalance within layers**: Random gender assignment can skew a
  layer's ratio, reducing pairing possibilities.

### Approach: Post-generation connection densifier

Instead of replacing the generator, we add a **post-processing pass** that
operates on the already-generated graph. The densifier finds disconnected
components and merges them through graph-level operations — cross-component
marriages, orphan adoption, remarriage, and single-parent upgrades. This
requires no changes to the core generation logic; it runs after
`generate_random` completes.

| Aspect | Tree-centric plan | Connection densifier |
|--------|------------------|---------------------|
| New code | ~2,000+ lines across 6 files + config refactor | ~500-600 lines in one new file |
| Changes to existing code | Extract `BaseGenConfig`, new CLI `--mode` enum, scenario YAML changes | One new config struct, one optional CLI flag |
| Config surface | `TreeConfig` with tree buckets, depths, endogamy, partner pools | `DensifyConfig` with ~4 fields |
| Intermediate representation | `TreePlan`, `ChildAllocation`, budget math | None — graph operations only |
| Guarantees tree shape | Yes — explicit generation planning | No — post-hoc graph rewiring |
| Guarantees connectivity | Yes — by construction | Heuristic — configurable target |
| Reuses existing code | Partially (name/date/place generators) | Heavily (GraphBuilder, add_node, add_edge) |

### Desired Outcome

| Component | Share of Persons | Example (200 persons) |
|-----------|-----------------|----------------------|
| Large tree #1 | 60-80% | 120-160 people |
| Small trees (2-3) | 15-30% | 30-60 people |
| Isolated / tiny | 0-10% | 0-20 people |

The densifier targets connectivity percentage, not genealogical depth.
A `target_connectivity` of 0.85 means at least 85% of persons end up in
the largest connected component.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| New module vs. modify existing | New `densify.rs` in `generate/` | Clean separation; minimal signature addition to `random.rs` (`densify_config` parameter) |
| When to run | After `generate_random`, before validation Gate 1 | Follows the existing pipeline; validation catches any issues |
| Enable by default? | Yes, with configurable opt-out | The goal is better output for all users; `--no-densify` flag for adversarial testing |
| Connection target | Configurable fraction (0.85 default) | Different use cases need different densities |
| Component finding | BFS over Person↔Family↔Person edges | Straightforward graph traversal; no new dependencies |
| Gender handling | Use existing `gender_value` / `into_gender_field` helpers; gender=0 = Male, gender=1 = Female | Multi-version schema compatibility |
| Age constraint enforcement | Same parent-child age windows as `assign_children` | Maintains genealogical plausibility |
| Edge construction helpers | Use `make_child_ref()` and `make_event_ref()` for version-compatible ref structs | Required by multi-version schema — field shapes differ across 5.1/5.2 |
| Node creation | Use `GraphBuilder` for Family nodes (handles required-field init); raw `add_edge` for edges | Avoids incomplete node construction that would fail validation |
| Adversarial strategies | Run after densifier (existing pipeline order) | Category B strategies are validity-breaking; the densifier produces a valid graph for them to attack |
| Tracking skipped merges | `DensifyResult.components_skipped` field | Debugging visibility when target connectivity can't be reached |

---

## Specification

### 1. DensifyConfig

```rust
/// Configuration for the connection densifier post-processor.
///
/// Controls how aggressively the densifier merges disconnected
/// components in the generated family graph.
#[derive(Clone, Debug, PartialEq)]
pub struct DensifyConfig {
    /// Fraction of persons that should end up in the largest connected
    /// component. The densifier will attempt to merge components until
    /// this target is reached or no more merges are possible.
    ///
    /// Default: 0.85 (85% of persons in the largest component).
    pub target_connectivity: f64,

    /// Maximum number of families a person can be a parent in after
    /// densification. This overrides the generation-time `max_parent_roles`
    /// by allowing additional parent edges during densification.
    ///
    /// Default: 3 (allows remarriage).
    pub max_parent_roles: usize,

    /// Maximum age difference between spouses when merging components
    /// via cross-component marriage.
    ///
    /// Default: 20 (years).
    pub max_age_diff: i32,

    /// Whether the densifier should attempt to merge components across
    /// different temporal layers aggressively. When true, parents from
    /// any layer can be paired; when false, only adjacent-layer pairing
    /// is attempted.
    ///
    /// Default: true.
    pub allow_cross_generation: bool,

    /// Whether to attempt orphan adoption (attaching isolated persons
    /// as children of existing families).
    ///
    /// Default: true.
    pub adopt_orphans: bool,

    /// Whether to attempt upgrading single-parent childless families
    /// to two-parent by adding a spouse.
    ///
    /// Default: true.
    pub upgrade_single_parents: bool,
}

impl Default for DensifyConfig {
    fn default() -> Self {
        DensifyConfig {
            target_connectivity: 0.85,
            max_parent_roles: 3,
            max_age_diff: 20,
            allow_cross_generation: true,
            adopt_orphans: true,
            upgrade_single_parents: true,
        }
    }
}
```

### 2. Densification algorithm

The densifier runs in up to four passes, ordered by impact:

#### Pass 1: Find connected components

Build an undirected graph over Person nodes where an edge exists if two
persons share a Family (either as parents, or as parent-child):

1. Collect all Family nodes from the graph.
2. For each Family, collect the set of Person handles involved (father,
   mother, all children).
3. Build an adjacency list: for each Person, list all other Persons they
   share a Family with.
4. Run BFS to identify all connected components.
5. Sort components by size (persons per component), largest first.
6. For Pass 3 and Pass 4, a Family node is considered to *belong to* a
   component if at least one of its associated Person handles (father,
   mother, or any child) is in that component. If a Family has members
   in multiple components (possible after a merge), it is assigned to
   the component containing its father, or its mother, or its first
   child — whichever is first in the component set.
7. Compute the fraction of persons in the largest component.

If the largest component already meets `target_connectivity`, stop.

```
largest_fraction = largest_component.size() / total_persons
if largest_fraction >= target_connectivity → done
```

#### Pass 2: Cross-component marriage (highest impact)

For each non-largest component, attempt to merge it into the largest component
by creating a marriage between one person in the non-largest component and one
person in the largest component:

1. Iterate components in descending size order (largest first, excluding
   component #1 which is the target).
2. For each component C:
   a. Collect eligible males in C: `gender_value(person.gender) == 0`,
      `parent_count < max_parent_roles`.
   b. Collect eligible females in the largest component:
      `gender_value(person.gender) == 1`,
      `parent_count < max_parent_roles`.
   c. For each male-female pair, check:
      - Age difference ≤ `max_age_diff`
      - Father's minimum child-bearing year (birth+16) ≤ mother's
        maximum child-bearing year (birth+50)
   d. If a compatible pair is found:
      - Generate a new UUID v4 handle and a gramps_id (e.g.,
        `format!("F{:04}", densify_family_counter)`).
      - Create a `FamilyData` node using `GraphBuilder::add_family(&mut graph, family_handle)`
        with `.with_father(male_handle).with_mother(female_handle).build()`.
        `FamilyBuilder::build()` handles all required-field initialization
        (`type_field`, `family_rel_type`, `gramps_id`) and validates
        handle existence.
      - **Manually** add `Edge::PersonFamily` edges for both parents and
        push the family handle onto each parent's `family_list` via
        `graph.get_node_mut()` — the `FamilyBuilder` does NOT do this
        automatically (unlike `PersonBuilder`).
      - Component C is now merged.
   e. If no male-in-C→female-in-largest pair works, try the reverse
      (female-in-C→male-in-largest).
   f. If still no pair, skip this component (Pass 3 may handle it).
3. Recompute components and check `target_connectivity`. If met, stop.

Edge case: if eligible persons in the non-largest component are all the same
gender, the reverse-direction attempt (step e) may succeed. If both components
are single-gender, the merge fails.

#### Pass 3: Orphan adoption

For each isolated person (no parent family, no child family):

1. Collect all two-parent families in the largest component.
2. For each family, check if the person's birth year fits the parenting
   window: `> max(father_birth+16, mother_birth+16)` and `< mother_birth+50`.
3. If compatible, add the person as a child of the family:
   - Create a `ChildRef` metadata struct using `make_child_ref(child_handle.clone(), Some(ChildRefType::Birth))` (the version-safe helper from `graph.rs`).
   - Add a `FamilyChildRef { source: family_handle, target: child_handle, metadata: Box<ChildRef> }` edge via `graph.add_edge()`.
   - Mutate the child's `PersonData` via `graph.get_node_mut(&child_handle)`:
     push the family handle onto `parent_family_list`.
   - Mark person as no longer isolated.
4. Recompute components and check `target_connectivity`. If met, stop.

> **Plausibility note**: The same age-constraint logic from `assign_children`
> is used. Persons outside the window are skipped (no implausible children).

#### Pass 4: Single-parent upgrade + remarriage

For each single-parent childless family:

1. Find the parent's gender.
2. Scan for an eligible opposite-gender person (in any component) with
   `parent_count < max_parent_roles` and age difference ≤ `max_age_diff`.
3. If found:
   - Create a Family node using `GraphBuilder::add_family(&mut graph, family_handle)` with
     the existing parent and the new spouse.
   - Add the corresponding edge (`FamilyFather` or `FamilyMother`) — the
     builder handles this automatically.
   - **Manually** add `Edge::PersonFamily` edges for both parents and
     update both parents' `family_list` via `graph.get_node_mut()` — the
     `FamilyBuilder` does NOT do this automatically.
4. Now that the family has two parents, Pass 3 (orphan adoption) can assign
   children to it on a subsequent iteration.

For remarriage (persons already in one family joining another as a second
parent role):

1. Find persons with `parent_count < max_parent_roles`.
2. For each, find a compatible family (or eligible single parent) for a
   cross-component or intra-component second marriage.
3. Create the Family and edges as in Pass 2.

### 3. The `DensifyResult` struct

```rust
/// Result of the densification pass.
#[derive(Clone, Debug, PartialEq)]
pub struct DensifyResult {
    /// Number of connected components before densification.
    pub components_before: usize,
    /// Number of connected components after densification.
    pub components_after: usize,
    /// Size (persons) of the largest component before densification.
    pub largest_before: usize,
    /// Size (persons) of the largest component after densification.
    pub largest_after: usize,
    /// Number of new Family nodes created during densification.
    pub families_added: usize,
    /// Number of new edges added during densification.
    pub edges_added: usize,
    /// Number of orphans adopted into existing families.
    pub orphans_adopted: usize,
    /// Number of single-parent families upgraded to two-parent.
    pub single_parents_upgraded: usize,
    /// Number of components that could not be merged (skipped across all passes).
    pub components_skipped: usize,
}
```

### 4. Integration into the pipeline

The densifier is a separate, optional pass called after `generate_random`
and before the first validation gate:

```
Generate → Densify (new, optional) → Validate (Gate 1) → Adversarial → Validate (Gate 2) → Serialize
```

#### In `generate_random`

The `generate_random` function gains an optional `densify_config: Option<&DensifyConfig>`
parameter. When `Some`, it calls `densify_connections()` after all four
generation stages and includes `DensifyResult` in the returned `GenerationResult`.

```rust
pub fn generate_random(
    config: &RandomConfig,
    adversarial_config: &AdversarialConfig,
    densify_config: Option<&DensifyConfig>,
    schema: &crate::Schema,
) -> Result<GenerationResult, GenerationError> {
    // ... existing generation stages 1-4 ...

    // Compute the seed early (before the seed variable goes out of scope):
    let densify_seed = seed.wrapping_add(0xD3NS1F1ER);

    // Stage 4.5 (new): Connection densification
    let densify_result = if let Some(dc) = densify_config {
        let mut rng_densify = rand::rngs::StdRng::seed_from_u64(densify_seed);
        Some(densify_connections(&mut graph, dc, &mut rng_densify))
    } else {
        None
    };

    // Recompute stats AFTER densification so they reflect the final graph.
    // Densification may add families and edges, so the GenerationStats
    // counters built during stages 1-4 are stale if densification ran.
    let stats = if densify_result.is_some() {
        collect_stats(&graph)
    } else {
        // ... existing stats from stages 1-4 ...
    };

    // ... existing stage 5 (adversarial), return ...
}
```

The `GenerationResult` struct gains:

```rust
pub densify_result: Option<DensifyResult>,
```

**Stats recomputation**: When `densify_config` is provided, generation
stats (`person_count`, `family_count`, `edge_count`, etc.) are recomputed
from the graph *after* densification via a `collect_stats(&graph)` helper.
Without densification, the existing stage counters are used unchanged.

#### In the CLI

A new `--no-densify` flag disables densification:

```
gramps-gen generate --count 200 --no-densify    # existing behavior
gramps-gen generate --count 200                  # densified by default
gramps-gen generate --count 200 --connectivity-target 0.90  # aggressive
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--no-densify` | bool | `false` | Disable the connection densifier |
| `--connectivity-target` | f64 | `0.85` | Fraction of persons to place in largest component |
| `--densify-max-parent-roles` | usize | `3` | Max families per person after densification |

When `--no-densify` is set, the pipeline runs exactly as before (backward
compatible). The `--adversarial` flag is compatible with densification:
the densifier runs first (producing a valid, well-connected graph), then
adversarial strategies can break validity as usual.

#### In scenario YAML

```yaml
mode: random
person_count: 200
generations: 4
densify:
  enabled: true
  target_connectivity: 0.90
  max_parent_roles: 3
```

The existing `Scenario` struct gains an optional `densify: Option<DensifyConfigYaml>`
field. When absent, densification uses defaults. When `enabled: false`, no
densification runs.

### 5. Component-finding BFS

```rust
/// Find connected components in the family graph.
///
/// Two persons are in the same component if there exists a path through
/// Family nodes (parent↔family or child↔family edges).
fn find_components(graph: &Graph) -> Vec<Vec<Handle>> {
    // 1. Build adjacency list: Person → [neighbor Person handles]
    //    - For each Family node:
    //      - Add edges between father and mother (if both present)
    //      - Add edges between each parent and each child
    //      - Add edges between siblings (shared parent-family)
    // 2. BFS from each unvisited Person to find its component
    // 3. Return components sorted by size (persons), descending
}
```

### 6. Age-constraint helpers

Reuse or mirror the existing constraints from `random.rs`:

```rust
/// Check whether two persons are a compatible parent pair.
fn is_compatible_pair(
    graph: &Graph,
    male_handle: &Handle,
    female_handle: &Handle,
    max_age_diff: i32,
) -> bool {
    let male_birth = get_person_birth_year(graph, male_handle);
    let female_birth = get_person_birth_year(graph, female_handle);

    if let (Some(mb), Some(fb)) = (male_birth, female_birth) {
        let age_diff = (mb - fb).abs();
        if age_diff > max_age_diff {
            return false;
        }
        // Father must be able to have a child who could be mothered by mother
        let father_min = mb + 16;
        let mother_max = fb + 50;
        father_min <= mother_max
    } else {
        // Missing birth dates: be permissive
        true
    }
}

/// Check whether a person could be a child of the given parents.
fn is_compatible_child(
    graph: &Graph,
    child_handle: &Handle,
    father_handle: &Handle,
    mother_handle: &Handle,
) -> bool {
    let child_birth = get_person_birth_year(graph, child_handle);
    let father_birth = get_person_birth_year(graph, father_handle);
    let mother_birth = get_person_birth_year(graph, mother_handle);

    if let (Some(cb), Some(fb), Some(mb)) = (child_birth, father_birth, mother_birth) {
        let min_child_year = std::cmp::max(fb + 16, mb + 16);
        let max_child_year = mb + 50;
        cb > min_child_year && cb < max_child_year
    } else {
        // Missing birth dates: be permissive
        true
    }
}
```

### 7. Parent-count tracking

The densifier needs to know how many families each person is already a parent
in. This is computed from the graph at the start of densification:

```rust
fn parent_count(graph: &Graph, handle: &Handle) -> usize {
    match graph.get_node(handle) {
        Some(Node::Person(p)) => p.family_list.len(),
        _ => 0,
    }
}
```

Unlike `generate_random`'s in-memory `PersonSummary::parent_count`, this
reads directly from the graph, so it's always accurate — even after the
densifier adds new families. Re-reading the graph after each merge ensures
that concurrent eligibility checks never see stale counts.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `target_connectivity` not reached after all passes | Emit warning with achieved connectivity; return partial `DensifyResult` with `components_skipped` count |
| No eligible cross-component pair found | Skip component; increment `components_skipped`; Pass 3 or 4 may still merge it |
| All persons in a component are the same gender | Component cannot be merged via marriage; increment `components_skipped`; try orphan adoption from that component into the largest component |
| No two-parent families exist for orphan adoption | Skip Pass 3 |
| `max_parent_roles` set to 0 | Densifier is a no-op |
| Empty graph (0 persons) | Return `DensifyResult` with all zeros |
| `DensifyConfig` has `target_connectivity > 1.0` | Clamp to 1.0 |

---

## Implementation Plan

### Step 1: Define `DensifyConfig` and `DensifyResult`

**Files:** `crates/typed-graph/src/generate/densify.rs` (new)

- Define `DensifyConfig` struct with `Default` impl.
- Define `DensifyResult` struct.
- Add unit tests for defaults.
- Re-export from `crates/typed-graph/src/generate/mod.rs`.

### Step 2: Implement component finding

**Files:** `crates/typed-graph/src/generate/densify.rs`

- Implement `find_components(graph: &Graph) -> Vec<Vec<Handle>>`.
- Implement adjacency-list builder from Family nodes.
- Unit tests: single component, two disconnected components, empty graph,
  isolated person as its own component.

### Step 3: Implement cross-component marriage (Pass 2)

**Files:** `crates/typed-graph/src/generate/densify.rs`

- Implement `merge_components_via_marriage()`.
- Implement `is_compatible_pair()` age-constraint helper (using
  `gender_value()` for version-safe gender comparison).
- Create Family nodes via `GraphBuilder::add_family(handle)` for robust
  required-field initialization; add edges with raw `add_edge`.
- **After creating the family**, manually add `Edge::PersonFamily` edges
  for both parents and push the family handle onto each parent's
  `family_list` via `graph.get_node_mut()` — the `FamilyBuilder` does NOT
  do this automatically.
- Unit tests: two components merged into one, incompatible components skipped,
  gender-homogeneous component handled.
- Schema-aware test: call `graph.validate(&schema)` after each test merge
  and assert `ValidationState::Valid`.

### Step 4: Implement orphan adoption (Pass 3)

**Files:** `crates/typed-graph/src/generate/densify.rs`

- Implement `adopt_orphans()`.
- Implement `is_compatible_child()` helper.
- Use `make_child_ref()` (version-compatible helper) for `ChildRef` metadata.
- Unit tests: compatible child adopted, incompatible child skipped,
  child with missing dates adopted (permissive).
- Schema-aware test: call `graph.validate(&schema)` after adoption
  and assert `ValidationState::Valid`.

### Step 5: Implement single-parent upgrade + remarriage (Pass 4)

**Files:** `crates/typed-graph/src/generate/densify.rs`

- Implement `upgrade_single_parents()`.
- Implement `add_remarriage_edges()`.
- **After creating families**, manually add `Edge::PersonFamily` edges
  and update `family_list` on both parents via `graph.get_node_mut()`.
- Unit tests: single father upgraded to two-parent, single mother upgraded,
  remarriage within component.
- Schema-aware test: call `graph.validate(&schema)` after upgrade
  and assert `ValidationState::Valid`.

### Step 6: Implement top-level `densify_connections()`

**Files:** `crates/typed-graph/src/generate/densify.rs`

- Implement `densify_connections()` orchestrating passes 1-4.
- Return `DensifyResult` with before/after metrics.
- Unit tests: full densification pipeline, no-op when already connected,
  connectivity target reached, connectivity target unreachable.
- Test that `DensifyResult` field counts match the actual graph changes
  (e.g., `families_added` equals the difference in `node_count()`).

### Step 7: Add property-based tests

**Files:** `crates/typed-graph/src/generate/densify.rs`

- `prop_densified_graph_validates` — for random valid configs, densified
  graph passes `graph.validate(&schema)`.
- `prop_densify_increases_connectivity` — largest component size never
  decreases after densification.
- `prop_densify_never_removes_persons` — person count unchanged.
- `prop_densify_seed_determinism` — same seed → same densified graph.

### Step 8: Integrate into `generate_random` and `GenerationResult`

**Files:** `crates/typed-graph/src/generate/random.rs`

- Make `get_person_birth_year` and `collect_stats` `pub(crate)` so the
  densifier module can use them.
- Add `densify_config: Option<&DensifyConfig>` parameter to `generate_random`.
- Add `densify_result: Option<DensifyResult>` to `GenerationResult`.
- Call `densify_connections()` after Stage 4 (event generation) and before
  adversarial application.
- Produce a separate RNG for the densifier (seed derived from main seed).
- Update existing tests to pass `None` for densify_config.
- Add tests that exercise `generate_random` with densification enabled:
  - `generate_random_densified_produces_large_component`
  - `generate_random_densified_seed_reproducibility`
  - `generate_random_densified_validates_ok`

### Step 9: Add CLI flags

**Files:** `crates/cli/src/commands/generate.rs`, `crates/cli/src/main.rs`

- Add `--no-densify` flag (default: `false` — densification enabled).
- Add `--connectivity-target` flag (default: `0.85`).
- Add `--densify-max-parent-roles` flag (default: `3`).
- Build `DensifyConfig` from CLI args in `build_config()`.
- Pass to `generate_random()`.
- Verify `--help` output shows the new flags.
- Update E2E tests to verify flags work.

### Step 10: Add scenario YAML support

**Files:** `crates/cli/src/scenario.rs`

- Add `densify: Option<DensifyConfigYaml>` to `Scenario`.
- Wire into `to_random_config()` → `DensifyConfig`.

### Step 11: Run full test suite

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings

# Manual smoke tests
cargo run -- generate --count 200 --seed 42 -o /tmp/dense.gramps
cargo run -- generate --count 200 --seed 42 --no-densify -o /tmp/sparse.gramps
```

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Densifier can't reach target connectivity if initial graph is too fragmented | Make `target_connectivity` configurable; emit warning with achieved fraction |
| Cross-component marriages may create implausible age gaps | Apply the same `max_age_diff` and parenting-window constraints already used by `select_parents` |
| Added edges may cause validation failures | Run `graph.validate()` after densifying in tests; use property-based tests to verify across random configurations |
| Dense graph may look "over-connected" (too many marriages per person) | Cap `max_parent_roles` (default 3); the user can lower it |
| Densifier changes behavior for users who depend on current sparse output | `--no-densify` flag restores exact current behavior |
| RNG determinism: densifier's random choices depend on a derived seed | Derive from main seed via `wrapping_add(constant)`; document that `--seed N` with and without `--no-densify` produce different graphs |
| Dense graph may expose bugs in the XML serializer or Gramps import | Already tested by adversarial `max_ref_chains` and `deep_nesting` strategies which also create highly connected graphs |

---

## Future Extensions (not in scope)

- **Cross-component child adoption**: Currently orphan adoption only attaches
  children to parents, not vice versa. A future extension could find isolated
  persons to serve as parents for parentless children.
- **Sibling marriage prevention**: The densifier could track sibling sets
  (shared parent family) and avoid creating marriages between siblings.
- **Genealogical depth targeting**: Track the longest parent→child→grandchild
  chain and bias merges toward extending it.
- **Densification statistics in XML metadata**: Include `DensifyResult` fields
  as comments or processing instructions in the Gramps XML output.
