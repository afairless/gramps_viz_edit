# Multi-Version Schema Support — Implementation Plan

## 1. Overview

**Goal**: Extend gramps-gen to support multiple Gramps schema versions (currently 5.2; user needs 5.1.6), with a download/discovery tool that presents available versions from the Gramps GitHub repo and lets users choose which to install, plus runtime schema selection in the CLI.

**User decisions** (from design questionnaire):

| Decision | Choice |
|---|---|
| Schema switching mechanism | Cargo features — one feature per version, all enabled schemas compiled into the binary |
| Schema download source | Clone Gramps repo at a git tag, run the Python extractor |
| Storage layout | `schemas/schema-{version}.json` (flat, e.g. `schemas/schema-5.2.json`) |
| Default version | Highest installed version |

**Scope**: Gramps versions ≥ 5.0, excluding pre-release tags (beta, alpha, rc). The GitHub API confirms tags exist for: 5.0.0–5.0.2, 5.1.0–5.1.6, 5.2.0–5.2.4, 6.0.0–6.0.8, and 6.1.0-beta1.

---

## 2. Current State (Baseline)

### 2.1 Schema flow today

```
extract/extract_schema.py          # Python: imports gramps.gen.lib, outputs schema.json
        │
        ▼
schemas/schema.json                # Single committed artifact, hardcoded version "5.2"
        │
        ▼
crates/typed-graph/build.rs        # Reads schemas/schema.json at compile time
        │                            # Generates $OUT_DIR/generated_schema.rs
        ▼
crates/typed-graph/src/schema.rs   # include!($OUT_DIR/generated_schema.rs)
        │
        ▼
All downstream code                # Uses generated Node, Edge, XxxData, Schema types
```

### 2.2 All touchpoints that reference the schema

| File | Role |
|---|---|
| `schemas/schema.json` | Committed schema artifact |
| `extract/extract_schema.py` | Python extractor (hardcodes version "5.2") |
| `crates/typed-graph/build.rs` | Compile-time codegen (hardcodes path `schemas/schema.json`) |
| `crates/typed-graph/src/schema.rs` | `include!` generated code |
| `crates/typed-graph/src/lib.rs` | Re-exports `Schema`, tests reference `Schema::new()`, `schema.version == "5.2"` |
| `crates/typed-graph/src/validate.rs` | Uses `Schema.required_fields`, `Schema.cardinality_constraints` |
| `crates/typed-graph/src/generate/mod.rs` | Takes `&Schema` parameter |
| `crates/typed-graph/src/generate/builder.rs` | GraphBuilder; uses generated types |
| `crates/typed-graph/src/generate/random.rs` | Random generation; uses generated types |
| `crates/typed-graph/src/generate/adversarial.rs` | Adversarial transforms; uses generated types |
| `crates/typed-graph/src/date.rs` | `DateValue` convenience impls |
| `crates/output/src/serialization_map.rs` | Hand-coded `SerializationMap`; assumes 5.2 structure |
| `crates/output/src/xml.rs` | `GraphXmlWriter`; hardcodes `version="5.2"` in header |
| `crates/cli/src/commands/generate.rs` | Calls `Schema::new()`, passes to pipeline |
| `crates/cli/src/commands/extract_schema.rs` | Stub command |
| `crates/cli/src/scenario.rs` | YAML scenario parsing; no schema dependency |

### 2.3 Hardcoded version references

- `schema.json`: `"version": "5.2"` (top-level field)
- `extract_schema.py`: `"version": "5.2"` in `extract_schema()` function
- `build.rs`: path `schemas/schema.json` is hardcoded
- `xml.rs`: header writes `version="5.2"` in `<created>` element
- `lib.rs` tests: `assert_eq!(schema.version, "5.2")`

---

## 3. Design

### 3.1 Schema storage layout

```
schemas/
├── schema-5.0.json       # Schema for Gramps 5.0.x
├── schema-5.1.json       # Schema for Gramps 5.1.x (user's target: 5.1.6)
├── schema-5.2.json       # Schema for Gramps 5.2.x (current default)
└── schema-6.0.json       # Schema for Gramps 6.0.x
```

