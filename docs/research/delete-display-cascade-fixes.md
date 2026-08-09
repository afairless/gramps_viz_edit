# Delete Tool: Display & Cascade Fix Plan

## Summary

The delete tool has four issues. Each has a specific root cause.

---

## Root Cause Analysis

### Issue 1 — Family items show "no parents"

**Symptom:** Every family candidate lists `no parents`, even when parents exist.

**Root cause:** `describe_node()` in `crates/delete/src/review.rs` reads
`data.father_handle` and `data.mother_handle` from the `FamilyData` struct.
The XML parser (`crates/gramps-reader/src/xml/graph.rs`) never populates
these fields — it creates `Edge::FamilyFather` / `Edge::FamilyMother` edges
instead.  The `FamilyBuilder::into_data()` returns `FamilyData` with
`..FamilyData::default()` which leaves `father_handle` and `mother_handle`
as `None`.

**Fix location:** `review.rs` — `describe_node` for `Node::Family`.

**Fix approach:** Use edge traversal instead of data fields for all family
members:

- Look at edges FROM the family handle that are `FamilyFather` → get the target Person's name.
- Same for `FamilyMother`.
- Use `FamilyChildRef` edges (FROM the family, TO each child). The
  parser creates these edges but `FamilyBuilder::into_data()` leaves
  `child_ref_list` empty, so `describe_node` currently sees zero children.
  This means child names are also broken, not just parent names.

---

### Issue 2 — Marriage/Degree events don't show associated people

**Symptom:** Birth, death, burial, and census events sometimes list people names,
but marriage and degree events never do.

**Root cause:** `describe_node()` for `Node::Event` searches only
`Edge::PersonEventRef` edges (Person → Event).  Marriage events are typically
connected via `Edge::FamilyEventRef` (Family → Event).  The code does not
check `FamilyEventRef` edges at all.

**Fix location:** `review.rs` — `describe_node` for `Node::Event`.

**Fix approach:** Also check `Edge::FamilyEventRef` edges.  When found, resolve
the family's parents (father/mother) via `FamilyFather`/`FamilyMother` edges
and include their names in the description.

---

### Issue 3 — Citations always show "no source, p. no page"

**Symptom:** Every citation displays `Citation → no source, p. no page`.

**Root cause:** Two separate bugs:

1. **Parser missing `<sourceref>` handler.** The XML `<sourceref hlink="...">`
   element inside `<citation>` is never parsed.  Neither `source_handle` on
   the `CitationBuilder` nor an `Edge::CitationSource` edge is ever created.
   The `CitationBuilder` already has a `source_handle: Option<Handle>` field
   ready to receive the value.

2. **Confirmed: `<page>` uses text content, not an attribute.** Gramps writes
   `<page>text</page>` (text content) via `write_line("page", citation.get_page(), ...)`.
   The parser's text-reading handler should work.  The "no page" display is
   likely because the citations in question genuinely have empty page fields.
   No parser fix needed for `<page>` — only `<sourceref>` is missing.

**Fix location:** `crates/gramps-reader/src/xml/graph.rs` (parser) and
`review.rs` (display as a robustness fallback).

**Fix approach:**

- Add `<sourceref hlink="...">` handling in the parser (both `Start` and
  `Empty` events) inside `<citation>` context — create an
  `Edge::CitationSource` edge and set the builder's `source_handle`.
- In `describe_node`, use both the data field AND the edge for robustness.

---

### Issue 4 — No places appear for deletion

**Symptom:** The cascade reports zero places to delete even when selected
people's events reference places that would become unreferenced.

**Root cause:** Two bugs:

1. **Parser missing `<place hlink="...">` handler inside `<event>`.** The XML
   element `<place hlink="pl0001"/>` inside `<event>` is never parsed.  No
   `Edge::EventPlace` edge is created.  The `EventBuilder` already has a
   `place_handle: Option<Handle>` field but it is never set.

2. **Cascade engine missing edge checks for Place.** The
   `type_specific_orphan_rule` for `Node::Place` only checks `Edge::EventPlace`
   and `Edge::PlacePlaceRef`.  Places can also be kept alive by
   `Edge::PlaceCitation` (Place → Citation), and those citations may be
   deleted.  The cascade should check all outgoing edges from a place
   (`PlaceCitation`, `PlaceMediaRef`, `PlaceNote`, `PlaceTag`) when
   determining orphan status.

**Fix location:**

- `crates/gramps-reader/src/xml/graph.rs` — add `<place hlink="...">` parsing
- `crates/delete/src/cascade.rs` — extend place orphan rule

**Fix approach:**

- Parser: handle `<place hlink="h">` inside `<event>` (both `Start` and
  `Empty` events), creating `Edge::EventPlace` and setting `place_handle`.
- Cascade: add `PlaceCitation`, `PlaceMediaRef`, `PlaceNote`, `PlaceTag`
  edge checks to the `Node::Place` orphan rule for correctness.

---

## Implementation Plan

### Step 1 — Parser: add `<sourceref>` handling

**File:** `crates/gramps-reader/src/xml/graph.rs`

- Add `b"sourceref"` match arm for `Start` and `Empty` events inside
  citation context.
- Extract `hlink` attribute via `read_hlink_attr(e)`.
- On `citationref hlink="h"` inside a `<citation>`, create:
  - `Edge::CitationSource { source: citation.handle, target: h }`
  - Set `current_citation.source_handle = Some(h)`.

### Step 2 — Parser: add `<place hlink="...">` handling inside `<event>`

