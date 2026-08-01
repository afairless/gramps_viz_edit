# Fix Plan: Missing Gender Enum for 5.1 Schema Breaks Family Connections

## Overview

When generating Gramps 5.1 family trees with `--schema-version 5.1`, **all persons are assigned gender 0 (male)** regardless of the RNG seed. With no females in the pool, no parent pairs can form, and all generations produce disconnected individuals with empty families. The issue is systemic — it affects every 5.1 generation run — not just unlucky seeds.

Two defects contribute:

| # | Defect | Impact |
|---|--------|--------|
| 1 | **Missing `Gender` enum in the 5.1 per-version schema** | Every person gets `gender = 0` (male), making `select_parents` always fail |
| 2 | **`create_single_parent_family` creates parentless families** | Even if a single parent is found, the resulting family has no parent edges and no children |

---

## Problem Analysis

### Defect 1: Missing Gender Enum for 5.1

#### Chain of causation

1. **The 5.1 JSON Schema file lacks a `Gender` entry in `enum_types`.**

   The schema file `schemas/schema-5.1.json` (produced by Gramps' own `cls.get_schema()` Python API in JSON Schema format) defines `Person.gender` as a plain integer field with `"minimum": 0, "maximum": 2`. Unlike the 5.2 schema, Gramps 5.1 does not expose Gender as a formal enumeration.

   ```
   5.2 enum_types:  Gender, DateModifier, DateQuality, EventType, …  (16 entries)
   5.1 enum_types:  EventType, EventRoleType, …                      (11 entries — no Gender)
   ```

2. **The `enum_constants_5_1.json` lookup table also lacks `Gender`.**

   `crates/typed-graph/build/enum_constants_5_1.json` provides integer→name mappings for the 11 enums that *are* present in the 5.1 schema's `enum_types`. It has no `Gender` entry.

3. **The schema converter creates an orphan `enum_ref` pointer.**

   In `schema_convert.rs`, the `convert_field` function detects `"gender"` via `detect_enum_ref` and produces `{"type": "enum_ref", "target": "Gender"}`. But because `Gender` is absent from the source schema's `enum_types`, the converter never creates a `Gender` enum type definition. The converted flat-format schema has a field referencing a target that doesn't exist.

4. **The per-version `Schema` has no Gender in `valid_enum_values`.**

   `build.rs` generates the `Schema` instance from whatever `enum_types` were in the converted schema. For version 5.1, `valid_enum_values` has no `"Gender"` key.

5. **The gender-selection logic silently defaults to male.**

   In `generate_random_person` (`random.rs:506-520`):

   ```rust
   let valid_genders = schema_enum_values(schema, "Gender");  // → empty vec
   let has_female = valid_genders.contains(&"1");             // → false
   // …
   if roll < 0.475 || !has_female {
       0   // ← ALWAYS taken when has_female is false
   }
   ```

   Since `has_female` is `false`, the short-circuit `||` makes the entire condition true for every person. Every person gets `gender = 0`.

6. **With only males, no parent pairs can form.**

   `select_parents` → `find_compatible_pair` separates candidates into `males` and `females`. With zero females, `females.is_empty()` returns `true` and the function returns `None`.

#### Scope: other missing enum types

Three enum targets referenced by `detect_enum_ref` are absent from the 5.1 schema:

| Enum target | Referenced by | In 5.1 enum_types? | In constants file? | Causes user-visible bug? |
|-------------|--------------|---------------------|---------------------|--------------------------|
| `Gender` | `Person.gender` | No | No | **Yes** — all persons male |
| `DateModifier` | `Date.modifier` (on Event, Citation, Media) | No | No | No* |
| `DateQuality` | `Date.quality` (on Event, Citation, Media) | No | No | No* |

\* `DateModifier` and `DateQuality` don't cause runtime bugs because `schema_enum_values` is never called for them. The date-quality randomization code (`randomize_date_quality`) uses compile-time enum variants (`crate::DateQuality::Exact`, etc.) derived from the *merged* schema (which includes 5.2 definitions). However, they create silent inconsistencies in the per-version `valid_enum_values` and should be fixed alongside Gender for correctness.

### Defect 2: `create_single_parent_family` Produces Parentless Families

In `random.rs:978-1015`, when `select_parents` returns `None` (no male+female pair found), `generate_family` falls through to `create_single_parent_family`:

```rust
fn create_single_parent_family(graph, persons, layer, rng) -> Result<Handle, …> {
    // 1. Find an eligible person who isn't already a parent
    let eligible = persons.iter()
        .filter(|(_, s)| s.layer == layer && !s.is_parent)
        .collect();
    let parent_handle = eligible[rng.gen()];

    // 2. Mark as parent in the summary vector
    persons[].is_parent = true;

    // 3. Create family — BUG: no father_handle or mother_handle!
    let family = crate::FamilyData {
        handle: family_handle.clone(),
        ..crate::FamilyData::default()  // ← all fields default, parents are None
    };
    graph.add_node(family_handle, Family(family));

    // 4. BUG: No FamilyFather or FamilyMother edge added!

    // 5. Update person's family_list
    graph.get_node_mut(&parent_handle).family_list.push(family_handle);
}
```

Compare with the normal two-parent path (`generate_family`, lines 939–970), which does:

- Sets `father_handle: Some(…)` and `mother_handle: Some(…)` on `FamilyData`
- Creates `Edge::FamilyFather` and `Edge::FamilyMother` edges
- Adds the family handle to both parents' `family_list`

And the adversarial one-parent path (lines 845–925), which correctly sets the single parent handle and creates the corresponding edge.

The single-parent fallback does none of this — the family node ends up with `father_handle: None` and `mother_handle: None`, and no parent edges exist. The output XML shows empty `<family …/>` elements.

#### Impact

Even after fixing Defect 1 (Gender enum), small person counts or imbalanced gender distributions can still trigger `create_single_parent_family`. A generation with 2 males and 0 females in a given layer would produce empty families from this fallback.

---

## Fix Plan

### Fix 1: Synthesize Missing Enum Types for 5.1 Schema Conversion

**Goal:** Ensure the converted 5.1 flat-format schema includes `Gender`, `DateModifier`, and `DateQuality` enum type definitions, so the per-version `Schema`'s `valid_enum_values` contains proper values and `generate_random_person`'s gender selection works correctly.

#### Approach

Modify `schema_convert.rs` so that after converting all enum_types from the source JSON Schema, it scans all converted primary and secondary type fields for `enum_ref` entries. For each `enum_ref` target that has no matching entry in the converted `enum_types`, synthesize an enum type definition.

For each missing enum:

1. **Collect all source fields** that reference the same enum target (e.g., `Person.gender`, `FamilyRelation.type`, etc.). From these field definitions in the original JSON Schema, extract the `minimum`/`maximum` range to determine the valid integer values.

2. **Produce synthetic enum values** as integers `[min ..= max]` — preserving the same format the 5.2 schema uses (integer values in the flat `values` array). The `build.rs` code generator already handles integer enum values correctly for `valid_enum_values` (it stringifies them as `"0"`, `"1"`, etc., which is what `has_female = valid_genders.contains(&"1")` expects).

3. **Add entries to `enum_constants_5_1.json`** for the missing enum types, mapping integer→name (e.g., `"0": "Male"`). The build.rs code generator uses this for naming Rust enum variants (`Gender::Male`, `Gender::Female`). Without these entries, the variants would be named `_0`, `_1`, `_2`.

#### Specific changes

**A. `crates/typed-graph/build/enum_constants_5_1.json`** — add three entries:

```json
"Gender": {
    "0": "Male",
    "1": "Female",
    "2": "Unknown"
},
"DateModifier": {
    "0": "NONE",
    "1": "BEFORE",
    "2": "AFTER",
    "3": "ABOUT",
    "4": "RANGE",
    "5": "SPAN",
    "6": "TEXTUAL"
},
"DateQuality": {
    "0": "NONE",
    "1": "ESTIMATED",
    "2": "CALCULATED"
}
```

(These integer→name mappings match Gramps 5.1 Python source constants.)

**B. `crates/typed-graph/src/schema_convert.rs`** — add a `synthesize_missing_enum_types` function called at the end of `convert()`, just before the final `Ok(Value::Object(result))` (after the synthetic secondary types have already been added):

1. Collect all `enum_ref` targets from converted primary_types and secondary_types fields.
2. For each target not already in `converted_enums`:
   - Look up the **original** JSON Schema field(s) (passed as a parameter to the function — the original `schema: Value` is still in scope at the call site) for the `minimum`/`maximum` bounds. Walk `primary_types.X.fields.properties.Y` to find the integer field definition.
   - Build a synthetic `{"values": [min, min+1, …, max]}` entry in the converted enum_types.
   - Emit a `cargo::note` (not `cargo::warning`, since this is expected synthesis for known schema version gaps) noting the synthesis for debugging.
3. Insert the synthesized types into the result's `enum_types` map.

**C. No changes needed to `generate_random_person`** — the existing `valid_genders.contains(&"1")` check will work correctly once `valid_enum_values` contains `["0", "1", "2"]` for Gender.

#### Edge cases

- **Gender field has `"minimum": 0, "maximum": 2` in 5.1.** The 5.1 schema allows 0 (Male), 1 (Female), 2 (Unknown). There is no value 3 (Other). The synthetic enum should produce `[0, 1, 2]`, and `has_other` will correctly return `false`, causing `generate_random_person` to never produce gender 3 for 5.1 — which is the correct behavior for this schema version.

- **What if a future schema introduces new `enum_ref` targets?** The synthesis logic should be generic: any `enum_ref` target missing from `enum_types` gets synthesized from the source field's min/max. If no min/max is available (plain integer without bounds), log a warning and skip — the field stays as `enum_ref` pointing to a non-existent type, and callers that use `schema_enum_values` will get empty results (same as current behavior, but guaranteed to be visible in build logs).

### Fix 2: Fix `create_single_parent_family` to Properly Assign the Parent

**Goal:** When no male+female pair is available, the single-parent fallback should create a family that has the single parent properly linked as either father or mother (based on the parent's gender), with the correct edge.

#### Changes

In `create_single_parent_family` (`random.rs:978-1015`):

1. After selecting the eligible parent, determine their gender from their `PersonSummary` (or the graph node).
2. Build the `FamilyData` with the appropriate parent handle set:
   - If `gender == 1` (Female): `mother_handle: Some(parent_handle)`, `father_handle: None`
   - Otherwise: `father_handle: Some(parent_handle)`, `mother_handle: None`
3. Create the corresponding edge:
   - `Edge::FamilyFather` or `Edge::FamilyMother`
4. Keep the existing `family_list.push` code that adds the family to the parent's `family_list`. This is consistent with how the two-parent path and the adversarial one-parent path both update `family_list`. While edges provide the same linkage, maintaining `family_list` consistency across all family-creation paths avoids edge cases in downstream consumers that may read the list field directly.

#### Edge cases

- **Parent gender is unknown (2) or other (3):** Default to `FamilyFather` edge (follows the "otherwise" branch). This is a reasonable default — in real Gramps data, unknown-gender parents are rare and either choice is valid.

### Fix 3 (optional but recommended): Update Gender Handling for Merged Schemas

The version-specific helpers in `graph.rs` (`into_gender_field`, `gender_value`) wrap Gender as `Option<i32>` when both 5.1 and 5.2 are compiled together. The generation code correctly uses `into_gender_field(gender)` to produce the right type. No changes are needed here — this is noted for awareness during implementation.

---

## Verification

### Manual verification

1. **Regenerate with the problematic seed:**

   ```bash
   target/release/gramps-gen generate --schema-version 5.1 \
     --output /tmp/fixed.gramps --count 8 --seed 2026 --depth 4
   ```

   Expected: mixed genders, non-empty families with father/mother/childref elements.

2. **Grid test across seeds:**

   ```bash
   for seed in 2020..2049; do
     # generate with 5.1, count genders and family edges
   done
   ```

   Expected: no seed produces all-male; family edges > 0 in most runs.

3. **Test with 5.2 to confirm no regression:**

   ```bash
   target/release/gramps-gen generate --schema-version 5.2 \
     --output /tmp/regression.gramps --count 200 --seed 42 --depth 3
   ```

   Expected: behaves identically to before the fix.

### Automated tests

**A. Unit test for `create_single_parent_family`** (in `random.rs` tests):

- Create a graph with a single female person.
- Call `create_single_parent_family`.
- Assert that the created Family node has `mother_handle: Some(…)` and `father_handle: None`.
- Assert that a `FamilyMother` edge exists from the family to the person.
- Assert the person's `family_list` contains the family handle.
- Repeat with a single male person → asserts `father_handle: Some(…)` and `FamilyFather` edge.

**B. Integration test for 5.1 gender distribution:**

- Generate with `--schema-version 5.1`, 100 persons, fixed seed.
- Count genders in the output XML.
- Assert: at least 10% female (a reasonable lower bound to rule out the all-male bug).

**C. Schema conversion unit test** (in `schema_convert.rs` tests):

- Feed the converter a minimal fixture with `Person.gender` as an integer field (`"type": "integer", "minimum": 0, "maximum": 2`) and no Gender in `enum_types`.
- Assert the converted output has a `Gender` entry in `enum_types` with `values: [0, 1, 2]`.
- Repeat for `Date.modifier` → `DateModifier` (values `[0..=6]`) and `Date.quality` → `DateQuality` (values `[0..=2]`).

**D. Integration test for `property_families_have_at_least_one_parent`**:

- After the fix, strengthen this existing test (currently a no-op that computes `has_parent` but only does `let _ = has_parent;`) to actually assert `has_parent` for every family. This would have caught the `create_single_parent_family` bug and provides ongoing regression protection.

---

## Implementation Steps

1. **Add Gender, DateModifier, DateQuality to `enum_constants_5_1.json`**
   - File: `crates/typed-graph/build/enum_constants_5_1.json`
   - Add three entries with proper integer→name mappings.

2. **Add orphan enum_ref synthesis to `schema_convert.rs`**
   - New function: `synthesize_missing_enum_types(converted_types, source_schema, enum_constants) -> Map<String, Value>`
   - Call at end of `convert()` before building the final result.
   - Scan all converted fields for `enum_ref` with missing targets.
   - Derive integer range from source field's `minimum`/`maximum`.

3. **Fix `create_single_parent_family` in `random.rs`**
   - Set `father_handle` or `mother_handle` on FamilyData based on parent's gender.
   - Create the corresponding `FamilyFather` or `FamilyMother` edge.

4. **Add tests** (see Verification section above)

5. **Run full test suite:**

   ```bash
   cargo test --workspace
   cargo clippy --all-targets --all-features -- -D warnings
   ```

6. **Validate with the original reproduction command** and the seed grid.
