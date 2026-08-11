# Delete Tool Output Round-Trip Fidelity

> **Status: OBSOLETED** — The bugs described in this document (1–4) are
> resolved by the Python backend restructure
> ([`gramps-python-delete-backend.md`](./gramps-python-delete-backend.md)).
> The `delete` command now delegates all XML I/O to a Python subprocess that
> uses Gramps' own `importxml` and `exportxml` libraries, eliminating the
> entire class of XML round-trip bugs. The detailed analysis below is
> preserved for historical reference.

## Problem

Running `gramps-gen delete` on a Gramps 5.1 `.gramps` file produces output
that crashes the Gramps UI on import:

```
TypeError: __str__ returned non-string (type int)
  File "gramps/gen/db/generic.py", line 1957, in commit_note
    self.note_types.add(str(note.type))
```

The crash occurs on the `<note>` elements in the output. Four categories
(people, families, events, notes) were deleted; surviving notes trigger the
crash during Gramps import.

## Root Cause Analysis

### Bug 1 — Missing `type` attribute on `<note>` elements (primary)

**Gramps-side latent bug.** In `gramps/plugins/importer/importxml.py` line
1899, Gramps parses a top-level `<note>` element:

```python
self.note.type.set_from_xml_str(attrs.get('type', NoteType.UNKNOWN))
```

When the `type` attribute is absent, `attrs.get` returns the default
`NoteType.UNKNOWN`, which is the **integer** `-1`.  `set_from_xml_str` checks
whether the value is in its `_E2IMAP` string→int map.  The integer `-1` is not
a string key, so it falls through to the else branch:

```python
self.__value = self._CUSTOM   # 0
self.__string = value         # -1 (int!)
```

Later, `str(note.type)` calls `NoteType.__str__` which returns
`self.__string` (the integer `-1`), and Python raises:

```
TypeError: __str__ returned non-string (type int)
```

**Our trigger.** When the delete pipeline omits the `type` attribute on a
`<note>` element, it exposes this Gramps bug.

**How we trigger it.** The round-trip pipeline for delete is:

```
Input .gramps → gramps-reader parse → Graph → delete cascade → output serialize → .gramps
```

Prior to the fix commits (see below), the `type` attribute was not preserved
across this pipeline for two reasons:

1. `NoteData.type_field` was `Option<NoteType>` — an enum that couldn't
   represent arbitrary Gramps XML note-type strings like `"To Do"` or
   `"Person Note"`.
2. The serialization map for `Note` didn't include `type` as an output
   attribute.

**Fixes already committed (76ed57e … c098134):**

| Commit    | What it does |
|-----------|-------------|
| `76ed57e` | Changes `NoteData.type_field` from `Option<NoteType>` to `Option<String>` in the schema for lossless round-tripping |
| `3cb68ee` | Adds `read_attr(e, b"type")` parsing in `gramps-reader` `NoteBuilder` |
| `42d0813` | Adds `type` and `format` as XML attributes in `serialization_map.rs` for `Note` |
| `d4b4dbf` | Removes dead `format` child-element handler (format is now attribute-only) |
| `59ff3d5` | Adds unit tests: note type/format round-trip, property tests |
| `18eb066` | Adds E2E test: `e2e_delete_notes_import_roundtrip` — constructs a fixture, runs delete, imports into real Gramps, asserts no errors |
| `c098134` | Adds `scripts/validate-gramps-import.sh` CI validation script |

**⚠️ Stale build artifacts.** The generated code under `target/` can linger
from before the schema change.  After `cargo clean -p typed-graph` the
generated `NoteData` correctly has `pub type_field: Option<String>`.  Without
cleaning, an old `NoteData` with `pub type_field: Option<NoteType>` may be
used, and the fix doesn't take effect.

**Remaining gap — no default.** If the input `.gramps` file has a `<note>`
without a `type` attribute (shouldn't happen in Gramps exports but possible
with hand-edited files), the parser stores `note_type: None`, and the
serialization omits the attribute, re-triggering the Gramps crash.  We should
default to `type="General"`.

### Bug 2 — Date value truncation

The parser in `gramps-reader/src/xml/graph.rs` only extracts the **year** from
`<dateval val="YYYY-MM-DD"/>`:

```rust
fn parse_year_from_val(e: &quick_xml::events::BytesStart) -> i64 {
    // Only extracts year — month and day are discarded.
    if let Some(year_str) = val.split('-').next() { … }
}
```

The resulting `DateValue` has `day: Some(year as i32)` (wrong — year value
stored in `day`), `month: None`, `year: <year>`.  Serialization then formats
only the year (`{:04}`), so `"1868-09-20"` becomes `"1868"`.

This is a significant data-loss bug independent of the note-type issue.

### Bug 3 — Namespace/format mismatch

The delete command preserves the input XML namespace:

```rust
let writer = GraphXmlWriter::new(serialization_map, gramps_version)
    .with_namespace(&namespace)   // namespace from parsed input
    ...
```

The input is Gramps 5.1 XML (namespace `http://gramps-project.org/xml/1.7.1/`),
but the **content is serialized in Gramps 5.2 format** (e.g., events use
`<eventtype><type>Birth</type></eventtype>` instead of `<type>Birth</type>`).
Gramps uses the namespace to decide how to interpret elements.  This mismatch
could cause subtle parse failures in strict implementations, though the
current Gramps UI's `importxml.py` appears tolerant of mixed namespaces.

Additionally, the output omits the `<!DOCTYPE>` declaration present in the
original file.  Gramps does not appear to validate against the DTD, so this
is cosmetic but worth noting for fidelity.

### Bug 4 — Missing `change` attribute

The output omits the `change` timestamp attribute present in the input on
elements like `<event handle="..." change="1765667134" id="E0070">`.  The
schema doesn't define a `change` field for most primary types, so this
attribute is dropped during parse→serialize round-trip.  Gramps does not
require the `change` attribute — it auto-generates one on import — so this
is a fidelity concern but not a crash bug.

**Documentation contract:** Document this intentional omission in
`crates/output/src/serialization_map.rs` with a comment explaining that
`change` is deliberately excluded because Gramps auto-generates the
timestamp on import.  This prevents future developers from treating it
as a bug.

## Attack Surface Summary

| # | What | Severity | Status |
|---|------|----------|--------|
| 1a | Missing `type` attr on `<note>` → Gramps crash | **Critical** | Fixed in commits 76ed57e…c098134; stale artifacts risk; missing default |
| 1b | No default for notes without `type` in input | **High** | Not yet addressed |
| 2 | Date value truncated to year-only | **High** | Not yet addressed |
| 3 | Namespace/format mismatch (5.1 ns + 5.2 content) | **Medium** | Not yet addressed |
| 4 | Missing `change` attribute | **Low** | Known limitation |

## Why the E2E Test Didn't Catch This

The E2E test `e2e_delete_notes_import_roundtrip` (commit `18eb066`)
constructs a **Gramps 5.2-format fixture** with explicit `type="General"` and
`type="To Do"` attributes.  It correctly verifies that:

1. Every `<note>` element has a `type` attribute in the output
2. The type values survive the round-trip
3. Gramps import succeeds without errors

However, the test **only covers the 5.2 input path**.  The user's file is
**Gramps 5.1 format** (`xmlns="http://gramps-project.org/xml/1.7.1/"`).  The
5.1 schema goes through `schema_convert.rs` JSON Schema → flat format
conversion, which maps the `type` field with `title: "Type"` to `enum_ref`
targeting `NoteType`.  When merged with the 5.2 schema (which has `type` as
plain `string`), the merge algorithm uses the last version's type info (5.2
→ `string`).  **After a clean build** this works correctly, but stale build
artifacts can retain the old `Option<NoteType>` representation.

Additionally, real-world Gramps 5.1 files may contain edge cases not present
in the minimal test fixture:

- Notes referenced from multiple parent types with different implicit types
- Inline notes embedded inside other elements (ignored by parser, harmless)
- The full mix of 10 primary types with interconnections

## Plan

### Phase 1 — Guarantee crash-free output (defense in depth)

1. **Add default note type in serialization.** In `get_field_value` for
   `Node::Note`, when `type_field` is `None`, return `Some("General")`.
   This ensures Gramps never receives a `<note>` without `type`.