**File:** `crates/gramps-reader/src/xml/graph.rs`

- Add `b"place"` match arm for `Start` and `Empty` events inside event
  context (not the `b"place" save="Section::Places">` section start).
- Extract `hlink` attribute via `read_hlink_attr(e)`.
- Create `Edge::EventPlace { source: event.handle, target: h }`.
- Set `current_event.place_handle = Some(h)`.

### Step 3 — Fix `describe_node` for Family

**File:** `crates/delete/src/review.rs`

- Replace `data.father_handle.as_ref()` / `data.mother_handle.as_ref()` with
  edge traversal:
  - Iterate `graph.edges_incident_to(handle)`, filter for `FamilyFather`,
    get target handle, resolve Person name.
  - Same for `FamilyMother`.
  - Also use edge traversal for children: filter for `FamilyChildRef`
    edges FROM the family handle, get target handles, resolve Person names.

### Step 4 — Fix `describe_node` for Event

**File:** `crates/delete/src/review.rs`

- In addition to checking `Edge::PersonEventRef`, also check
  `Edge::FamilyEventRef` edges (Family → Event).
- For each `FamilyEventRef`, get the Family node and resolve its parent
  names via `FamilyFather`/`FamilyMother` edges, then include those names.
- Show up to 3 people total across PersonEventRef and FamilyEventRef sources.

### Step 5 — Fix `describe_node` for Citation

**File:** `crates/delete/src/review.rs`

- In addition to `get_source_handle(&data.source_handle)` (data field),
  also check `Edge::CitationSource` edges from the citation.
- When the edge is used, resolve the **target** handle (the Source node)
  via `graph.get_node()` and apply the same display formatting already used
  for the data-field path (gramps_id + title).
- Use whichever source-of-truth (data field or edge) gives a result.

### Step 6 — Fix cascade for Place

**File:** `crates/delete/src/cascade.rs`

- Extend `Node::Place` orphan rule to also check:
  - `Edge::PlaceCitation` (source=Place, target=Citation)
  - `Edge::PlaceMediaRef` (source=Place, target=Media)
  - `Edge::PlaceNote` (source=Place, target=Note)
  - `Edge::PlaceTag` (source=Place, target=Tag)
- A place is orphaned if ALL of these edge types have their non-place
  endpoint in the deletion set.

### Step 7 — Add/update tests

| Test file | What to test |
|---|---|
| `crates/delete/src/review.rs` | **Update `family_graph` helper** to create `FamilyFather`/`FamilyMother`/`FamilyChildRef` edges alongside data fields (existing tests currently use only data fields and will break after Step 3 switches to edge traversal) |
| | All existing family describe tests updated and continue to pass |
| | Family with parents and children → correct display string |
| | Event with FamilyEventRef (marriage) → shows parent names |
| | Citation with CitationSource edge → shows source title and gramps_id |
| | Citation with no source → shows "no source" |
| `crates/delete/src/cascade.rs` | Place with EventPlace edge → cascades when event deleted |
| | Place with PlaceCitation → cascades when citation deleted |
| `crates/gramps-reader/src/xml/graph.rs` | Parse `<sourceref hlink="..."/>` → CitationSource edge created |
| | Parse `<place hlink="..."/>` inside `<event>` → EventPlace edge created |

### Step 8 — Update documentation

**File:** `docs/delete-tool.md`

- Update the Place row in the **Per-Type Orphan Rules** table to match the
  corrected cascade logic.  The current text mentions `LdsOrdPlace` which
  does not exist in the code; replace with the actual keeping-alive edge
  types: `EventPlace`, `PlacePlaceRef`, `PlaceCitation`, `PlaceMediaRef`,
  `PlaceNote`, `PlaceTag`.

### Step 9 — Build, lint, and verify

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Edge Cases & Risks

1. **Family with father only / mother only / no children.**
   The display should handle each case gracefully.  Existing tests
   (`describe_family_only_father`, etc.) currently use data fields on
   `FamilyData`; after Step 3 switches to edge traversal the
   `family_graph` helper must be updated to also create the
   corresponding edges.  See Step 7.

2. **Events with both PersonEventRef and FamilyEventRef.**
   Should display both person names and family parent names, up to the 3-name
   limit.  The display format for mixed sources is intentionally simple
   (all names comma-separated) since this is a review/debugging view, not
   a public API.

3. **Citation with source_handle field but no CitationSource edge.**
   The display code should try both sources of truth.

4. **Citation with CitationSource edge but empty source_handle field.**
   Same fallback logic.

5. **Performance.** The `describe_node` function is called during review
   display, not in a hot loop.  Edge traversal is O(n) in the incident edges,
   which is fine for review.

6. **Schema version compatibility.**  All changes use edges (which are
   version-agnostic) rather than data fields.  No new schema feature gating
   is needed.

---

## Dependency Order

```
Step 1 (parser: sourceref)  ─┐
Step 2 (parser: place)      ─┤
                              ├── Step 3-5 (display fixes)
                              ├── Step 6 (cascade fix)
                              └── Step 7 (tests)
                                       │
                                  Step 8 (docs)
                                       │
                                  Step 9 (verify)
```

Steps 1–2 and 3–5 can be done in any relative order, but the display tests
(Step 7) depend on Step 1–2 being done first to produce the edges being
verified.  Step 6 (cascade fix) also depends on Step 2 — the parser must
produce `EventPlace` edges before the cascade can detect place orphans
end-to-end, though the cascade fix itself is testable independently.
