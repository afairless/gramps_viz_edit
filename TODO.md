# Implementation Plan: Fix `schema-5.1.json` Format Incompatibility

**Source documents:**

- `docs/research/fix-schema-5.1-format.md`
- `docs/research/schema-5.1-vs-5.2.md`
- `docs/ARCHITECTURE.md`
- `AGENTS.md`

## Summary

`cargo build --features schema-5-1` produces 220+ compilation errors because `schema-5.1.json` is in **JSON Schema format** while `build.rs` expects the **custom flat format** used by `schema-5.2.json`. This plan adds a build-time converter that detects and normalizes JSON Schema format to the custom flat format in memory before the merge and code generation stages.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 0 | `feat: extract Gramps 5.1 enum constants` | Enum lookup tables | `extract/extract_schema.py` (add `--enum-names` flag), `crates/typed-graph/build/enum_constants_5_1.json` | Smoke |
| 1 | `feat: add schema conversion module (JSON Schema → flat format)` | Conversion logic | `crates/typed-graph/src/schema_convert.rs`, `crates/typed-graph/src/lib.rs` (add `pub mod schema_convert`) | Unit, Property-based |
| 2 | `feat: wire schema_convert into build.rs via #[path] include` | Build integration | `crates/typed-graph/build.rs`, `crates/typed-graph/tests/schema_convert_tests.rs` | Unit |
| 3 | `fix: handle merged Option<T> fields in generate/random.rs` | Generator fixes | `crates/typed-graph/src/generate/random.rs` | Unit |
| 4 | `fix: handle merged Option<T> fields in generate/builder.rs` | Builder fixes | `crates/typed-graph/src/generate/builder.rs` | Unit |
| 5 | `fix: handle merged Option<T> fields in generate/adversarial.rs` | Adversarial fixes | `crates/typed-graph/src/generate/adversarial.rs` | Unit |
| 6 | `test: add integration tests for merged schema (5.1+5.2)` | Integration tests | `crates/typed-graph/tests/merged_schema.rs` | Integration |
| 7 | `feat: add --enum-names flag and JSON Schema warning to extract_schema.py` | Extractor future-proofing | `extract/extract_schema.py` | — |
| 8 | `docs: document schema conversion process and merged schema` | Documentation | `docs/` (update ARCHITECTURE.md or AGENTS.md if needed) | — |

---

## Detailed Steps

### Step 0 — Extract Gramps 5.1 enum constants

**Prerequisites:** Clone Gramps 5.1.6 source into a known location (e.g., `/tmp/gramps-5.1.6`) for enum member introspection.

**Actions:**

1. Clone Gramps 5.1.6 from `https://github.com/gramps-project/gramps.git` tag `v5.1.6`.
2. Add a `--enum-names` flag to `extract/extract_schema.py` that:
   - Imports `gramps.gen.lib` from the cloned checkout (using `PYTHONPATH`).
   - Iterates over all enum classes (e.g., `EventType`, `EventRoleType`, `NameType`, `ChildRefType`, etc.).
   - Extracts `(member.value, member.name)` pairs for each enum.
   - Writes a JSON mapping: `{"EnumType": {"0": "Custom", "1": "Marriage", ...}}`.
3. Run the extraction to generate `crates/typed-graph/build/enum_constants_5_1.json`.
4. Commit the static artifact.

**Key deliverables:**

- `extract/extract_schema.py` — extended with `--enum-names` flag
- `crates/typed-graph/build/enum_constants_5_1.json` — enum lookup tables

**Decisions (from user):**

- Clone Gramps 5.1.6 source rather than using the installed package.
- Skip manual required-field source audit; rely on structural heuristics.
- Include Step 0 in the implementation plan.

---

### Step 1 — Create `schema_convert.rs` conversion module

**Actions:**

1. Create `crates/typed-graph/src/schema_convert.rs` with three public functions:
   - `is_json_schema_format(schema: &Value) -> bool` — detects JSON Schema format by checking for `"properties"` key inside `fields`.
   - `convert(schema: Value, version: &str, enum_constants: &Value) -> Result<Value, String>` — converts JSON Schema format to custom flat format in memory.
   - `validate_flat_format(schema: &Value) -> Result<(), Vec<String>>` — validates structural correctness after conversion.
2. Add `pub mod schema_convert;` to `crates/typed-graph/src/lib.rs`.
3. Implement conversion logic per Step 1a–1e of the plan:
   - **Primary type conversion:** Extract `properties` → new `fields`, map types, determine `kind` (handle, handle_ref), infer `required` via `oneOf` null-wrapper heuristic with explicit overrides (`gramps_id`, `father_handle`, `mother_handle` = optional).
   - **Secondary type conversion:** Same logic; skip `Tag` secondary type with warning, strip `_class`/`private`/`change`.
   - **Enum type conversion:** Map integer values → string names using lookup tables from Step 0; warn on unknown integers.
   - **Date normalization:** Convert 5.1's `Date` object (with `dateval` array) to 5.2-compatible `DateValue` with `year`/`month`/`day` scalars; map `modifier`/`quality` integers to enums.
   - **Output validation:** `validate_flat_format()` checks for missing `type`, missing `target` on `handle_ref`, missing `schema` on `embedded`, missing `items` on `array`.