2. **Add default note type in builder/generator.** In `GraphBuilder::build_note()`
   (NOT in the generated `NoteData::default()`), default `type_field` to
   `Some("General".to_string())`.  The serialization default in
   `get_field_value` (step 1) is the primary defense for the delete path;
   the builder default is defense-in-depth for the generation path only.
   The generated `NoteData::default()` can remain `None` — modifying
   `build.rs` for this would affect all consumers unnecessarily.

3. **Verify with `cargo clean && cargo test`.** Ensure stale artifacts
   don't mask the fix.

   > **Note on `format` attribute:** The `format` attribute on `<note>`
   > shares the same omission risk as `type`, but Gramps does not crash
   > on a missing `format`.  The serialization map already includes
   > `format` as an attribute; verify it survives the round-trip in
   > Phase 4 testing.

### Phase 2 — Fix date truncation

**Decision: Structured + raw text.** Add a `text` field to `DateValue` for
   the original `val` attribute string (handles modifiers, ranges, etc.),
   while still parsing year/month/day into structured fields.

1. **Store raw `val` string in the existing `text` field.** The `DateValue`
   struct already has a `text: Option<String>` field (confirmed in generated
   code and `date.rs`).  Populate it with the raw `val` attribute string
   from the XML for exact round-trip fidelity.  No struct changes needed.

2. **Parse full date into structured fields.** Change `parse_year_from_val`
   to return a `ParsedDate` struct with all components:

   ```rust
   struct ParsedDate {
       year: i32,
       month: Option<i32>,
       day: Option<i32>,
       raw_text: Option<String>,
   }
   ```

   Rename to `parse_date_from_val`.  Extract year, month, and day from the
   `val` string (split on `-`).  Also capture the raw `val` string as
   `raw_text`.

   Fix the call site at `crates/gramps-reader/src/xml/graph.rs` line ~1250:
   currently constructs `DateValue { day: Some(year as i32), month: None,
   year: year as i32, text: None }`.  Change to use the parsed fields:
   `day: parsed.day`, `month: parsed.month`, `year: parsed.year`,
   `text: parsed.raw_text`.  This fixes both the year-as-day bug and the
   missing month/day/text.

3. **Serialize `text` when available.** In `write_date_element`, when
   `d.text` is `Some`, use it as the `val` attribute value instead of
   reconstructing from year/month/day.  This guarantees exact round-trip
   for any date format Gramps can express.

4. **Update tests.** Add round-trip tests for dates with month/day, and
   for dates with modifiers ("before 1868-09-20", ranges, etc.).

   **Property-based test:** For all valid `DateValue` inputs (year-only,
   year-month, full YMD, with modifiers like "before", "after", "about",
   ranges, spans), verify the round-trip invariant:
   `parse_xml(serialize_xml(date))` produces an equivalent `DateValue`.
   This catches edge cases like leap days, month boundaries, and modifier
   text that single-example tests might miss.

> **⚠️ Dependency:** Phase 3 builds on Phase 2's parser and output changes
> (`parse_date_from_val`, `write_date_element`, `get_field_value`).
> Complete Phase 2 before starting Phase 3 to avoid merge conflicts.

### Phase 3 — Namespace/format consistency

**Decision: Version-aware output (Option A).** Detect the input version
   and emit matching XML format — 5.2-style `<eventtype>` wrapping for
   5.2 input, flat `<type>` for 5.1 input.

1. **Extract version from namespace.** The `namespace` string already
   carries the version (e.g., `.../xml/1.7.1/` → 5.1, `.../xml/1.7.2/`
   → 5.2).  Add a helper `namespace_to_version()` that returns `"5.1"`
   or `"5.2"`.

2. **Audit serialization for format switch points.** Before implementing,
   inspect `crates/output/src/xml.rs` for all locations where 5.2-specific
   element nesting is hardcoded.  Key areas to check:
   - Event type serialization (currently uses `<eventtype><type>` nesting)
   - Any other format differences between 5.1 flat XML and 5.2 nested XML
   - The `GraphXmlWriter::new()` call in `delete.rs` which hardcodes
     `gramps_version = "5.2"`
   Document the exact switch points before coding.

