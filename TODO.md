# Implementation Plan: Family Connection Density Parameters

Source: `docs/research/family-connection-density.md`

## Summary

Add three mechanisms to increase family connection density in generated Gramps
XML: `--family-ratio` (control family count per person), `--max-parent-roles`
(allow remarriage/step-families), and layer-linking mode (coordinate families
across layers to build deep multi-generation trees). The original bug — `--count
16 --seed 2026 --depth 4` producing a largest connected tree of only 3 people —
is the primary regression target.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add --family-ratio CLI flag for family density control` | Family-ratio config | `crates/cli/src/main.rs`, `crates/cli/src/commands/generate.rs`, `crates/typed-graph/src/generate/random.rs`, `crates/cli/src/scenario.rs` | Unit |
| 2 | `feat: add --max-parent-roles CLI flag to allow remarriage` | Max parent roles | `crates/cli/src/main.rs`, `crates/cli/src/commands/generate.rs`, `crates/typed-graph/src/generate/random.rs`, `crates/cli/src/scenario.rs` | Unit |
| 3 | `feat: implement layer-linking mode for sequential family assignment` | Layer-linking mode | `crates/typed-graph/src/generate/random.rs`, `crates/cli/src/commands/generate.rs`, `crates/cli/src/scenario.rs` | Unit |
| 4 | `test: add deep-tree regression test for count 16 depth 4` | Deep-tree regression test | `crates/cli/tests/integration.rs` | Integration |
| 5 | `chore: verify full test suite and clippy` | Verification | — (run `cargo test --workspace`, `cargo clippy --all-targets --all-features -- -D warnings`) | — |
| 6 | Manual verification with Gramps GUI | Manual verification | — | — |

## Step Details

### Step 1 — `--family-ratio` CLI flag

**Rationale:** `family_count = person_count / 2` is fixed with no CLI override.
This flag lets users control family density, with `--family-ratio 1.0` creating
as many families as persons.

**Files:**

- `crates/cli/src/main.rs`
- `crates/cli/src/commands/generate.rs`
- `crates/typed-graph/src/generate/random.rs`
- `crates/cli/src/scenario.rs`

**Changes in `main.rs` (GenerateArgs):**

- Add `#[arg(long, default_value = "0.5")] pub family_ratio: f64` field.

**Changes in `random.rs`:**

- Add `family_ratio: f64` field to `RandomConfig` (default `0.5`).
- Change `RandomConfig::default().family_count` from `100` to `0` (sentinel meaning "not explicitly set").
- Add `resolve_family_count()` method (or inline its logic):

  ```rust
  pub fn resolve_family_count(&self) -> usize {
      if self.family_count == 0 {
          (self.person_count as f64 * self.family_ratio).round().max(1.0) as usize
      } else {
          self.family_count
      }
  }
  ```

  **Note:** `generate_random()` takes `&RandomConfig`, so a `&self` method
  returning `usize` is preferred over `&mut self`. Call it to get the resolved
  count before the family creation loop, storing in a local variable.

- In `generate_random()`: call `config.resolve_family_count()` and use the
  result for `families_to_create`. The cap remains `persons.len() / 2` for now
  (max_parent_roles doesn't exist yet — the cap formula changes in Step 2).

**Changes in `generate.rs` (build_config):**

- In `build_config()` (CLI path): validate `family_ratio`:
  - `ratio <= 0.0` → error: `"family-ratio must be > 0.0, got {ratio}"`
  - `ratio > 1.0` → error: `"family-ratio must be <= 1.0, got {ratio}"`
  - Use `CliError::ConfigError` for validation failures.
- Compute `family_count = (args.count as f64 * args.family_ratio).round().max(1) as usize`.
- Pass `family_ratio` and `family_count` to `RandomConfig`.

**Changes in `scenario.rs`:**

- Add `family_ratio: Option<f64>` field to `Scenario` struct.
- In `to_random_config()`: thread `family_ratio` through (`self.family_ratio.unwrap_or(base.family_ratio)`).
  **Note:** The scenario path does not validate ratio bounds; invalid values
  will be caught by `generate_random()`'s config validation (or produce a
  constrained family count).

**Tests:**

- Update `random_config_defaults`: assert `family_count == 0`, `family_ratio == 0.5`.
- Update `random_config_custom`: include custom `family_ratio` value.
- Add `resolve_family_count` test: verify computation from `person_count * ratio`
  when `family_count == 0`, and that explicit `family_count` is not overridden.
- Add `family_ratio_validation` tests:
  - `family_ratio_zero_rejected` — build_config returns error for ratio 0.0.
  - `family_ratio_negative_rejected` — error for negative ratio.
  - `family_ratio_above_one_rejected` — error for ratio > 1.0.
  - `family_ratio_valid_accepted` — 0.5 produces expected `family_count`.
- Update `generate_command_build_config_from_args` in `generate.rs` tests:
  add `family_ratio: 0.5` to the `GenerateArgs` literal.
- Update `generate_command_empty_output_path` and `generate_command_zero_count_rejected`:
  add `family_ratio: 0.5` to the `GenerateArgs` literals.

### Step 2 — `--max-parent-roles` CLI flag

**Rationale:** `PersonSummary.is_parent: bool` prevents remarriage and
step-families. This flag allows a person to be a parent in multiple families.

**Files:**

- `crates/cli/src/main.rs`
- `crates/cli/src/commands/generate.rs`
- `crates/typed-graph/src/generate/random.rs`
- `crates/cli/src/scenario.rs`

