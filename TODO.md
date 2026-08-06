# Implementation Plan: Auto-Detect Gramps Schema Version

Source: `docs/research/auto-detect-schema-version.md`

## Overview

Make gramps-gen tools transparently support all installed Gramps schema versions
without requiring `--features` flags. The runtime auto-detection and multi-schema
merge infrastructure already exists — only the default feature set, build.rs
robustness, error messages, and test gates need updating.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: change default features to include both schema-5-1 and schema-5-2` | Feature defaults | `crates/typed-graph/Cargo.toml`, `crates/typed-graph/build.rs` | — |
| 2 | `feat: graceful build.rs for missing schema files with GRAMPS_STRICT_SCHEMA` | Build-time resilience | `crates/typed-graph/build.rs` | — |
| 3 | `feat: improve UnsupportedSchema error with schema_version field and hint` | Error message | `crates/gramps-reader/src/error.rs`, `crates/gramps-reader/src/xml/header.rs` | Unit tests for new Display format |
| 4 | `feat: improve CLI error conversion for UnsupportedSchema` | CLI error conversion | `crates/cli/src/error.rs` | Unit test for From conversion |
| 5 | `fix: update adversarial tests for merged schema with version-abstracting helpers` | Test gate + gender helpers | `crates/typed-graph/src/generate/adversarial.rs` | Unit (existing tests now run — verify they compile and pass in both merged and single-schema modes) |
| 6 | `docs: update AGENTS.md, ARCHITECTURE.md, README.md for dual-schema defaults` | Documentation | `AGENTS.md`, `docs/ARCHITECTURE.md`, `README.md` | — |

## Step Details

### Step 1 — Update feature defaults and remove phantom features

**Deliverables**:

`crates/typed-graph/Cargo.toml`:

- Change `default = ["schema-5-2"]` → `default = ["schema-5-1", "schema-5-2"]`
- Remove `schema-5-0` and `schema-6-0` feature entries (no schema files exist)
- Update `all-schemas` to reflect the new set: `["schema-5-1", "schema-5-2"]`
- Add a comment: `# Both schemas compiled by default for transparent auto-detection`

`crates/typed-graph/build.rs`:

- Remove `("schema-5-0", "5.0")` and `("schema-6-0", "6.0")` entries from `FEATURE_VERSIONS` static array (lines 36–40)

**Tests**: — (verified by all downstream tests compiling with new defaults)

**Note**: After this step, `cargo build` will compile both schemas. `cargo build --all-features` will no longer fail (previously broken by phantom 5.0/6.0 features). The `merged_schema.rs` integration tests (gated behind `#![cfg(feature = "schema-5-1")]`) will now run by default.

---

### Step 2 — Graceful build.rs for missing schema files

**File**: `crates/typed-graph/build.rs`

Replace the hard `process::exit(1)` in `load_schemas()` with:

- Read `GRAMPS_STRICT_SCHEMA` env var once at the top of `load_schemas()`:

  ```rust
  let strict_mode = env::var("GRAMPS_STRICT_SCHEMA").is_ok();
  ```

- In the schema-exists check, replace the `eprintln!` / `process::exit(1)` block with:

  ```rust
  if !schema_path.exists() {
      if strict_mode {
          eprintln!("error: {} not found.", schema_path.display());
          std::process::exit(1);
      }
      println!("cargo::warning=schema-{}.json not found; skipping schema {}", version, version);
      continue;
  }
  ```

- Track `let mut loaded_count: usize = 0;` and increment it after each successful load.
- After the loop, if `loaded_count == 0`, emit a fatal error:

  ```rust
  eprintln!("error: no schema files found. Run `gramps-gen schema download <version>` or place schema files in schemas/.");
  std::process::exit(1);
  ```

**Tests**: — (CI matrix will cover single-schema and dual-schema builds implicitly)

---

### Step 3 — Improve UnsupportedSchema error

**Files**:

`crates/gramps-reader/src/error.rs`:

- Add `schema_version` field to the `UnsupportedSchema` variant:

  ```rust
  UnsupportedSchema {
      /// The full version string found in the XML header (e.g. "5.1.6").
      version: String,
      /// The truncated major.minor version (e.g. "5.1") that was checked.
      schema_version: String,
  }
  ```

- Update the `Display` impl:

  ```rust
  Error::UnsupportedSchema { version, schema_version } => {
      write!(
          f,
          "unsupported schema version '{}' (file reports {}; not compiled in). \
           hint: run `gramps-gen schema download {}` to download the schema, \
           or build with --no-default-features --features schema-5-2 to use only 5.2",
          schema_version, version, schema_version
      )
  }
  ```

- Update the existing test `unsupported_schema_display` to include the new field.
- Add a new test `unsupported_schema_display_with_hint` verifying the full message format.

`crates/gramps-reader/src/xml/header.rs`:

- In `detect_schema_version()`, update the error path to pass both `schema_version` (truncated) and `v` (full):

  ```rust
  Err(Error::UnsupportedSchema {
      version: v.clone(),
      schema_version,
  })
  ```

---

### Step 4 — Improve CLI error conversion

**File**: `crates/cli/src/error.rs`

