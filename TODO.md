# Implementation Plan: Multi-Version Schema Support

Source: `docs/research/multi-version-schema.md`

## Summary

Extend gramps-gen to support multiple Gramps schema versions (currently 5.2; user needs 5.1.6), with a download/discovery tool that presents available versions from the Gramps GitHub repo, Cargo features for compile-time schema selection, and runtime schema version selection in the CLI.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: rename schema.json to schema-5.2.json and update build.rs path` | Schema file rename | `schemas/schema.json` → `schemas/schema-5.2.json`, `crates/typed-graph/build.rs` (path), `extract/extract_schema.py` (default version string) | Smoke |
| 2 | `feat: add --version and --output flags to extract_schema.py` | Extractor flags | `extract/extract_schema.py` | Unit |
| 3 | `chore: add Cargo features for multi-version schema support` | Cargo features | `crates/typed-graph/Cargo.toml` (features block) | Smoke |
| 4 | `feat: implement multi-schema build.rs with union merge algorithm` | Multi-schema build.rs | `crates/typed-graph/build.rs`, `crates/typed-graph/src/schema.rs` (new API), `$OUT_DIR/generated_schema.rs` (generated output) | Unit |
| 5 | `refactor: migrate typed-graph internal Schema::new() call sites to versioned API` | typed-graph API migration | `crates/typed-graph/src/lib.rs`, `validate.rs`, `graph.rs`, `generate/builder.rs`, `generate/adversarial.rs`, `generate/random.rs` | Unit |
| 6 | `feat: update GraphXmlWriter with schema_version, add --schema-version CLI flag` | Version in serialization + CLI | `crates/output/src/xml.rs`, `crates/cli/src/commands/generate.rs`, `crates/cli/tests/integration.rs` | Unit, Integration |
| 7 | `feat: add schema CLI subcommand (list + download)` | Schema CLI | `crates/cli/src/commands/schema.rs` (new), `crates/cli/src/main.rs`, `crates/cli/src/error.rs` (new variants), `crates/cli/Cargo.toml` (ureq) | Unit, E2E |
| 8 | `test: download 5.1 schema, verify compatibility, adapt SerializationMap if needed` | Schema 5.1 verification | `schemas/schema-5.1.json` (new), `crates/output/src/serialization_map.rs` (conditional logic if needed) | Manual, Unit |
| 9 | `ci: add CI matrix for feature combinations, E2E tests, update docs` | CI & docs | `.github/workflows/` (CI matrix), `crates/cli/tests/e2e.rs` (5.1 tests), `docs/ARCHITECTURE.md`, `AGENTS.md`, `README.md` | Integration, E2E |

---

## Step Details

### Step 1 — Schema file rename

Rename `schemas/schema.json` → `schemas/schema-5.2.json`. Update `build.rs` to read `schemas/schema-5.2.json`. Update `extract_schema.py`'s default version string to `"5.2"` (no functional change, just pointing at the right file). Update any other references to `schema.json` that would break (README.md, AGENTS.md if they reference the exact path).

**Deliverables**: `schemas/schema.json` → `schemas/schema-5.2.json`, modified `build.rs`, `extract_schema.py`, `README.md`, `AGENTS.md`

**Tests**: `cargo test --workspace` passes (no behavior change, path update only)

**Files changed**: ~5 files

---

### Step 2 — Extractor flags

Add `--version` and `--output` flags to `extract/extract_schema.py`. When `--version` is provided, use it as the version string in the output JSON instead of the hardcoded `"5.2"`. When `--output` is provided, write to that path instead of stdout. When `--version` is omitted, auto-detect from the Gramps source (e.g., `gramps/version.py`).

**Deliverables**: Modified `extract/extract_schema.py`

**Tests**: Run the script with `--version 5.1 --output /tmp/test-schema.json` and verify the JSON has `"version": "5.1"`

**Files changed**: 1 file

---

### Step 3 — Cargo features

Add feature flags to `crates/typed-graph/Cargo.toml`:

```toml
[features]
default = ["schema-5-2"]
schema-5-0 = []
schema-5-1 = []
schema-5-2 = []
schema-6-0 = []
all-schemas = ["schema-5-0", "schema-5-1", "schema-5-2", "schema-6-0"]
```

**Deliverables**: Modified `crates/typed-graph/Cargo.toml`

**Tests**: `cargo build --workspace` still works (default features = schema-5-2, no behavior change since build.rs doesn't use features yet)

**Files changed**: 1 file

---

### Step 4 — Multi-schema build.rs

Rewite `build.rs` to:

1. Detect enabled features via `CARGO_FEATURE_SCHEMA_*` env vars
2. For each enabled feature, check that `schemas/schema-X.Y.json` exists; emit a clear build error with remediation hint if missing
3. Read each enabled schema JSON file
4. Implement the multi-schema merge algorithm:
   - Union all field names per type across versions
   - Union all enum variants across versions
   - Apply optionality rule (field is `Option<T>` if ANY version doesn't require it or doesn't define it)
   - Detect conflicting field types and emit a build error
5. Generate per-version `static SCHEMA_X_Y: Schema` instances from each version's own JSON
6. Generate `Schema::available_versions()`, `Schema::default_version()`, `Schema::for_version()`
7. Generate `Schema::field_availability` map for runtime field presence checks
8. Keep `Schema::new()` as a backward-compatible alias for `Schema::for_version(Schema::default_version())`

Update `schema.rs` to expose the new API functions.

**Deliverables**: Modified `crates/typed-graph/build.rs`, `crates/typed-graph/src/schema.rs`

**Tests**:

- `cargo test --workspace` passes (Schema::new() alias works)
- Unit tests for merge algorithm: field union, enum union, optionality rule, conflict detection
- Test that `Schema::for_version("5.2")` returns `Some`
- Test that `Schema::available_versions()` returns `["5.2"]`
- Test that `Schema::default_version()` returns `"5.2"`
- Test that missing schema file emits a build error

**Files changed**: 2 files actively, plus generated output

---

### Step 5 — typed-graph internal API migration

Update all `Schema::new()` call sites inside the `typed-graph` crate to use the explicit versioned API. This covers:

- `crates/typed-graph/src/lib.rs` (3 call sites in tests)
- `crates/typed-graph/src/validate.rs` (14 call sites in tests)
- `crates/typed-graph/src/graph.rs` (2 call sites in tests)
- `crates/typed-graph/src/generate/builder.rs` (1 call site in test)
- `crates/typed-graph/src/generate/adversarial.rs` (12 call sites in tests)
- `crates/typed-graph/src/generate/random.rs` (31 call sites in tests)

Replace `Schema::new()` with `Schema::default_version()` where the test just needs a default schema, or with `Schema::for_version("5.2")` where the test explicitly checks 5.2 behavior.

**Deliverables**: Modified test files in `crates/typed-graph/src/`

**Tests**: `cargo test -p typed-graph` passes

**Files changed**: ~6 files

---

### Step 6 — GraphXmlWriter schema_version + CLI --schema-version

1. Update `GraphXmlWriter::new()` to accept a `schema_version: &str` parameter
2. Store the version and write it in the XML `<created version="...">` attribute
3. Update all 30 call sites in `crates/output/src/xml.rs` tests
4. Update the call site in `crates/cli/tests/integration.rs`
5. Add `--schema-version` flag to `GenerateArgs` in `crates/cli/src/commands/generate.rs`
6. Implement default resolution: if `--schema-version` is `"default"` (or omitted), use `Schema::default_version()`
7. Pass the resolved version string through to `GraphXmlWriter`
8. Add error handling for requested version not compiled in

**Deliverables**: Modified `crates/output/src/xml.rs`, `crates/cli/src/commands/generate.rs`, `crates/cli/tests/integration.rs`

**Tests**:

- `cargo test --workspace` passes
- Generate with `--schema-version 5.2` and verify XML header contains `version="5.2"`
- Generate without `--schema-version` and verify default is used
- Request non-compiled version and verify error message

**Files changed**: ~3 files

---

### Step 7 — Schema CLI subcommand

Add a new `schema` subcommand group with two sub-subcommands:

1. **`gramps-gen schema list`** — show local schemas (scanned from `schemas/schema-*.json`) + remote schemas available from GitHub API. Cache GitHub API response to `~/.cache/gramps-gen/tags.json` (TTL: 1 hour). If API unreachable, show only local schemas with a warning.

2. **`gramps-gen schema download [VERSION]`** / **`--all`** — resolve version to best git tag, shallow-clone Gramps repo, run extractor, write validated schema to `schemas/`. Show progress and warning about executing code from GitHub.

Add `CliError` variants:

- `GitHubApiError` — GitHub API unreachable/rate-limited
- `GitCloneFailed` — git clone fails
- `SchemaExtractionFailed` — extractor fails or produces invalid JSON
- `SchemaDownloadError` — version not found in tag map
- `PythonNotFound` — Python 3 not available

Add `ureq` dependency to `cli/Cargo.toml` for blocking HTTP calls.

**Deliverables**: New `crates/cli/src/commands/schema.rs`, modified `crates/cli/src/main.rs`, `crates/cli/src/error.rs`, `crates/cli/Cargo.toml`

**Tests**:

- Unit tests for tag list parsing (mock GitHub API response)
- Unit tests for version resolution (input → best tag)
- Unit tests for cache hit/miss/expiry
- E2E test: `gramps-gen schema list` output is well-formed
- E2E test: `gramps-gen schema download invalid-version` exits non-zero
- E2E test: `gramps-gen schema download` works (requires network, could be marked `#[ignore]`)

