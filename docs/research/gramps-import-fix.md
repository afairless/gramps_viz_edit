# Fix Gramps 5.1 Import Failure

Date: 2026-07-30

## Problem

Running:

```
cargo build --release --features schema-5-1
target/release/gramps-gen generate --schema-version 5.1 --output ~/exp01.gramps --count 8 --seed 2026 --depth 4
```

Produces a `.gramps` file that Gramps 5.1.6 rejects with:

> The .gramps file you are importing was made by version 5.1 of Gramps, while you
> are running an older version 5.1.6. The file will not be imported.

## Diagnosis

Inspecting the generated XML reveals three bugs, two of which are blocking:

### Bug 1 (BLOCKING): Wrong XML namespace for schema version

**Root cause**: `crates/output/src/xml.rs:135` hardcodes the namespace:

```rust
r#"<database xmlns="http://gramps-project.org/xml/1.7.2/">"#
```

The namespace `1.7.2` is the Gramps 5.2 XML format. Gramps 5.1.6 ships with
`GRAMPS_XML_VERSION_TUPLE = (1, 7, 1)` in `/usr/lib/python3/dist-packages/gramps/plugins/lib/libgrampsxml.py`.
During import, Gramps extracts the namespace version tuple and compares it:

```python
# importxml.py line 988
self.__xml_version = version_str_to_tup(xmlns[4], 3)  # → (1, 7, 2)

# line 1022
if self.__xml_version > libgrampsxml.GRAMPS_XML_VERSION_TUPLE:  # (1,7,2) > (1,7,1) → True
    raise GrampsImportError(...)  # <--- User sees this
```

The `<created version="5.1"/>` attribute itself is **not** the cause of the
rejection — it only appears in the error message text. The actual gate is the
namespace tuple comparison.

**Fix needed**: The `GraphXmlWriter` must accept the XML namespace (or derive it
from the schema version). Mapping:

| Gramps version | XML namespace |
|---|---|
| 5.0 | `http://gramps-project.org/xml/1.7.0/` |
| 5.1 | `http://gramps-project.org/xml/1.7.1/` |
| 5.2 | `http://gramps-project.org/xml/1.7.2/` |
| 6.0 | `http://gramps-project.org/xml/1.8.0/` (TBC) |

If a version lacks a confirmed namespace mapping, derive it with the pattern
`http://gramps-project.org/xml/1.{major}.{minor}/` and emit a warning.

### Bug 2 (BLOCKING): Event type renders as `Some(Variant)` with schema-5-1

**Root cause**: In `crates/output/src/xml.rs`, line ~370, the event type is
written using `format!("{:?}", e.event_type)`. When `schema-5-1` is compiled
alongside `schema-5-2` (or alone where `EventType` is an `enum_ref` not an
inline `enum`), the merged schema wraps `event_type` as `Option<EventType>`:

```xml
<eventtype><type>Some(Death)</type></eventtype>
<!-- should be: -->
<eventtype><type>Death</type></eventtype>
```

This would cause a second import failure even after fixing the namespace:
Gramps won't recognize `Some(Death)` as a valid event type.

**Fix needed**: Use an `event_type_display()` helper (or inline pattern match)
that handles both `EventType` (5.2) and `Option<EventType>` (5.1-merged) and
returns only the variant name string.

### Bug 3 (COSMETIC): Version string in header is bare major.minor

The `<created version="5.1"/>` uses the schema's major.minor string.
Real Gramps exports use the full version (e.g., `"5.1.6"`). While this
isn't what causes import rejection, it's incorrect metadata and would
show misleading version information.

**Fix needed**: Accept a full Gramps version string (e.g., `"5.1.6"`) in
`GraphXmlWriter::new()`, replacing the current bare `schema_version: &str`
parameter. The full version string drives both the `<created version="..."/>`
header attribute and (by parsing its major.minor prefix) the XML namespace
lookup. The CLI passes a hardcoded mapping from schema version to
representative Gramps release:

| Schema | Representative Gramps version |
|---|---|
| 5.0 | `5.0.2` |
| 5.1 | `5.1.6` |
| 5.2 | `5.2.0` |
| 6.0 | `6.0.0` (TBC) |

Backward compatibility: the default build (feature `schema-5-2`) continues to
emit namespace `1.7.2` and version `"5.2.0"` — unchanged from current behavior.

## Other potential issues (additional discovery)

### Gender rendering

The `gender` field is written as an integer (`0`, `1`, `2`). When the merged
schema makes it `Option<i32>`, the attribute value for `gender` could become
empty or "None" in some edge cases. The code uses `typed_graph::gender_value(p.gender)`
which looks correct. This should be verified.

### Other Debug-formatted values

A full audit should check all `{:?}` format calls in `xml.rs`:

- `get_edge_role()` uses `format!("{:?}", ...)` — this is fine since it goes through
  the `EventRoleType` and `ChildRefType` enums, not `Option<>` wrappers.
  An explicit unit test (Phase 3.3) will confirm this under `--all-features`.

## Plan

### Phase 0: Restructure test gates for schema-5-1 coverage

The existing test module in `xml.rs` is gated with
`#[cfg(all(test, not(feature = "schema-5-1")))]`, which means all serialization
tests are skipped when building with `--features schema-5-1`. Before adding new
tests, restructure so that schema-independent tests always run:

- [ ] 0.1 Move schema-independent tests (document structure, header, section
  order, escaping, well-formedness) outside the `#[cfg]` gate — they should
  run under all feature combinations
- [ ] 0.2 Keep the existing `#[cfg(not(feature = "schema-5-1"))]` gate only on
  tests that depend on the 5.2-specific type shapes (e.g., `EventType::Birth`)
