# Bug: Missing `type` attribute on `<note>` elements causes Gramps import crash

**Date**: 2026-08-10
**Status**: Plan (ready for implementation)

## Summary

When the `delete` tool writes a filtered `.gramps` file, `<note>` elements are
missing the `type` attribute. This causes Gramps to crash during import with:

```
TypeError: __str__ returned non-string (type int)
```

The root cause is a **two-sided round-trip gap**: neither the parser nor the
serializer handle the `type` attribute on `<note>` elements.

## Root Cause Analysis

### Gramps importxml behavior

Gramps' `start_note` handler (importxml.py line 1899–1900):

```python
self.note.type.set_from_xml_str(attrs.get('type', NoteType.UNKNOWN))
```

When the `type` attribute is **missing** from the XML, the default
`NoteType.UNKNOWN` (`-1`, an **integer**) is passed to `set_from_xml_str()`.
Since `-1` is not a string key in `_E2IMAP`, the method falls through to:

```python
self.__value = self._CUSTOM   # = 0 (int)
self.__string = value         # = -1 (int)
```

Later, `commit_note()` calls `str(note.type)` → `__str__()` returns
`self.__string` which is `-1` (int), violating Python's requirement that
`__str__` must return a `str`.  This raises `TypeError` and crashes the import.

**Gramps version tested**: 5.1.6 (installed). Gramps 5.2.0 is also affected —
the same code path exists in both versions. The bug was reproduced directly via:

```python
from gramps.gen.lib.note import Note
from gramps.gen.lib.notetype import NoteType
n = Note()
n.type.set_from_xml_str(NoteType.UNKNOWN)  # -1
str(n.type)  # TypeError: __str__ returned non-string (type int)
```

### Why the attribute is missing

**Parser side** (`crates/gramps-reader/src/xml/graph.rs`):
When parsing `<note>` elements from XML, only `handle` and `gramps_id` are
extracted. The `type` attribute is never read.

```rust
b"note" if self.state.current_section == Section::Notes => {
    let handle = read_handle_attr(e).unwrap_or_default();
    let gramps_id = read_id_attr(e);
    current_note = Some(NoteBuilder {
        handle,
        gramps_id,
        ..NoteBuilder::default()
    });
    // NOTE: type attribute NOT read
}
```

**Serializer side** (`crates/output/src/serialization_map.rs` + `xml.rs`):
The Note serialization map only declares `handle` and `gramps_id` as
attributes. The `type` attribute is never written.

```rust
type_map.insert("Note", XmlTypeInfo {
    element_name: "note",
    section_name: "notes",
    attributes: vec![
        XmlAttribute { field: "handle", attr_name: "handle" },
        XmlAttribute { field: "gramps_id", attr_name: "id" },
        // NOTE: type attribute NOT listed
    ],
    ...
});
```

Even if added to the map, `get_field_value()` in `xml.rs` does not handle
`type_field` for `Node::Note`.

### Schema note / design decision

The `NoteType` enum in the generated schema contains the merged union of Gramps
5.1 and 5.2 note type constants. **Many Rust variant names do NOT match the XML
attribute values** Gramps uses:

| Rust variant | Gramps XML string |
|---|---|
| `HTMLCODE` | `"Html code"` |
| `REPORTTEXT` | `"Report"` |
| `TODO` | `"To Do"` |
| `SOURCETEXT` | `"Source text"` |
| `LINK` | `"Link"` |
| `UNKNOWN` | `"Unknown"` |
| `LDS` | `"LDS"` (ok) |
| `PERSONNAME` | `"Person Name"` |

Plus duplicate variants (`Citation`/`CITATION`, `Report`/`REPORTTEXT`,
`Research`/`RESEARCH`, `Transcript`/`TRANSCRIPT`) with different casing.

**Decision**: Store note type as `Option<String>` instead of `Option<NoteType>`
in the schema. This ensures lossless XML round-tripping — whatever the input
file says, the output file says the same thing. No enum mapping complexity.

