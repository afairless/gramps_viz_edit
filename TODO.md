# Implementation Plan: Fix missing `type` attribute on `<note>` elements (Gramps import crash)

Source: [`docs/research/note-type-attribute-roundtrip-bug.md`](docs/research/note-type-attribute-roundtrip-bug.md)

**Problem**: Gramps crashes on import (`TypeError: __str__ returned non-string (type int)`) because `<note>` elements produced by the `delete` tool are missing the `type` attribute. The two-sided round-trip gap is in **both** the parser (`gramps-reader`) and the serializer (`output`).

**Strategy**: Round-trip fix through the whole pipeline — change the schema field type to a lossless `String` (no enum-mapping), then fix parser → serializer → diff consumer, then lock it in with unit, property-based, round-trip, and E2E tests.

## Impact summary

| Component | Change |
|---|---|
| `schemas/schema-5.2.json` | `NoteData.type` field: `enum_ref` → `string` (breaking) |
| `schemas/schema-5.1.json` | **No change** (build-time converter keeps integer→name mapping) |
| `crates/diff/src/compare.rs` | `compare_note` `FieldKind::Enum` → `FieldKind::Text`; update `make_note`/`note_change_type` tests |
| `crates/gramps-reader/src/xml.rs` | Add generic `read_attr` helper |
| `crates/gramps-reader/src/xml/graph.rs` | Parse `type` and `format` attrs on `<note>` |
| `crates/output/src/serialization_map.rs` | Add `type`/`format` attributes; remove `format` from Note children |
| `crates/output/src/xml.rs` | Handle `type`/`format` in `get_field_value`; remove dead `format` inline-struct handlers |
| `scripts/validate-gramps-import.sh` | New CI-validation script (gated on Gramps; CI deferred — no `.github/workflows/`) |

