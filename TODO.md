# Implementation Plan: Fix All Nodes Gray — Event-Reference Resolution

Source: `docs/research/fix-gray-nodes-event-reference-resolution.md`

## Overview

The `gramps-reader` crate's `extract_persons()` only handles inline `<birth>`/`<death>` elements inside `<person>`, but `gramps-gen` output (and standard Gramps 5.x format) uses an event-reference pattern where birth/death dates live in separate `<event>` elements referenced via `<eventref hlink="...">`. This causes all `birth_year` values to be `None`, resulting in every node rendering gray.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat(gramps-reader): add ParsedEvent type and event_refs field` | Types | `crates/gramps-reader/src/types.rs` | Unit |
| 2 | `feat(gramps-reader): add extract_events parser for <event> elements` | Event extraction | `crates/gramps-reader/src/xml/extract.rs` | Unit |
| 3 | `feat(gramps-reader): capture <eventref> hlinks in extract_persons` | Eventref capture | `crates/gramps-reader/src/xml/extract.rs` | Unit |
| 4 | `feat(gramps-reader): add resolve_event_refs and re-export` | Resolution + public API | `crates/gramps-reader/src/xml/extract.rs`, `crates/gramps-reader/src/lib.rs` | Unit |
| 5 | `feat(visualize): integrate event resolution in load_graph_data` | Pipeline integration | `crates/visualize/src/lib.rs` | Unit, Integration |

## Step Details

### Step 1 — Types

**Deliverable:** Add `ParsedEvent` struct (with `handle`, `event_type`, `date_val`, `date_year`) and add `event_refs: Vec<String>` to `ParsedPerson`.

**Design notes:**

- `ParsedEvent` derives `Debug, Clone, PartialEq, Default`
- `event_refs` defaults to empty (`Vec::default()`), fully backward-compatible
- Both changes are in `crates/gramps-reader/src/types.rs`

**Tests:**

- `parsed_event_default` — verify `ParsedEvent::default()` has all fields `None`/empty
- `parsed_event_full` — verify all fields settable
- `parsed_person_event_refs_default` — verify `ParsedPerson::default().event_refs` is empty
- `parsed_person_event_refs_populated` — verify `event_refs` holds values

### Step 2 — Event extraction

**Deliverable:** Add `extract_events(content: &str) -> Result<Vec<ParsedEvent>, Error>` in `xml/extract.rs`.

**Design notes:**

- Same streaming pattern as `extract_persons`/`extract_families` — global-scan for `<event>` elements
- On `<event handle="...">` → begin new `ParsedEvent`
- On `<eventtype>` start → begin tracking type
- On `<type>` text inside eventtype → capture via `reader.read_text()`, set `event_type`
- On `<dateval val="..."/>` → set `date_val` and `date_year` via existing `read_dateval_val`/`parse_year_from_val` helpers
- On `</event>` → push completed event
- Uses existing `Error::XmlParseError` for error handling

**Tests:**

- `extract_event_birth` — parse Birth event with dateval
- `extract_event_death` — parse Death event with dateval
- `extract_event_marriage` — parse Marriage event
- `extract_event_no_dateval` — event without date
- `extract_event_self_closing_dateval` — self-closing `<dateval/>`
- `extract_events_multiple` — multiple events in order
- `extract_events_empty` — empty content
- `extract_events_no_events_section` — XML with no events
- `extract_events_namespace_prefixed` — namespace-prefixed elements

### Step 3 — Eventref capture

**Deliverable:** Extend `extract_persons()` to capture `<eventref hlink="...">` elements inside `<person>`.

**Design notes:**

- On `Start`/`Empty` `<eventref>` → read `hlink` attr via `read_hlink_attr`, push to `current.event_refs`
- `<role>` child element is ignored (determine Birth vs Death from event type, not role)
- Backward compatible: inline `<birth>`/`<death>` elements still work, eventrefs add to what inline provides

**Tests:**

- `extract_person_with_eventrefs` — person with eventref hlinks → event_refs populated
- `extract_person_mixed_inline_and_eventrefs` — both inline birth and eventrefs
- `extract_person_eventref_no_hlink` — eventref without hlink attribute
- `extract_person_eventref_namespace_prefixed` — namespace-prefixed eventref
- `extract_person_eventref_inline_birth_still_works` — inline birth parsing unaffected

### Step 4 — Resolution + public API

**Deliverable:** Add `resolve_event_refs(persons: &mut [ParsedPerson], events: &[ParsedEvent])` function and re-export `extract_events`, `ParsedEvent`, `resolve_event_refs` from `lib.rs`.

**Design notes:**

- Builds a `HashMap<&str, &ParsedEvent>` keyed by event handle
- For each person, iterates `event_refs`, looks up event by handle
- If event type is "Birth" and `birth_year` is `None` → populate `birth_date`, `birth_year`
- If event type is "Death" and `death_date` is `None` → populate `death_date`
- Does NOT overwrite already-populated fields (inline takes precedence)
- Unknown event types (e.g., "Marriage") are silently skipped
- Missing event handles are silently skipped (graceful degradation)

**Re-exports in `lib.rs`:**

- `pub use xml::extract::extract_events;`
- `pub use types::ParsedEvent;`
- `pub use xml::extract::resolve_event_refs;`

**Tests:**

- `resolve_event_refs_populates_dates` — cross-reference births and deaths
- `resolve_event_refs_no_overwrite` — inline birth/death not overwritten by eventref
- `resolve_event_refs_unknown_event_type` — Marriage event not applied to birth/death
- `resolve_event_refs_missing_handle` — person references nonexistent event handle
- `resolve_event_refs_empty_events` — empty events list
- `resolve_event_refs_empty_persons` — empty persons list

### Step 5 — Pipeline integration

**Deliverable:** Wire up event extraction and resolution in `visualize`'s `load_graph_data()`.

**Design notes:**

- Add `extract_events` call after `extract_persons` and before `extract_families`
- Call `resolve_event_refs(&mut persons, &events)` before building graph data
- Error handling follows same pattern: `map_err(|e| format!("Not a valid Gramps XML file: {}", e))`

**Tests (in `crates/visualize/src/lib.rs`):**

- `load_graph_data_event_ref_format` — Full pipeline with `<events>` section + `<eventref>` references: verify persons get populated `birth_year`/`death_date` from event reference resolution
- `load_graph_data_valid_file` — Extend existing test to confirm inline birth/death parsing is unaffected (backward compatibility)
- `load_graph_data_mixed_inline_and_eventrefs` — Person with both inline birth and eventrefs; inline takes precedence
