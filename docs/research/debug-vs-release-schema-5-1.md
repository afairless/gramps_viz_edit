# Fix Plan: Schema 5.1 Not Available in Release Binary

## Investigation Summary

The user ran these commands and got a confusing result:

```bash
# Step 1: Run release binary — schema 5.1 not available
$ target/release/gramps-gen generate --schema-version 5.1 --count 8 --seed 2026 --depth 4
Error: "schema version 5.1 is not available in this build.
  Available versions: 5.2
  Rebuild with: cargo build --features schema-5-1"

# Step 2: Rebuild with feature (as instructed by error message)
$ cargo build --features schema-5-1
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.65s

# Step 3: Run release binary again — STILL fails!
$ target/release/gramps-gen generate --schema-version 5.1 --count 8 --seed 2026 --depth 4
Error: "schema version 5.1 is not available in this build.
  Available versions: 5.2
  Rebuild with: cargo build --features schema-5-1"
```

The debug binary works correctly:

```bash
$ target/debug/gramps-gen generate --schema-version 5.1 --count 8 --seed 2026 --depth 4
Generated 8 persons, 4 families, 14 events → /tmp/test_51.gramps
```

---

## Root Cause

**The user is running the release binary (`target/release/gramps-gen`) but only rebuilt the debug binary (`cargo build` → `target/debug/gramps-gen`).** `cargo build` without `--release` builds the dev (debug) profile. The release binary was never rebuilt with the feature flag and still only has schema-5-2 compiled in.

The correct rebuild command is:

```bash
cargo build --release --features schema-5-1
```

### Why the error message is misleading

The error message in `crates/cli/src/commands/generate.rs` says:

```
Rebuild with: cargo build --features schema-5-1
```

This assumes a debug build. When the binary is compiled in release mode (`cfg!(not(debug_assertions))`), it should say:

```
Rebuild with: cargo build --release --features schema-5-1
```

### Why the `schema list` command is misleading

The `schema list` command checks whether schema **files** exist on disk under `schemas/`, not whether schema versions are **compiled into the binary**. The user sees "✓ 5.1" and assumes it's usable, but this only means the file `schemas/schema-5.1.json` is present — not that the feature `schema-5-1` was active during compilation.

---

## Q1: Why Did the Test Suite Not Detect This Failure?

Three independent reasons:

### 1. E2E tests use the debug binary, never the release binary

The E2E tests in `crates/cli/tests/e2e.rs` use:

```rust
let bin = env!("CARGO_BIN_EXE_gramps-gen");
```

This resolves to `target/debug/gramps-gen` when running `cargo test`. The release binary is never exercised. There is no CI job or test script that builds `--release --features schema-5-1` and then runs the binary.

### 2. `e2e_generate_with_schema_version_51` is lenient — it silently skips

```rust
#[test]
fn e2e_generate_with_schema_version_51() {
    // ...
    if code != Some(0) {
        // If 5.1 is not available, the command should fail with a clear error.
        assert!(stderr.contains("not available"), "Unexpected error: {}", stderr);
        eprintln!("Skipping: schema-5-1 not compiled in this build");
        return;  // ← silently passes!
    }
    // ...
}
```

This design was intentional — the test file can't `cfg!`-gate individual tests by cargo feature (the test binary itself is compiled once with the features the user requests). But it means that when the test DOES have schema-5-1 compiled in, it exercises it; when it DOESN'T, it silently passes. There's no way for this test to fail with a "schema-5-1 was supposed to be present but isn't" error.

When running `cargo test --features schema-5-1`, the E2E binary HAS the feature compiled in, so the test exercises the success path and passes. The test is correct in this case.

### 3. CFG-gated integration tests are silently absent when feature is missing

`crates/typed-graph/tests/merged_schema.rs` starts with:

```rust
#![cfg(feature = "schema-5-1")]
```

If `cargo test` is run without `--features schema-5-1`, this entire file is silently not compiled. There is no test that says "I expect schema-5-1 to be available, fail if it's not."

### Why CI would not have caught this

A CI pipeline that runs `cargo test --features schema-5-1` would pass all tests — the debug binary has schema-5-1 compiled in. The problem only manifests when a human builds the debug binary (`cargo build --features schema-5-1`) but executes the release binary. No CI job tests the release binary with optional features.

---

## Q2: How to Fix It

Three changes, each independently useful:

### Fix 1: Correct the rebuild suggestion based on build profile

**File**: `crates/cli/src/commands/generate.rs`

Use `cfg!(debug_assertions)` to detect the build profile and include `--release` in the suggestion when the binary is a release build.

```rust
let build_cmd = if cfg!(debug_assertions) {
    "cargo build"
} else {
    "cargo build --release"
};

return Err(crate::error::CliError::ConfigError(format!(
    "schema version {} is not available in this build.\n  Available versions: {}\n  Rebuild with: {} --features schema-{}",
    args.schema_version,
    Schema::available_versions().join(", "),
    build_cmd,
    args.schema_version.replace('.', "-")
)));
```

**Test**: Add a unit test for the error message format that verifies the correct `cargo build` / `cargo build --release` string is used based on `cfg!(debug_assertions)`.