- Update the `From<gramps_reader::Error>` impl's `UnsupportedSchema` arm:

  ```rust
  gramps_reader::Error::UnsupportedSchema { version, schema_version } => {
      CliError::ConfigError(format!(
          "unsupported schema version '{}' (file reports {}; not compiled in). \
           hint: run `gramps-gen schema download {}` to add support",
          schema_version, version, schema_version
      ))
  }
  ```

- Add a test verifying the conversion preserves both version fields in the `ConfigError` message:

  ```rust
  #[test]
  fn cli_error_from_gramps_reader_unsupported_schema() {
      let reader_err = gramps_reader::Error::UnsupportedSchema {
          version: "5.1.6".to_string(),
          schema_version: "5.1".to_string(),
      };
      let cli_err: CliError = reader_err.into();
      match &cli_err {
          CliError::ConfigError(msg) => {
              assert!(msg.contains("5.1"), "should contain schema_version");
              assert!(msg.contains("5.1.6"), "should contain full version");
              assert!(msg.contains("hint:"), "should contain hint");
          }
          other => panic!("Expected ConfigError, got: {:?}", other),
      }
      let display = format!("{}", cli_err);
      assert!(display.contains("5.1"));
      assert!(display.contains("5.1.6"));
  }
  ```

---

### Step 5 — Fix adversarial tests for merged schema

**Problem**: The adversarial test module is gated with `#[cfg(all(test, not(feature = "schema-5-1")))]` (line 839). With `schema-5-1` now in the default feature set, these tests never run. Additionally, the test code uses raw integer comparisons (`p.gender == 0`) and initializations (`gender: 0`) that won't compile when `gender` is `Option<i32>` in the merged schema.

**Fix**:

**5a** — Change the gate from `#[cfg(all(test, not(feature = "schema-5-1")))]` to `#[cfg(test)]`.

**5b** — Update the `double_gender` function's match arms (lines 815–825) to use the version-abstracting helpers. Replace the `#[cfg]` gated match with a uniform expression:

```rust
match crate::gender_value(crate::gender_cmp(&person.gender)) {
    0 => crate::set_gender(person, 1),
    1 => crate::set_gender(person, 0),
    _ => {}
}
```

`gender_value(gender_cmp(...))` returns `i32` in all feature configurations:

- With `schema-5-1`: `gender_cmp` returns `Option<i32>` → `gender_value` unwraps to `i32` ✓
- Without `schema-5-1`: `gender_cmp` returns `i32` → `gender_value` is identity → `i32` ✓

**5c** — Add a local helper function at the top of the test module for convenient gender access:

```rust
/// Normalize gender to i32 for test assertions across schema versions.
fn gender_val(p: &crate::PersonData) -> i32 {
    crate::gender_value(crate::gender_cmp(&p.gender))
}
```

**5d** — Update all test code that directly accesses or compares `p.gender`. Replace every:

- `p.gender == N` → `gender_val(p) == N`
- `p.gender != N` → `gender_val(p) != N`
- `assert_eq!(p.gender, N, ...)` → `assert_eq!(gender_val(p), N, ...)`
- `gender: N` in struct initializers → `gender: crate::into_gender_field(N)`
- `gender: *g` → `gender: crate::into_gender_field(*g)`

Target locations (from grep):

- Line 1267: `gender: 0` in `disconnected_subgraphs_single_person`
- Lines 1977, 1981, 1990, 1994: `p.gender == 0|1` in `double_gender_swaps_some_genders`
- Line 2022: `p.gender == 0 || p.gender == 1` in `double_gender_partial_fraction`
- Lines 2039, 2043: `p.gender == 0|1` in `double_gender_partial_fraction`
- Lines 2059, 2064: `gender: 2|3` in `double_gender_keeps_unknown_other`
- Lines 2081, 2083: `p.gender == 2|3` in `double_gender_keeps_unknown_other`
- Line 2109: `gender: 0` in `double_gender_fraction_zero`
- Line 2120: `p.gender == 0` in `double_gender_fraction_zero`
- Lines 2251, 2255: `p.gender == 0|1` in `apply_adversarial_strategies_multiple_transforms`
- `build_two_family_graph()`: `gender: *g` (multiple occurrences in test helper)
- `build_graph_with_persons()`: no direct gender — uses `generate_random` which produces correct types

**Verification**:

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
cargo test --no-default-features --features schema-5-2 -p typed-graph  # single-schema smoke check
```

The `merged_schema.rs` tests (previously gated behind `#![cfg(feature = "schema-5-1")]`) will now run by default ✓. Adversarial tests will run by default ✓.

---

### Step 6 — Update documentation

**Files**:

`AGENTS.md`:

- Update the workspace tree note / feature section to state that both schemas are compiled by default.

`docs/ARCHITECTURE.md`:

- Update the schema section to reflect auto-detection: mention that all schema files found in `schemas/` are compiled in by default.

`README.md`:

- Update build instructions to remove the `--features` requirement for 5.1 support.
- Note that `cargo build` now includes both 5.1 and 5.2.

**CI check**: No `.github/workflows/` directory found — no CI config to update.