4. **Unit tests** (minimum 20 tests covering all conversion paths — see plan Step 1f for the full list).
5. **Property-based tests:** `property_convert_idempotent` (already-flat schema unchanged), `property_is_json_schema_false_after_convert`.

**Key deliverables:**

- `crates/typed-graph/src/schema_convert.rs` (~500–600 lines including tests)
- `crates/typed-graph/src/lib.rs` — add `pub mod schema_convert;`

---

### Step 2 — Wire conversion into `build.rs`

**Actions:**

1. Use `#[path = "src/schema_convert.rs"] mod schema_convert;` at the top of `build.rs` to inline the module (avoids circular build-dependency).
2. Add enum constants loading in `main()` before `load_schemas()`:

   ```rust
   let enum_constants_path = crate_dir.join("build/enum_constants_5_1.json");
   let enum_constants = if enum_constants_path.exists() { serde_json::from_str(&fs::read_to_string(&enum_constants_path)?)? } else { serde_json::Value::Null };
   ```

3. Wire format detection + conversion into `load_schemas()` after each schema is parsed from JSON.
4. Add flat-format validation after conversion, with `panic!` on failure.
5. Create `crates/typed-graph/tests/schema_convert_tests.rs` that also includes the module via `#[path]` for integration-level testing (since `#[cfg(test)]` in `build.rs` won't run via `cargo test`).

**Key deliverables:**

- `crates/typed-graph/build.rs` — enum constants loading, format detection, conversion wiring, validation
- `crates/typed-graph/tests/schema_convert_tests.rs` — integration tests for the build path

---

### Step 3 — Fix `generate/random.rs` for merged `Option<T>` fields

**Actions:**

1. Identify struct literal constructions in `generate/random.rs` that reference fields which become `Option<T>` under the merged schema.
2. Wrap values in `Some()` for optional fields.
3. Use `Schema::field_availability` (or `field_exists_for_version` helper) to conditionally populate version-specific fields.
4. Run `cargo test -p typed-graph` to verify.

**Key deliverables:**

- `crates/typed-graph/src/generate/random.rs` — fixes for `Option<T>` handling

---

### Step 4 — Fix `generate/builder.rs` for merged `Option<T>` fields

**Actions:**

1. Verify builder `build()` methods correctly handle `Option<T>` fields from the merged schema.
2. Ensure `Some()` / `None` wrapping is correct for fields that may or may not exist in each version.
3. Run `cargo test -p typed-graph` to verify.

**Key deliverables:**

- `crates/typed-graph/src/generate/builder.rs` — fixes for `Option<T>` handling

---

### Step 5 — Fix `generate/adversarial.rs` for merged `Option<T>` fields

**Actions:**

1. Update adversarial strategies to use `if let Some(ref mut val) = data.optional_field { ... }` patterns where fields are now `Option<T>`.
2. Run `cargo test -p typed-graph` to verify.

**Key deliverables:**

- `crates/typed-graph/src/generate/adversarial.rs` — fixes for `Option<T>` handling

---

### Step 6 — Add integration tests for merged schema

**Actions:**

1. Create `crates/typed-graph/tests/merged_schema.rs` with tests covering:
   - **Shape test:** Construct `PersonData` under merged schema; verify fields accessible and 5.1-only fields (e.g., `birth_ref_index`) are `Option<i32>`.
   - **Generation test:** Run `generate_random()` with `Schema::for_version("5.1")` and verify `graph.validate()` passes. Repeat with merged schema.
   - **Enum values test:** Verify `Schema::valid_enum_values` for `EventType` contains both 5.1 and 5.2 values with no duplicate variant names.
   - **DateValue test:** Construct `DateValue` via `DateValue::new(1870)`; verify `is_valid()` and `display_text()` work.
   - **XML round-trip:** Generate a small graph (10 persons, 2 families), serialize to `.gramps` XML, verify well-formed `<database>` root.

**Key deliverables:**

- `crates/typed-graph/tests/merged_schema.rs` — merged schema integration tests

---

### Step 7 — Update `extract_schema.py` (future-proofing)

**Actions:**

1. Ensure the `--enum-names` flag from Step 0 is merged and functional.
2. Add a detection check: if `cls.get_schema()` returns JSON Schema format (after the fact), print a warning directing users that the build system handles format conversion automatically at compile time.
3. Optionally add a `--convert-to-flat` flag that outputs the custom flat format directly (for use outside the build system).

**Key deliverables:**

- `extract/extract_schema.py` — `--enum-names` flag, JSON Schema format warning

---

### Step 8 — Documentation updates

**Actions:**

1. Update `docs/ARCHITECTURE.md` (or create a new section) documenting:
   - The two schema formats (JSON Schema vs. custom flat).
   - The build-time conversion pipeline.
   - How the merge algorithm handles version-specific fields.
   - How to update schemas in the future.
2. If `AGENTS.md` needs updates to reflect the new module (`schema_convert.rs`) and new build pipeline, apply them.

**Key deliverables:**

- `docs/ARCHITECTURE.md` — updated schema documentation
- `AGENTS.md` — updated if needed
