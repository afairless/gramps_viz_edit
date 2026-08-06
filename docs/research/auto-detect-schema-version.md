# Auto-Detect Gramps Schema Version — Implementation Plan

## 1. Overview

**Goal**: Make gramps-gen tools (generate, diff, visualize, validate, stats) transparently support
all installed Gramps schema versions without requiring the user to choose at build time or pass
`--features` flags.

**Current problem**: The default Cargo feature is `["schema-5-2"]` only. Opening a Gramps 5.1 file
(e.g., version `5.1.6`) produces a confusing `unsupported schema version '5.1.6' (not compiled in)`
error. The runtime auto-detection and multi-schema merge infrastructure already exist — only the
default feature set needs changing.

**User impact**: After this change, `cargo build && gramps-gen diff file_a.gramps file_b.gramps`
works regardless of whether the files are 5.1 or 5.2 (or a mix). No `--features` flag needed.

## 2. Current State

### 2.1 What already works

| Capability | Status |
|---|---|
| Multi-schema merge at build time (union types, optional fields, enum union) | ✅ Complete |
| `Schema::available_versions()` returns all compiled-in versions | ✅ Complete |
| `Schema::for_version("5.1")` / `Schema::for_version("5.2")` runtime lookup | ✅ Complete |
| `detect_schema_version()` reads `<created version="X.Y.Z"/>` from XML header | ✅ Complete |
| Version truncation (`"5.1.6"` → `"5.1"`) | ✅ Complete |
| `#[cfg(feature = "schema-5-1")]` helpers for version-specific field access | ✅ Complete |
| Integration tests for merged 5.1+5.2 schema (`merged_schema.rs`) | ✅ Complete |
| Both schema files present in `schemas/` (`schema-5.1.json`, `schema-5.2.json`) | ✅ Complete |
| `--schema` CLI flag on `generate` command for runtime version selection | ✅ Complete |

### 2.2 What's blocking auto-detection

| Issue | Detail |
|---|---|
| Default features = `["schema-5-2"]` | Only 5.2 is compiled in unless user passes `--features schema-5-1` |
| `all-schemas` feature references phantom versions | `schema-5-0` and `schema-6-0` have no JSON files → build fails |
| Confusing error message | Shows full patch version `"5.1.6"` instead of the checked version `"5.1"` |
| No build-time warning for missing schema files | `build.rs` calls `process::exit(1)` — no graceful fallback |

### 2.3 Feature configuration (current)

```toml
# crates/typed-graph/Cargo.toml
[features]
default = ["schema-5-2"]                                           # only 5.2
schema-5-0 = []
schema-5-1 = []
schema-5-2 = []
schema-6-0 = []
all-schemas = ["schema-5-0", "schema-5-1", "schema-5-2", "schema-6-0"]  # broken: 5.0, 6.0 files don't exist
```

## 3. Design

### 3.1 Principle: compile all installed schemas by default

The user places schema files in `schemas/` (via `schema download` or manually). The build
system detects which files exist and compiles them all in. No Cargo feature selection needed.

### 3.2 New feature configuration

```toml
[features]
default = ["schema-5-1", "schema-5-2"]
schema-5-1 = []
schema-5-2 = []
all-schemas = ["schema-5-1", "schema-5-2"]
```

**Changes**:

- `schema-5-0` and `schema-6-0` features removed (no schema files exist for them)
- `default` includes both installed schemas
- `all-schemas` mirrors `default` (kept as a semantic alias; future-proof when new versions are added)

### 3.3 Build-time detection: graceful

`build.rs` currently exits with `process::exit(1)` when a schema file is missing. This is too
aggressive — it blocks `all-schemas` if any referenced file is absent. The new behavior:

- **If a feature is enabled but the schema file is missing**: emit a `cargo::warning` and skip it (don't error). This lets `all-schemas` always work even when only a subset of files are present.
- **If NO schema files are found** (no enabled features have files): error with a clear message.
- **Strict mode** (opt-in via env var `GRAMPS_STRICT_SCHEMA=1`): restore the old hard-error behavior for CI.

```rust
// build.rs pseudocode
for version in enabled_versions {
    if !schema_path.exists() {
        if strict_mode {
            eprintln!("error: {} not found.", schema_path.display());
            std::process::exit(1);
        } else {
            println!("cargo::warning=schema-{}.json not found; skipping schema {}", version, version);
            continue;
        }
    }
    // ... load and merge
}
```

### 3.4 Improved error messages

**Error path in `detect_schema_version`** (`gramps-reader/src/xml/header.rs`):

```rust
// Before (confusing):
Err(Error::UnsupportedSchema { version: v })  // v = "5.1.6"

// After (clear):
Err(Error::UnsupportedSchema {
    version: v.clone(),              // full: "5.1.6"
    schema_version: schema_version,  // truncated: "5.1"
})
```

Updated `Display` impl:

```
unsupported schema version '5.1' (file reports 5.1.6; not compiled in)
```

Add a suggestion at the end of the message:

```
hint: run `gramps-gen schema download 5.1` to download the schema,
or build with --no-default-features --features schema-5-2 to use only 5.2
```

This hint reflects the new reality where both schemas are compiled by default —
the user would only see this error if they explicitly disabled a feature or deleted
the schema file. It also points them to the `schema download` command as the primary
recovery path.

**Error display in CLI** (`cli/src/error.rs`): The `gramps_reader::Error::UnsupportedSchema`
gets converted to `CliError::ConfigError`, losing the version context. Add a dedicated variant
or enrich the conversion.

### 3.5 No codegen or merge changes needed

The union merge algorithm, `#[cfg]` helpers, `Schema::for_version()`, and `available_versions()`
all already work correctly when both `schema-5-1` and `schema-5-2` are enabled. The
`merged_schema.rs` integration tests prove this. No changes to `graph.rs`, `build.rs` merge
logic, or generation/validation code are required.

## 4. Implementation Steps

### Step 1 — Update feature defaults and remove phantom features

**File**: `crates/typed-graph/Cargo.toml`

- Change `default` from `["schema-5-2"]` to `["schema-5-1", "schema-5-2"]`
- Remove `schema-5-0` and `schema-6-0` feature entries (no schema files exist)
- Update `all-schemas` to reflect the new set
- Add a comment explaining the auto-detect philosophy

**File**: `crates/typed-graph/build.rs`

- Remove `"schema-5-0"` and `"schema-6-0"` entries from the `FEATURE_VERSIONS`
  static array (lines 36–40). These become permanently unreachable once the
  corresponding Cargo features are removed. If desired for forward-compatibility,
  comment them out with a note: `// Future: re-add when Gramps 5.0/6.0 schema files become available.`

### Step 2 — Graceful build.rs for missing schema files

**File**: `crates/typed-graph/build.rs`

- In `load_schemas()`, replace `process::exit(1)` on missing file with `cargo::warning` + `continue` (skip).
  Track a `loaded_count: usize` variable, increment it for each successfully loaded schema,
  and check after the loop: if zero schemas were loaded, emit a fatal error with a clear message
  suggesting `gramps-gen schema download <version>`.
- Honor `GRAMPS_STRICT_SCHEMA` env var for CI strictness. Read it once at the top:
  `let strict_mode = env::var("GRAMPS_STRICT_SCHEMA").is_ok();`

Implementation sketch for the modified loop:

```rust
let mut loaded_count: usize = 0;
for version in versions {
    let filename = version_to_filename(version);
    let schema_path = schemas_dir.join(&filename);

    if !schema_path.exists() {
        if strict_mode {
            eprintln!("error: {} not found.", schema_path.display());
            std::process::exit(1);
        } else {
            println!("cargo::warning=schema-{}.json not found; skipping schema {}", version, version);
            continue;
        }
    }

    // ... load, convert, validate schema ...
    loaded_count += 1;
}

if loaded_count == 0 {
    eprintln!("error: no schema files found. Run `gramps-gen schema download <version>` or place schema files in schemas/.");
    std::process::exit(1);
}
```

### Step 3 — Improve UnsupportedSchema error

**Files**:

- `crates/gramps-reader/src/error.rs` — add `schema_version` field to `UnsupportedSchema` variant
- `crates/gramps-reader/src/xml/header.rs` — pass both `version` (full) and `schema_version` (truncated)
- `crates/gramps-reader/src/error.rs` — update `Display` to show both and add hint

### Step 4 — Improve CLI error conversion

**Files**:

- `crates/cli/src/error.rs` — update the `From<gramps_reader::Error>` impl.
  The current conversion maps `UnsupportedSchema { version }` to a generic
  `ConfigError(format!("unsupported schema version: {}", version))`, losing the
  truncated `schema_version` field that will be added in Step 3. Update it to
  preserve both fields — the simplest approach is to enrich the `ConfigError`
  message string:

  ```rust
  gramps_reader::Error::UnsupportedSchema { version, schema_version } => {
      CliError::ConfigError(format!(
          "unsupported schema version '{}' (file reports {}; not compiled in). \
           hint: run `gramps-gen schema download {}` to add support",
          schema_version, version, schema_version
      ))
  }
  ```

  `DiffFailed` already exists as a variant and is used by the diff command —
  no new variant is needed. The improvement is purely in the error message text.

- `crates/cli/src/error.rs` — update the `UnsupportedSchema` match arm in the
  existing `From<gramps_reader::Error>` impl (around line 155).

- `crates/cli/src/commands/diff.rs` — no changes needed; it already maps errors
  through `CliError::DiffFailed` which will now receive the improved message.

### Step 5 — Fix adversarial tests for merged schema, then verify all tests

**Problem**: The adversarial test module is gated behind
`#[cfg(all(test, not(feature = "schema-5-1")))]` (line 839). With `schema-5-1`
now in the default feature set, these tests will **never run** by default.  This
is a significant coverage gap — the adversarial strategy tests, including
`double_gender`, `one_parent_families`, and others, would become completely
inactive.

**Fix — update the gate and test code**:

1. Change the gate from `#[cfg(all(test, not(feature = "schema-5-1")))]` to
   `#[cfg(test)]` so the tests always compile and run.

2. Update the `double_gender` function (lines 817–823) to use the
   version-abstracting helpers instead of raw `#[cfg]`-gated integer comparisons.
   Currently it uses `match crate::gender_cmp(&person.gender)` with `#[cfg]` arms
   for `Some(0)` / `Some(1)` vs `0` / `1`. Replace with a single path using the
   existing helpers:

   ```rust
   match crate::gender_cmp(&person.gender) {
       Some(0) => crate::set_gender(person, 1),
       Some(1) => crate::set_gender(person, 0),
       _ => {}  // Unknown(2), Other(3), or None unchanged
   }
   ```

   `crate::gender_cmp` already returns `Option<i32>`, and `set_gender` already
   handles both `i32` and `Option<i32>` via the existing `#[cfg]` helpers — the
   call sites don't need version-specific gating, only the match arms do.
   (If `gender_cmp` is not yet exposed, export it from `graph.rs` as a `pub`
   function.)

3. Review any other tests in the module that may use raw integer gender values
   and update them to use the helpers.

**Files**:

- `crates/typed-graph/src/generate/adversarial.rs` — update gate + match arms
- `crates/typed-graph/src/graph.rs` — ensure `gender_cmp` and `set_gender` are
  `pub` (they may already be — verify)

**Then run the verification**:

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

- `merged_schema.rs` tests (previously gated behind `#![cfg(feature = "schema-5-1")]`)
  will now run by default ✓
- Adversarial tests will now run by default instead of being silently skipped ✓
- Run a single-schema smoke check to ensure nothing regressed:
  `cargo test --no-default-features --features schema-5-2 -p typed-graph`

**Add new tests for the changed behavior**:

- **New test**: `gramps-reader/src/error.rs` — add a test verifying the updated
  `Display` format for `UnsupportedSchema` with both `version` and `schema_version`
  fields produces the expected output including the hint text.
- **New test**: `cli/src/error.rs` — add a test verifying the updated `From`
  conversion preserves both version fields in the `ConfigError` message.
- **No new test needed** for the build.rs change — the CI matrix will cover
  single-schema and dual-schema builds implicitly.

### Step 6 — Update documentation and check CI

**Files**:

- `AGENTS.md` — update workspace overview to note both schemas are compiled by default
- `docs/ARCHITECTURE.md` — update schema section to reflect auto-detection
- `README.md` — update build instructions (remove `--features` requirement)

**CI impact check**: Before committing, search the repository for any CI
configuration files (`.github/workflows/`, `Makefile`, `justfile`, etc.) that
reference `--features`, `--all-features`, or `all-schemas`. Verify that:

- `cargo build --all-features` no longer fails (previously broken by phantom
  5.0/6.0 features)
- `cargo test --all-features` passes with the updated feature set
- Any CI matrix that includes `--features schema-5-1` still works (though it's
  now redundant with the default)

## 5. Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Adversarial tests silently disabled by default | **Addressed** | Step 5 updates the gate from `not(feature = "schema-5-1")` to `test` and fixes the raw integer comparisons to use the version-abstracting helpers. A single-schema smoke check (`--no-default-features --features schema-5-2`) is part of the verification. |
| Tests that assumed single-schema break with both enabled | Low | `merged_schema.rs` already passes; the adversarial gate is now fixed (see above) |
| Binary size increase from two schemas | Negligible | Schema JSON files are ~150KB total; generated code size is dominated by the union types which already handle `Option<T>` |
| Compile time increase | Negligible | Merge algorithm is O(fields × versions) — insignificant for two versions |
| New warning noise from `--all-features` CI | Low | The `all-schemas` feature is updated to match reality; CI check added to Step 6 |
| User confusion about `schema-5-0` / `schema-6-0` features being removed | Very low | Nobody can build with them (no files exist), so nobody uses them |
| `FEATURE_VERSIONS` dead code in build.rs after feature removal | Low | Addressed in Step 1 — entries for 5.0 and 6.0 are removed from the array |

## 6. Acceptance Criteria

1. `cargo build` (no feature flags) compiles both schema 5.1 and 5.2 into the binary
2. `gramps-gen diff file_51.gramps file_52.gramps` works when files are different versions
3. `gramps-gen diff file_51.gramps file_51b.gramps` works for same-version 5.1 files
4. `gramps-gen generate --schema 5.1` and `--schema 5.2` both work without `--features`
5. `cargo test --workspace` passes (all tests, including adversarial, run with the new defaults)
6. `cargo clippy --all-targets --all-features -- -D warnings` passes
7. `Schema::available_versions()` returns `["5.1", "5.2"]`
8. **Manual smoke test**: Build with `cargo build --no-default-features --features schema-5-2`,
   then run `gramps-gen diff` against a 5.1 file. Verify the error message shows both the
   truncated version (`"5.1"`) and the full patch version (`"5.1.6"`), plus the hint
   suggesting `gramps-gen schema download 5.1`.
9. `cargo test --no-default-features --features schema-5-2 -p typed-graph` passes
   (single-schema build still works and the adversarial tests run there too)

## 7. Out of Scope

- Downloading missing schema files automatically at runtime (requires network + Python; use `schema download` command instead)
- Supporting Gramps 5.0 or 6.0 (no schema files exist; add features later when files are available)
- Changing the `diff` tool to handle cross-version semantic differences (it already compares the merged type representations)