> **Design decision**: Store note type as `Option<String>` rather than `Option<NoteType>`. The `NoteType` enum variant names do not match Gramps XML string values, so enum mapping would break round-tripping. `String` makes the output byte-for-byte faithful to the input. The `NoteType` enum stays in the schema (unused by `NoteData`) for potential future use.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix(schema): change NoteData type field from enum to string` | Schema change + diff consumer ripple | `schemas/schema-5.2.json`, `crates/diff/src/compare.rs` | Unit, smoke |
| 2 | `feat(gramps-reader): add generic read_attr XML attribute helper` | Generic attribute reader | `crates/gramps-reader/src/xml.rs` | Unit |
| 3 | `fix(gramps-reader): parse note type and format attributes` | Note builder + parser | `crates/gramps-reader/src/xml/graph.rs` | Unit, integration |
| 4 | `fix(output): add type/format attributes to note, drop format child` | Serialization map | `crates/output/src/serialization_map.rs` | Unit |
| 5 | `fix(output): serialize note type/format fields, remove dead handlers` | Field-value writer | `crates/output/src/xml.rs` | Unit |
| 6 | `test: add note type/format round-trip and property tests` | Round-trip coverage | `crates/typed-graph/tests/`, `crates/cli/tests/` | Unit, property |
| 7 | `test: add gramps import E2E with fixture notes` | Gramps-gated E2E | `crates/cli/tests/e2e.rs` | Integration (E2E) |
| 8 | `chore: add CI gramps import validation script` | CI validation | `scripts/validate-gramps-import.sh` | — |

## Step details

### Step 1 — Schema change + downstream consumer ripple

**`schemas/schema-5.2.json`** — change Note `type` field (under `primary_types.Note.fields.type`):

```json
// Before
"type": { "type": "enum_ref", "target": "NoteType", "required": false }
// After
"type": { "type": "string", "required": false }
```

Generated `NoteData.type_field` becomes `Option<String>`. `NoteType` enum remains in the schema.

**Do not** touch `schemas/schema-5.1.json` (build-time converter maps integer→name; a raw `string` field would conflict).

**Verify merge**: run `cargo check --features schema-5-2` (single-version) first, then `cargo check --all-features` (merged 5.1+5.2). The `diff` crate is the only typed consumer of `NoteData.type_field`. If the all-features merge emits a build error (5.1 `enum_ref` vs 5.2 `string` differ), apply a fallback:

1. Merge treats `enum_ref`→`string` as a compatible promotion (store as `String`).
2. Change both schema files to `string` (updates `schema_convert.rs`).
3. Accept single-schema `--features` selection for this field.

Do not proceed until `cargo check --all-features` passes.

**`crates/diff/src/compare.rs`** — `compare_note()` (line ~1350): change `FieldKind::Enum` + `format!("{v:?}")` to `FieldKind::Text` + raw clone:

```rust
// After
field_kind: FieldKind::Text,
old_value: a.type_field.clone(),
new_value: b.type_field.clone(),
```

Update `make_note()` test helper (line ~2338) and `note_change_type` test (line ~2384): `NoteType::General`/`Research` → `"General".to_string()`/`"Research".to_string()`.

`crates/diff/src/matcher.rs` `NoteData { type_field: None, .. }` (line ~1012) is compatible — no change.

**Verify**: `cargo check --workspace --all-targets` compiles. **Acceptance**: merged `--all-features` build passes.

### Step 2 — Generic `read_attr` attribute reader

**`crates/gramps-reader/src/xml.rs`** — add, following the existing `read_handle_attr`/`read_id_attr`/`read_hlink_attr` pattern:

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

**Tests** (existing `mod tests` in xml.rs): present → `Some("value")`; missing → `None`; namespaced (`ns:type`) → matches.

> Note: `ends_with(b"type")` also matches a hypothetical `mimetype`; Gramps XML has none, so this matches the existing helper pattern.

### Step 3 — Parser reads `type` and `format` on `<note>`

**`crates/gramps-reader/src/xml/graph.rs`**:

- Add fields to `NoteBuilder`: `note_type: Option<String>`, `note_format: Option<i32>`.
- `into_data()`: pass through both into `NoteData { type_field, format, .. }`.
- Import `read_attr` alongside `read_handle_attr`/`read_id_attr`/`read_hlink_attr`.
- In the `b"note"` parser arm (line ~327), read both attributes; **filter empty `""` to `None`** (semantically equivalent to missing):

```rust
let note_type = read_attr(e, b"type").filter(|s| !s.is_empty());
let note_format = read_attr(e, b"format").and_then(|s| s.parse::<i32>().ok());
```

**Tests** (in `crates/gramps-reader/tests/`): `type="Research"` → `Some("Research")`; missing `type` → `None`; `type=""` → `None`; `format="1"` → `Some(1)`; missing `format` → `None`.

### Step 4 — Serializer attribute map: add `type`/`format`, remove `format` child

**`crates/output/src/serialization_map.rs`** Note `XmlTypeInfo`:

- Add to `attributes`: `{ field: "type", attr_name: "type" }`, `{ field: "format", attr_name: "format" }`.
- Remove the `format` child entry (`XmlChildSource::InlineStruct("format")`) from `children` so `format` is not emitted twice.

**Tests**: mapping test asserting the Note attributes include `type`/`format` and the `format` child is gone.

### Step 5 — Serializer field writer + dead-code removal

**`crates/output/src/xml.rs`**:

- `get_field_value()` `Node::Note` arm: add `"type" => n.type_field.clone()` and `"format" => n.format.map(|v| v.to_string())`.
- Remove the `"format"` arm for `Node::Note` in both `has_inline_struct_value` (line ~433) and `write_inline_struct` (line ~661) — dead after Step 4, prevents double-emission if re-added later.

**Tests** (existing xml.rs test module): `type_field: Some("Research")` → `type="Research"` on `<note>`; `None` → no `type` attr; `format: Some(1)` → `format="1"`.

### Step 6 — Round-trip + property-based regression tests

**Property-based round-trip** (in `crates/typed-graph/tests/` or `crates/cli/tests/`):

- Known note-type strings (`"General"`, `"Research"`, `"Transcript"`, `"Citation"`, `"Report"`, `"Html code"`, `"To Do"`, `"Source text"`, `"Link"`, `"Unknown"`, `"LDS"`, `"Person Name"`) each survive parse→serialize→parse unaltered.
- Known `format` values (`0`, `1`) survive the same round-trip.
- Assert `format` appears only as an attribute on `<note>`, never a child element (regression for Step 4).

**Round-trip test** (`crates/cli/tests/`): parse a `.gramps` file with notes → serialize → parse → note types intact.

### Step 7 — Gramps-gated E2E

**`crates/cli/tests/e2e.rs`** (gated on Gramps being installed): run `gramps-gen delete` on a fixture with notes → import output via `timeout 15 gramps -y -q -i output.gramps 2>&1` → assert no `ERROR:`, `Traceback`, `TypeError`, or `Failed to import`. Key regression assertion: every `<note>` element carries a `type` attribute.

### Step 8 — CI validation script

**`scripts/validate-gramps-import.sh`** — takes a `.gramps` file, runs `gramps -y -q -i` under a 15s timeout, scans combined stdio for `ERROR:|Traceback|TypeError|Failed to import`, exit 0 on success / 1 on failure (treating timeout-124 as success since Gramps stays open). Add to CI only if `.github/workflows/` exists and Gramps is in the runner image — **deferred** here (no workflow directory currently present).

## Test commands

```bash
cargo check --features schema-5-2          # Step 1 single-version
cargo check --all-features                  # Step 1 merged
cargo check --workspace --all-targets      # Step 1 ripple
cargo test -p gramps-reader                 # Steps 2–3
cargo test -p output                        # Steps 4–5
cargo test -p cli                           # Steps 6–7
cargo test -p typed-graph                   # Step 6 property tests
cargo clippy --all-targets --all-features -- -D warnings   # final
```
