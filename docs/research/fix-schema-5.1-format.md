# Fix Plan: `schema-5.1.json` Format Incompatibility

## Summary

`cargo build --features schema-5-1` produces 220+ compilation errors. The root cause is that `schema-5.1.json` was extracted in **JSON Schema format** (the native output of Gramps 5.1's `cls.get_schema()`), while `build.rs` code generation and merge logic expects the **custom flat format** used by `schema-5.2.json`. When both schemas are merged (default feature `schema-5-2` + `schema-5-1`), the JSON Schema meta-fields (`type`, `title`, `properties`) are misinterpreted as data fields, producing garbage generated code.

---

## Root Cause Analysis

### Two incompatible schema formats

**Format A: Custom flat format (schema-5.2.json)** — the target format:

```json
{
  "primary_types": {
    "Person": {
      "fields": {
        "handle":          { "type": "string", "kind": "handle", "required": true },
        "gramps_id":       { "type": "string", "required": false },
        "gender":          { "type": "enum", "values": [0, 1, 2, 3], "required": true },
        "family_list":     { "type": "array", "items": { "kind": "handle_ref", "target": "Family" }, "required": false },
        "event_ref_list":  { "type": "array", "items": { "embedded": "EventRef", "edges": [{"link": "EventRef.ref", "target": "Event"}] }, "required": false },
        "primary_name":    { "type": "embedded", "schema": "Name", "required": true },
        "attribute_list":  { "type": "array", "items": "Attribute", "required": false }
      },
      "inherit_mixins": ["CitationBase", "NoteBase", "MediaBase", "AttributeBase", "AddressBase", "UrlBase", "LdsOrdBase", "TagBase"]
    }
  }
}
```

**Format B: JSON Schema format (schema-5.1.json)** — the current problematic format:

```json
{
  "primary_types": {
    "Person": {
      "fields": {
        "type": "object",
        "title": "Person",
        "properties": {
          "_class":        { "enum": ["Person"] },
          "handle":        { "type": "string", "maxLength": 50, "title": "Handle" },
          "gramps_id":     { "type": "string", "title": "Gramps ID" },
          "gender":        { "type": "integer", "minimum": 0, "maximum": 3, "title": "Gender" },
          "family_list":   { "type": "array", "items": { "type": "string", "maxLength": 50 }, "title": "Families" },
          "event_ref_list":{ "type": "array", "items": { "type": "object", "title": "EventRef", "properties": {…} }, "title": "Events" },
          "primary_name":  { "type": "object", "title": "Name", "properties": {…} }
        }
      },
      "inherit_mixins": ["CitationBase", "NoteBase", "AttributeBase", "MediaBase", "AddressBase", "UrlBase", "LdsOrdBase", "TagBase"]
    }
  }
}
```

### How the misinterpretation cascades

When `build.rs` processes the 5.1 JSON Schema structure:

1. `info.get("fields").and_then(|v| v.as_object())` finds the JSON Schema metadata dict
2. It iterates keys `type`, `title`, `properties` as if they were **data field names**
3. `type` → generates a field `pub type_field: String` (Rust keyword escape)
4. `title` → generates a field `pub title: String`
5. `properties` → the field info for this key is the **entire properties sub-object**, which has `type: "object"` and `required: false`, generating `pub properties: Option<String>` — all real fields are lost

Meanwhile, the merge with 5.2 (which has correct fields) tries to unify both sets, producing incoherent struct definitions. Edge enum generation produces duplicate variants from misidentified fields.

### Enum value duplication

| | 5.2 | 5.1 |
|---|---|---|
| Value type | Strings: `"Birth"`, `"Death"`, `"Marriage"` | Integers: `0`, `1`, `2`, `11`, `12` |
| Variant name | `Birth`, `Death`, `Marriage` | `_0`, `_1`, `_2`, `_11`, `_12` |

When merged, integer values from different enums collide: both `EventType` and `EventRoleType` have value `0`, producing duplicate `_0` variants.

### Version-specific differences

| Category | 5.1 only | 5.2 only |
|---|---|---|
| Person fields | `_class`, `birth_ref_index`, `death_ref_index`, `private`, `change`, `urls` | `url_list` |
| Secondary types | `Tag` | `DateValue` |
| Enum types | — | `DateModifier`, `DateQuality`, `Gender`, `LdsOrdType`, `NameOriginType` |

The 5.1 schema embeds `Date` inline as `{type: object, title: "Date", properties: {...}}` on fields like `Citation.date`, while 5.2 uses the `DateValue` secondary type.

---

## Fix Strategy

**Approach: Add an in-build conversion from JSON Schema format → custom flat format.**

Rather than modifying the `schema-5.1.json` file on disk or writing a separate conversion tool, a new Rust module `build/schema_convert.rs` detects whether a loaded schema is in JSON Schema format and normalizes it to the custom flat format in memory before it reaches the merge and code generation stages. The conversion is called from `build.rs` immediately after loading each schema file.

Advantages:

1. **Transparent** — schema-5.1.json stays as-is on disk; no separate conversion step
2. **Future-proof** — any schema version extracted in JSON Schema format is automatically handled
3. **Modular** — conversion logic is isolated in its own module, testable with `cargo test -p typed-graph`
4. **Pure Rust** — uses `serde_json::Value`, no Python dependency
5. **Minimal `build.rs` changes** — one function call per loaded schema

### Architecture

```
crates/typed-graph/
├── build.rs                # Detects format, calls schema_convert::convert() before merge
├── build/
│   └── enum_constants_5_1.json  # Committed enum lookup tables from Step 0
├── src/
│   └── schema_convert.rs   # Conversion logic + unit tests (regular library module)
├── ...
```

The conversion logic lives in `src/schema_convert.rs` as a regular library module
(re-exported from `lib.rs`). This makes its `#[cfg(test)]` unit tests runnable with
`cargo test -p typed-graph`.

**Entry point in `build.rs`** — after loading each schema JSON:

```rust
fn load_schemas(schemas_dir: &Path, versions: &[String],
                enum_constants: &Value) -> Vec<(String, Value)> {
    for version in versions {
        let mut schema: Value = serde_json::from_str(&raw_json)?;
        if typed_graph::schema_convert::is_json_schema_format(&schema) {
            eprintln!("cargo::warning=schema-{}.json is in JSON Schema format; converting to flat format", version);
            schema = typed_graph::schema_convert::convert(schema, &version, enum_constants)
                .expect("schema conversion should succeed for valid Gramps JSON Schema");
        }
        schemas.push((version.clone(), schema));
    }
    schemas
}
```

`build.rs` either depends on `typed-graph` via `[build-dependencies]` in `Cargo.toml`,
or uses `#[path = "src/schema_convert.rs"] mod schema_convert;` to inline the module
(if a build-dependency cycle must be avoided). See **Risk Assessment** for trade-offs.

**Format detection heuristic** — `is_json_schema_format()` checks whether the `fields` object of any primary or secondary type contains a key `"properties"` (the JSON Schema hallmark). If all types lack `properties`, the schema is treated as custom flat format and passed through unchanged.

---

## Step-by-step Plan

### Step 0: Research Gramps 5.1 enum constants and verify type compatibility

**This step is a blocker for Step 1** — the conversion module needs accurate enum lookup
tables and verified type compatibility before any code is written.

#### 0a: Extract enum integer-to-name mappings

Create a one-shot Python script (or extend `extract_schema.py` with an `--enum-names` flag)
that imports `gramps.gen.lib` from a Gramps 5.1.6 checkout and iterates over each enum class
to produce `{value: name}` mappings. Gramps 5.1 enum classes are either standard Python `Enum`
(where `member.value` is the integer and `member.name` is the string) or classes with uppercase
constants.

Note: the existing `extract_schema.py` calls `cls.get_schema()` which returns JSON Schema with
integer values (and often duplicated values from `oneOf` alternatives). It does NOT produce
name mappings. A separate code path that captures `(member.name, member.value)` tuples is needed.

**Procedure:**

```bash
# Clone Gramps 5.1.6 if not already available
# Run extraction script with PYTHONPATH pointing to the Gramps checkout
PYTHONPATH=/path/to/gramps-5.1.6 python extract_schema.py \
    --enum-names --output crates/typed-graph/build/enum_constants_5_1.json
```

Output format:

```json
{
  "EventType": {
    "0": "Custom",
    "1": "Marriage",
    "2": "Marriage Settlement",
    "...": "..."
  },
  "EventRoleType": { "...": "..." },
  "DateModifier": { "...": "..." },
  "DateQuality": { "...": "..." }
}
```

Commit `build/enum_constants_5_1.json` as a static artifact. Example for `EventType`:

| Integer | Name |
|---|---|
| 0 | `"Custom"` |
| 1 | `"Marriage"` |
| 2 | `"Marriage Settlement"` |
| … | … |

#### 0b: Verify required/optional field status

Cross-reference the `required` inference heuristics (Step 1a, item 5) against the Gramps 5.1.6
Python source. For each primary type, check the `__init__` signatures and document which fields
are truly required vs. optional. Pay special attention to:

- `gramps_id`: likely optional (Gramps auto-generates IDs if empty)
- `father_handle` / `mother_handle` on Family: likely optional (a Family can have children without parents)
- `type_field` on Event: likely required
- `handle`: always required on every primary type
- `gender` on Person: likely required (defaults to `Person.UNKNOWN`)

Commit a table of expected required fields per type (in `docs/research/` or as a section of
`enum_constants_5_1.json`) to serve as the ground truth for converter tests.

#### 0c: Verify URL type compatibility across versions

Extract the 5.1 `Url` secondary type's fields (after conceptual conversion to flat format) and
diff against 5.2's `Url` secondary type. The plan renames 5.1 `urls` → `url_list` for consistency;
confirm that the `Url` embedded type has compatible fields between versions to avoid silent merge
corruption. If the `Url` struct has different fields, the merge optionality rule will handle it —
but flag any structural differences.

#### 0d: Enum variant name cross-reference

For `DateModifier` and `DateQuality` (which 5.2 defines but 5.1 uses as raw integers), compare
the 5.2 PascalCase variant names against the names extracted from the 5.1 enum members. Confirm
they match so the merge produces clean deduplication. If any variant names differ, document the
mapping explicitly.

### Step 1: Create `crates/typed-graph/src/schema_convert.rs`

The module lives in `src/` as a regular library module (not under `build/`), so its
`#[cfg(test)]` unit tests run with `cargo test -p typed-graph`. It is declared in `lib.rs`
via `pub mod schema_convert;` so `build.rs` can call it through the crate dependency.

The module exposes three public functions:

```rust
/// Returns true if the schema is in JSON Schema format (fields contain a
/// "properties" key) rather than the custom flat format.
pub fn is_json_schema_format(schema: &Value) -> bool;

/// Convert a schema from JSON Schema format to the custom flat format.
/// `enum_constants` is the parsed `enum_constants_5_1.json` (passed in for testability).
/// Returns the converted schema, or a descriptive error message if the
/// schema has structural inconsistencies that prevent conversion.
pub fn convert(schema: Value, version: &str, enum_constants: &Value) -> Result<Value, String>;

/// Validate that a flat-format schema has all required keys and valid
/// structure before passing it to the merge algorithm. Catches converter
/// bugs early with clear error messages.
pub fn validate_flat_format(schema: &Value) -> Result<(), Vec<String>>;
```

#### 1a: Primary type conversion

For each entry in `primary_types`, the JSON Schema format has:

```json
"Person": {
  "fields": {
    "type": "object",
    "title": "Person",
    "properties": { … actual fields … }
  },
  "inherit_mixins": [ … ]
}
```

Conversion logic:

1. **Extract the `properties` object** from `fields.properties` — these become the new `fields` value.
2. **Skip internal fields**: `_class`, `private`, `change` — these are Gramps internal bookkeeping, skipped during conversion.
3. **Map JSON Schema type → custom format type**:

   | JSON Schema | Custom flat |
   |---|---|
   | `"string"` | `"string"` |
   | `"integer"` | `"integer"` |
   | `"boolean"` | `"boolean"` |
   | `"array"` | `"array"` |
   | `"object"` (title matches secondary type) | `"embedded"` with `"schema": "<Type>"` |
   | `"object"` (title matches `"Date"`) | treat as `DateValue` embedded (see Step 1e) |

4. **Determine `kind`**:
   - Field named `handle` → `"kind": "handle"`
   - Field name ends with `_handle` → `"kind": "handle_ref"`, `"target"` derived from stem
   - Array field ending with `_list` whose items are `{type: "string", maxLength: 50}` → `"kind": "handle_ref"` on items, target derived from stem
5. **Determine `required`** (cross-reference with Step 0b verification table):
   - The 5.1 JSON Schema doesn't include a top-level `required` array. Use structural heuristics to infer required-ness:
     - `handle` → always required on every primary type
     - Fields whose JSON Schema definition does **not** include a `oneOf [null, ...]` wrapper → required; null is the 5.1 way of expressing optionality
     - Fields present directly on the object (not wrapped in `oneOf`) → required
     - See the `oneOf` null-wrapper pattern on `primary_name.date` as an example of optional; fields without this pattern are required
   - **Verified required** (confirmed against Gramps 5.1.6 Python source in Step 0b):
     - `handle` (all types), `gender` (Person), `primary_name` (Person), `type_field` (Event), `name` (Place).
   - **Verified optional** (despite no `oneOf` null wrapper in JSON Schema):
     - `gramps_id` (Gramps auto-generates),
     - `father_handle` / `mother_handle` on Family (valid families can exist with children only).
     These get `"required": false` explicitly — do not rely on the `oneOf` heuristic alone.
   - For remaining fields, the `oneOf` heuristic is the primary signal; emit a
     `cargo::warning` for any ambiguous field so it can be manually reviewed.
6. **Convert array items** — disambiguate by title suffix:
   - `{type: "string", maxLength: 50}` → `{"kind": "handle_ref", "target": "<Target>"}`
   - `{type: "object", title: "XxxRef"}` (title ends in `"Ref"`) → `{"embedded": "XxxRef", "edges": [{"link": "XxxRef.ref", "target": "<Target>"}]}`
   - `{type: "object", title: "Xxx"}` (title does NOT end in `"Ref"`) → `"Xxx"` (plain embedded, no edge metadata — used for LdsOrd, Attribute, Address, Url, Name)
7. **Add cardinality**: All array fields get `"cardinality": {"min": 0, "max": null}` (the 5.1 JSON Schema carries no cardinality constraints; this is a limitation — validation will accept any array size for 5.1-origin fields).
8. **Field name normalization**: `urls` → `url_list` (consistency with 5.2). The 5.1 `urls` items are `{type: object, title: "Url", ...}` which convert to `"Url"` (embedded). The 5.2 `url_list` items are also `"Url"` (embedded). Step 0c verified that the `Url` secondary type has compatible fields across versions — so the merge should produce a clean union. If Step 0c finds field differences, document them here and verify the merge optionality rule handles them correctly.
9. **Field-name-based enum_ref detection**: Certain integer fields are recognized by name and mapped to `enum_ref` with the appropriate target:
   - `modifier` → `"type": "enum_ref", "target": "DateModifier"`
   - `quality` → `"type": "enum_ref", "target": "DateQuality"`
   - `gender` → `"type": "enum_ref", "target": "Gender"`
   - `type_field` (aka `type` in the JSON Schema, for events/names/attributes) → `"type": "enum_ref", "target": "<parent-context>Type"` (heuristic: parent type name + "Type")
10. **Keep `inherit_mixins`** unchanged — already in correct format.

#### 1b: Secondary type conversion

Same logic as primary types. Additionally:

- For the `Tag` secondary type, if it appears in `secondary_types`: emit a one-shot `cargo::warning` (tracked with a `static AtomicBool` to avoid repeating every build) noting it's a probable extraction artifact (Tag is a primary type), then skip it during conversion. File a follow-up issue against `extract_schema.py`.
- Strip `_class`, `private` fields from all secondary types.

#### 1c: Enum type conversion

The 5.1 schema has enum values as integers:

```json
"EventType": {
  "values": [0, 1, 2, 3, 11, 12, 14, 15, 16, 17, 18]
}
```

Convert to named strings using the lookup tables from Step 0:

```json
"EventType": {
  "values": ["Custom", "Marriage", "Marriage Settlement", "Divorce", …]
}
```

If an integer value is not found in the lookup table, emit a `cargo::warning` and use the integer as a fallback string (e.g., `"Unknown(42)"`). This prevents silent data corruption.

#### 1d: Output validation

After conversion but before returning, call `validate_flat_format()` on the result to catch
structural errors early:

```rust
pub fn validate_flat_format(schema: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for (type_category, _label) in &[("primary_types", "primary"), ("secondary_types", "secondary")] {
        if let Some(obj) = schema.get(*type_category).and_then(|v| v.as_object()) {
            for (type_name, type_info) in obj {
                if let Some(fields) = type_info.get("fields").and_then(|v| v.as_object()) {
                    for (field_name, field_info) in fields {
                        // Every field must have a "type" key
                        if field_info.get("type").is_none() {
                            errors.push(format!("{}.{}: missing 'type'", type_name, field_name));
                        }
                        // handle_ref must have target
                        if field_info.get("kind").and_then(|v| v.as_str()) == Some("handle_ref")
                            && field_info.get("target").is_none() {
                            errors.push(format!("{}.{}: handle_ref missing 'target'", type_name, field_name));
                        }
                        // embedded must have schema
                        if field_info.get("type").and_then(|v| v.as_str()) == Some("embedded")
                            && field_info.get("schema").is_none() {
                            errors.push(format!("{}.{}: embedded missing 'schema'", type_name, field_name));
                        }
                        // array must have items
                        if field_info.get("type").and_then(|v| v.as_str()) == Some("array")
                            && field_info.get("items").is_none() {
                            errors.push(format!("{}.{}: array missing 'items'", type_name, field_name));
                        }
                    }
                }
            }
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

This catches converter bugs (missing required flat-format keys) before the merge algorithm
fails with confusing errors.

#### 1e: Date type handling — normalize to 5.2-compatible `DateValue`

The 5.1 `Date` object stores date parts differently from 5.2's `DateValue`:

| Aspect | 5.1 `Date` | 5.2 `DateValue` |
|---|---|---|
| Year/month/day | `dateval: [year, month?, day?]` (array) | `year: i32`, `month: Option<i32>`, `day: Option<i32>` (scalars) |
| Calendar | `calendar: i32` (extra) | (not present) |
| Sort value | `sortval: i32` (extra) | (not present) |
| Modifier | `modifier: i32` | `modifier: DateModifier` (enum) |
| Quality | `quality: i32` | `quality: DateQuality` (enum) |
| Display text | `text: String` | `text: Option<String>` |

**Strategy**: The converter normalizes the 5.1 `Date` to match 5.2's `DateValue` structure so the merge produces a single, coherent struct. The raw 5.1 fields (`calendar`, `dateval`, `sortval`) are not preserved — they are translated and dropped.

**Conversion rules**:

1. Extract `year`, `month`, `day` from `dateval` array:
   - `dateval[0]` → `year` (always present; default 0 if missing)
   - `dateval[1]` → `month` (optional; missing if array length < 2)
   - `dateval[2]` → `day` (optional; missing if array length < 3)
2. Map `modifier: integer` → `modifier: DateModifier` using the lookup table from Step 0.
3. Map `quality: integer` → `quality: DateQuality` using the lookup table from Step 0.
4. `text: string` → `text: Option<String>`.
5. Drop `calendar` and `sortval` (internal Gramps bookkeeping, not exposed in XML).

**Generated secondary type** (added to `secondary_types` when 5.2 is not loaded; suppressed when 5.2 provides DateValue):

```json
"DateValue": {
  "fields": {
    "year":      {"type": "integer", "required": true},
    "month":     {"type": "integer", "required": false},
    "day":       {"type": "integer", "required": false},
    "modifier":  {"type": "enum_ref", "target": "DateModifier", "required": true},
    "quality":   {"type": "enum_ref", "target": "DateQuality", "required": true},
    "text":      {"type": "string", "required": false}
  }
}
```

**Converted field reference** (e.g., on `Citation.date`, `Name.date`, etc.):

```json
"date": {"type": "embedded", "schema": "DateValue", "required": false}
```

The `oneOf` null wrapper becomes `required: false`. This matches how `DateValue` works in 5.2.

**New enums**: The converter must also build `DateModifier` and `DateQuality` enum value tables for 5.1 from Step 0 research, using the **same PascalCase variant names** as 5.2 (e.g., `DateModifier::None`, `DateModifier::Before`, etc.) to ensure clean merging.

**Suppression when 5.2 provides DateValue**: If both 5.1 and 5.2 are loaded, 5.2 already defines `DateValue` in `secondary_types`. The converter should check whether `DateValue` already exists in the schema and skip adding it, since the field shapes should be identical after normalization. If they differ, the merge algorithm will report the conflict at build time.

#### 1f: Tests for the conversion module

The conversion module carries `#[cfg(test)]` unit tests in `src/schema_convert.rs`.
These run with `cargo test -p typed-graph` since the module is part of the library crate.
Minimum test coverage:

| Test | What it verifies |
|---|---|
| `detect_json_schema_format_true` | `is_json_schema_format()` returns true for a schema where fields contain `"properties"` |
| `detect_json_schema_format_false` | Returns false for the flat format (5.2 style) |
| `detect_json_schema_format_empty` | Returns false for a schema with no primary or secondary types |
| `convert_primary_string_field` | `"string"` → `{"type": "string"}` |
| `convert_primary_integer_field` | `"integer"` → `{"type": "integer"}` |
| `convert_primary_boolean_field` | `"boolean"` → `{"type": "boolean"}` |
| `convert_primary_embedded_object` | `"object"` with title → `{"type": "embedded", "schema": "…"}` |
| `convert_primary_handle_ref` | `_handle` suffix + string → `{"kind": "handle_ref", "target": "…"}` |
| `convert_primary_array_handle_list` | `_list` + `{type: string, maxLength: 50}` items → handle_ref items |
| `convert_primary_array_embedded_ref` | Array with `{title: "XxxRef"}` items → embedded with edges |
| `convert_secondary_strips_internal` | `_class`, `private` stripped from secondary types |
| `convert_enum_int_to_name` | Integer enum values map to string names using lookup tables |
| `convert_enum_unknown_int` | Unknown integers produce fallback strings (e.g., `"Unknown(42)"`) |
| `convert_date_embedded_field` | Date field `oneOf [null, object]` → `{"embedded": "DateValue", "required": false}` |
| `convert_date_normalized_to_52_shape` | 5.1 Date object → DateValue with `year`/`month`/`day` scalars, not `dateval` array |
| `convert_required_by_structure` | Fields without `oneOf` null wrapper → `required: true` |
| `convert_optional_by_oneof` | Fields with `oneOf [null, …]` → `required: false` |
| `convert_enum_ref_by_field_name` | `modifier`/`quality`/`gender` integer fields → enum_ref with correct target |
| `convert_primary_empty_properties` | Primary type with only `_class`/`private`/`change` fields → produces empty fields dict |
| `convert_oneof_more_than_two` | `oneOf` with 3+ alternatives picks first non-null type |
| `validate_flat_format_all_required_keys` | `validate_flat_format()` rejects output missing `type` on a field |
| `validate_flat_format_handle_ref_missing_target` | `validate_flat_format()` rejects handle_ref without target |
| `property_convert_idempotent` | Converting an already-flat schema is a no-op (idempotency property-based test) |
| `property_is_json_schema_false_after_convert` | `is_json_schema_format()` returns false for every converted output |

### Step 2: Integrate into `build.rs`

**Load enum constants** in `main()` (before `load_schemas`):

```rust
let crate_dir = Path::new(&manifest_dir);
let enum_constants_path = crate_dir.join("build/enum_constants_5_1.json");
let enum_constants: serde_json::Value = if enum_constants_path.exists() {
    serde_json::from_str(&fs::read_to_string(&enum_constants_path)?)?
} else {
    serde_json::Value::Null  // No 5.1 constants; converter will emit warnings for unknown ints
};
```

**Wire the converter** in `load_schemas()`:

```rust
// In load_schemas(), after parsing JSON successfully:
let schema = if typed_graph::schema_convert::is_json_schema_format(&schema) {
    let schema = typed_graph::schema_convert::convert(schema, version, &enum_constants)
        .expect("schema conversion should succeed for valid Gramps JSON Schema");
    // Validate the output before it reaches the merge algorithm
    if let Err(errs) = typed_graph::schema_convert::validate_flat_format(&schema) {
        for e in &errs {
            eprintln!("cargo::warning=schema-{}: {}", version, e);
        }
        panic!("Converted schema-{} failed flat-format validation: {:#?}", version, errs);
    }
    schema
} else {
    schema
};
```

**Resolve the build dependency** — one of:

- **Option A (recommended)**: Add `[build-dependencies]` in `Cargo.toml` depending on `typed-graph`.
  This works because Cargo builds build-dependencies before the crate itself. But it may cause a
  circular dependency if `typed-graph`'s `build.rs` depends on code that itself needs the build script.
- **Option B (fallback)**: Use `#[path = "src/schema_convert.rs"] mod schema_convert;` directly in
  `build.rs` to inline the module source. This avoids any dependency cycle but means the `#[cfg(test)]`
  tests in `schema_convert.rs` won't run from `build.rs`. In this case, add a separate integration
  test file `tests/schema_convert_tests.rs` that also includes the module via `#[path]`.

### Step 3: Verify builds pass

Build and test matrix to cover all configurations:

```bash
# 5.1 only (no default features)
cargo build --no-default-features --features schema-5-1

# Merged 5.1 + 5.2 (typical user command)
cargo build --features schema-5-1

# Default (5.2 only) — must still work unchanged
cargo build

# Full test suite
cargo test --workspace

# All feature combinations
cargo test --workspace --all-features

# Lint
cargo clippy --all-targets --all-features -- -D warnings
```

### Step 3a: Add integration tests for the merged schema path

After the builds pass, add integration tests that exercise the merged 5.1+5.2 schema at runtime:

1. **Shape test**: Construct a `PersonData` under the merged schema. Verify fields that exist in both versions are accessible; verify 5.1-only fields (e.g., `birth_ref_index`) are `Option<i32>` and default to `None`.
2. **Generation test**: Run `generate_random()` with `Schema::for_version("5.1")` and verify the graph passes `graph.validate()`. Repeat with `Schema::default_version()` (merged).
3. **Enum values test**: Verify that `Schema::valid_enum_values` for `EventType` contains both 5.1 and 5.2 values, with no duplicate variant names.
4. **DateValue test**: Construct a `DateValue` via `DateValue::new(1870)`, verify `is_valid()` returns `true`, and verify `display_text()` returns `"1870"` under both the 5.1-only and merged schemas.
5. **XML round-trip**: Generate a small graph (10 persons, 2 families) with the merged schema, serialize to `.gramps` XML, and verify it's well-formed XML with a `<database>` root element.

These tests live in `crates/typed-graph/tests/merged_schema.rs` or alongside existing integration tests.

### Step 4: Verify `date.rs` DateValue methods

Since the converter now normalizes 5.1 dates to 5.2's `DateValue` shape (see Step 1e), the struct fields (`year`, `month`, `day`, `quality`, `modifier`, `text`) should be identical across versions. The existing methods (`new`, `new_ymd`, `display_text`, `is_valid`) should work without changes. Run the existing tests to confirm and add a test that constructs a `DateValue` under the 5.1-only schema.

### Step 5: Fix `generate/random.rs` struct construction

Struct literal construction in the random generator may reference fields that become `Option<T>` under the merge. Each such field needs `Some()` wrapping. Use `Schema::field_availability` at runtime to conditionally populate version-specific fields:

```rust
let schema = Schema::for_version(version).unwrap();
let key = format!("{}.{}", type_name, field_name);
if schema.field_availability.get(key.as_str()).map(|v| v.contains(&version)).unwrap_or(false) {
    data.field_name = Some(value);
}
```

### Step 6: Fix `generate/builder.rs` build methods

Builder `build()` methods construct data structs inline. Verify that all fields are accessible with their merged names and that `Option<T>` fields use `Some()` or `None` appropriately.

### Step 7: Fix `generate/adversarial.rs` field manipulation

Adversarial strategies that read or write struct fields must handle `Option<T>` fields. Use `if let Some(ref mut val) = data.optional_field { ... }` patterns.

### Step 8: Verify `validate.rs`

Run the validation tests under all feature combinations. The validation code should be schema-driven (uses `Schema::required_fields` and `Schema::cardinality_constraints`) so it should automatically adapt to the merged schema.

### Step 9: Update `extract_schema.py` (future-proofing)

The `extract_schema.py` script should eventually emit the custom flat format directly. Two options:

- **Option A**: Add a post-processing step that calls the same conversion logic (ported to Python, or shelling out to a Rust binary).
- **Option B**: Detect when `cls.get_schema()` returns JSON Schema format and print a warning directing the user that the build system handles it automatically at compile time.

Option B is sufficient for now since the Rust converter handles it transparently.

---

## Open Questions

1. **Enum value lookup tables**: The Gramps 5.1.6 source must be consulted to build accurate integer↔name mappings for each enum type. Should we extract these programmatically via `extract_schema.py` (pointed at a 5.1 checkout) or manually transcribe them? **→ Decision: Run `extract_schema.py` against a 5.1.6 checkout once to capture the enum values, then commit the resulting lookup tables as a static data file (e.g., `build/enum_constants_5_1.json`). This gives us a reproducible artifact without adding a Python dependency to the build.**

2. **`birth_ref_index` / `death_ref_index`**: Include as `Option<i32>` fields on `PersonData` (preserving 5.1 fidelity), or drop them (they're convenience indexes, not canonical data)? **→ Decision: Keep them. The converter skips `_class`, `private`, `change` (internal bookkeeping) but preserves `birth_ref_index` and `death_ref_index` since they carry meaningful data (index into event_ref_list). The merge optionality rule will make them `Option<i32>` automatically.**

3. **`DateValue` field compatibility**: Resolved — the converter normalizes 5.1's `Date` object to match 5.2's `DateValue` structure by extracting `year`/`month`/`day` from the `dateval` array and dropping `calendar`/`sortval`. See updated Step 1e. When both 5.1 and 5.2 are loaded, the converter suppresses its DateValue and lets 5.2's definition take precedence since the field shapes are identical after normalization.

---

## Files Affected

| File | Change |
|---|---|
| `crates/typed-graph/src/schema_convert.rs` | **New**: conversion module (~500–600 lines including tests) |
| `crates/typed-graph/src/lib.rs` | Add `pub mod schema_convert;` |
| `crates/typed-graph/build/enum_constants_5_1.json` | **New**: enum lookup tables from Step 0 (~200 lines) |
| `crates/typed-graph/tests/merged_schema.rs` | **New**: integration tests for merged schema path (Step 3a) |
| `crates/typed-graph/Cargo.toml` | Add `[build-dependencies]` on `typed-graph` (or use `#[path]` include for cycle avoidance) |
| `crates/typed-graph/build.rs` | ~16 line addition: enum constants loading + format detection + conversion call + flat-format validation in `load_schemas()` |
| `schemas/schema-5.1.json` | **Unchanged** — stays in JSON Schema format on disk |
| `crates/typed-graph/src/generate/random.rs` | May need adjustments for merged `Option<T>` fields (Step 5) |
| `crates/typed-graph/src/generate/builder.rs` | May need adjustments for merged field types (Step 6) |
| `crates/typed-graph/src/generate/adversarial.rs` | May need adjustments for optional fields (Step 7) |
| `crates/typed-graph/src/date.rs` | Verify `DateValue` convenience methods (Step 4) |
| `crates/typed-graph/src/validate.rs` | Verify no breaking changes (Step 8) |
| `extract/extract_schema.py` | Add `--enum-names` flag for Step 0a; optional: add warning for JSON Schema output |
| `docs/research/required-fields-5.1.md` | **New**: required-fields verification table from Step 0b |

---

## Risk Assessment

- **Low risk**: The conversion runs at build time and is transparent. The `schema-5.1.json` file is untouched. Schema-5.2-only builds are unaffected (the format detector sees no `properties` key and passes through unchanged).
- **Date normalization risk**: The converter translates 5.1 `dateval` arrays to 5.2-style `year`/`month`/`day` scalars. If the extraction logic is wrong (e.g., off-by-one in array indexing), generated dates will be incorrect. Mitigation: unit tests covering `dateval` → scalar extraction with known examples; comparison against Gramps 5.1.6 XML exports. Additionally, `validate_flat_format()` catches structural errors before the merge algorithm sees them (see Step 1d).
- **Enum mapping risk**: If integer-to-name lookups are wrong, generated datasets will use incorrect event types, name types, etc. Mitigation: warn on unknown integers; validate against Gramps 5.1.6 source. Additionally, variant name collisions across versions (e.g., 5.1 integer "0" maps to `Custom` but 5.2 already has a `Custom` with different semantics) need verification. Run `cargo build --features schema-5-1` and check for duplicate variant errors.
- **Merge optionality risk**: Fields that become `Option<T>` due to version differences must be handled in generator code. The `Schema::field_availability` API provides the mechanism, but existing code may need updates in places it doesn't yet use it. Steps 4-8 address this systematically, each as a separate testable commit.
- **Build dependency cycle risk**: If `build.rs` depends on `typed-graph` (for `schema_convert`), and `typed-graph`'s build script is `build.rs` itself, this creates a circular build-dependency. Mitigation: use Option B from Step 2 — `#[path = "src/schema_convert.rs"]` in `build.rs` to inline the module, and add a separate integration test file `tests/schema_convert_tests.rs` that also includes the module via `#[path]`. This avoids the cycle at the cost of a slightly non-standard test setup.
