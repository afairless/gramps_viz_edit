# Enhancement Plan: Human-Readable Diff Output + CSV Export

Source: current `crates/diff/` implementation (committed as of planning)

## Motivation

The current diff output uses internal handles (UUIDs or generated identifiers like
`P001`) as the only entity identifiers. This makes reports hard to read for humans,
even though the handles are kept for machine use. Two enhancements are needed:

1. **Human-readable identifiers**: Show Gramps database IDs (e.g., `I0002`, `E0002`,
   `P0230`, `F0001`) alongside handles, plus a brief display label (person's name,
   note snippet, source title, etc.)
2. **CSV export**: Produce CSV output (one row per `FieldChange`) that users can
   open in spreadsheet applications for sorting, filtering, and analysis.

## Current State

### What's already available

| Component | File | State |
|---|---|---|
| `gramps_id` field | All `XxxData` structs (generated from schema) | Always present as `Option<String>` |
| `display_name_for_node()` | `crates/diff/src/matcher.rs:278-364` | Computes human labels for all 10 types; only used for `AmbiguousCase` contexts |
| Text output | `crates/diff/src/output.rs` | `format_text()` shows `[MODIFIED] Person (A: P001, B: P002)` using handles only |
| JSON output | `crates/diff/src/output.rs` | `format_json()` serializes full `DiffReport` via serde |
| CSV support | None | Doesn't exist yet |
| CLI `--output` flag | `crates/cli/src/commands/diff.rs` | Accepts `text` or `json` |

### Key gap: Parser doesn't populate `gramps_id`

The `gramps-reader` XML parser (`crates/gramps-reader/src/xml/parse.rs`) reads
the `handle` attribute from every primary type element but **never reads the
`id` attribute** (which holds the Gramps database identifier like `I0001`).
All `gramps_id` fields remain `None` after parsing real Gramps XML files.

When random generation produces data, `gramps_id` IS populated (via
`GraphBuilder::with_gramps_id`), so diffing generated files would show them —
but a diff between real `.gramps` files would see `None` everywhere.

## Design

### Data model changes: `ItemDiff` enrichment

```rust
pub struct ItemDiff {
    // Existing fields (unchanged)
    pub handle_a: Option<String>,
    pub handle_b: Option<String>,
    pub item_type: String,
    pub classification: Classification,
    pub field_changes: Vec<FieldChange>,
    pub confidence: f64,

    // New fields
    pub gramps_id_a: Option<String>,    // e.g. "I0002", "E0005"
    pub gramps_id_b: Option<String>,
    pub display_name_a: Option<String>, // e.g. "John Smith", "1840-07-13 Birth"
    pub display_name_b: Option<String>,
}
```

### Display name conventions (per user choices)

| Type | Display name | Source |
|---|---|---|
| Person | `"John Smith"` | `primary_name.first_name + " " + primary_name.surname_list[0].surname` |
| Family | `"Family of John Smith & Jane Doe"` | Resolve father_handle/mother_handle → their display names. Falls back to `"Family (F0001)"` if parents unknown. |
| Event | `"Birth"` | `event_type` display (existing) |
| Place | `"Springfield"` | `name.city` or `name.street` (existing) |
| Source | `"Family Bible Records, 1800–1890"` | `title` (existing) |
| Citation | `"Source S0001, p.42"` | `source_handle` + `page` (existing) |
| Repository | `"National Archives"` | `name` (existing) |
| Media | `"portrait.jpg"` | `desc` or `path` (existing) |
| Note | `"This is the first..."` | First 80 chars of `text` (existing) |
| Tag | `"important"` | `name` (existing) |

### Output format changes

**Text output** — entity header line becomes:

```
[MODIFIED] Person (A: abc123 [I0002] "John Smith", B: def456 [I0002] "John Smith")
```

For ADDED/REMOVED items, only one side has data:

```
[ADDED] Note (A: -, B: n789 [N0012] "This is the first 80 chars of the note...")
```

**CSV output** — one row per `FieldChange`. Columns:

| Column | Source | Description |
|---|---|---|
| `classification` | `ItemDiff.classification` | `Same`, `Modified`, `Added`, `Removed`, `NeedsReview`, `ExtrinsicOnly` |
| `item_type` | `ItemDiff.item_type` | `Person`, `Family`, `Event`, etc. |
| `handle_a` | `ItemDiff.handle_a` | A-side handle (empty for Added) |
| `gramps_id_a` | `ItemDiff.gramps_id_a` | A-side Gramps ID |
| `display_name_a` | `ItemDiff.display_name_a` | A-side display label |
| `handle_b` | `ItemDiff.handle_b` | B-side handle (empty for Removed) |
| `gramps_id_b` | `ItemDiff.gramps_id_b` | B-side Gramps ID |
| `display_name_b` | `ItemDiff.display_name_b` | B-side display label |
| `confidence` | `ItemDiff.confidence` | Match confidence |
| `field_name` | `FieldChange.field_name` | e.g. `primary_name.surname_list[0].surname` |
| `field_kind` | `FieldChange.field_kind` | `Text`, `Date`, `HandleRef`, etc. |
| `old_value` | `FieldChange.old_value` | A-side value |
| `new_value` | `FieldChange.new_value` | B-side value |
| `similarity` | `FieldChange.similarity` | 0.0–1.0 |

For items with no field changes (Same, Added, Removed, NeedsReview), one row
is emitted with empty field-level columns. (In the current codebase,
NeedsReview items never carry non-empty `field_changes` because the match
is too ambiguous to produce field-level diffs.)

CSV cells are quoted (standard CSV quoting). Newlines within values are
replaced with spaces.

## Implementation Plan

### Step 0 — Fix parser to populate `gramps_id`

| Commit | `fix: populate gramps_id when parsing Gramps XML` |
|---|---|
| Crate | `gramps-reader` |
| Changes | Add `read_id_attr()` to `crates/gramps-reader/src/xml.rs`; call it in parser for each primary type's opening tag; set `gramps_id` on each `XxxData` struct |
| Tests | Unit: parse snippet with `id="I0001"` on person, verify `gramps_id` is `Some("I0001")`; parse without `id` attr → `None`; all 10 primary types tested.
Also add dedicated unit tests for `read_id_attr()` in `xml.rs` following the same pattern as `read_handle_attr()`: plain `id="I0001"`, namespace-prefixed `ns:id="I0001"`, missing attribute, and an element with `handle` but not `id`. |

This also needs adding `gramps_id` to the `ParsedPerson`/`ParsedFamily`/`ParsedEvent`
types used by the existing lightweight extractors (`.xml/extract.rs`) — but those
are separate from the diff pipeline and can be deferred if they add complexity.

### Step 1 — Enrich `ItemDiff` with Gramps ID + display name fields

| Commit | `feat: add gramps_id and display_name fields to ItemDiff` |
|---|---|
| Crate | `diff` |
| Changes | `crates/diff/src/report.rs` — add four new fields to `ItemDiff` |
| Tests | Unit: type shape, serde round-trip with new fields populated |

### Step 2 — Populate new fields during matching

| Commit | `feat: populate gramps_id and display_name during diff matching` |
|---|---|
| Crate | `diff` |
| Changes | `crates/diff/src/matcher.rs`: |
| | - Extract `gramps_id` from node data at each `ItemDiff` construction site |
| | - Call `display_name_for_node()` and store result |
| | - Enhance `display_name_for_node` for Family: resolve father/mother handles to parent display names from the source graph |
| | **Note:** `display_name_for_node` currently takes `(kind: NodeKind, node: &Node)` — it has no graph access. Change the signature to `(kind: NodeKind, node: &Node, graph: &Graph) -> String` and update all call sites (internal to this module). The `graph` parameter is a no-op for non-Family types. |
| Tests | Unit: verify Gramps IDs flow through to ItemDiff; verify Family display name resolves parents; verify Family display name gracefully handles missing parents |

### Step 3 — Update text output formatter