- [ ] 0.3 Add a parallel `#[cfg(feature = "schema-5-1")]` test module for
  5.1-specific serialization behavior

### Phase 1: Unified namespace + full version parameter

Replace the current `schema_version: &str` parameter on `GraphXmlWriter::new()`
with a full Gramps version string (e.g., `"5.1.6"`). From this single string,
derive both the XML namespace (by parsing the major.minor prefix) and the
`<created version="..."/>` header attribute:

- [ ] 1.1 Change `GraphXmlWriter::new(map, schema_version)` →
  `GraphXmlWriter::new(map, gramps_version)` where `gramps_version` is a full
  version like `"5.1.6"`
- [ ] 1.2 Add a private method `derive_namespace(gramps_version: &str) -> String`
  that maps the major.minor prefix to the XML namespace URI using a lookup
  table in the `output` crate. For unknown versions, derive with the pattern
  `http://gramps-project.org/xml/1.{major}.{minor}/` and log a warning
- [ ] 1.3 Add a private method `schema_version_prefix(gramps_version: &str) -> String`
  that returns just the `"X.Y"` prefix for use in the header
- [ ] 1.4 In the CLI `generate.rs`, map the `--schema-version` flag to the
  representative full Gramps version (e.g., `"5.1"` → `"5.1.6"`)
- [ ] 1.5 Update all call sites (CLI, tests) to pass the full version string
- [ ] 1.6 Use the derived namespace instead of the hardcoded `1.7.2` value

### Phase 2: Fix event type rendering

- [ ] 2.1 Add a `pub fn event_type_display(event_type: ...) -> String` helper in
  `crates/typed-graph/src/graph.rs` (following the existing convention for
  version-specific helpers like `gender_value`, `into_event_type_field`,
  `event_type_eq`). Use `#[cfg(feature = "schema-5-1")]`/
  `#[cfg(not(feature = "schema-5-1"))]` pairs to handle `Option<EventType>`
  vs `EventType`
- [ ] 2.2 Replace `format!("{:?}", e.event_type)` in `xml.rs` with
  `typed_graph::event_type_display(e.event_type)`

### Phase 3: Audit XML output for correctness

- [ ] 3.1 Audit all `{:?}` format calls in `xml.rs` for potential `Option<>` wrapping
- [ ] 3.2 Verify gender, dates, and other fields render correctly with all schema features
- [ ] 3.3 Add an explicit unit test for `get_edge_role()` and `get_edge_relation()`
  that constructs edges with each `EventRoleType` and `ChildRefType` variant
  and asserts the output string matches the expected value (e.g., `"Primary"`,
  `"Birth"`), running under `--all-features`

### Phase 4: Rust integration tests (fast path, no Gramps needed)

- [ ] 4.1 In `crates/output/src/xml.rs`, add tests (in the restructured test
  modules from Phase 0) that construct a minimal `Graph`, serialize it with
  each schema version, and assert the correct namespace appears in the output
- [ ] 4.2 Add a test (gated on `#[cfg(feature = "schema-5-1")]`) that verifies
  event type renders as `Death`, not `Some(Death)`
- [ ] 4.3 Add a test that verifies `<created version="5.1.6"/>` (or equivalent
  full version) appears in the header
- [ ] 4.4 These tests run with `cargo test --all-features` and catch
  regressions without needing Gramps installed

### Phase 5: Automated validation script (the workflow runner)

A shell script `scripts/validate-gramps-roundtrip.sh` that reproduces the
user's exact workflow and checks correctness — one command, no manual steps.
Runs **after** the Rust tests pass (Phase 4) as a slower but higher-fidelity
acceptance gate:

```bash
./scripts/validate-gramps-roundtrip.sh
```

What it does:

- [ ] 5.1 **Build**: `cargo build --release --features schema-5-1` (and later
  `--features schema-5-2` to verify both versions)
- [ ] 5.2 **Generate**: runs the exact command from the bug report:
  `target/release/gramps-gen generate --schema-version 5.1 --output /tmp/test.gramps --count 8 --seed 2026 --depth 4`
- [ ] 5.3 **Structural checks** (no Gramps required — fast, always available):
  - `xmllint --noout` — well-formed XML
  - `grep` for namespace: must be `1.7.1` for 5.1, `1.7.2` for 5.2
  - `grep` for `Some(` — must be absent (catches leftover `{:?}` artifacts)
  - `grep` for `<created version="..."` — must contain a 3-part version string
- [ ] 5.4 **Gramps import check** (only if `gramps` is on `$PATH`):
  - Runs `gramps -i /tmp/test.gramps -a import -f gramps --dry-run` or
    equivalent to verify Gramps accepts the file
  - Skips gracefully with a message if Gramps is not installed
- [ ] 5.5 **Exit code**: non-zero on any failure, zero on success
  → usable in CI and as a pre-commit hook

The script serves double duty:

1. **Before fix**: run it to confirm the original error (namespace check fails)
2. **After fix**: run it to confirm everything passes

## Target state

After these fixes, running a single command reproduces the workflow:

```bash
# Full validation (requires Gramps installed for import check)
./scripts/validate-gramps-roundtrip.sh

# Fast path (Rust tests only, no Gramps needed)
cargo test --all-features -p output
```

The same `generate` command from the bug report produces a `.gramps` file that:

1. Opens successfully in Gramps 5.1.6 (correct namespace `1.7.1`)
2. Shows accurate metadata (version `"5.1.6"` in the header)
3. Has correctly rendered event types (`<type>Birth</type>`, not `<type>Some(Birth)</type>`)
4. Passes both the shell script and Rust test suites
