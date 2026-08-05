# Implementation Plan: Step 1 — Full Gramps XML → Graph Parser

Source: `docs/research/gramps-diff-plan.md` (Step 1 only)

> **Scope:** This plan implements **Step 1 (full graph parser)** of the Gramps Diff
> Analyzer plan. All other steps (2–14) are **out of scope** and deliberately not
> included. This step is the prerequisite for all diff logic: it builds a
> `typed-graph::Graph` from a complete `.gramps` XML document, covering all 10
> primary types and their edges.

## Design notes grounding the sub-steps

- **Two-phase parse.** `typed-graph::Graph::add_edge` errors with
  `GraphError::MissingNode` unless both source and target nodes already exist.
  Gramps XML defines nodes in `<people>`/`<families>`/`<events>`/… sections, and a
  person's `<eventref>` may reference an event defined **later** in the file.
  Sub-steps 3–6 therefore build **nodes only** (stashing handle references in
  pending lists on the parser); sub-step 7 builds **edges** from those pending
  refs, then sub-step 8 covers validation + round-trip integration.
- **Coexists with lightweight extractors.** The existing `xml::extract` /
  `xml::count` streaming extractors and `ParsedPerson`/`ParsedFamily`/`ParsedEvent`
  types are untouched. The new `xml::parse` module is additive.
- **Schema version from header.** `parse_graph` detects the version from the
  `<header><created version="..."/>` element and selects the compiled-in `Schema`
  (via `Schema::for_version`); unknown versions return `Error::UnsupportedSchema`.
- **Schema-variant helpers.** Because the generated structs differ by schema
  feature (e.g. `gender` is `i32` in 5.2, `Option<i32>` in merged), the parser
  must go through the `into_*_field` / `make_*_ref` / `edge_*` helpers re-exported
  from `typed-graph::graph` rather than direct field writes. See AGENTS.md §1.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: add typed-graph dependency and UnsupportedSchema error to gramps-reader` | Parser foundation | `crates/gramps-reader/Cargo.toml` — add `typed-graph` to `[dependencies]` (move from `[dev-dependencies]`). `crates/gramps-reader/src/error.rs` — add `Error::UnsupportedSchema { version: String }` variant with `Display`. | Unit (UnsupportedSchema `Display`/`source`), Smoke (crate compiles with new dep) |
| 2 | `feat: detect schema version from gramps XML header` | Schema version detection | `crates/gramps-reader/src/xml/header.rs` — `detect_schema_version(content: &str) -> Result<String, Error>` streaming-scanning the `<header><created version="..."/>` element; returns `Error::UnsupportedSchema` when the version is not in `Schema::available_versions()`. Wire into `xml/mod` (or `xml.rs`). | Unit (5.2 header → "5.2", 5.1 header, missing `<header>` → error, malformed XML → `XmlParseError`, unknown version → `UnsupportedSchema`) |
| 3 | `feat: add streaming graph parser framework and Person node parsing` | Parser framework + Person | `crates/gramps-reader/src/xml/parse.rs` — `Parser` struct holding `Graph` + pending-ref lists; `parse_node` dispatch over the 10 primary element names; `parse_graph(xml, schema) -> Result<Graph, Error>` entry (reads header, selects schema, streams top-level sections). Implement **Person** node: primary + alternate `Name`s, `gender` via `into_gender_field`, attributes/addresses/urls, and accumulation of `event_ref`/`family`/`parent_family`/`person_ref`/`citation`/`note`/`media`/`tag` handles into pending lists. | Unit (person with primary+alternate name, gender mapping, empty person, malformed person → `XmlParseError`) |
| 4 | `feat: parse Family and Event nodes into the graph` | Family + Event nodes | `crates/gramps-reader/src/xml/parse.rs` — **Family** node (`father_handle`, `mother_handle`, `child_ref_list` w/ `ChildRef` metadata, `event_ref_list`, citations/notes/media/tags) and **Event** node (`event_type` via `into_event_type_field`, `DateValue` from `<dateval>`, `place_handle`, `description`, attributes, citations/notes/media/tags). Accumulate referenced handles into pending lists. | Unit (family with father/mother/childrefs, event with date+type+place, malformed per type → `XmlParseError`) |
| 5 | `feat: parse Place, Source, Citation, Repository nodes into the graph` | Location/reference nodes | `crates/gramps-reader/src/xml/parse.rs` — **Place** (place hierarchy parents, lat/long, names), **Source** (title, author, pubinfo, `reporef_list`), **Citation** (`source_handle` via `into_source_handle_field`, page/date/volume), **Repository** (name, type, address). Accumulate referenced handles into pending lists. | Unit (one parse per type, empty variants, malformed per type → `XmlParseError`) |
| 6 | `feat: parse Media, Note, Tag nodes into the graph` | Media/Note/Tag nodes | `crates/gramps-reader/src/xml/parse.rs` — **Media** (description, `srcfile`, attributes, refs), **Note** (text, format), **Tag** (name, color via `normalize_tag_color` equivalent map if present in parser). Accumulate referenced handles into pending lists. | Unit (one parse per type, empty variants, malformed per type → `XmlParseError`) |
| 7 | `feat: build edges from pending handle references and validate the graph` | Edge wiring + validation | `crates/gramps-reader/src/xml/parse.rs` — after the node pass, build all `Edge` variants from the pending refs using `make_event_ref`/`make_child_ref`/`edge_place_place_ref`, looking up target handles against known nodes (skip or error on dangling refs). Run `graph.validate(&schema)` and surface `ValidationState::Invalid` as an error. | Unit (person→event eventref edge, family→child childref edge, citation→source edge, person→family/parent-family edges, note/media/tag refs, dangling ref → error, empty graph validates) |
| 8 | `test: add round-trip and mixed-type integration tests for the full parser` | Integration tests | `crates/gramps-reader/tests/parse_roundtrip.rs` — generate graphs via `generate_random()` with fixed seeds, serialize with `output::GraphXmlWriter`, re-parse with `parse_graph`, and compare node/edge sets; plus one hand-written mixed-type fixture (Person with attached Events, Citations, Notes, Media). | Integration (round-trip graphs identical for several seeds, mixed-type fixture parses to expected node/edge counts, malformed XML → `XmlParseError`) |

## Test scope summary

- **Unit** — sub-steps 1–7 (pure functions and per-type parse paths).
- **Smoke** — sub-step 1 (crate compiles with new dependency).
- **Integration** — sub-step 8 (round-trip + mixed-type; uses `output` and
  `generate_random` fixtures already in the workspace).

## Out of scope (deferred to the original plan)

Steps 2–14 of `docs/research/gramps-diff-plan.md` (diff crate skeleton, similarity,
normalization, report types, compare, matcher, cascading, output, visualizer index,
orchestrator, CLI, interactive resolution, integration tests). Nothing in those
steps is built here.