| Commit | `feat: include Gramps ID and display name in diff text output` |
|---|---|
| Crate | `diff` |
| Changes | `crates/diff/src/output.rs` — update `write_item()` to render the enriched header format |
| Tests | Unit: text output contains Gramps IDs and display names when present; handles None gracefully with `-` |

### Step 4 — Add CSV output formatter

| Commit | `feat: add CSV output formatter with one row per field change` |
|---|---|
| Crate | `diff` |
| Changes | `crates/diff/src/output.rs` — add `format_csv(report, include_extrinsic) -> String` |
| Dependencies | No new crate dependency; write a minimal CSV escaper inline |
| Tests | Unit: CSV header row is correct; one row per FieldChange for Modified items; single row for Added/Removed/Same; special characters quoted; empty values produce empty quoted cells |

### Step 5 — Wire CSV option into CLI

| Commit | `feat: add --output csv to diff CLI subcommand` |
|---|---|
| Crate | `cli` |
| Changes | `crates/cli/src/commands/diff.rs` — add `"csv"` arm to output format match; use `format_csv()` |
| Tests | Integration: `gramps-gen diff file_a.gramps file_b.gramps --output csv` produces valid CSV.
E2E: add a test fixture pair of small `.gramps` files with known `id` attributes,
run the full diff pipeline, and verify Gramps IDs appear in both text and CSV output. |

## Dependencies Between Steps

```
Step 0 (parser fix: gramps_id) ──► Step 1 (ItemDiff fields) ──► Step 2 (matcher populates)
                                                                       │
                                                                       ├──► Step 3 (text output)
                                                                       │
                                                                       └──► Step 4 (CSV output) ──► Step 5 (CLI wire) ──► Step 6 (deferred)
```

Steps 3 and 4 can be done in parallel after Step 2. Step 5 depends on Step 4.
Step 6 is deferred follow-up (not blocking the main feature).

### Step 6 (deferred) — Add `gramps_id` to lightweight extractors

| Commit | `feat: add gramps_id to ParsedPerson/ParsedFamily/ParsedEvent` |
|---|---|
| Crate | `gramps-reader` |
| Changes | Add `gramps_id: Option<String>` field to `ParsedPerson`, `ParsedFamily`, and `ParsedEvent` in `crates/gramps-reader/src/types.rs`; populate from `read_id_attr()` in `.xml/extract.rs` parsing code |
| Tests | Unit: extract snippet with `id="I0001"` on person, verify `gramps_id` is populated; extract without `id` → `None` |
| Priority | Post-feature. These types are used by `cli` stats and `visualize`, not by the diff pipeline, so this is a separate quality improvement. |

## Risk Assessment

- **Parser change (Step 0)**: Adding `read_id_attr()` touches the streaming XML
  parser for all 10 primary types. Risk is moderate — each type needs the same
  2-line pattern (read `id` attr, assign to struct). The parser already does
  this for `handle`, so the pattern is proven.
- **Family name resolution (Step 2)**: Resolving parent handles to display names
  requires looking up Person nodes within the source graph at match time. This
  is O(1) via `graph.get_node()` and safe since parent handles are always
  present in the same file (A-side parents in graph A, B-side parents in graph B).
  Only risk: the parent might not exist (malformed file), but `get_node()`
  returns `Option` and we handle the None case gracefully.
- **CSV quoting (Step 4)**: Custom CSV escaper is simple (~15 lines) and avoids
  pulling in a CSV crate. Values may contain commas and quotes, both of which
  require proper escaping.

## Notes

- The `gramps_id` field is already compared by the diff engine (all `compare_*`
  functions in `compare.rs` call `compare_field_optional_text("gramps_id", ...)`),
  so enriching `ItemDiff` with it doesn't affect matching — it's just output
  presentation.
- `display_name_for_node()` in the matcher is currently only called for
  `AmbiguousCase` contexts. After this change, it will be called for every
  matched entity. The Family enhancement (resolving parent names) will be the
  only logic change; other types just get their existing labels stored.
- The `format_csv` function will NOT introduce a new dependency (csv crate).
  A minimal CSV writer is sufficient given the controlled set of field values.