Each file is a complete schema.json with the same structure as today. The existing `schema.json` is migrated to `schema-5.2.json` in a single commit — no symlink or transitional copy is needed because `build.rs` is updated simultaneously to read the new paths. Any external scripts or CI that reference `schema.json` must be updated to point at `schema-5.2.json` or use the new `schema list` / `schema download` commands.

**Rationale**: Flat naming is simpler to scan, avoids deep directory trees for a small number of files, and maps directly to version strings. A symlink adds confusion about which path is canonical and when the transition is "complete."

### 3.2 Version detection heuristics

When the user downloads a schema for, say, tag `v5.1.6`, the major.minor version (`5.1`) is used for:

- The filename: `schema-5.1.json`
- The Cargo feature name: `schema-5-1`
- The `version` field in the JSON: `"5.1"`

Only the major.minor is significant because Gramps patch releases (5.1.5 → 5.1.6) don't change the data model. The download tool maps each git tag to its major.minor and deduplicates: only the **latest patch** for each major.minor is offered. So the user sees "5.1 (latest: 5.1.6)" rather than every patch.

### 3.3 Schema download & discovery tool

#### UX flow

```
$ gramps-gen schema list

Local schemas:
  ✓ 5.2  (schemas/schema-5.2.json)

Available from GitHub (gramps-project/gramps):
  • 5.0  (latest: 5.0.2)  — not downloaded
  • 5.1  (latest: 5.1.6)  — not downloaded
  • 6.0  (latest: 6.0.8)  — not downloaded

Run `gramps-gen schema download <version>` to download.
Run `gramps-gen schema download --all` to download all available.

$ gramps-gen schema download 5.1

Cloning gramps-project/gramps at tag v5.1.6...
Running schema extractor...
Schema written to schemas/schema-5.1.json (10 primary types, 17 secondary types, 22 enum types)
```

#### Implementation: new CLI subcommand `schema`

Add a `schema` subcommand with sub-subcommands:

```text
gramps-gen schema list     — show local + remote, with download status
gramps-gen schema download [VERSION|--all] — download one or all
```

**Logic for `list`**:

1. Scan `schemas/schema-*.json` for local schemas, extract version from filename
2. Query `https://api.github.com/repos/gramps-project/gramps/tags?per_page=100`
3. Parse tags: extract major.minor, filter ≥ 5.0, exclude pre-release (beta/alpha/rc), keep highest patch per major.minor
4. Cache the resolved tag map to `~/.cache/gramps-gen/tags.json` (TTL: 1 hour) so `list` and `download` don't re-query on every invocation; if the API is unreachable, fall back to the cached data with a warning
5. Present a table: local checkmarks + remote availability

**Logic for `download`** (follows the three-stage data pipeline model):

