# Fix Schema Version Wiring — Implementation Plan

## 1. Overview

**Problem**: The `--schema-version` CLI flag only controls the XML `<created version="...">` header, not the schema used for generation and validation. When a user runs `generate` without `--schema-version`, the output always uses the highest compiled-in schema (e.g., 5.2), even if the user expects 5.1. The `Schema::new()` convenience method hardcodes the default version, and the CLI pipeline calls it unconditionally for generation and validation, ignoring the user's version preference.

**Root cause**: In `crates/cli/src/commands/generate.rs`, the schema is resolved in two separate places:

| Location | What it does | Version used |
|---|---|---|
| Line 101: `let schema = Schema::new()` | Generation + validation (Stages 1–4) | Always default (highest) |
| Lines 127–141: `let schema_version = ...` | XML header (Stage 5) | Respects `--schema-version` |

These are decoupled. The schema used for generation/validation is always `Schema::new()` (hardcoded default), while only the serialization step respects the flag.

**Secondary issue**: `Schema::new()` is a convenience alias that obscures which version is being used. With ~50 call sites across tests and production code, it silently pins behavior to whatever the build's default version is.

**Scope**: This plan covers:

1. Restoring `schema-5.2.json` to the working tree (it was deleted but exists in git)
2. Wiring schema version resolution through the full CLI pipeline
3. Deprecating `Schema::new()` and migrating all call sites
4. Making `generate_random()` actually use the schema parameter
5. Making tests version-agnostic

---

## 2. Design Decisions

| Decision | Rationale |
|---|---|
| Deprecate `Schema::new()` — replace with `Schema::for_version()` | Makes version selection explicit at every call site. No more silent default-pinning. |
| Keep `Schema::for_version()` returning `Option<&'static Schema>` | Already designed. Returns a reference to a static `LazyLock<Schema>` instance. Borrow is fine for pipeline use; callers that need ownership call `.clone()`. |
| Restore `schema-5.2.json` via `git checkout` | The file is tracked in git at HEAD. No feature changes needed. |
| Fix `_schema` parameter in `generate_random` | The generator should use `schema.valid_enum_values` when assigning enum-typed fields, so the generated data is version-appropriate. |
| Make tests version-agnostic | Replace `assert_eq!(schema.version, "5.2")` with `assert_eq!(schema.version, Schema::default_version())`. |

---

## 3. Implementation Phases

### Phase A: Restore schema-5.2.json (1 commit)

**Status**: `schema-5.2.json` exists in git HEAD but is deleted from the working tree:

```
$ git status -- schemas/
    deleted:    schemas/schema-5.2.json
```

**Action**: Run `git checkout schemas/schema-5.2.json` to restore it.

**Verification**: `cargo build` succeeds with default features.

**Files changed**:

- `schemas/schema-5.2.json` (restored from git)

---

### Phase B: Wire schema version resolution through the CLI pipeline (1 commit)

