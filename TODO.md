# Implementation Plan: Fix Gramps UI Event Type Parsing in gramps-reader

Source: `docs/research/fix-gramps-ui-event-type-parsing.md`

## Context

Gramps UI files (Gramps 5.1 / XML 1.7.1) place `<type>Birth</type>` directly
inside `<event>`, while our generator (Gramps 5.2 / XML 1.7.2) wraps it in
`<eventtype><type>...</type></eventtype>`. `extract_events()` only reads the
nested form, so `event_type` stays `None` for UI-generated files and no
birth/death dates are copied from events to persons — the visualizer shows
all birth years as `null`.

The fix is localized to one function (`extract_events()`); downstream code
(`resolve_event_refs()`, `build_graph_data()`) is correct once `event_type`
is populated.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix(gramps-reader): parse bare <type> inside <event> from Gramps UI files` | Flat-format event type parsing | `crates/gramps-reader/src/xml/extract.rs` (add `b"type" if current.is_some() && !in_eventtype` arm in `extract_events()` + `log::debug!()` line) | — |
| 2 | `test(gramps-reader): add unit tests for flat-format event type parsing` | Flat-format parsing unit tests | `crates/gramps-reader/src/xml/extract.rs` (8 tests: birth/death/marriage flat, no-dateval, mixed formats, self-closing `<type/>`, empty `<type></type>`, custom multi-word type) | Unit |
| 3 | `test(visualize): verify full pipeline loads dates from Gramps UI event format` | Visualizer pipeline integration test | `crates/visualize/src/lib.rs` (full chain `extract_events` → `resolve_event_refs` → `build_graph_data` with Gramps-UI-style snippet) | Integration |

## Notes

- Step 1's verification: existing `<eventtype><type>` tests must still pass
  (the new arm is placed after the `in_eventtype` arm so nested format wins).
- Step 2's tests: `extract_event_birth_flat`, `extract_event_death_flat`,
  `extract_event_marriage_flat`, `extract_event_no_dateval_flat`,
  `extract_events_mixed_formats`, `extract_event_type_self_closing_flat`,
  `extract_event_type_empty_flat`, `extract_event_custom_type_flat`.
- Step 3 is required (not optional): it guards the entire chain against
  regressions that a unit test on `extract_events()` alone would miss.
- Known accepted limitation (no action): a `<event>` containing both
  `<eventtype><type>` and a bare `<type>` lets the bare one overwrite —
  Gramps never emits this, and it is not handled specially.
