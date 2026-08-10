# Implementation Plan: Delete Tool Output Round-Trip Fidelity

Source: [`docs/research/delete-output-roundtrip.md`](docs/research/delete-output-roundtrip.md)

**Problem**: `gramps-gen delete` output is not byte-faithful to its input.
Four classes of fidelity gaps remain after the already-committed note-type
fix (76ed57e…c098134): (1) no crash-prevention default when a `<note>` has no
`type` on input, (2) dates are truncated to year-only (with a **year-stored-
in-`day`** bug), (3) the output mixes a 5.1 namespace with 5.2-style content
(e.g. nested `<eventtype>`), and (4) the `change` attribute is dropped
(deliberate, needs documenting).

**Strategy**: Defense-in-depth for the note-type default → fix date
truncation (structured + raw text) → make output version-aware so content
format matches the preserved namespace → expand round-trip/E2E coverage →
harden CI against stale build artifacts.

> Prior note-type plan (committed): see `git log 76ed57e..c098134`. This
> plan is a fresh sequence for the delete-output-roundtrip research doc.

## Impact summary

| Phase | Component | Change |
|---|---|---|
| 1 — Crash-free output | `crates/output/src/xml.rs` | `get_field_value` Note arm defaults `type` → `"General"` |
| 1 — Crash-free output | `crates/typed-graph/src/generate/builder.rs` | `NoteBuilder::build` defaults `type_field` → `"General"` |
| 2 — Date fidelity | `crates/gramps-reader/src/xml/graph.rs` | `parse_year_from_val` → `parse_date_from_val` (year/month/day + raw text) |
| 2 — Date fidelity | `crates/output/src/xml.rs` | `write_date_element` serializes `text` when present |
| 3 — Version-aware | `crates/output/src/xml.rs` | `gramps_xml_version` field; `namespace_to_version()`; 5.1-flat vs 5.2-nested event types |
| 3 — Version-aware | `crates/cli/src/commands/delete.rs` | use input version instead of hardcoded `"5.2"` |
| 4 — Tests | `crates/*/tests/` | 5.1 fixture round-trip, note default/format, import golden |
| 5 — CI | `.github/workflows/` (deferred), `scripts/` | stale-artifact guard, wire `validate-gramps-import.sh` |