> **Note — `--schema-version` + `--config` interaction**: When both `--schema-version` and `--config <scenario.yaml>` are passed, the CLI `--schema-version` flag takes precedence (it's explicitly set by the user). Scenario YAML files do not currently support a `schema_version` field; this is deferred to a follow-up change. If neither is specified, the compiled-in default version is used.

**Current code** (`crates/cli/src/commands/generate.rs`):

```rust
// Stage 1: Generate
let schema = Schema::new();                                    // ← always default
let mut result = generate_random(&config, &adversarial_config, &schema)?;

// ... Stage 2–4 ...

// Stage 5: Serialize
let schema_version = if args.schema_version == "default" {     // ← respects flag
    Schema::default_version().to_string()
} else {
    args.schema_version.clone()
};
let writer = GraphXmlWriter::new(map, &schema_version);
```

**Fix**: Resolve the schema version once, early, and use it for everything:

```rust
// Stage 0: Resolve schema version
let schema_version = if args.schema_version == "default" {
    Schema::default_version().to_string()
} else {
    // Validate that the requested version is available
    if Schema::for_version(&args.schema_version).is_none() {
        return Err(CliError::ConfigError(format!(
            "schema version {} is not available in this build.\n  Available versions: {}\n  Rebuild with: cargo build --features schema-{}",
            args.schema_version,
            Schema::available_versions().join(", "),
            args.schema_version.replace('.', "-")
        )));
    }
    args.schema_version.clone()
};
let schema = Schema::for_version(&schema_version)
    .expect("schema version was validated above");

// Stage 1: Generate
let mut result = generate_random(&config, &adversarial_config, schema)?;

// Stage 2: Validation Gate 1
let errors = result.graph.validate(schema);

// Stage 4: Validation Gate 2
let errors = result.graph.validate(schema);

// Stage 5: Serialize
let writer = GraphXmlWriter::new(map, &schema_version);
```

**Files changed**:

- `crates/cli/src/commands/generate.rs`: Move version resolution to Stage 0; use `Schema::for_version()` instead of `Schema::new()`.

**Verification**:

- `cargo test -p cli` passes
- `target/release/gramps-gen generate --count 1 --seed 2026 --schema-version 5.1` writes `version="5.1"` in header
- `target/release/gramps-gen generate --count 1 --seed 2026` writes `version="5.2"` in header (default)

> **⚠️ Behavioral completeness note**: After Phase B, the _wiring_ is correct — `Schema::for_version()` passes the right schema to `generate_random()` and `validate()`. However, `generate_random()` still ignores the schema parameter (see Phase E). Generation output won't differ between versions until Phase E is complete. The verification above only confirms the XML header reflects `--schema-version`; behavioral verification is deferred to Phase E.

---

### Phase C: Deprecate Schema::new() and migrate all call sites (~3 commits)

#### C.1: Add deprecation annotation to generated code

**File**: `crates/typed-graph/build.rs` — `generate_schema_metadata()` function.

Change:

```rust
code.push_str("    pub fn new() -> Self {\n");
```

To:

```rust
code.push_str("    #[deprecated = \"Use Schema::for_version(version) or Schema::for_version(Schema::default_version()) instead\"]\n");
code.push_str("    pub fn new() -> Self {\n");
```

Also change `Default` impl to not call deprecated method:

```rust
// Before:
code.push_str("impl Default for Schema {\n");
code.push_str("    fn default() -> Self {\n");
code.push_str("        Self::new()\n");
code.push_str("    }\n");
code.push_str("}\n\n");

// After:
code.push_str("impl Default for Schema {\n");
code.push_str("    fn default() -> Self {\n");
code.push_str("        Self::for_version(Self::default_version())\n");
code.push_str("            .expect(\"default version should always be available\")\n");
code.push_str("            .clone()\n");
code.push_str("    }\n");
code.push_str("}\n\n");
```

> **Note — `Schema::default()` allocates**: After this change, `Schema::default()` calls `.clone()` on the static `LazyLock<Schema>`, allocating new `HashMap`s. This is fine for tests and one-off CLI use (called once per pipeline run). Callers that only need a reference should use `Schema::for_version(version)` instead to borrow the static instance without cloning.

**Verification**: `cargo build` produces deprecation warnings for all `Schema::new()` call sites.

#### C.2: Migrate typed-graph crate call sites

All `Schema::new()` → `Schema::for_version(Schema::default_version()).unwrap().clone()`. But to keep tests concise, wrap in a local helper or use `Schema::default()` (which calls `for_version` directly after the fix above).

Actually, since we're fixing `Default::default()` to not call the deprecated `new()`, we can use `Schema::default()` everywhere:

| File | # Call sites | Migration |
|---|---|---|
| `crates/typed-graph/src/lib.rs` | ~5 tests | `Schema::new()` → `Schema::default()` |
| `crates/typed-graph/src/graph.rs` | 2 tests | `Schema::new()` → `Schema::default()` |
| `crates/typed-graph/src/validate.rs` | 14 tests | `Schema::new()` → `Schema::default()` |
| `crates/typed-graph/src/generate/random.rs` | ~30 tests | `Schema::new()` → `Schema::default()` |
| `crates/typed-graph/src/generate/adversarial.rs` | 11 tests | `Schema::new()` → `Schema::default()` |
| `crates/typed-graph/src/generate/builder.rs` | 1 test | `Schema::new()` → `Schema::default()` |

**Pattern**:

```rust
// Before:
let schema = Schema::new();

// After:
let schema = Schema::default();
```

**Verification**: `cargo test -p typed-graph --lib` passes with no deprecation warnings.

After all typed-graph call sites are migrated, add `#![deny(deprecated)]` to `crates/typed-graph/src/lib.rs` to prevent future regressions. This gates CI against any new code using the deprecated `Schema::new()`.

#### C.3: Migrate CLI crate call sites

| File | # Call sites | Migration |
|---|---|---|
| `crates/cli/src/commands/generate.rs` (production) | 1 call (line ~101) | Already fixed in Phase B |
| `crates/cli/src/commands/generate.rs` (tests) | 1 call (line ~428) | `Schema::new()` → `Schema::default()` |
| `crates/cli/tests/integration.rs` | 5 tests | `Schema::new()` → `Schema::default()` |
| `crates/cli/tests/e2e.rs` | 0 direct uses | Version checks kept as-is (see Phase F.2) |

**Verification**: `cargo test -p cli` passes with no deprecation warnings.

After migrating CLI call sites, add `#![deny(deprecated)]` to `crates/cli/src/lib.rs` as well.

> **Note on transient warnings**: Between commit C.1 (adding `#[deprecated]`) and the completion of C.2 + C.3 (migrating all call sites), `cargo build` will emit ~70 deprecation warnings. This is expected and temporary — commits C.2 and C.3 should be applied immediately after C.1 to suppress them.

---

### Phase D: Verify schema 5.1 vs 5.2 compatibility (1 commit)

> **Why this phase runs before generator changes**: Phase E (making `generate_random()` schema-aware) depends on knowing the actual differences between schema versions — which enum values differ, which fields are version-specific. Running this comparison first eliminates guesswork and prevents rework.

**Output**: `docs/research/schema-5.1-vs-5.2.md` — a concrete diff report listing every enum value difference, field difference, and type-level change between the two versions.

Compare the two schema files to produce the diff report:

```bash
# Full structural diff of the two schemas (no truncation)
diff <(jq -S . schemas/schema-5.1.json) <(jq -S . schemas/schema-5.2.json)
```

Checklist (document findings in the output file):

- [ ] All 10 primary types exist in both versions
- [ ] All secondary types exist in both versions
- [ ] No field type conflicts between versions
- [ ] Enum value differences enumerated (with explicit lists: which values are 5.1-only, 5.2-only, shared)
- [ ] Fields that exist in 5.2 but not 5.1 are listed (informs Phase E.2)
- [ ] XML element/attribute names are identical (SerializationMap compatibility)

If differences are found that require code changes beyond this plan's scope, document them as follow-up work. The findings in `docs/research/schema-5.1-vs-5.2.md` directly inform Phase E (enum value filtering) and Phase E.2 (field availability gating).

Also: run a cross-version validation smoke test — generate with 5.1 schema, validate with both 5.1 and 5.2 schemas, and document any structural validation differences in the output file.

---

### Phase E: Make generate_random() respect the schema (1–2 commits)

Currently, `generate_random` accepts `_schema: &Schema` but never reads it. The generator assigns enum values (EventType, Gender, etc.) without consulting the schema's `valid_enum_values`, so it could theoretically assign a 5.2-only enum variant when targeting 5.1.

#### E.1: Gate enum value assignment on schema version

The generator should check `schema.valid_enum_values` before assigning any enum-typed value. This applies to:

| Enum type | Where assigned |
|---|---|
| `EventType` | `generate_random` — event creation (Birth, Death, Marriage, …) |
| `EventRoleType` | `generate_random` — event ref metadata |
| `Gender` | `generate_random` — person gender |
| `ChildRefType` | `generate_random` — child relation metadata |
| `NameType` | `generate_random` — alternate name types |

**Implementation approach**: Add a helper that filters enum values to the version-specific set:

```rust
fn schema_enum_values(schema: &Schema, enum_name: &str) -> Vec<String> {
    schema.valid_enum_values
        .get(enum_name)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|s| s.to_string())
        .collect()
}
```

Then, wherever an enum value is randomly picked, filter the candidate list against the schema:

```rust
// Before:
let event_type = rng.gen_range(0..EVENT_TYPES.len());
let event_type = EVENT_TYPES[event_type];

// After:
let allowed = schema_enum_values(schema, "EventType");
let event_type = allowed[rng.gen_range(0..allowed.len())];
```

**Trade-off**: This adds schema-awareness to the generator. For single-schema builds (the default), there's no behavior change — all enum values are valid. For multi-schema builds targeting an older version, the generator now correctly excludes newer-version variants.

#### E.2: Gate field availability (informed by Phase D findings)

If a field exists only in newer schema versions, the generator should skip it when targeting an older version. For example, if a field was added in 5.2, the generator shouldn't populate it when targeting 5.1.

Currently, this is unlikely to cause issues because the union-type merge makes all fields optional when any version doesn't define them, and the serializer skips `None`/`Vec::empty()` fields. But for correctness, the generator should check `field_availability`.

Add a private helper in `crates/typed-graph/src/generate/random.rs`:

```rust
/// Check whether a field exists for the given schema version.
/// Uses the `field_availability` map generated at compile time.
fn field_exists_for_version(
    schema: &crate::Schema,
    type_name: &str,
    field_name: &str,
    version: &str,
) -> bool {
    let key = format!("{}.{}", type_name, field_name);
    schema
        .field_availability
        .get(key.as_str())
        .map(|versions| versions.contains(&version))
        .unwrap_or(false)
}
```

**Decision**: The Phase D output (`docs/research/schema-5.1-vs-5.2.md`) determines whether this is needed. If Phase D finds fields that exist in 5.2 but not 5.1, implement field-availability gating in this phase using `schema.field_availability`. If no such fields exist, this phase is a no-op and can be skipped (still worth the commit for the helper function — it's a 5-line addition that future-proofs the codebase).

**Verification**:

- Existing tests pass (behavior unchanged for single-schema builds)
- New test: generate with 5.1 schema, verify no 5.2-only enum values appear
- New test: generate with 5.2 schema, verify 5.2-only enum values can appear

---

### Phase F: Make tests version-agnostic (1 commit)

Replace hardcoded `"5.2"` in version assertions with `Schema::default_version()`:

#### F.1: typed-graph tests

**File**: `crates/typed-graph/src/lib.rs`

```rust
// Before:
assert_eq!(schema.version, "5.2");
assert_eq!(Schema::default_version(), "5.2");
assert!(versions.contains(&"5.2"), "5.2 should be available");
let schema_52 = Schema::for_version("5.2").expect("5.2 should be available");

// After:
assert_eq!(schema.version, Schema::default_version());
assert!(versions.contains(&Schema::default_version()));
let default_ver = Schema::default_version();
let schema_default = Schema::for_version(default_ver)
    .expect("default version should be available");
```

**File**: `crates/typed-graph/src/lib.rs` — `test_schema_new_and_versioned_api_consistent`

This test verifies `Schema::new()` == `for_version(default_version())`. Since we're deprecating `Schema::new()`, either:

- Remove this test (it tests deprecated behavior)
- Or keep it with `#[allow(deprecated)]` until the method is fully removed

Keep it with `#[allow(deprecated)]` so CI still verifies backward compat.

#### F.2: E2E tests

**File**: `crates/cli/tests/e2e.rs`

For E2E tests (subprocess-based), the binary is always compiled with default features (`schema-5-2`).

**Decision**: Keep E2E version checks hardcoded for 5.2. E2E tests run against the compiled binary; since the default feature set always includes `schema-5-2`, checking for `"5.2"` is correct for CI. Add a comment on each assertion noting the coupling to the default feature set.

```rust
// Before:
assert!(combined.contains("5.2"), "Should mention version 5.2");
assert!(content.contains(r#"version="5.2""#), "XML should have version 5.2");

// After — keep the same assertions, annotate with a clarifying comment:
// Note: this assertion assumes default features (schema-5-2).
// Single-version builds (e.g. --features schema-5-1) need separate test jobs.
assert!(combined.contains("5.2"), "Should mention version 5.2");
assert!(content.contains(r#"version="5.2""#), "XML should have version 5.2");
```

Alternative feature builds (e.g., `--no-default-features --features schema-5-1`) need separate CI test jobs — see the feature matrix in Section 5.

---

### Phase G: Integration test for version-aware pipeline (1 commit)

Add a new integration test that verifies the full version-aware behavior:

```rust
#[test]
fn generate_respects_schema_version() {
    // Generate with 5.1 schema
    let schema_51 = Schema::for_version("5.1")
        .expect("5.1 should be available")
        .clone();
    let config = RandomConfig { ... seed: Some(42), ... };
    let result = generate_random(&config, &AdversarialConfig::default(), &schema_51).unwrap();

    // Validate with 5.1
    let errors = result.graph.validate(&schema_51);
    let structural: Vec<_> = errors.iter()
        .filter(|e| !matches!(e, ValidationError::PlausibilityWarning { .. }))
        .collect();
    assert!(structural.is_empty(), "{:?}", structural);

    // Serialize — verify version in header
    let map = SerializationMap::new();
    let mut buf = Vec::new();
    let writer = GraphXmlWriter::new(map, schema_51.version);
    writer.write(&result.graph, &mut buf).unwrap();
    let xml = String::from_utf8(buf).unwrap();
    assert!(xml.contains(r#"version="5.1""#));
}
```

And a corresponding E2E test:

```rust
#[test]
fn e2e_generate_with_schema_version_51() {
    let output = temp_output_path("schema_51");
    let (_stdout, stderr, code) = gramps_gen(&[
        "generate",
        "--count", "5",
        "--seed", "42",
        "--schema-version", "5.1",
        "--output", &output,
    ]);
    assert_eq!(code, Some(0), "Generate with 5.1 failed: {}", stderr);

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains(r#"version="5.1""#), "XML should have version 5.1");
}
```

#### Cross-version validation test

Add a test verifying that a graph generated against one schema version validates correctly against that same version:

```rust
#[test]
fn cross_version_validation_consistent() {
    // Generate with 5.1 schema
    let schema_51 = Schema::for_version("5.1")
        .expect("5.1 should be available")
        .clone();

    let config = RandomConfig { seed: Some(42), ..RandomConfig::default() };
    let result = generate_random(&config, &AdversarialConfig::default(), &schema_51).unwrap();

    // Validate with 5.1 — must pass (same version used for generation)
    let errors_51 = result.graph.validate(&schema_51);
    let structural_51: Vec<_> = errors_51.iter()
        .filter(|e| !matches!(e, ValidationError::PlausibilityWarning { .. }))
        .collect();
    assert!(structural_51.is_empty(), "5.1-generated graph should pass 5.1 validation: {:?}", structural_51);
}
```

> **Note**: Cross-version structural validation differences (5.1-generated graph validated against 5.2) are documented in Phase D (`docs/research/schema-5.1-vs-5.2.md`) rather than asserted in tests. The generated data is always version-conformant for its target schema; the cross-version case is informational, not a correctness invariant.

---

## 4. Call Site Migration Summary

Complete migration map for `Schema::new()` → `Schema::default()` (or `Schema::for_version()`) across the workspace:

```
crates/typed-graph/src/lib.rs                    5 calls   → Schema::default()
crates/typed-graph/src/graph.rs                  2 calls   → Schema::default()
crates/typed-graph/src/validate.rs              14 calls   → Schema::default()
crates/typed-graph/src/generate/random.rs       ~30 calls   → Schema::default()
crates/typed-graph/src/generate/adversarial.rs  11 calls   → Schema::default()
crates/typed-graph/src/generate/builder.rs       1 call    → Schema::default()
crates/cli/src/commands/generate.rs              2 calls   → Schema::for_version(&resolved)
crates/cli/tests/integration.rs                  5 calls   → Schema::default()
─────────────────────────────────────────────────────────────────────────
Total                                           ~70 calls
```

All test call sites use `Schema::default()` (non-deprecated, same behavior). The production hot path in `generate.rs` uses `Schema::for_version()` to respect the user's choice.

---

## 5. Testing Strategy

### Unit tests modified

- All `Schema::new()` → `Schema::default()` in test code
- `test_schema_new_and_versioned_api_consistent` annotated with `#[allow(deprecated)]`
- Version assertions use `Schema::default_version()` instead of `"5.2"`

### New tests added

- **Integration**: `generate_respects_schema_version` — full pipeline with 5.1 schema (Phase G)
- **E2E**: `e2e_generate_with_schema_version_51` — subprocess with `--schema-version 5.1` (Phase G)
- **Cross-version**: `cross_version_validation_consistent` — validates 5.1-generated graph against 5.1 schema (Phase G)
- **Generator**: Verify enum values are filtered by `schema.valid_enum_values` (Phase E)
- **Property-based** (Phase E): For a range of seeds and schema versions, assert all generated enum values are members of that version's `valid_enum_values` set. This catches regressions where a new enum variant leaks into older-version generation. Implement as a loop over seeds rather than a full proptest crate dependency if that keeps things simpler.

### CI feature matrix

| Job | Features | Command |
|---|---|---|
| Default | `schema-5-2` (default) | `cargo test --workspace && cargo clippy -- -D warnings` |
| Single 5.1 | `--no-default-features --features schema-5-1` | `cargo test --workspace` |
| All schemas | `--all-features` | `cargo test --workspace && cargo clippy --all-features -- -D warnings` |

The Single-5.1 job verifies that tests don't break when 5.2 is absent (version-agnostic assertions use `default_version()` which returns `"5.1"` in that build).

---

## 6. Rollback / Risk Mitigation

| Risk | Mitigation |
|---|---|
| `Schema::new()` deprecation breaks downstream consumers | The method body is unchanged — it still works. Only a compiler warning is emitted. Downstream code can suppress with `#[allow(deprecated)]`. |
| Schema 5.1 vs 5.2 has breaking differences | Phase D (verification) runs before generator changes, produces a concrete diff report, and catches issues before investing in Phase E implementation. |
| `generate_random` using schema slows generation | The lookup is O(log n) in a small HashMap. Negligible impact. |
| E2E tests fail when only 5.1 is compiled | Phase F accounts for this — CI matrix includes single-version jobs. E2E assertions are annotated with comments noting the default-feature coupling. |
| Transient deprecation warnings between commits C.1 and C.3 | Expected and temporary (~70 warnings). Commits C.2 and C.3 immediately follow C.1 to suppress all warnings. |
| `Schema::default()` clones allocate in tests | Each test's `Schema::default()` clones 4 HashMaps. For ~70 test call sites this is acceptable (tests are not the hot path). If it becomes a CI bottleneck, switch tests to use `Schema::for_version(Schema::default_version()).unwrap()` which borrows without cloning. |

---

## 7. Commit Plan

| # | Phase | Commit message | Files changed |
|---|---|---|---|
| 1 | A | `fix: restore deleted schema-5.2.json from git` | `schemas/schema-5.2.json` |
| 2 | B | `fix: wire schema version resolution through full CLI pipeline` | `crates/cli/src/commands/generate.rs` |
| 3 | C.1 | `refactor: deprecate Schema::new() in generated code` | `crates/typed-graph/build.rs` |
| 4 | C.2 | `refactor: migrate typed-graph Schema::new() call sites` | 6 files in `crates/typed-graph/` |
| 5 | C.3 | `refactor: migrate CLI Schema::new() call sites` | `crates/cli/tests/integration.rs` |
| 6 | D | `docs: add 5.1 vs 5.2 schema difference report` | `docs/research/schema-5.1-vs-5.2.md` |
| 7 | E.1 | `feat: make generate_random respect schema valid_enum_values` | `crates/typed-graph/src/generate/random.rs` |
| 8 | E.2 | `feat: gate field availability by schema version` (if Phase D reveals differences) | `crates/typed-graph/src/generate/random.rs` |
| 9 | F | `test: make unit-test version assertions agnostic, annotate E2E assertions` | `crates/typed-graph/src/lib.rs`, `crates/cli/tests/e2e.rs` |
| 10 | G | `test: add integration tests for version-aware pipeline` | `crates/cli/tests/` |
