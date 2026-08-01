# Design Plan: Family Connection Density Parameters

## Problem Statement

With `--count 16 --seed 2026 --depth 4`, the generated Gramps XML produces a
largest connected tree of only 3 people across 2 generations despite requesting
4 generations. The generation algorithm has several structural constraints that
limit connection density:

1. **Fixed family count** — `family_count = person_count / 2` with no CLI override.
2. **One parent role per person** — `is_parent: bool` prevents remarriage and
   step-families.
3. **Random family layer assignment** — each family picks a random layer
   uniformly, so families don't coordinate across generations to build deep trees.
4. **Small per-layer population** — with 4 people per layer and gender
   constraints, parent-pair selection frequently fails and falls through to
   single-parent families (which skip child assignment).

## Design Decisions

After review, the following mechanisms were selected:

| Mechanism | Decision | Rationale |
|-----------|----------|-----------|
| Family count | `--family-ratio` flag, ratio 1.0 = 1 family per person | Natural scale; users pick < 1.0 for sparser or = 0.5 for current default |
| Children per family | Keep current 1..4 default | Not the primary bottleneck; other changes drive density better |
| Parent roles | `--max-parent-roles` flag, default 1 | Allows remarriage / step-families as opt-in |
| Layer assignment | Layer-linking mode (always on when `--depth > 1`) | Guarantees cross-generation chains |

---

## Specification

### 1. `--family-ratio` CLI flag

**Type:** `f64`
**Default:** `0.5` (current behavior: `family_count = person_count / 2`)
**Range:** `> 0.0` and `≤ 1.0` (validated at CLI parse time)
**Semantics:** `family_count = (person_count as f64 * ratio).round() as usize`

Examples:

| `--count` | `--family-ratio` | Resulting `family_count` |
|-----------|-------------------|--------------------------|
| 16 | 0.5 (default) | 8 |
| 16 | 1.0 | 16 |
| 16 | 0.25 | 4 |
| 16 | 0.75 | 12 |

Setting `family_count = person_count` (ratio 1.0) means every person can be a
parent. Combined with `--max-parent-roles 2`, this enables dense, multiply-
connected webs.

**Implementation note:** The `family_count` field already exists in
`RandomConfig`. CLI exposure and the computation formula change in
`build_config()`. Additionally, the internal cap in `generate_random()`
(`families_to_create = config.family_count.min(persons.len() / 2)`) must be
adjusted to account for `max_parent_roles`. With the current `is_parent: bool`,
each family needs 2 unique parents so the cap is `persons.len() / 2`. With
`max_parent_roles > 1`, parents can be reused, so the cap should become
`config.family_count.min(persons.len() * config.max_parent_roles / 2)`.

`family_ratio` is also added to `RandomConfig` (type `f64`, default `0.5`),
so non-CLI callers (scenarios, direct API) can configure it. The existing
`family_count` field on `RandomConfig` changes default from `100` to `0`
(sentinel); when `family_count == 0`, it is computed from the ratio at
use-site.

**Validation** (in `build_config()`, using `CliError::ConfigError`):

- `family_ratio <= 0.0` → error: `"family-ratio must be > 0.0, got {ratio}"`
- `family_ratio > 1.0` → error: `"family-ratio must be <= 1.0, got {ratio}"`
- `family_ratio` is `f64`, validated at `build_config()` time (not Clap parse
  time, since Clap cannot enforce a float range).

**Edge case — `--count 1`:** With `--count 1 --family-ratio 0.5`, the formula
gives `max(round(0.5), 1) = 1`, but the internal cap still limits families to
`min(1, persons.len()/2) = 0`. A single person cannot form a family regardless
of ratio. Document as a known limitation; no special handling needed.

### 2. `--max-parent-roles` CLI flag

**Type:** `usize`
**Default:** `1` (current behavior)
**Range:** `≥ 1`
**Semantics:** Maximum number of families a person can be a parent in.
Setting to `2` allows one remarriage (e.g., widow remarries); `3` allows two, etc.

**Internal change:** `PersonSummary.is_parent: bool` → `parent_count: u32`.
Add a separate `is_solo: bool` field to `PersonSummary` (default `false`).
Filter in `select_parents` and `create_single_parent_family` changes from
`!s.is_parent` to `!s.is_solo && s.parent_count < max_parent_roles`. On parent
selection, increment `parent_count` instead of setting `is_parent = true`.

Solo persons (from the solo-persons adversarial strategy) set `is_solo = true`
instead of incrementing `parent_count`. With `max_parent_roles > 1`,
incrementing `parent_count` by 1 would still leave the solo person eligible
as parent in `max_parent_roles - 1` families, which defeats the purpose.
Using a dedicated boolean keeps the solo-exclusion intent independent of the
parent-role counter.

Examples:

| `--max-parent-roles` | Effect |
|---------------------|--------|
| 1 (default) | Current behavior: one family per person as parent |
| 2 | Each person can be in two families as father/mother |
| 3 | Three parent roles per person |

### 3. Layer-linking mode

**Type:** New field `layer_linking: bool` on `RandomConfig` (default `false`).
**Activation:** Always enabled when `--depth > 1`. No separate CLI flag;
controlled implicitly by the depth parameter. (Rationale: if you ask for depth
> 1, you want a multi-generation tree, and random layer assignment contradicts
that goal.)

**Semantics:** Instead of each family picking a random layer via
`rng.gen_range(0..generations)`, families are assigned to layers in sequential
descending order, wrapping around:

```
family 0 → layer (generations - 1)   [oldest cohort]
family 1 → layer (generations - 2)
...
family (generations-1) → layer 0     [youngest cohort]
family (generations) → layer (generations - 1)  [wraps]
```

This guarantees every layer gets at least one family, and family ordering
naturally produces parent-child chains: the family at the oldest layer provides
parents; those parents' children sit in younger layers, where they become
available as parents for the next families.

**How chains form:** Because family creation (parent selection) happens before
child assignment, a layer-N person can be both:

- A child in a family at layer N+1 (oldest), and
- A parent in a family at layer N (younger)

This is the exact genealogy pattern needed for deep trees.

**Compatibility with `--family-ratio`:** With a higher ratio, more families
wrap around. For example, `--count 16 --depth 4 --family-ratio 1.0` creates
16 families, cycling 4 times through the 4 layers → 4 families per layer.

### 4. Updated default `family_count` formula

Add `family_ratio: f64` to `RandomConfig` (default `0.5`). The existing
`family_count` field on `RandomConfig` changes its default from `100` to `0`
(sentinel meaning "not explicitly set"). When `family_count == 0` or when
`family_ratio` is explicitly set by the caller, `family_count` is computed as
`(person_count as f64 * family_ratio).round().max(1) as usize`.

For CLI usage, `build_config()` always computes `family_count` from the ratio,
overriding `RandomConfig::default().family_count`. For non-CLI callers
(scenarios, direct API), `family_ratio` provides the same configurability.

Convenience method on `RandomConfig`:

```rust
impl RandomConfig {
    /// Resolve family_count from family_ratio if not explicitly set.
    pub fn resolve_family_count(&mut self) {
        if self.family_count == 0 {
            self.family_count = (self.person_count as f64 * self.family_ratio)
                .round().max(1.0) as usize;
        }
    }
}
```

`generate_random()` calls `config.resolve_family_count()` (or inlines the
logic) before the family creation loop. This eliminates the ambiguity of
the old 100-default sentinel.

---

## Implementation Plan

### Step 1: Add `--family-ratio` to CLI

**Files:** `crates/cli/src/main.rs` (GenerateArgs), `crates/cli/src/commands/generate.rs`,
`crates/typed-graph/src/generate/random.rs` (RandomConfig)

- Add `#[arg(long, default_value = "0.5")] pub family_ratio: f64` to `GenerateArgs`.
- Add `family_ratio: f64` field to `RandomConfig` (default `0.5`).
- Change `RandomConfig::default().family_count` from `100` to `0` (sentinel).
- In `build_config()`: validate `0.0 < ratio <= 1.0`, compute
  `family_count = (args.count as f64 * args.family_ratio).round().max(1) as usize`.
  Use `CliError::ConfigError` for validation failures with messages:
  `"family-ratio must be > 0.0, got {ratio}"` and
  `"family-ratio must be <= 1.0, got {ratio}"`.
- Pass `family_ratio` and `family_count` to `RandomConfig`.
- In `generate_random()`: call `config.resolve_family_count()` (or inline) to
  compute `family_count` from ratio when `family_count == 0`. Also adjust the
  internal cap from `persons.len() / 2` to
  `persons.len() * config.max_parent_roles / 2` to account for parent reuse.
- In `Scenario::to_random_config()`: thread through `family_ratio` from new
  optional YAML field `family_ratio: Option<f64>` (see Backward Compatibility).
- Run existing tests to confirm default behavior is unchanged (ratio 0.5 with
  count 200 produces family_count 100, same as before).

### Step 2: Add `--max-parent-roles` to CLI

**Files:** `crates/cli/src/main.rs`, `crates/cli/src/commands/generate.rs`,
`crates/typed-graph/src/generate/random.rs`

- Add `#[arg(long, default_value = "1")] pub max_parent_roles: usize` to `GenerateArgs`.
- Add `max_parent_roles: usize` field to `RandomConfig` (default 1).
- In `PersonSummary`: change `is_parent: bool` → `parent_count: u32`; add
  `is_solo: bool` field (default `false`).
- In `select_parents()`: change filter from `!s.is_parent` to
  `!s.is_solo && s.parent_count < config.max_parent_roles as u32`.