> **Design decisions** (from source doc): default note type `"General"`;
> Option A version-aware output; structured + raw-text date fidelity; keep
> `NoteData.type_field` as `Option<String>` (already merged).

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix(output): default missing note type to "General" at serialization` | Serialization default | `crates/output/src/xml.rs` | Unit |
| 2 | `fix(typed-graph): default note type to "General" in builder` | Builder default | `crates/typed-graph/src/generate/builder.rs` | Unit |
| 3 | `fix(gramps-reader): parse full date into year/month/day/raw text` | Date parser | `crates/gramps-reader/src/xml/graph.rs` | Unit |
| 4 | `fix(output): serialize original date text when present` | Date serializer | `crates/output/src/xml.rs` | Unit |
| 5 | `docs(output): document intentional change attribute omission` | Doc contract | `crates/output/src/serialization_map.rs` | — |
| 6 | `test: add date round-trip and property-based fidelity tests` | Date round-trip coverage | `crates/gramps-reader/tests/`, `crates/output/tests/` | Unit, property |
| 7 | `feat(output): derive XML format version from namespace` | Version derivation | `crates/output/src/xml.rs` | Unit |
| 8 | `feat(output): version-aware event type serialization (5.1 flat / 5.2 nested)` | Format switch-points | `crates/output/src/xml.rs` | Unit |
| 9 | `fix(delete): use input Gramps version for output XML format` | Delete wiring | `crates/cli/src/commands/delete.rs` | Integration |
| 10 | `test: add Gramps 5.1 fixture round-trip import test` | 5.1 E2E | `crates/cli/tests/e2e.rs` | Integration (E2E) |
| 11 | `test: note default type and format attribute round-trip` | Note fidelity tests | `crates/cli/tests/`, `crates/output/tests/` | Unit, integration |
| 12 | `chore: add Gramps import golden test with committed fixture` | Import golden | `crates/cli/tests/e2e.rs`, fixture | Integration (E2E) |
| 13 | `chore: harden CI against stale build artifacts` | CI guardrail | `.github/workflows/` (if created), `scripts/` | — |

> After steps 1 and 2, run `cargo clean -p typed-graph && cargo test` once to
> confirm the freshly-generated `NoteData` (with `type_field: Option<String>`)
> — stale `$OUT_DIR` artifacts can mask the note-type fix.

## Step details

### Step 1 — Serialization default for missing note type

**`crates/output/src/xml.rs`** — in `get_field_value`'s `Node::Note` arm,
change the `"type"` arm so a `None` `type_field` emits `"General"`:

```rust
"type" => Some(n.type_field.clone().unwrap_or_else(|| "General".to_string())),
```

This is the **primary** guard ensuring Gramps never receives a `<note>`
without a `type` attribute on the delete round-trip path.

**Tests** (existing xml.rs test module): `type_field: Some("Research")` →
`type="Research"`; `type_field: None` → `type="General"` (regression: no
bare `<note>` ever emitted).

### Step 2 — Builder default for note type (generation path only)

**`crates/typed-graph/src/generate/builder.rs`** — in `NoteBuilder::build()`
(and/or the `GraphBuilder::add_note` initializer), set `type_field` to
`Some("General".to_string())` when the caller left it `None`. Do **not**
change the generated `NoteData::default()` — keep that `None` for all other
consumers (build.rs untouched).

**Tests**: `add_note().with_text("x").build()` yields a note with
`type_field == Some("General")`; explicit `.with_type("Research")` (if such
a setter exists / add one) is preserved.

### Step 3 — Full date parsing (fixes year-as-day + truncation)

**`crates/gramps-reader/src/xml/graph.rs`**:

- Replace `parse_year_from_val` with `parse_date_from_val` returning a small
  `ParsedDate { year: i32, month: Option<i32>, day: Option<i32>, raw_text: Option<String> }`.
  Split the `val` string on `-` for year/month/day; capture the entire raw
  `val` as `raw_text` (reuse the existing `read_dateval_val` helper).
- Update the `b"dateval"` arm (~line 1250) to build
  `DateValue { quality: None, modifier: None, day: parsed.day, month: parsed.month, year: parsed.year, text: parsed.raw_text }`.
  This removes the `day: Some(year as i32)` bug and preserves month/day/text.

**Tests** (xml.rs/graph.rs unit tests): `"1868-09-20"` → `{year:1868,
month:Some(9), day:Some(20)}`; `"1868"` → `{year:1868, month:None, day:None}`;
`"1868-09"` → day `None`; a modifier/range `val` string preserved verbatim in
`text`.

### Step 4 — Serialize original date text when present

**`crates/output/src/xml.rs`** — in `write_date_element`, when `d.text` is
`Some` and non-empty, use it directly as the `val` attribute instead of
reconstructing from year/month/day:

```rust
let date_str = match &d.text {
    Some(t) if !t.is_empty() => t.clone(),
    _ => { /* existing year/month/day reconstruction */ }
};
```

**Tests**: a `DateValue` with `text` set round-trips to the exact `val`
string; a `DateValue` constructed via `new_ymd` (with `text: None`) still
emits `YYYY-MM-DD`.

### Step 5 — Document intentional `change` attribute omission

**`crates/output/src/serialization_map.rs`** — add a doc/`//` comment at the
`type_map` definition (near the Note/Primary entries) explaining that the
`change` timestamp attribute is **deliberately** excluded from the
serialization map on all primary types because Gramps auto-generates the
timestamp on import. Prevents future devs treating it as a bug.

No tests. (`—`)

### Step 6 — Date round-trip + property-based tests

Add round-trip tests across `gramps-reader` and `output`:

- `"1868-09-20"` survives parse→serialize→parse unchanged (the original
  Bug 2 regression).