This requires changing the `NoteData` schema field from:

```json
"type": { "type": "enum_ref", "target": "NoteType", "required": false }
```

to:

```json
"type": { "type": "string", "required": false }
```

## Fix Plan

### Step 1: Schema — change `NoteData.type_field` from `enum_ref` to `string`

**File**: `schemas/schema-5.2.json`

Change the Note `type` field from:

```json
"type": { "type": "enum_ref", "target": "NoteType", "required": false }
```

to:

```json
"type": { "type": "string", "required": false }
```

This changes the generated `NoteData.type_field` from `Option<NoteType>` to
`Option<String>`. The `NoteType` enum itself remains in the schema (it is not
removed) for potential future use, but is no longer referenced by `NoteData`.

**Schema-5.1.json**: Do **not** change `schema-5.1.json`. The 5.1 schema uses
integer enum values that are converted to names at build time by
`schema_convert.rs`. Adding a raw `string` field there would conflict with the
converter. The merged schema (when both 5.1 and 5.2 are enabled) already
handles field-type differences by making the field `Option<T>` where T is the
union type; the 5.2 string type will be the effective type after the change.
Verify with `cargo check --all-features` after editing only `schema-5.2.json`.

> **⚠️ Merge verification**: The build script's merge algorithm emits a build
> error when the same field has different types across schema versions. Since
> schema-5.1.json retains `enum_ref` for the `type` field while schema-5.2.json
> changes to `string`, the merge of these two versions may produce a build
> error. Run `cargo check --features schema-5-2` first (single-version) to
> confirm the change works in isolation. Then run `cargo check --all-features`
> (merged 5.1 + 5.2). If the merge fails, one of these fallbacks applies:
>
> 1. Teach the merge algorithm to treat `enum_ref`→`string` as a compatible
>    promotion (store as `String` in the merged type).
> 2. Change both schema files to use `string` (requires updating
>    `schema_convert.rs` to handle the 5.1 Note type field as string too).
> 3. Accept that all-features builds require single-schema `--features`
>    selection for this field.
>
> Do not proceed past Step 1 until `cargo check --all-features` passes.

#### Sub-step 1a: Fix compilation errors in `crates/diff/`

**File**: `crates/diff/src/compare.rs`

`compare_note()` (line ~1350) compares `type_field` using `FieldKind::Enum` and
`format!("{v:?}")`. After the type change, it must use `FieldKind::Text` and
raw string comparison:

```rust
// Before:
field_kind: FieldKind::Enum,
old_value: a.type_field.map(|v| format!("{v:?}")),
new_value: b.type_field.map(|v| format!("{v:?}")),

// After:
field_kind: FieldKind::Text,
old_value: a.type_field.clone(),
new_value: b.type_field.clone(),
```

Update the `make_note()` test helper (line ~2338) and the `note_change_type`
test (line ~2384) to use string values instead of enum variants:

```rust
// Before:
type_field: Some(typed_graph::NoteType::General),
// After:
type_field: Some("General".to_string()),
```

**File**: `crates/diff/src/matcher.rs` — no functional changes needed, but
the `NoteData` construction at line 1012 uses `type_field: None` which is
compatible.

#### Sub-step 1b: Verify other crates compile

- `crates/typed-graph/src/generate/builder.rs` — `NoteBuilder` uses
  `..NoteData::default()` which sets `type_field` to `None`; compatible.
- `crates/typed-graph/src/generate/adversarial.rs` — same pattern; compatible.
- `crates/typed-graph/src/validate.rs` — test uses `..NoteData::default()`;
  compatible.

Run `cargo check --workspace --all-targets` after the schema change and fix
any remaining compilation errors before proceeding.

### Step 2: Add a generic attribute reader to `xml.rs`

**File**: `crates/gramps-reader/src/xml.rs`

Add a new helper function to read an arbitrary named attribute from an XML
element. This follows the existing pattern of `read_handle_attr`,
`read_id_attr`, and `read_hlink_attr`:

```rust
/// Read an arbitrary string attribute from an element.
///
/// Returns `None` when the element has no attribute with the given name
/// (whether namespaced or bare).
pub fn read_attr(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == name || key.ends_with(name) {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}
```

Add a unit test for this function in the existing `mod tests` block at the
bottom of the file:

- `read_attr` with a present attribute returns `Some("value")`
- `read_attr` with a missing attribute returns `None`
- `read_attr` with a namespaced attribute (`ns:type`) also matches

> **Note**: The `key.ends_with(name)` check for `name = b"type"` will match
> any attribute ending in `type` (e.g., a hypothetical `mimetype`). In
> practice Gramps XML has no such conflicting attributes, so this follows
> the existing pattern used by `read_handle_attr` et al. The risk is
> cosmetic.

### Step 3: Parser — read `type` attribute from `<note>` elements

**File**: `crates/gramps-reader/src/xml/graph.rs`

Add a `note_type` field to the `NoteBuilder` struct and read the `type`
attribute during parsing using the new `read_attr` helper. Also add a
`note_format` field for the `format` attribute (safe default of 0 in Gramps,
but parsing it completes the round-trip):

```rust
struct NoteBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    text: String,
    note_type: Option<String>,   // NEW
    note_format: Option<i32>,     // NEW (completeness)
}
```

Update `into_data()` to transfer both fields:

```rust
fn into_data(self) -> NoteData {
    NoteData {
        handle: self.handle,
        gramps_id: self.gramps_id,
        text: self.text,
        type_field: self.note_type,   // direct pass-through
        format: self.note_format,     // direct pass-through
        ..NoteData::default()
    }
}
```

Add `read_attr` to the imports at the top of `graph.rs` (it lives in the
parent `xml.rs` module):

```rust
use super::{read_handle_attr, read_id_attr, read_hlink_attr, read_attr, ...};
```

In the parser, add the attribute reads:

```rust
b"note" if self.state.current_section == Section::Notes => {
    let handle = read_handle_attr(e).unwrap_or_default();
    let gramps_id = read_id_attr(e);
    let note_type = read_attr(e, b"type");
    let note_format = read_attr(e, b"format")
        .and_then(|s| s.parse::<i32>().ok());
    current_note = Some(NoteBuilder {
        handle,
        gramps_id,
        note_type,
        note_format,
        ..NoteBuilder::default()
    });
}
```

Empty `type=""` attributes: `read_attr` returns `Some("".to_string())`. Filter
these to `None` by using `.filter(|s| !s.is_empty())` on the `read_attr` call
result — an empty type string is semantically equivalent to missing.

### Step 4: Serializer — add `type` and `format` to Note's attribute map, remove `format` from children

**File**: `crates/output/src/serialization_map.rs`

Add the `type` and `format` attributes to the Note entry:

```rust
attributes: vec![
    XmlAttribute { field: "handle", attr_name: "handle" },
    XmlAttribute { field: "gramps_id", attr_name: "id" },
    XmlAttribute { field: "type", attr_name: "type" },
    XmlAttribute { field: "format", attr_name: "format" },
],
```

**Remove `format` from the Note children list.** The `format` field is
currently emitted as a child element `<format>…</format>`, but in Gramps
XML it is an attribute on `<note>`, not a child. Remove this entry from
the `children` vec:

```rust
// REMOVE this entry:
XmlChild {
    element_name: "format".to_string(),
    source: XmlChildSource::InlineStruct("format".to_string()),
},
```

Leaving both would cause `format` to be emitted twice (both as an
attribute and as a child element).

### Step 5: Serializer — handle `type_field` in `get_field_value`, clean up dead `format` handlers

**File**: `crates/output/src/xml.rs`

Add `"type"` and `"format"` arms to the `Node::Note` match in
`get_field_value()`:

```rust
Node::Note(n) => match field_name {
    "handle" => Some(n.handle.clone()),
    "gramps_id" => n.gramps_id.clone(),
    "type" => n.type_field.clone(),   // Option<String> — direct pass-through
    "format" => n.format.map(|v| v.to_string()),
    _ => None,
},
```

**Clean up dead `format` inline-struct handlers.** Since `format` is no
longer a child element (removed from the children list in Step 4), remove
its arms from both `has_inline_struct_value` and `write_inline_struct` for
`Node::Note` to prevent future double-emission if someone re-adds `format`
to the children list:

- In `has_inline_struct_value`: remove the `"format" => { … Node::Note(n) … }` arm.
- In `write_inline_struct`: remove the `"format" => { … Node::Note(n) … }` arm.

### Step 6: Write unit tests

**Parser tests** (`crates/gramps-reader/tests/`):

- `type="Research"` → `Some("Research".to_string())`
- Missing `type` attribute → `None`
- `type=""` (empty string) → `None` (filtered)
- `format="1"` → `Some(1)`
- Missing `format` attribute → `None`

**Serializer tests** (`crates/output/src/xml.rs` existing test module):

- `type_field: Some("Research".to_string())` → `type="Research"` on `<note>`
- `type_field: None` → no `type` attribute emitted
- `format: Some(1)` → `format="1"` on `<note>`

**Property-based round-trip test** (in `crates/cli/tests/` or `crates/typed-graph/tests/`):

- Given a set of known Gramps note type strings (`"General"`, `"Research"`,
  `"Transcript"`, `"Citation"`, `"Report"`, `"Html code"`, `"To Do"`,
  `"Source text"`, `"Link"`, `"Unknown"`, `"LDS"`, `"Person Name"`),
  verify that every one round-trips through parse→serialize→parse without
  alteration. This catches enum-mapping regressions and encoding issues.
- Given a set of known `format` integer values (`0`, `1`),
  verify that each round-trips through parse→serialize→parse without
  alteration.
- Verify that `format` appears **only** as an attribute on `<note>`, not
  as a child element (regression test for the double-emission fix from
  Step 4).

**Round-trip test** (in `crates/cli/tests/`):

- Parse a `.gramps` file with notes, serialize it, parse it again, and
  verify note types survive intact.

**E2E test** (in `crates/cli/tests/e2e.rs`, gated on Gramps being installed):

- Run `gramps-gen delete` on a fixture file with notes.
- Verify the output imports cleanly via
  `timeout 15 gramps -y -q -i output.gramps 2>&1`.
- Check that stderr contains no `ERROR:`, `Traceback`, `TypeError`, or
  `Failed to import` patterns.
- **Caveat**: The Gramps CLI may silently handle some import errors
  differently than the GUI. The CLI timeout approach catches the `TypeError`
  in the user's traceback (confirmed Gramps bug), but other validation gaps
  may exist. The key regression test is: does the output file have a
  `type` attribute on every `<note>` element?

### Step 7: Add Gramps import validation script for CI

Use the CLI `timeout` approach only (the Python API alternative is dropped —
it is fragile across Gramps versions and the CLI approach is sufficient).

Create **`scripts/validate-gramps-import.sh`**:

```bash
#!/usr/bin/env bash
set -euo pipefail

INPUT_FILE="${1:-}"
if [ -z "$INPUT_FILE" ]; then
    echo "Usage: $0 <file.gramps>" >&2
    exit 1
fi

TIMEOUT_SECONDS=15
OUTPUT=$(mktemp)
trap 'rm -f "$OUTPUT"' EXIT

# Run gramps import with timeout; capture stdout+stderr
if timeout "$TIMEOUT_SECONDS" gramps -y -q -i "$INPUT_FILE" >"$OUTPUT" 2>&1; then
    # Check for error patterns in the output
    if grep -qE 'ERROR:|Traceback|TypeError|Failed to import' "$OUTPUT"; then
        echo "IMPORT FAILED: errors detected in output" >&2
        cat "$OUTPUT" >&2
        exit 1
    fi
    exit 0
else
    exit_code=$?
    if [ $exit_code -eq 124 ]; then
        # timeout is expected — gramps stays open after import
        # Still check the output for errors
        if grep -qE 'ERROR:|Traceback|TypeError|Failed to import' "$OUTPUT"; then
            echo "IMPORT FAILED: errors detected in output (timed out as expected)" >&2
            cat "$OUTPUT" >&2
            exit 1
        fi
        exit 0
    fi
    echo "IMPORT FAILED: gramps exited with code $exit_code" >&2
    cat "$OUTPUT" >&2
    exit 1
fi
```