- In `create_single_parent_family()`: same filter change.
- In `generate_family()` and `create_single_parent_family()`: change
  `summary.is_parent = true` → `summary.parent_count += 1`.
- In solo-persons marking: change `is_parent = true` → `is_solo = true`
  (uses the new `is_solo` field, not `parent_count`, because a solo person
  must be excluded regardless of `max_parent_roles`).
- Update all tests referencing `is_parent` to use `parent_count` (do this
  in the same step so tests pass before proceeding).

### Step 3: Implement layer-linking mode

**Files:** `crates/typed-graph/src/generate/random.rs`

- Add `layer_linking: bool` to `RandomConfig` (default `false`).
- In `build_config()`: set `layer_linking = args.depth > 1`.
- In `generate_random()`, inside the family creation loop:

  ```rust
  let layer = if config.layer_linking {
      (config.generations - 1) - (family_index % config.generations)
  } else {
      rng.gen_range(0..config.generations)
  };
  ```

- Add a `family_index` counter to the loop (already implicit in the loop;
  use `.enumerate()`).

### Step 4: Add new tests

- **`random_config_defaults` test**: add assertions for new fields
  (`max_parent_roles == 1`, `layer_linking == false`, `family_ratio == 0.5`,
  `family_count == 0`).
- **`random_config_custom` test**: add custom values including `family_ratio`.
- **`resolve_family_count` test**: verify that `family_count` is computed
  correctly from `person_count * family_ratio` when `family_count == 0`,
  and that an explicitly set `family_count` is not overridden.
- **`family_ratio_validation` tests**:
  - `family_ratio_zero_rejected` — `build_config` returns error for ratio 0.0.
  - `family_ratio_negative_rejected` — error for negative ratio.
  - `family_ratio_above_one_rejected` — error for ratio > 1.0.
  - `family_ratio_valid_accepted` — 0.5 produces expected `family_count`.
- **New test `layer_linking_sequential_assignment`**: verify with 4 layers,
  8 families, that each layer gets exactly 2 families.
- **New test `max_parent_roles_allows_remarriage`**: verify with
  `max_parent_roles = 2`, that a person can be parent in 2 families.
- **New test `solo_persons_excluded_regardless_of_max_parent_roles`**: verify
  that a solo person (is_solo = true) is never selected as parent even with
  `max_parent_roles > 1`.
- **New integration test `generate_depth_4_produces_deep_tree`**: run
  `generate_random` with `--count 16 --seed 2026 --depth 4` (the original
  problematic config) and assert the largest connected component size ≥ 8
  (at least half the person count). This prevents regression to the original
  bug where only 3 people were connected.

### Step 5: Update schema version helpers (minor)

- No changes to schema version helpers are needed. The new fields are
  generation-level parameters, not schema-version-specific.

### Step 6: Run full test suite

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

### Step 7: Run full test suite and manual verification

```bash
# Automated: all tests including the new regression test
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings

# Manual smoke tests:
# Original problematic case — should now produce deeper trees:
cargo run -- generate --count 16 --seed 2026 --depth 4 --family-ratio 0.5 -o /tmp/test.gramps

# High density test:
cargo run -- generate --count 16 --seed 2026 --depth 4 --family-ratio 1.0 --max-parent-roles 2 -o /tmp/dense.gramps

# Verify with gramps-gen validate or xmllint
# Expected: largest connected component > 3 people
# The automated integration test in Step 4 covers this regression case.
```

---

## Backward Compatibility

| Aspect | Compatible? |
|--------|-------------|
| Default `--family-ratio 0.5` | Yes — produces same `family_count` as before (`person_count / 2`) |
| Default `--max-parent-roles 1` | Yes — same behavior as `is_parent: bool` |
| Layer-linking with `--depth 1` | Not activated (depth must be > 1) |
| Layer-linking with `--depth > 1` | **Behavior change** — families no longer random per layer. This is intentional and desirable. |
| Scenario YAML files | New optional fields: `family_ratio: f64` (default `0.5`), `max_parent_roles: usize` (default `1`). `layer_linking` is implicit from `depth > 1` and not exposed in YAML. All fields are optional and default to current behavior. The `Scenario` struct gains `family_ratio: Option<f64>` and `max_parent_roles: Option<usize>`, threaded through `to_random_config()`. |

The only behavior change is layer-linking activating automatically for
`--depth > 1`. This is the correct default: requesting depth > 1 means you
want a multi-generation tree, and random layer assignment produces shallow
trees by not coordinating families across layers.

---

## Future Extensions (not in scope)

- **`--max-child-roles`**: Like `--max-parent-roles` but for child assignment,
  allowing step-family children to appear in multiple families.
- **Gender-skew parameter**: Control the male:female ratio to affect parent
  pairing probability.
- **Connected-component target**: A parameter that guarantees a specific
  number or fraction of persons in the largest connected component.