**Stage 1 — Ingestion (validate, don't transform):**

1. Resolve `5.1` → best tag (e.g., `v5.1.6`) via the cached tag list
2. Shallow-clone `https://github.com/gramps-project/gramps.git` at that tag into a temp directory (`git clone --depth 1 --branch v5.1.6`)
3. Validate the clone: verify the expected Python package (`gramps/gen/lib/`) exists

**Stage 2 — Transformation:**

1. Run `PYTHONPATH=<temp_clone> python extract/extract_schema.py --version 5.1 --output schemas/schema-5.1.json`
2. Validate the output: parse the resulting JSON, verify it has `primary_types`, `secondary_types`, `enum_types`, and a `version` field matching `"5.1"`

**Stage 3 — Output:**

1. Write the validated JSON to `schemas/schema-5.1.json` atomically (write to temp file, then rename)
2. Clean up temp clone directory
3. Report result: type counts, file size, path

**Error handling matrix for `download`:**

| Failure | Detection | `CliError` variant | User message |
|---|---|---|---|
| GitHub API unreachable / rate-limited | HTTP status ≠ 200, no cached tag list | `GitHubApiError` | "Cannot reach GitHub API. Check your network or try again later." |
| Tag not found for version | Version not in cached/remote tag map | `SchemaDownloadError` | "No tag found for version X.Y. Run `schema list` to see available versions." |
| Git clone fails | git exits non-zero | `GitCloneFailed` | "Failed to clone gramps-project/gramps at tag vX.Y.Z. Check network and disk space." |
| Python not available | `ENOENT` spawning python | `PythonNotFound` | "Python 3 is required. Install it or manually place a schema-X.Y.json file." |
| Extractor fails | Non-zero exit or invalid JSON output | `SchemaExtractionFailed` | "Schema extraction failed. The Gramps version may have an incompatible API." |
| Output validation fails | JSON missing required keys | `SchemaExtractionFailed` | "Extracted JSON is not a valid schema (missing primary_types)." |

**Alternative for environments without Python/Git**: The `list` command notes that downloading requires Python 3 and git. Users in constrained environments can manually place a `schema-{version}.json` file.

### 3.4 Build system: Cargo features

#### Feature naming

```toml
# crates/typed-graph/Cargo.toml
[features]
default = ["schema-5-2"]          # Default: only 5.2 (matches current behavior)
schema-5-0 = []
schema-5-1 = []
schema-5-2 = []
schema-6-0 = []
all-schemas = ["schema-5-0", "schema-5-1", "schema-5-2", "schema-6-0"]
```

Each feature `schema-X-Y` gates inclusion of the corresponding `schemas/schema-X.Y.json` file.

#### build.rs changes

Current `build.rs` reads one schema file. The new version:

1. Determines which schema features are enabled via `cfg!`-equivalent env vars (`CARGO_FEATURE_SCHEMA_5_2`, etc.)
2. Reads each enabled schema JSON
3. Generates code for the **union superset** of all enabled schemas

#### Union-type generation strategy

Gramps minor versions (5.0, 5.1, 5.2) share the same 10 primary types with largely the same field structure. Version differences manifest as:

- New optional fields added in later versions
- New enum variants added in later versions
- Different required/cardinality rules

**Strategy**: Generate Rust types from the union of all enabled schema versions.

**Multi-schema merge algorithm:**

1. **Load all** enabled `schema-X.Y.json` files into memory
2. **Primary types**: Intersect the type names across versions. For each type, union all fields — the set of field names across all loaded schemas. For each field, track which versions it appears in.
3. **Field type resolution**: For each field in the union set:
   - If the field has the same type and `kind` across all versions that define it → use that Rust type
   - If the field has different types across versions → **error at build time** with a message listing the conflict (e.g., `PersonData.foo: string in 5.1, enum_ref in 5.2`). This requires manual resolution.
4. **Optionality rule**: A field is `Option<T>` if ANY enabled version has it as not-required OR if ANY enabled version doesn't define it at all. A field is non-`Option<T>` only when ALL enabled versions define it AND ALL mark it required. This preserves type safety for the common single-schema default build.
5. **Enum types**: Union all variant values across versions. A variant that doesn't exist in all enabled versions gets a `#[doc]` comment listing which versions include it. The per-version `valid_enum_values` HashMap controls runtime validation.
6. **Edge enum**: Union all edge variants across versions using the same deduplication logic as today.
7. **Secondary/embedded types**: Union of all such types across versions; same field union rules apply.

**Type generation summary:**

1. **Enum types**: Union of all values across versions. Variants are always compiled in (the type is shared); the per-version `Schema.valid_enum_values` controls whether a variant is valid for a given version at runtime.
2. **Data structs**: Union of all fields across versions, with the optionality rule above. In a single-schema build (the default), structs are identical to today's generated types — no regression.
3. **Edge enum**: Union of all edge variants across versions, deduplicated as today.
4. **Schema metadata**: One `static SCHEMA_X_Y: Schema` instance per compiled-in version, generated by iterating each version's schema JSON individually (not the union). `Schema::for_version("5.1")` returns the 5.1-specific constraints.

**Multi-Schema instance generation** (generated code pattern):

```rust
// Generated once per enabled version from that version's own schema JSON:
pub static SCHEMA_5_1: Schema = Schema {
    version: "5.1",
    required_fields: /* from schema-5.1.json */,
    cardinality_constraints: /* from schema-5.1.json */,
    valid_enum_values: /* from schema-5.1.json */,
};
pub static SCHEMA_5_2: Schema = Schema { /* from schema-5.2.json */ };

impl Schema {
    pub fn available_versions() -> &'static [&'static str] {
        &["5.1", "5.2"]  // sorted, generated from enabled features
    }
    pub fn default_version() -> &'static str {
        "5.2"  // last in sorted list = highest
    }
    pub fn for_version(version: &str) -> Option<&'static Schema> {
        match version {
            "5.1" => Some(&SCHEMA_5_1),
            "5.2" => Some(&SCHEMA_5_2),
            _ => None,
        }
    }
}
```

**Handling missing schema files at build time**: For each enabled feature `schema-X-Y`, build.rs checks that `schemas/schema-X.Y.json` exists before reading. If missing, it emits a clear error and exits:

```text
error: schemas/schema-5.1.json not found.
  hint: Run `gramps-gen schema download 5.1` to download it,
  or build with only the default schema: `cargo build`
  (which uses --features schema-5-2).
```

```rust
// generated_schema.rs (conceptual output for multiple features)

/// Runtime metadata for a specific schema version.
#[derive(Clone, Debug)]
pub struct Schema {
    pub version: &'static str,
    pub required_fields: HashMap<&'static str, Vec<&'static str>>,
    pub cardinality_constraints: HashMap<&'static str, (Option<u32>, Option<u32>)>,
    pub valid_enum_values: HashMap<&'static str, Vec<&'static str>>,
}

impl Schema {
    /// Returns schemas for all versions compiled into this binary.
    pub fn available_versions() -> &'static [&'static str] {
        // Generated from enabled features, e.g.:
        &["5.0", "5.1", "5.2"]
    }

    /// Returns the highest available version.
    pub fn default_version() -> &'static str {
        // Sorted, returns last. E.g., "5.2"
    }

    /// Get the Schema for a specific version.
    pub fn for_version(version: &str) -> Option<&'static Schema> {
        // Match version string to pre-built static Schema instance
    }
}
```

**Why union types work here**: Gramps data model changes slowly. A field like `PersonData.primary_name` exists in all versions. A field added in 5.2 would just be a new `Option<T>` field in the struct — always present in the Rust type, but the 5.1 Schema metadata marks it as "not available," so validation and serialization skip it when running against 5.1.

**Trade-off — runtime enum validation**: With union types, enum variants from a newer version can be assigned to data structs even when targeting an older version. The error is caught at runtime (via `Schema.valid_enum_values`) rather than at compile time. This is an acceptable trade-off because:

- The generator always respects the version-specific enum set, so generated data never contains out-of-version variants
- For the default single-schema build, enum types are identical to today — no regression
- Adding `#[cfg(feature = "schema-5-2")]` guards on version-specific variants would restore compile-time safety for single-schema builds but would make multi-schema builds fail to compile when referencing those variants. The runtime-validation approach keeps the codebase unified.

**Alternative rejected**: Per-version type families (`NodeV5_1`, `NodeV5_2`, …). This would require making `Graph` generic or duplicating all generation/serialization code. The union approach keeps the codebase unified.

**Field availability at runtime**: Union-type structs always contain all fields from all compiled-in versions. To let downstream code (generator, serializer, validator) know which fields are valid for the selected version, `Schema` exposes a `field_availability` map:

```rust
pub struct Schema {
    // ... existing fields ...
    /// Maps "Type.field_name" → list of versions where that field exists.
    /// E.g., ("Person.new_field", ["5.2", "6.0"]).
    /// Generated per-version: a field is in the list iff the selected
    /// version's schema JSON defines it.
    pub field_availability: HashMap<&'static str, Vec<&'static str>>,
}
```

Downstream usage:

- **Generator / GraphBuilder**: Before setting a field, check `schema.field_availability.get("Person.new_field").map_or(false, |vs| vs.contains(&selected_version))`. If false, leave the field as `None` / empty vec / default.
- **Serializer**: Before emitting a field's XML, check field availability for the selected version. Fields absent from that version are skipped (they are guaranteed `None`/empty by the generator, but the serializer provides a defensive check).
- **Validator**: `required_fields` entries for a version are only populated when the field exists in that version, so no special handling is needed beyond the existing logic.

For single-schema builds, `field_availability` is a no-op — every field exists in the one compiled-in version. For multi-schema builds, it gates version-specific fields.

### 3.5 Runtime schema selection

#### CLI flag

```
gramps-gen generate --schema-version 5.1 --count 200
gramps-gen generate                         # uses default (highest installed)
gramps-gen schema list                      # shows which versions are available locally
```

The `--schema-version` flag is added to `GenerateArgs`:

```rust
#[arg(long, default_value = "default")]  // "default" means auto-select
pub schema_version: String,
```

#### Default resolution

At invocation, if `--schema-version` is `"default"` (or omitted):

1. Call `Schema::default_version()` → returns highest compiled-in version
2. Report: `Using schema version 5.2 (default; also available: 5.1, 5.0)`

#### Error: requested version not compiled in

If the user requests a version that isn't among `Schema::available_versions()` (e.g., `--schema-version 5.1` in a default build that only includes `schema-5-2`), the CLI emits a clear error:

```text
error: schema version 5.1 is not available in this build.
  Available versions: 5.2
  Rebuild with: cargo build --features schema-5-1
```

This is a `CliError::ConfigError` so it prints cleanly and exits non-zero.

#### Validation with version-specific rules

`graph.validate(&schema)` already takes a `&Schema`. It uses:

- `schema.required_fields` → per-type required field lists
- `schema.cardinality_constraints` → per-field cardinality

With multi-version, the same code works unchanged — just pass the version-specific `Schema` instance. No changes to `validate.rs`.

#### Serialization with version-specific behavior

- **XML `<header>`**: The `<created version="...">` attribute uses the selected schema version.
- **Field presence**: The serializer already skips fields that are `None`. Union-type fields that don't exist in the selected version will be `None` at runtime because the builder/generator code respects schema version (see §3.6).
- **Enum value serialization**: If an enum variant doesn't exist in the target version, it should never appear because the generator respects the version-specific enum set.

**Risk**: The hand-coded `SerializationMap` currently assumes 5.2 structure. We need to verify it against 5.1.6. The Gramps XML namespace (`http://gramps-project.org/xml/1.7.2/`) appears stable across 5.x versions. If field structure differs, the `SerializationMap` or `GraphXmlWriter` may need version-conditional logic.

**`GraphXmlWriter` API change**: Currently `GraphXmlWriter::new(map: SerializationMap)` takes no version parameter. To write the correct version in the XML `<header>`, the writer needs the selected schema version. The new signature will be:

```rust
pub fn new(map: SerializationMap, schema_version: &str) -> Self
```

The CLI's `generate.rs` must pass the resolved schema version through: `GraphXmlWriter::new(map, &selected_version)`. This propagates to integration and E2E tests that construct a `GraphXmlWriter` directly.

### 3.6 Downstream impact summary

| Component | Impact | Effort |
|---|---|---|
| `extract_schema.py` | Accept `--version` flag to set version in output JSON | Small |
| `schemas/` | Migrate `schema.json` → `schema-5.2.json` | Trivial |
| `build.rs` | Multi-file reading, union merge algorithm, per-version Schema statics, missing-file errors | Medium |
| `schema.rs` | Replace `Schema::new()` with `Schema::for_version()`, `available_versions()`, `default_version()` | Medium |
| `lib.rs` (typed-graph) | Update all call sites of `Schema::new()` (tests, re-exports); test each version's metadata | Medium |
| `graph.rs` | No changes needed (union types are transparent to Graph) | None |
| `validate.rs` | No changes needed (already takes `&Schema`) | None |
| `generate/*.rs` | Generation respects version-specific enum values (`Schema.valid_enum_values`) and field availability | Small |
| `serialization_map.rs` | Verify against 5.1.6 per checklist in Phase D; conditional logic if needed | Small |
| `xml.rs` | `GraphXmlWriter::new()` gains `schema_version` parameter; write correct version in header; update all ~50 test helpers | Medium |
| `cli/src/commands/generate.rs` | Add `--schema-version` flag; resolve default; pass version to `GraphXmlWriter` | Small |
| `cli/src/commands/schema.rs` | New subcommand group: `schema list`, `schema download` | Medium |
| `cli/src/main.rs` | Register new `schema` subcommand | Small |
| `cli/Cargo.toml` | Add `ureq` dependency for GitHub API calls (blocking, minimal deps) | Small |
| `scenario.rs` | No changes needed (schema-agnostic config) | None |
| `error.rs` | New error variants: `GitHubApiError`, `GitCloneFailed`, `SchemaExtractionFailed`, `SchemaDownloadError`, `PythonNotFound` | Small |

### 3.7 Compatibility between versions

The critical question: **how much does the Gramps data model change between 5.0, 5.1, and 5.2?**

Based on the Gramps release history and the nature of the project, minor version changes in the 5.x line are expected to be additive:

- New optional fields on existing types
- New enum variants
- Possibly new secondary types

The union-type strategy (§3.4) handles additive changes gracefully. If a **breaking change** exists (field removed, renamed, or type changed), the merge algorithm detects conflicting field types at build time and reports them as errors requiring manual resolution. The first download of a 5.1 schema will reveal any breaking differences, and the implementation can adapt.

**Known trade-off**: Enum variants that exist only in newer schema versions are always present in the compiled Rust type. Assigning such a variant when targeting an older version is caught at runtime by validation (via `Schema.valid_enum_values`), not at compile time. For the default single-schema build there is no regression. See §3.4 for details.

### 3.8 The `schema.json` migration path

Current state:

```
schemas/schema.json   ← the only file; build.rs hardcodes this path
```

Desired state:

```
schemas/schema-5.2.json   ← former schema.json, renamed
schemas/schema-5.1.json   ← newly downloaded
...
```

Migration steps:

1. Rename `schemas/schema.json` → `schemas/schema-5.2.json`
2. Update `build.rs` to read `schemas/schema-{version}.json` based on enabled features
3. No symlink is needed — update any external scripts/CI that reference `schema.json` to point at `schema-5.2.json`
4. Commit all schema files to git for build reproducibility (see §7, question 1 — resolved)

---

## 4. Implementation Phases

### Phase A: Schema migration & build system (Day 1–2)

**Note**: Phase A's six sub-steps should each be implemented and committed as an independent unit following the incremental-development discipline — one commit per sub-step, with tests passing at each boundary.

**Step 0 — Catalog all `Schema::new()` call sites** (pre-implementation):

```bash
rg "Schema::new\|Schema::default" crates/
```

This discovers every location that constructs a `Schema`, which all need updating when the API changes from `Schema::new()` to `Schema::for_version()`. The same search should be run for `GraphXmlWriter::new(` to find all test call sites that need the new `schema_version` parameter.

1. **Rename** `schemas/schema.json` → `schemas/schema-5.2.json`
2. **Update `extract_schema.py`**: Accept optional `--version` flag to override the version string in the output JSON. When `--version` is omitted, auto-detect the Gramps version from the cloned source (e.g., read `gramps/version.py` or import `gramps.version`). The download command validates that the detected/extracted version matches the expected git tag. Also accept `--output` to control the file path (already partially supported).
3. **Add Cargo features** to `typed-graph/Cargo.toml`: `schema-5-0`, `schema-5-1`, `schema-5-2`, `schema-6-0`
4. **Rewrite `build.rs`**:
   - Detect enabled features via `CARGO_FEATURE_SCHEMA_*` env vars
   - For each enabled feature, check that the corresponding `schemas/schema-X.Y.json` exists; emit a clear error with remediation hint if missing
   - Read each enabled `schemas/schema-X.Y.json`
   - Run the multi-schema merge algorithm (§3.4) to generate union types
   - Generate per-version `static SCHEMA_X_Y: Schema` instances from each version's own JSON
   - Generate `Schema::available_versions()`, `default_version()`, `for_version()`
5. **Update `schema.rs`**: Expose `Schema::available_versions()`, `Schema::default_version()`, `Schema::for_version()`
6. **Update all tests** that reference `Schema::new()` or `"5.2"` version checks

### Phase B: Schema download tool (Day 3)

**Prerequisite**: Phase A step 2 (extract_schema.py `--version` flag) must be completed before download can write versioned schemas. If Phase B is started in parallel, the download command can initially write schemas without the `--version` flag and the extractor's own version field, adding the flag afterward.

1. **Add `cli/src/commands/schema.rs`** with:
   - `SchemaListArgs` / `SchemaDownloadArgs` structs
   - `cmd_list()`: local scan + GitHub API query + formatted output
   - `cmd_download()`: clone at tag, run extractor, save to `schemas/`
2. **Register `Schema` subcommand** in `cli/src/main.rs`:

   ```
   gramps-gen schema {list | download [VERSION] [--all]}
   ```

3. **Add new `CliError` variants**: `GitCloneFailed`, `SchemaExtractionFailed`, `GitHubApiError`
4. **GitHub API handling**:
   - Use `ureq` or `reqwest` (blocking) for the tags API call
   - Or shell out to `curl`/`wget` as a zero-dependency approach
   - If the API is unavailable, fall back to listing only local schemas with a note

### Phase C: Runtime schema selection (Day 4)

1. **Add `--schema-version` flag** to `GenerateArgs`
2. **Schema resolution logic**: If `"default"`, resolve to `Schema::default_version()`
3. **Pass the selected `&Schema`** through the generate → validate → serialize pipeline
4. **Update `xml.rs`**: Write the correct version in `<created version="...">`
5. **Test**: Generate with `--schema-version 5.1` and verify the output header

### Phase D: Verify & harden (Day 5)

1. **Download 5.1.6 schema**: `gramps-gen schema download 5.1`
2. **Diff `schema-5.1.json` vs `schema-5.2.json`**: Identify type/field/enum differences
3. **Verify `SerializationMap` against 5.1.6** using this checklist:
   - [ ] XML element names match between versions for all 10 primary types
   - [ ] XML attribute names match for all fields serialized as attributes
   - [ ] Child element names match for all fields serialized as child elements
   - [ ] Section ordering in `<database>` is consistent (tags, events, people, families, citations, sources, places, objects, repositories, notes)
   - [ ] XML namespace URI (`http://gramps-project.org/xml/1.7.2/`) is identical
   - [ ] No fields exist in 5.1 that are absent from 5.2 (or vice versa) without a corresponding serialization rule
   - [ ] If any difference is found, add version-conditional logic to `SerializationMap` or `GraphXmlWriter`
4. **Adapt code** if differences require it:
   - If union types need adjustment for breaking changes, handle them per the merge algorithm's conflict resolution
5. **Run CI matrix** for feature combinations:
   - `cargo test --workspace` (default features = schema-5-2 only)
   - `cargo test --workspace --no-default-features --features schema-5-1` (requires downloaded 5.1 schema)
   - `cargo test --workspace --all-features`
   - `cargo clippy --all-targets --all-features -- -D warnings`
6. **Add E2E tests**: Generate with 5.1.6 schema, validate, serialize
7. **Update ARCHITECTURE.md** and `AGENTS.md` to reflect multi-version support

---

## 5. Testing Strategy

### Unit tests

- `build.rs` tests: parse multiple schema files, verify union merge algorithm correctness (field union, enum union, optionality rule, conflict detection)
- `Schema` tests per version: required fields, cardinality, enum values, `for_version()` lookup, `default_version()` ordering
- Default version resolution logic
- Missing schema file → clear error message
- Schema merge conflict detection → build error with version details

### Integration tests

- Generate graph with 5.1 schema, validate with 5.1 schema → passes
- Generate graph with 5.2 schema, validate with 5.2 schema → passes
- Generate with 5.2, validate with 5.1 → may fail if 5.2 has fields 5.1 doesn't know about (expected behavior)
- Same-seed generation across versions → deterministic
- Download command with mocked GitHub API responses (all error cases from matrix in §3.3)
- Download command with mocked git clone and extractor (success path)

### Property-based tests

- **Round-trip invariant**: For any seed and any single schema version, `generate_random → validate → serialize → parse XML` produces output with the correct `<created version="...">` attribute
- **Cross-version determinism**: Same seed with different `--schema-version` values produces the same person names, genders, and event dates. Family structure and edge counts may differ when cardinality constraints or required-field rules differ between versions, but the core genealogical data (who is born when, to which parents) is deterministic across versions.
- **Union type soundness**: For any combination of enabled features, the generated Rust code compiles and all `Schema` instances have non-empty `required_fields` and `valid_enum_values`

### E2E tests

- `gramps-gen schema list` output is well-formed
- `gramps-gen schema list` with network unavailable → shows only local schemas with a note
- `gramps-gen schema download 5.1` produces `schemas/schema-5.1.json`
- `gramps-gen schema download invalid-version` → exits non-zero with helpful message
- `gramps-gen generate --schema-version 5.1 --count 10 --seed 42` produces valid output
- `gramps-gen generate` (no --schema-version) → uses default version in header
- Output XML has correct version in header
- Generated `.gramps` with `--schema-version 5.1` passes `gramps-gen validate`

### CI feature matrix

A CI job matrix covering key feature combinations to prevent regressions:

| Job | Features | Command |
|---|---|---|
| Default | `schema-5-2` (default) | `cargo test --workspace && cargo clippy -- -D warnings` |
| Single 5.1 | `--no-default-features --features schema-5-1` | `cargo test --workspace` |
| Single 5.0 | `--no-default-features --features schema-5-0` | `cargo test --workspace` |
| All schemas | `--all-features` | `cargo test --workspace && cargo clippy --all-features -- -D warnings` |

### Manual verification

- Import generated `.gramps` file into Gramps 5.1.6
- Verify all objects appear correctly
- Verify no version-mismatch warnings

---

## 6. Risks & Mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Gramps 5.1.6 has breaking schema differences from 5.2 | Low | Phase D diffs the schemas; merge algorithm detects conflicting field types at build time; breaking changes get case-by-case handling |
| GitHub API rate limiting (60 req/hr unauthenticated) | Medium | Cache the tags list at `~/.cache/gramps-gen/tags.json` (TTL: 1 hour, checked via `created_at` field); `list` command is infrequent; `download` only calls API once; if API is unreachable, fall back to cached data with a warning |
| git clone of full Gramps repo is slow (~200 MB) | Medium | Use `--depth 1 --branch v5.1.6` for a shallow clone (~20 MB); display progress |
| Python extractor fails on older Gramps versions | Medium | The extractor has a fallback introspection mode; if `get_schema()` is absent, it introspects `__init__` and type hints. Test against 5.0. |
| Union types bloat compile times | Low | Even with 4 schemas, it's ~40 enum types and ~30 structs — negligible. The generated code is already fast to compile. |
| SerializationMap (hand-coded) needs version conditionals | Low-Medium | Phase D diffs will reveal this. If needed, add a `version` field to `SerializationMap` or make the writer version-aware. |
| Supply-chain risk: downloading and executing code from GitHub | Medium | The `download` command displays a warning before cloning: "This will download and execute Python code from gramps-project/gramps at tag vX.Y.Z. Continue?" The Gramps repo is a well-known project; the extractor only imports `gramps.gen.lib` (not arbitrary code). For air-gapped environments, schemas can be manually placed. |
| Runtime enum validation loses compile-time safety for multi-schema builds | Low | Only affects multi-schema builds; single-schema default build is unchanged. The generator always respects version-specific enum sets, so out-of-version variants cannot appear in generated data. Documented as a known trade-off in §3.4 and §3.7. |
| Schema file missing at build time for non-default features | Medium | build.rs checks for each enabled schema file and emits a clear error with remediation hint (see §3.4). |

---

## 7. Open Questions for Future Iterations

1. **Should schemas be committed to git?** → **RESOLVED: Yes, commit all schemas.** Currently `schema.json` is committed. All `schema-X.Y.json` files will be committed for build reproducibility — `cargo build` with any feature flag works immediately after clone, no separate download step required. The `schema download` command remains available for obtaining new versions. The `.gitignore` will NOT exclude schema files.

2. **Schema for Gramps 6.x?** The GitHub API shows tags for 6.0.x. Gramps 6.0 may have significant data model changes. We should include the feature flag but mark it as experimental until verified.

3. **RelaxNG-driven SerializationMap?** Decision 5 in the design doc describes extracting the serialization mapping from the Gramps RelaxNG schema. Multi-version support makes this more valuable — each version's RelaxNG could drive its serialization rules. Worth revisiting after this feature lands.

4. **`validate` command schema awareness?** The current `gramps-gen validate` does a minimal XML check. It could be made schema-aware to validate `.gramps` files against a specific version's expectations.

5. **YAML scenario schema version?** Should scenario files be able to specify a target schema version? This could be useful for CI/CD pipelines. Add `schema_version: "5.1"` as an optional scenario field.