1. Takes a `.gramps` file path as its single argument.
2. Runs `gramps -y -q -i` with a 15-second timeout (Gramps stays open after
   import, so `timeout` is required).
3. Scans combined stdout+stderr for `ERROR:`, `Traceback`, `TypeError`, or
   `Failed to import`.
4. Exits 0 on success, 1 on failure.

Add this script to the CI pipeline and gate it on Gramps being installed:

1. Check if a `.github/workflows/` directory exists. If it does, add a
   step to the relevant workflow that runs `scripts/validate-gramps-import.sh`
   on a freshly-generated output file. If no CI pipeline exists yet, defer
   CI integration to a follow-up task — create it only if Gramps is
   available in the CI runner image.

## Impact Assessment

| Component | Change | Risk |
|---|---|---|
| `schemas/schema-5.2.json` | Change `type` field from `enum_ref` to `string` | Medium — schema change; affects codegen |
| `schemas/schema-5.1.json` | **No change** — keep as-is (converter handles integer→name mapping) | None |
| `typed-graph/build.rs` | Regenerate with new schema | Low — automatic |
| `gramps-reader/src/xml.rs` | Add `read_attr` generic attribute helper | Low — new function, follows existing patterns |
| `gramps-reader/src/xml/graph.rs` | Read `type` and `format` attrs as strings | Low — additive |
| `output/src/serialization_map.rs` | Add `type` and `format` attributes to Note; remove `format` from Note children list | Low — small changes |
| `output/src/xml.rs` | Handle `type` and `format` fields in `get_field_value`; remove dead `format` inline-struct handlers for `Node::Note` | Low — simple string pass-through + dead-code removal |
| `diff/src/compare.rs` | Update `compare_note` from `FieldKind::Enum` to `FieldKind::Text`, fix tests. Note: `format` already uses `FieldKind::Numeric` and needs no change. | Medium — logic change, test values change |
| `diff/src/matcher.rs` | Verify compatibility (no functional changes needed) | Low — `type_field: None` is compatible |
| `crates/*/tests/` | New round-trip, property-based, and E2E tests | Low — additive |
| `scripts/validate-gramps-import.sh` | New CI validation script | Low — additive, gated on Gramps install |

**Schema change**: `NoteData.type_field` changes from `Option<NoteType>` to
`Option<String>`. This is a **breaking change** for code that pattern-matches
on `NoteData.type_field`. The `diff` crate is the only downstream consumer that
requires updates (see Step 1a). Other crates use `..NoteData::default()` which
sets `type_field` to `None` regardless of the inner type. The `NoteType` enum
itself remains in the schema for potential future use but is no longer
referenced by `NoteData`.

## Notes

- The `format` attribute on `<note>` elements is included in this fix (Steps
  3–5) for round-trip completeness since it's trivial to add alongside `type`.
  Gramps defaults to `0` (FLOWED) when it's missing, so its absence was
  harmless, but parsing it completes the Note serialization contract.
- The `priv` and `change` attributes on `<note>` (privacy flag and change
  timestamp) are also parsed by Gramps but have safe defaults (False and
  current time). Our simplified data model does not track these.
- Other node types may have similar attribute gaps — a full audit of the
  serialization map against the Gramps RelaxNG schema is a good follow-up
  task.
