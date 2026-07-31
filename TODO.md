# Implementation Plan: Fix Gramps 5.1 Import Failure

Source: `docs/research/gramps-import-fix.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `refactor: restructure serialization test gates for schema-5-1 coverage` | Test gate restructuring | `crates/output/src/xml.rs` | Unit |
| 2 | `feat: add namespace derivation and full version parameter to GraphXmlWriter` | Unified namespace + version parameter | `crates/output/src/xml.rs` | Unit |
| 3 | `feat: wire schema version to full Gramps version in CLI` | CLI version mapping | `crates/cli/src/commands/generate.rs` | Unit |
| 4 | `fix: add event_type_display helper for correct event type rendering` | Event type rendering | `crates/typed-graph/src/graph.rs`, `crates/output/src/xml.rs`, `crates/typed-graph/src/lib.rs` | Unit |
| 5 | `test: add edge role/relation serialization tests and audit Debug formatting` | Audit + edge role tests | `crates/output/src/xml.rs` | Unit |
| 6 | `test: add integration tests for namespace, version, and event type` | Integration tests | `crates/output/src/xml.rs` | Unit |
| 7 | `chore: add validate-gramps-roundtrip.sh script` | Validation script | `scripts/validate-gramps-roundtrip.sh` | — |

## Step Details

### 1 — Test gate restructuring

**What**: Move schema-independent tests (document structure, header, section order, escaping, well-formedness, element serialization for all 10 types, ref serialization) outside the `#[cfg(not(feature = "schema-5-1"))]` gate so they run under all feature combinations. Keep the existing gate only on tests that depend on 5.2-specific type shapes (e.g., `EventType::Birth` direct field access, hardcoded `"5.2"` version strings). Add an empty `#[cfg(feature = "schema-5-1")]` module scaffold for future 5.1-specific tests.

**Helper functions** (`make_person`, `make_family`, `make_event`, etc.) must be moved to a shared location accessible to both gated modules (e.g., as private helpers in the outer scope of the test module, or duplicated minimally).

**Tests**: All existing tests continue to pass under `cargo test -p output --features schema-5-2` (default). `cargo test -p output --features schema-5-1` now runs the schema-independent tests (previously it ran zero tests).

### 2 — Unified namespace + full version parameter

**What**:

- Change `GraphXmlWriter::new(map, schema_version: &str)` → `GraphXmlWriter::new(map, gramps_version: &str)` where `gramps_version` is a full version like `"5.1.6"`.
- Add private method `derive_namespace(gramps_version: &str) -> String` with a lookup table mapping major.minor prefix to XML namespace URI. For unknown versions, derive with pattern `http://gramps-project.org/xml/1.{major}.{minor}/` and emit a warning (via `eprintln!` or logging).
- Add private method `schema_version_prefix(gramps_version: &str) -> String` that returns the `"X.Y"` prefix for the `<created version="..."/>` header attribute.
- Replace the hardcoded `r#"<database xmlns="http://gramps-project.org/xml/1.7.2/">"#` with the derived namespace.
- Update all call sites in tests to pass `"5.2.0"` instead of `"5.2"` (and `"5.1.6"` for 5.1 tests).

**Tests**: `xml_header_version_parameter` is updated to check `"5.1.6"` instead of `"5.1"`. A new test verifies the correct namespace is emitted for each version.

### 3 — CLI version mapping

**What**: In `crates/cli/src/commands/generate.rs`, add a mapping from `--schema-version` flag value to the representative full Gramps version:

| Schema | Full version |
|---|---|
| `"5.0"` | `"5.0.2"` |
| `"5.1"` | `"5.1.6"` |
| `"5.2"` | `"5.2.0"` |
| `"6.0"` | `"6.0.0"` (TBC) |

Pass the full version string to `GraphXmlWriter::new()` instead of the bare schema version. The `schema_version` variable used for schema lookups remains the short form.

**Tests**: Update the CLI's `generate` command test (if any) or verify via `cargo build`.

### 4 — Event type rendering

**What**:

- Add `pub fn event_type_display(event_type: ...) -> String` in `crates/typed-graph/src/graph.rs` following the existing `#[cfg(feature = "schema-5-1")]` / `#[cfg(not(feature = "schema-5-1"))]` pattern:
  - **5.1 (merged)**: takes `Option<EventType>`, returns `format!("{:?}", event_type.unwrap())` or similar
  - **5.2 (default)**: takes `EventType`, returns `format!("{:?}", event_type)`
- Export `event_type_display` from `crates/typed-graph/src/lib.rs`.
- Replace `format!("{:?}", e.event_type)` in `crates/output/src/xml.rs` at line ~390 with `typed_graph::event_type_display(e.event_type)`.

**Tests**: Under `#[cfg(feature = "schema-5-1")]`, verify `<type>Death</type>` (not `<type>Some(Death)</type>`) in serialized output. Under `#[cfg(not(feature = "schema-5-1"))]`, existing test `serialize_event_element` already asserts `<type>Birth</type>`.

### 5 — Audit + edge role/relation tests

**What**:

- Audit all `{:?}` format calls in `xml.rs` (lines 390, 974, 984) to confirm none produce `Option<>` wrapping artifacts.
- Line 974 (`get_edge_role`): `format!("{:?}", metadata.role.as_ref().unwrap_or(&EventRoleType::Primary))` — this is fine since `role` is always `Option<EventRoleType>` regardless of schema version, and the `unwrap_or` provides a default.
- Line 984 (`get_edge_relation`): `format!("{:?}", metadata.relation.as_ref().unwrap_or(&ChildRefType::Birth))` — same reasoning, safe.
- Add explicit unit tests for `get_edge_role()` and `get_edge_relation()` that construct edges with each `EventRoleType` and `ChildRefType` variant and assert the output string matches the expected value (e.g., `"Primary"`, `"Birth"`).

**Tests**: New tests exercise `get_edge_role` and `get_edge_relation` with all variants, running under `--all-features`.

### 6 — Integration tests

**What**: Add integration tests (in the restructured test modules from Step 1) that:

- Construct a minimal `Graph`, serialize with each schema version, and assert the correct namespace appears in the output.
- Gate on `#[cfg(feature = "schema-5-1")]` to verify event type renders as `Death`, not `Some(Death)`.
- Verify `<created version="5.1.6"/>` (or equivalent full version) appears in the header for 5.1 output, and `"5.2.0"` for 5.2 output.

**Tests**: These run with `cargo test --all-features -p output` and catch regressions without needing Gramps installed.

### 7 — Validation script

**What**: Create `scripts/validate-gramps-roundtrip.sh` that:

1. Builds: `cargo build --release --features schema-5-1`
2. Generates: runs the exact command from the bug report
3. Structural checks (no Gramps required):
   - `xmllint --noout` — well-formed XML
   - `grep` for namespace: must be `1.7.1` for 5.1, `1.7.2` for 5.2
   - `grep` for `Some(` — must be absent (catches leftover `{:?}` artifacts)
   - `grep` for `<created version="..."` — must contain a 3-part version string
4. Gramps import check (optional, only if `gramps` is on `$PATH`): runs import and checks acceptance
5. Exit code: non-zero on any failure, zero on success

Also runs the same checks for schema-5-2 (default build) to verify no regression.

**Tests**: N/A (shell script, tested manually)