**Changes in `main.rs` (GenerateArgs):**

- Add `#[arg(long, default_value = "1")] pub max_parent_roles: usize` field.

**Changes in `random.rs`:**

- Add `max_parent_roles: usize` to `RandomConfig` (default `1`).
- **PersonSummary refactor:**
  - `is_parent: bool` → `parent_count: u32`
  - Add `is_solo: bool` field (default `false`)
- In `select_parents()`: change filter from `!s.is_parent` to
  `!s.is_solo && s.parent_count < config.max_parent_roles as u32`.
- In `create_single_parent_family()`: same filter change.
- In `generate_family()`: change `summary.is_parent = true` → `summary.parent_count += 1`
  (all three branches: two-parent, skip-father one-parent, skip-mother one-parent).
- In `create_single_parent_family()`: change `summary.is_parent = true` → `summary.parent_count += 1`.
- In solo-persons marking (inside `generate_random()`): change
  `persons[idx].1.is_parent = true` → `persons[idx].1.is_solo = true`.
  (A solo person must be excluded regardless of `max_parent_roles`, so a
  dedicated boolean is needed.)
- **Cap adjustment:** Change `families_to_create` from
  `config.family_count.min(persons.len() / 2)` to
  `config.family_count.min(persons.len() * config.max_parent_roles / 2)`.
  With `max_parent_roles=1`, this is equivalent to the old formula.

**Changes in `generate.rs` (build_config):**

- Pass `max_parent_roles` to `RandomConfig`.
- No validation needed (any `usize` ≥ 1 is valid; Clap rejects `0` for `usize`).

**Changes in `scenario.rs`:**

- Add `max_parent_roles: Option<usize>` field to `Scenario` struct.
- In `to_random_config()`: thread through (`self.max_parent_roles.unwrap_or(base.max_parent_roles)`).

**Tests:**

- Update `random_config_defaults`: assert `max_parent_roles == 1`.
- Update `random_config_custom`: include custom `max_parent_roles` value.
- Update ALL test `PersonSummary` literals in `random.rs` tests:
  - `is_parent: false` → `parent_count: 0`
  - Add `is_solo: false` (all ~15+ instances).
- Update `generate_command_build_config_from_args` and other `GenerateArgs` tests:
  add `max_parent_roles: 1` to the literals.
- Add `max_parent_roles_allows_remarriage` test: verify that with
  `max_parent_roles = 2`, a person can be parent in 2 families.
- Add `solo_persons_excluded_regardless_of_max_parent_roles` test: verify that
  a solo person (`is_solo = true`) is never selected as parent even with
  `max_parent_roles > 1`.

### Step 3 — Layer-linking mode

**Rationale:** Random uniform layer assignment means families don't coordinate
across generations. Sequential layer assignment guarantees cross-generation
chains.

**Files:**

- `crates/typed-graph/src/generate/random.rs`
- `crates/cli/src/commands/generate.rs`
- `crates/cli/src/scenario.rs`

**Changes in `random.rs`:**

- Add `layer_linking: bool` to `RandomConfig` (default `false`).
- In `generate_random()`, inside the family creation loop, replace the layer
  assignment:

  ```rust
  let layer = if config.layer_linking {
      (config.generations - 1) - (family_index % config.generations)
  } else {
      rng.gen_range(0..config.generations)
  };
  ```

  Use `.enumerate()` on the loop to get `family_index`.

**Changes in `generate.rs` (build_config):**

- Set `layer_linking = args.depth > 1` in the `RandomConfig` construction.

**Changes in `scenario.rs`:**

- In `to_random_config()`: set `layer_linking` based on depth:

  ```rust
  layer_linking: self.generations.as_ref()
      .map(|g| g.depth.unwrap_or(base.generations))
      .unwrap_or(base.generations) > 1,
  ```

**Tests:**

- Add `layer_linking_sequential_assignment` test: verify with 4 layers,
  8 families, that the assignment follows the sequential descending pattern
  (layers 3, 2, 1, 0, 3, 2, 1, 0) and each layer gets exactly 2 families.
- Update `random_config_defaults`: assert `layer_linking == false`.

### Step 4 — Deep-tree regression test

**Rationale:** The original bug — `--count 16 --seed 2026 --depth 4` producing
a largest connected component of only 3 people — must not regress.

**Files:**

- `crates/cli/tests/integration.rs`

**New test:**

- `generate_depth_4_produces_deep_tree`: run `generate_random` with
  `--count 16 --seed 2026 --depth 4` (the original problematic config) and
  assert the largest connected component size ≥ 8 (at least half the person
  count).
- **Implementation note:** This requires a connected-component helper.
  Add a function `largest_connected_component_size` in the test or in the
  `random.rs` test module (or as a helper in `graph.rs`) that traverses the
  graph's person-family-person edges using BFS/DFS and returns the size of
  the largest connected component of Person nodes.

### Step 5 — Verify full test suite and clippy

**Commands:**

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

Fix any issues found. No code changes beyond what's needed to pass.

### Step 6 — Manual verification with Gramps 5.1.6 GUI

**Commands:**

```bash
# Original problematic case — should now produce deeper trees:
cargo run -- generate --count 16 --seed 2026 --depth 4 --family-ratio 0.5 -o /tmp/test.gramps

# High density test:
cargo run -- generate --count 16 --seed 2026 --depth 4 --family-ratio 1.0 --max-parent-roles 2 -o /tmp/dense.gramps
```

Verify with `gramps-gen validate` or `xmllint`. The automated integration test
in Step 4 covers the regression case programmatically.