3. **Make serialization version-aware.** Add a `gramps_xml_version: &str`
   field to `GraphXmlWriter` (distinct from the schema version used for
   the header).  When the version is 5.1, emit:
   - Events: `<type>Birth</type>` (flat) instead of `<eventtype><type>Birth</type></eventtype>`
   - Use the 5.1 namespace in `xmlns`
   - Use `version="5.1"` in the `<created>` header element

4. **Use input version as default.** When `with_namespace` is called,
   automatically derive and set the output XML format version.

5. **Update the 5.1 test fixture.** The existing `parse_event_5_1_flat_format`
   test already covers 5.1 event parsing.  Add a full round-trip test
   that verifies 5.1 input → delete → 5.1 output → Gramps import.

### Phase 4 — Test coverage expansion

1. **Add Gramps 5.1 fixture test.** Create a real Gramps 5.1 `.gramps`
   fixture (or a representative synthetic one with the 5.1 namespace and
   5.1-format elements) and verify round-trip through delete → Gramps
   import.

2. **Add a "Gramps import golden test"** using the smallest possible real
   Gramps export as a committed test fixture.  The test: parse → serialize
   → invoke `gramps -y -q -i <output>` → assert no errors on stdout/stderr.

3. **Add date round-trip tests.** Assert that `"1868-09-20"` survives
   parse→serialize unchanged.  Test year-only, year-month, full YMD,
   and dates with modifier text ("before 1868-09-20", ranges, spans).

4. **Add note-type default test.** Assert that note without `type` in
   input receives `type="General"` in output.

5. **Add note `format` attribute test.** Verify that the `format` attribute
   on `<note>` elements survives the round-trip (e.g., `format="1"` for
   HTML notes).  While `format` omission doesn't crash Gramps, it's a
   fidelity gap worth testing.

### Phase 5 — CI hardening

1. **Prevent stale build artifacts.** The stale artifact problem is about
   `target/` caching of generated code, not source drift (the generated
   code lives in `$OUT_DIR` and isn't source-controlled).  Implement:
   - CI runs `cargo clean -p typed-graph` before `cargo test` for the
     delete round-trip tests.  This ensures the generated code always
     matches the current schema files without comparing against a
     non-existent source-controlled copy of generated code.

2. **Wire `scripts/validate-gramps-import.sh` into CI** once a CI
   pipeline is set up.

## Decisions Made

| Topic | Decision |
|-------|----------|
| Note type default | Default to `"General"` when missing |
| Namespace strategy | **Option A**: Version-aware output (emit 5.1 or 5.2 format matching input) |
| Date fidelity | **Structured + raw text**: Store original `val` string + parse year/month/day |
| NoteData.type_field change | **Accept breaking change** (enum→string, already committed) |

## References

- Gramps source: `/usr/lib/python3/dist-packages/gramps/`
  - `plugins/importer/importxml.py` line 1869–1968 (`start_note`)
  - `gen/db/generic.py` line 1956 (`commit_note`)
  - `gen/lib/grampstype.py` (`GrampsType.__str__`, `set_from_xml_str`)
  - `gen/lib/notetype.py` (`NoteType` class, `_DATAMAP`, `_E2IMAP`)
- Our code:
  - `schemas/schema-5.2.json` — Note `type` field: `{"type":"string","required":false}`
  - `schemas/schema-5.1.json` — JSON Schema format, auto-converted at build time
  - `crates/typed-graph/build.rs` — schema merge, code generation
  - `crates/typed-graph/src/schema_convert.rs` — JSON Schema → flat format conversion (NoteType mapping at line 323)
  - `crates/gramps-reader/src/xml/graph.rs` — `NoteBuilder`, `parse_year_from_val`
  - `crates/output/src/xml.rs` — `get_field_value`, `write_date_element`
  - `crates/output/src/serialization_map.rs` — Note `XmlTypeInfo` with `type` attr
  - `crates/cli/src/commands/delete.rs` — delete pipeline
  - `crates/cli/tests/e2e.rs` — `e2e_delete_notes_import_roundtrip`

---

> **Update (Gramps API shift):** Orphaned secondary objects (events, notes,
> places, etc.) are now **intentionally preserved** in the output. The Gramps
> DB API (`delete_person_from_database`) only deletes people and auto-clears
> empty families. All other types survive as orphans. This is correct behavior:
> Gramps itself never garbage-collects orphaned secondary objects, and they
> survive XML export/import without errors.