- Year-only, year-month, full YMD, and modifier/range/spans ("before
  ...", ranges) each survive round-trip.
- **Property-based** invariant for all valid `DateValue` inputs:
  `parse_xml(serialize_xml(date))` yields an equivalent `DateValue` —
  catches leap days, month boundaries, and modifier text single-example
  tests miss.

### Step 7 — Derive XML format version from namespace

**`crates/output/src/xml.rs`**:

- Add `namespace_to_version(ns)` helper reading the namespace URI
  (`.../xml/1.7.1/` → `"5.1"`, `.../xml/1.7.2/` → `"5.2"`).
- Add a `gramps_xml_version: String` field to `GraphXmlWriter` (distinct
  from the schema `gramps_version` used for the header).
- Have `with_namespace` derive and store the format version from the passed
  namespace (internal consistency with the preserved namespace).

**Tests**: `namespace_to_version` maps known URIs correctly and falls back
safely for unknown ones.

### Step 8 — Version-aware event-type serialization

**`crates/output/src/xml.rs`** — audit all format switch points where 5.2
nesting is hardcoded (document findings in a code comment):

- Event type: currently `<eventtype><type>Birth</type></eventtype>` →
  for `gramps_xml_version == "5.1"` emit flat `<type>Birth</type>`.
- Ensure the `<created>` header uses `version` matching the derived format.

For each switch point emit the 5.1-flat or 5.2-nested form per
`gramps_xml_version`. **Tests**: event type serializes flat for 5.1, nested
for 5.2.

### Step 9 — Delete uses input version

**`crates/cli/src/commands/delete.rs`** — replace the hardcoded
`let gramps_version = "5.2";` with the version derived from the parsed
`namespace` (via `namespace_to_version`), so the writer's content format and
header match the input. **Tests**: integration test that delete output for a
5.1-namespace input emits 5.1-flat event types.

### Step 10 — Gramps 5.1 fixture round-trip import test

**`crates/cli/tests/e2e.rs`** — add a gated test: a real (or representative
synthetic) 5.1-namespace/5.1-format `.gramps` fixture → `gramps-gen delete`
→ `timeout 15 gramps -y -q -i output.gramps` → assert no `ERROR:`,
`Traceback`, `TypeError`, or `Failed to import`. This covers the user's
actual 5.1 input path that the existing 5.2-only note test misses.

### Step 11 — Note default-type and `format` attribute tests

- Assert a `<note>` with **no** `type` in input receives `type="General"` in
  output (lock in Step 1).
- Assert the `format` attribute (e.g. `format="1"` HTML notes) survives the
  round-trip — fidelity gap (not crash) regression for the Note `XmlTypeInfo`
  added in the prior plan.

### Step 12 — Gramps import golden test

Add a committed fixture of the smallest possible real Gramps export;
test: parse → serialize → `gramps -y -q -i` → no errors on stdout/stderr.
Guard on Gramps availability next to the other gated E2E tests.

### Step 13 — CI stale-artifact hardening

- CI runs `cargo clean -p typed-graph` before the delete round-trip tests so
  generated `$OUT_DIR` code always matches the current schema files.
- Wire `scripts/validate-gramps-import.sh` into CI **only if** `.github/
  workflows/` exists and Gramps is in the runner image — **deferred** here
  (no workflow directory present).

## Repository health / pre-work

Before implementing, satisfy the incremental-development pre-work gate:
`git status` clean, `cargo test --workspace` green, `git branch --show-current`
on `main`. Create a feature branch (e.g. `agent/delete-output-roundtrip`) per
git-workflow before the first step commit.

## Test commands

```bash
cargo test -p output                                   # Steps 1, 4, 7, 8, 11
cargo test -p typed-graph                              # Step 2
cargo test -p gramps-reader                            # Steps 3, 6
cargo test -p cli                                      # Steps 9-12
cargo test --workspace                                 # full suite before each commit
cargo clippy --all-targets --all-features -- -D warnings   # final gate
cargo clean -p typed-graph && cargo test               # stale-artifact verification
```