### Fix 2: Show compiled-in schema versions in `schema list`

**File**: `crates/cli/src/commands/schema.rs`

The `schema list` command currently shows which schema files exist on disk but not which versions are compiled into the binary. Add a section that calls `Schema::available_versions()` so users can see the mismatch.

```
Local schemas (on disk):
  ✓ 5.1 (latest: v5.1.6)
  ✓ 5.2 (latest: v5.2.4)

Compiled into this binary:
  ✓ 5.2
  ✗ 5.1 — file exists but not compiled in. Rebuild with: cargo build --release --features schema-5-1
```

This directly surfaces the disconnect that the user can see the file but can't use it.

**Implementation**: In `cmd_list()`, call `Schema::available_versions()` and cross-reference with the local file list. Mark files that exist but aren't compiled in.

**Test**: Add unit tests that verify the output format for each case (compiled in and present, file present but not compiled, neither).

### Fix 3: Add a runtime assertion that the expected feature is actually available

**File**: `crates/typed-graph/tests/merged_schema.rs` (modify) or new test

Add a test that, when the `schema-5-1` feature is active (via `cfg!`), unconditionally asserts the schema is available. This is distinct from the existing `#![cfg(feature = "schema-5-1")]` file-level gate — the cfg gate prevents the test from compiling at all when the feature is off. A `cfg!`-based check would compile the test in all configurations and only assert when the feature is expected to be on.

```rust
/// This test compiles regardless of features but asserts at runtime
/// that schema-5-1 is available when the feature is active.
#[test]
fn schema_51_feature_implies_availability() {
    if cfg!(feature = "schema-5-1") {
        assert!(
            Schema::for_version("5.1").is_some(),
            "schema-5-1 feature is active but Schema::for_version(\"5.1\") returned None. \
             This indicates a build.rs code generation bug."
        );
    }
}
```

This code compiles even without the feature, but only asserts when the feature flag is set. If the build.rs conversion or merge silently drops schema-5-1, this test catches it.

**Test**: Verify the test passes with `cargo test --features schema-5-1` and also passes (as a no-op) with `cargo test` (no features).

---

## Files Affected

| File | Change |
|---|---|
| `crates/cli/src/commands/generate.rs` | Fix 1: `cfg!(debug_assertions)` in error message |
| `crates/cli/src/commands/schema.rs` | Fix 2: Show compiled-in vs on-disk schemas |
| `crates/typed-graph/tests/merged_schema.rs` | Fix 3: Runtime assertion test |
| `crates/typed-graph/src/lib.rs` | No change (re-exports already include Schema) |

---

## Step-by-Step Implementation Plan

### Step 1: Fix the rebuild suggestion (Fix 1)

In `generate.rs`, compute the correct build command based on `cfg!(debug_assertions)`.

- [x] Design: change above
- [ ] Implement: modify `generate.rs` error construction
- [ ] Test: verify error message format in both debug and release profiles

### Step 2: Fix `schema list` to show compiled-in versions (Fix 2)

In `schema.rs`, add a section after the local file listing that calls `Schema::available_versions()`.

- [ ] Implement: modify `cmd_list()` to show compiled-in versions
- [ ] Test: add unit tests covering all combinations:
  - File present, compiled in (shows "✓" with no warning)
  - File present, not compiled in (shows "✗" with rebuild hint)
  - File not present, compiled in (should not happen, but show it)
  - File not present, not compiled in (current "— not downloaded" behavior)

### Step 3: Add runtime assertion (Fix 3)

In `merged_schema.rs`, add a `cfg!`-based runtime assertion.

- [ ] Implement: add `schema_51_feature_implies_availability` test
- [ ] Test: run with both `cargo test` and `cargo test --features schema-5-1`

### Step 4: Verify all configurations

```bash
# Default (5.2 only) — all tests pass
cargo test --workspace

# 5.1 + 5.2 — all tests pass
cargo test --workspace --features schema-5-1

# Release build with 5.1 — schema list shows compiled-in info
cargo build --release --features schema-5-1
target/release/gramps-gen schema list
target/release/gramps-gen generate --schema-version 5.1 --count 5

# Clippy
cargo clippy --all-targets --all-features -- -D warnings
```

### Step 5: Consider CI improvements (optional, separate document)

- Add a CI job: `cargo build --release --features schema-5-1 && target/release/gramps-gen schema list | grep "5.1.*compiled"`
- Add a CI job: `cargo build --release --features schema-5-1 && target/release/gramps-gen generate --schema-version 5.1 --count 5 --output /dev/null`

---

## Risk Assessment

- **All low risk**: These are display/UX changes and a single test addition. No changes to generation, validation, or serialization logic.
- **Fix 2 risk**: The `schema list` command currently doesn't link against `typed_graph::Schema`. The `schema` module will need to `use typed_graph::Schema;` to call `Schema::available_versions()`. This adds a dependency on `typed-graph` that already exists (cli → typed-graph).
- **Backward compatibility**: No breaking changes. Existing behavior is preserved; new information is additive.