**Files changed**: ~4 files

---

### Step 8 — Schema 5.1 verification

1. Run `gramps-gen schema download 5.1` to get `schemas/schema-5.1.json`
2. Diff `schema-5.1.json` vs `schema-5.2.json` to identify field/enum/type differences
3. Verify `SerializationMap` against 5.1 structure:
   - XML element names match for all 10 primary types
   - XML attribute names match for all serialized fields
   - Child element names match for all serialized child elements
   - Section ordering in `<database>` is consistent
   - XML namespace URI is identical
4. If differences found, add version-conditional logic to `SerializationMap` or `GraphXmlWriter`
5. Run `cargo test --workspace --no-default-features --features schema-5-1` to verify 5.1-only build

**Deliverables**: `schemas/schema-5.1.json` (new, committed), possibly updated `crates/output/src/serialization_map.rs`

**Tests**: Manual verification of schema diff, then `cargo test --workspace` with 5.1 features

**Files changed**: 1–2 files

---

### Step 9 — CI matrix, E2E tests, documentation

1. Add CI job matrix to `.github/workflows/` (or equivalent CI config):
   - Default features: `cargo test --workspace && cargo clippy -- -D warnings`
   - Single 5.1: `--no-default-features --features schema-5-1`
   - All schemas: `--all-features`
