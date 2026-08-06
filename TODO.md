# Implementation Plan: Human-Readable Diff Output + CSV Export

Source: `docs/research/diff-human-readable-output.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix: populate gramps_id when parsing Gramps XML` | Parser `gramps_id` field | `crates/gramps-reader/src/xml.rs` — add `read_id_attr()`; `crates/gramps-reader/src/xml/parse.rs` — call it in `parse_person`, `parse_family`, `parse_event`, `parse_place`, `parse_source`, `parse_citation`, `parse_repository`, `parse_media`, `parse_note`, `parse_tag` and all `Event::Empty` handlers | Unit |
| 2 | `feat: add gramps_id and display_name fields to ItemDiff` | `ItemDiff` data model | `crates/diff/src/report.rs` — add `gramps_id_a`, `gramps_id_b`, `display_name_a`, `display_name_b` to `ItemDiff` | Unit |
| 3 | `feat: populate gramps_id and display_name during diff matching` | Matcher enrichment | `crates/diff/src/matcher.rs` — extract Gramps IDs from node data, change `display_name_for_node` signature to accept `&Graph`, resolve parent names for Family type, store both in `ItemDiff` at each construction site | Unit |
| 4 | `feat: include Gramps ID and display name in diff text output` | Text output enhancement | `crates/diff/src/output.rs` — update `write_item()` to render enriched header format with Gramps ID and display name | Unit |
| 5 | `feat: add CSV output formatter` | CSV output module | `crates/diff/src/output.rs` — add `format_csv()` with inline CSV escaper | Unit |
| 6 | `feat: add --output csv to diff CLI subcommand` | CLI wiring | `crates/cli/src/commands/diff.rs` — add `"csv"` output format arm; `crates/cli/tests/e2e.rs` — add E2E test with fixture `.gramps` files | Integration, E2E |
| 7 | `feat: add gramps_id to lightweight extractors` (deferred) | Extractor enrichment | `crates/gramps-reader/src/types.rs` — add `gramps_id` to `ParsedPerson`, `ParsedFamily`, `ParsedEvent`; `crates/gramps-reader/src/xml/extract.rs` — populate from `read_id_attr()` | Unit |

## Dependencies

```
Step 1 ──► Step 2 ──► Step 3
                 │
                 └──► Step 4 ──► Step 5
Step 7 (deferred, no dependency on above sequence)
```

## Test detail for each step

### Step 1 — `read_id_attr()` + populate in parser

- **Unit tests in `xml.rs`**: plain `id="I0001"`, namespace-prefixed `ns:id="I0001"`, missing attribute, element with `handle` but not `id` (same pattern as existing `read_handle_attr` tests).
- **Unit tests in `parse.rs` or integration**: parse a snippet with `id="I0001"` on a person element, verify `gramps_id` is `Some("I0001")`; parse without `id` attr → `None`. Test all 10 primary types.

### Step 2 — `ItemDiff` enrichment

- **Unit**: type shape — construct `ItemDiff` with new fields populated, verify serde round-trip, verify new fields survive JSON serialization/deserialization.

### Step 3 — Matcher enrichment

- **Unit**: verify Gramps IDs flow through to `ItemDiff` after matching.
- **Unit**: verify Family `display_name` resolves parent names from the graph.
- **Unit**: verify Family `display_name` gracefully handles missing parents.

### Step 4 — Text output enhancement

- **Unit**: text output contains Gramps IDs and display names when present.
- **Unit**: handles `None` gracefully with `-` placeholders.

### Step 5 — CSV output

- **Unit**: CSV header row is correct.
- **Unit**: one row per `FieldChange` for Modified items; single row for Added/Removed/Same.
- **Unit**: special characters quoted; empty values produce empty quoted cells.
- **Unit**: `format_csv` signature matches `format_text`/`format_json`.

### Step 6 — CLI wiring

- **Integration**: `gramps-gen diff file_a.gramps file_b.gramps --output csv` produces valid CSV.
- **E2E**: test fixture pair of small `.gramps` files with known `id` attributes, run full diff pipeline, verify Gramps IDs appear in both text and CSV output.

### Step 7 (deferred) — Extractor enrichment

- **Unit**: extract snippet with `id="I0001"` on person, verify `gramps_id` is populated; extract without `id` → `None`.
