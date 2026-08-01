# Implementation Plan: Fix 5.1 Gender Enum and Family Connections

Source: `docs/research/fix-5.1-gender-and-family-connections.md`

## Summary

Two defects cause Gramps 5.1 generation to produce all-male populations with empty families:

1. **Missing Gender enum in 5.1 schema conversion** — `Gender`, `DateModifier`, and `DateQuality` enum types are absent from the converted 5.1 flat-format schema, causing `generate_random_person` to always default to gender 0 (male).
2. **`create_single_parent_family` creates parentless families** — When no male+female pair is found, the fallback creates a family node with no parent edges and no parent handles.

The plan synthesizes the missing enum types from the source field's `minimum`/`maximum` bounds during schema conversion, and fixes the single-parent fallback to properly link the parent.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: add Gender, DateModifier, DateQuality to enum_constants_5_1.json` | Enum constant mappings | `crates/typed-graph/build/enum_constants_5_1.json` | — |
| 2 | `feat: synthesize missing enum types during 5.1 schema conversion` | Enum ref synthesis | `crates/typed-graph/src/schema_convert.rs` | Unit (schema_convert) |
| 3 | `fix: properly assign parent in create_single_parent_family` | Single-parent family fix | `crates/typed-graph/src/generate/random.rs` | Unit (random.rs) |
| 4 | `test: add integration tests for 5.1 gender distribution and family structure` | Integration tests | `crates/typed-graph/tests/merged_schema.rs`, `crates/cli/tests/e2e.rs` | Integration |
| 5 | `chore: run full test suite, clippy, and manual validation` | Verification | — | — |

## Step Details

### Step 1 — Enum constant mappings

**What:** Add `Gender`, `DateModifier`, and `DateQuality` entries to `crates/typed-graph/build/enum_constants_5_1.json` with proper integer→name mappings matching Gramps 5.1 Python source constants.

**Why:** The build.rs code generator uses these mappings for naming Rust enum variants. Without them, synthesized enum variants would be named `_0`, `_1`, `_2` instead of `Male`, `Female`, `Unknown`.

**Deliverables:**

- `crates/typed-graph/build/enum_constants_5_1.json` — three new entries added

**Tests:** — (data-only change; build compiles without errors)

---

### Step 2 — Enum ref synthesis

**What:** Add a `synthesize_missing_enum_types` function in `schema_convert.rs` that is called at the end of `convert()`. After all enum_types from the source JSON Schema are converted, it scans all converted primary and secondary type fields for `enum_ref` entries. For each `enum_ref` target missing from the converted `enum_types`, it:

1. Finds the original source field definition via the JSON Schema to extract `minimum`/`maximum` bounds
2. Produces a synthetic enum entry with `values: [min, min+1, …, max]`
3. Emits a `cargo::note` for build-time debugging

**Edge cases:** If no `minimum`/`maximum` is found on the source field, emit a warning and skip the synthesis (field stays as `enum_ref` pointing to a non-existent type, same as current degraded behavior).

**Deliverables:**

- `crates/typed-graph/src/schema_convert.rs` — new function, call site in `convert()`

**Tests:** Unit test in `schema_convert.rs`:

- Feed a minimal fixture with `Person.gender` as integer field (min=0, max=2), no Gender in `enum_types`
- Assert converted output has `Gender` in `enum_types` with `values: [0, 1, 2]`
- Repeat for `Date.modifier` → `DateModifier` (values 0..=6) and `Date.quality` → `DateQuality` (values 0..=2)

---

### Step 3 — Single-parent family fix

**What:** Fix `create_single_parent_family` in `random.rs` to:

1. Determine the selected parent's gender from their `PersonSummary` (or the graph node)
2. Set `father_handle: Some(parent_handle)` if gender is not female (1), or `mother_handle: Some(parent_handle)` if gender is female
3. Create the corresponding `Edge::FamilyFather` or `Edge::FamilyMother` edge
4. Keep the existing `family_list.push` code

**Edge cases:** If gender is unknown (2) or other (3), default to `FamilyFather` edge.

**Deliverables:**

- `crates/typed-graph/src/generate/random.rs` — `create_single_parent_family` function

**Tests:** Unit test in `random.rs`:

- Create graph with single female person → assert `mother_handle: Some(…)`, `father_handle: None`, `FamilyMother` edge exists, person's `family_list` contains family handle
- Create graph with single male person → assert `father_handle: Some(…)`, `mother_handle: None`, `FamilyFather` edge exists

---

### Step 4 — Integration tests

**What:** Add integration tests that verify the fix end-to-end:

1. **5.1 gender distribution test** (e2e or integration): Generate with `--schema-version 5.1`, 100 persons, fixed seed. Assert at least 10% female in output XML.
2. **Strengthen `property_families_have_at_least_one_parent`** (in `merged_schema.rs`): Currently a no-op that computes `has_parent` and discards it. Change to actually assert `has_parent` for every family.

**Deliverables:**

- `crates/typed-graph/tests/merged_schema.rs` — strengthen `property_families_have_at_least_one_parent`
- `crates/cli/tests/e2e.rs` — add 5.1 gender distribution test (or `crates/cli/tests/integration.rs`)

**Tests:** Integration tests only (this step is itself tests).

---

### Step 5 — Verification

**What:** Run the full test suite, clippy, and manual validation:

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

**Manual validation:**

```bash
target/release/gramps-gen generate --schema-version 5.1 \
  --output /tmp/fixed.gramps --count 8 --seed 2026 --depth 4
# Verify: mixed genders, non-empty families with father/mother/childref elements

target/release/gramps-gen generate --schema-version 5.2 \
  --output /tmp/regression.gramps --count 200 --seed 42 --depth 3
# Verify: no regression on 5.2
```

**Deliverables:** None (verification only)

**Tests:** — (verification step)