2. Add E2E tests for 5.1 generation:
   - `gramps-gen generate --schema-version 5.1 --count 10 --seed 42`
   - Verify XML header has correct version
   - Verify output passes `gramps-gen validate`
3. Update `docs/ARCHITECTURE.md`:
   - Add multi-version storage layout to architecture diagram
   - Document Cargo features and union-type strategy
   - Document `schema` CLI subcommand
4. Update `AGENTS.md`:
   - Update schema file paths in workspace structure
   - Document feature flags and build system
   - Update schema-driven codegen section
5. Update `README.md`:
   - Document `--schema-version` flag
   - Document `schema list` and `schema download` commands
   - Update examples

**Deliverables**: CI config, `crates/cli/tests/e2e.rs` (new/updated tests), `docs/ARCHITECTURE.md`, `AGENTS.md`, `README.md`

**Tests**: Full CI matrix, E2E tests

**Files changed**: ~5 files

---

## Dependency Graph

```
Step 1 (rename) ──→ Step 3 (features) ──→ Step 4 (build.rs)
                                                    │
                                                    ▼
                                             Step 5 (typed-graph API)
                                                    │
                                                    ▼
Step 2 (extractor) ──→ Step 7 (schema CLI)    Step 6 (writer + CLI flag)
                              │                      │
                              ▼                      ▼
                         Step 8 (download 5.1) ←────┘
                              │
                              ▼
                         Step 9 (CI + docs)
```

Steps 1–3 are independent foundations. Step 4 is the core engine change. Step 5 migrates internal call sites. Step 6 updates serialization and adds the CLI flag. Step 7 (schema CLI) depends on Step 2 (extractor flags) but is otherwise independent of Steps 4–6. Steps 8 and 9 are verification and hardening.
