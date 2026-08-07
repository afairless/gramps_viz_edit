# Plan: Integrate Diff Results with Visualizer Selections

Source: existing `crates/diff/` CSV output format and `crates/visualize/frontend/src/` selection export JSON format (committed as of planning)

## Motivation

The diff tool (`gramps-gen diff`) compares two Gramps family tree files and produces a
structured report (text, JSON, or CSV) showing what changed between them. The
visualizer (`gramps-gen visualize`) renders a single Gramps file as a force-directed
family graph and lets the user click-select people of interest, exporting the
selection as JSON.

Currently there is no way to **combine** these two data sources. A user who runs a
diff between an old and new family tree and then selects people in the visualizer
must mentally correlate the two outputs. This plan describes a new `integrate`
subcommand that merges the diff CSV with the visualizer selection JSON into a single
table, matching people by handle.

## Design Decisions (from user consultation)

| Decision | Choice | Rationale |
|---|---|---|
| Crate placement | New `integrate` crate | Clean separation of concerns; no existing crate gets bloated |
| Output format | Both CSV and JSON | Maximizes flexibility — CSV for spreadsheets, JSON for programmatic use |
| Row structure | One row per field change | Preserves all field-level detail from the diff; visualizer data repeated on each row |
| Matching strategy | Try both diff sides by handle | Simple, no format changes to visualizer or diff; uses UUID handles as natural key |

## Current State

### Diff CSV format (`diff --output csv`)

Columns: `classification`, `item_type`, `handle_a`, `gramps_id_a`, `display_name_a`,
`handle_b`, `gramps_id_b`, `display_name_b`, `confidence`, `field_name`, `field_kind`,
`old_value`, `new_value`, `similarity`

One or more rows per item (one per `FieldChange`; zero-change items get one row with
empty field-level columns). `item_type` distinguishes People from Families, Events, etc.

### Visualizer Selection Export JSON

```json
{
  "exported_at": "2025-01-15T10:30:00.000Z",
  "file": "selections.json",
  "selections": [
    {
      "handle": "abc123-def456",
      "name": "John Smith",
      "birth_date": "1840-07-13",
      "death_date": "1910-03-22",
      "gender": "male",
      "family_group": 3
    }
  ]
}
```

Key: each `handle` is a Gramps UUID — the same handle that appears in the diff
CSV's `handle_a` or `handle_b` columns.

### Key insight: handles are UUIDs

Both the visualizer and the diff parser read handles directly from the `.gramps` file.
Since handles are UUIDs, the probability of accidental collision across files or sides
is negligible. This makes "try both sides" matching safe without any format changes.

## Design

### New crate: `crates/integrate`

A new library crate with no Tauri or frontend dependencies. It depends on `serde`,
`serde_json`, and `csv` for parsing/formatting.

#### Data flow

```
┌──────────────────┐   ┌──────────────────────┐
│  Diff CSV file   │   │  Visualizer JSON file│
│ (from gramps-gen │   │ (from Export Selected)│
│  diff --output    │   │                      │
│  csv > out.csv)  │   │                      │
└────────┬─────────┘   └──────────┬───────────┘
         │                        │
         ▼                        ▼
┌──────────────────────────────────────────────┐
│              integrate crate                 │
│                                              │
│  1. Parse diff CSV into Vec<DiffRow>         │
│  2. Parse selections JSON into Vec<Selection>│
│  3. Index selections by handle (HashMap)     │
│  4. Filter diff rows to item_type == "Person"│
│  5. For each Person diff row:                │
│     a. Match handle_a → side="a"             │
│     b. Match handle_b → side="b"             │
│     c. If matched: combine with viz data     │
│  6. Produce output (CSV + JSON)             │
│                                              │
└────────┬────────────────────────┬────────────┘
         │                        │
         ▼                        ▼
┌──────────────────┐   ┌──────────────────────┐
│ Combined CSV     │   │ Combined JSON         │
│ (spreadsheet)    │   │ (programmatic)        │
└──────────────────┘   └──────────────────────┘
```

#### Input parsing

**Diff CSV parser** — parses the existing CSV format into a `DiffRow` struct:

```rust
struct DiffRow {
    classification: String,
    item_type: String,
    handle_a: Option<String>,
    gramps_id_a: Option<String>,
    display_name_a: Option<String>,
    handle_b: Option<String>,
    gramps_id_b: Option<String>,
    display_name_b: Option<String>,
    confidence: f64,
    field_name: String,
    field_kind: String,
    old_value: String,
    new_value: String,
    similarity: f64,
}
```

Deserialization uses the `csv` crate's `#[derive(Deserialize)]` with header-name matching (the default when a header row is present), making the parser robust against column reordering.

The `item_type` field is NOT filtered here — all rows are returned. Filtering to `"Person"` happens in the merge/transformation step.

**Selections JSON parser** — deserializes the existing `SelectionExport` JSON format:

```rust
#[derive(Deserialize)]
struct SelectionExport {
    selections: Vec<Selection>,
}

#[derive(Deserialize)]
struct Selection {
    handle: String,
    name: String,
    birth_date: Option<String>,
    death_date: Option<String>,
    gender: String,
    family_group: usize,
}
```

#### Matching logic

For each diff row filtered to Person (the merge function receives ALL diff rows and builds a Person-only subset internally):

1. Try matching `handle_a` against the visualizer selections index
2. If found → produce a combined row with `side = "a"`
3. If not found, try matching `handle_b` against the index
4. If found → produce a combined row with `side = "b"`
5. If neither matches → row is excluded (inner join semantics)

A diff row where `handle_a` is `None` (an Added person) can only match via
`handle_b`. A row where `handle_b` is `None` (a Removed person) can only match
via `handle_a`.

#### Combined row columns (CSV)

All original diff columns + new columns:

| Column | Source | Description |
|---|---|---|
| `side` | Matching result | `"a"` if match via handle_a, `"b"` if via handle_b |
| `viz_name` | Selection.name | Person's full name from visualizer |
| `viz_birth_date` | Selection.birth_date | Birth date from visualizer |
| `viz_death_date` | Selection.death_date | Death date from visualizer |
| `viz_gender` | Selection.gender | Gender from visualizer |
| `viz_family_group` | Selection.family_group | DSU family group ID from visualizer |

#### Combined JSON format

```json
{
  "diff_file_a": "<taken from the --diff CLI argument>",
  "diff_file_b": "<taken from the --diff CLI argument>",
  "selection_file": "<path from --selections arg>",
  "matched_count": 42,
  "matches": [
    {
      "side": "a",
      "diff": {
        "classification": "Modified",
        "handle_a": "abc...",
        "handle_b": "def...",
        "gramps_id_a": "I0001",
        "gramps_id_b": "I0001",
        "display_name_a": "John Smith",
        "display_name_b": "John Smith",
        "confidence": 1.0,
        "field_name": "surname",
        "field_kind": "Text",
        "old_value": "Smith",
        "new_value": "Jones",
        "similarity": 0.5
      },
      "selection": {
        "name": "John Smith",
        "birth_date": "1840-07-13",
        "death_date": null,
        "gender": "male",
        "family_group": 3
      }
    }
  ]
}
```

### CLI integration

A new subcommand `gramps-gen integrate` with two modes of operation:

```
gramps-gen integrate diff-viz \
    --diff diff_report.csv \
    --selections selections.json \
    -o combined_output.csv
```

The `diff-viz` sub-subcommand explicitly names which two sources are being integrated
(allowing future integration modes like "diff + geodata").

In the CLI module:

```rust
#[derive(Args)]
struct IntegrateArgs {
    #[command(subcommand)]
    mode: IntegrateMode,
}

#[derive(Subcommand)]
enum IntegrateMode {
    /// Merge diff CSV with visualizer selection JSON
    DiffViz(DiffVizArgs),
}

#[derive(Args)]
struct DiffVizArgs {
    /// Path to the diff CSV file
    #[arg(long)]
    diff: String,

    /// Path to the visualizer selections JSON file
    #[arg(long)]
    selections: String,

    /// Output file path
    #[arg(short, long)]
    output: Option<String>,

    /// Output format: csv (default) or json
    #[arg(long, default_value = "csv")]
    format: String,
}
```

The `DiffViz` sub-subcommand name was chosen over flat (`integrate --diff`) or standalone (`merge-diff-viz`) alternatives for extensibility.

### Error handling

The `integrate` crate defines its own error type:

```rust
enum IntegrateError {
    /// The diff CSV file could not be read or parsed.
    DiffReadError(String),
    /// The selections JSON file could not be read or parsed.
    SelectionsReadError(String),
    /// Output write error.
    OutputError(String),
}
```

All implement `Display` + `Error`.

Empty-result states (no Person rows in the diff, no selections in the JSON) are
NOT errors. The orchestrator logs these as warnings via the `log` crate and
returns an empty `IntegrateReport` with `matched_count: 0`.

## Implementation Plan

### Dependencies

This plan depends on the existing diff CSV output (already implemented) and the
visualizer selection export (already implemented). No changes needed to either.

```
crates/diff (existing CSV output) ──┐
                                    ├──▶ crates/integrate (new)
crates/visualize (existing JSON     │
  selection export)              ───┘
```

### Step 1 — Scaffold integrate crate + diff CSV parser

| Commit | `feat: scaffold integrate crate with diff CSV parser` |
|---|---|
| Crate | `integrate` (new) |
| Changes | Create `crates/integrate/Cargo.toml` depending on `serde`, `serde_json`, `csv` (new workspace dep), and `log` (existing workspace dep). Create `crates/integrate/src/lib.rs` with `IntegrateError` enum + `parse_diff_csv(path)` function returning `Vec<DiffRow>` (ALL rows, no item_type filter). `DiffRow` uses `#[derive(Deserialize)]` with header-name matching. Add `crates/integrate/src/csv_reader.rs` module. Update root `Cargo.toml` members and add `csv` to `[workspace.dependencies]`. The `integrate` crate is a transitive dependency of `cli` — it should be added to `default-members` so it's built by default. |
| Tests | Unit: parse a valid CSV with 3 Person rows (Same, Modified with 2 field changes, Added) → 4 rows total, 2 from the Modified person. Edge: empty CSV → empty vec; CSV with no Person rows → empty vec (all rows returned, none filtered at this stage); CSV with special characters (commas, quotes in field values) → properly escaped. Invalid CSV → `IntegrateError::DiffReadError`. Cross-crate integration: run `diff::output::format_csv()` with a known `DiffReport`, feed the result into `parse_diff_csv()`, verify round-trip produces the same data. |

### Step 2 — Add selections JSON parser

| Commit | `feat: add visualizer selections JSON parser` |
|---|---|
| Crate | `integrate` |
| Changes | Add `crates/integrate/src/json_reader.rs` module with `parse_selections_json(path)` function. `Selection` and `SelectionExport` structs matching the visualizer export format. |
| Tests | Unit: parse valid selections JSON with 3 people → `Vec` of 3 `Selection`s. Empty selections array → empty vec. Invalid JSON → `IntegrateError::SelectionsReadError`. Missing `handle` field → deserialization error. |

### Step 3 — Implement handle-based matching and join logic

| Commit | `feat: implement diff-viz handle matching and row merging` |
|---|---|
| Crate | `integrate` |
| Changes | Add `crates/integrate/src/merge.rs` module. Core function:

```rust
fn merge_diff_viz(
    diff_rows: Vec<DiffRow>,
    selections: Vec<Selection>,
) -> Vec<MergedRow>
```

Where `MergedRow` contains all `DiffRow` fields + `side: String` + all `Selection` fields (name, birth_date, death_date, gender, family_group). The function FIRST filters diff rows to `item_type == "Person"`, logging a warning if zero Person rows remain. Then builds `HashMap<String, &Selection>` from selections keyed by handle. For each Person diff row, try `handle_a` first, then `handle_b`. If match found, emit a `MergedRow` with the side label. Logs a warning if zero selections are available. |
| Tests | Unit: one diff row matching handle_a → side="a". One diff row matching handle_b → side="b". Added person (handle_a=None) matching handle_b → side="b". Removed person (handle_b=None) matching handle_a → side="a". Person not in selections (neither handle matches) → excluded. Person in both handles (same UUID in both files) → side="a" (first match wins). Selections index empty → empty output. Diff CSV with Person rows but none match selections → empty output. Diff CSV with no Person rows → warning logged, empty output. |

### Step 4 — Add CSV and JSON output formatters

| Commit | `feat: add CSV and JSON output for merged results` |
|---|---|
| Crate | `integrate` |
| Changes | Add `crates/integrate/src/output.rs` module with two public functions:

```rust
fn format_csv(rows: &[MergedRow]) -> String
fn format_json(rows: &[MergedRow], diff_path: &str, sel_path: &str) -> String
```

CSV format: all diff columns + `side`, `viz_name`, `viz_birth_date`, `viz_death_date`, `viz_gender`, `viz_family_group`. Uses the same quoting/escaping pattern as `crates/diff/src/output.rs::write_csv_cell`.

JSON format: the structure shown in the Design section above. Uses `serde_json::to_string_pretty`.

The `format_` function signatures accept a `&[MergedRow]` — they do not perform matching themselves. |
| Tests | Unit (CSV): header row correct, one merged row with all fields populated, empty input → only header row, special characters escaped, None fields produce empty cells. Unit (JSON): valid JSON, matches count, empty selections → `"matches": []`. Property-based (CSV↔JSON round-trip): generate random `MergedRow` values via `proptest`, format as CSV and JSON, parse back, verify structural equivalence (same field count, same column values). |

### Step 5 — Expose `integrate_diff_viz` public API

| Commit | `feat: add public integrate_diff_viz orchestrator function` |
|---|---|
| Crate | `integrate` |
| Changes | Add to `crates/integrate/src/lib.rs`:

```rust
pub fn integrate_diff_viz(
    diff_path: &str,
    selections_path: &str,
) -> Result<IntegrateReport, IntegrateError>
```

Where `IntegrateReport` contains `matched_count`, `diff_path`, `sel_path`, and the raw `Vec<MergedRow>` for the caller to format.

Orchestrates: parse CSV → parse JSON → merge → return report. |
| Tests | Integration: write a small diff CSV and corresponding selections JSON to temp files, run `integrate_diff_viz`, verify matched count and row contents. Edge: mismatched handles → 0 matches. |

### Step 6 — Wire CLI subcommand

| Commit | `feat: add gramps-gen integrate diff-viz CLI subcommand` |
|---|---|
| Crate | `cli` |
| Changes | Add `crates/cli/src/commands/integrate.rs`:

- `IntegrateArgs` → `IntegrateMode::DiffViz(DiffVizArgs)`
- `run(args)` calls `integrate::integrate_diff_viz()`, then formats via `format_csv` or `format_json`, writes to stdout or file.
- Add `IntegrateFailed(String)` variant to `CliError` and a `From<integrate::IntegrateError>` impl.

Register in `crates/cli/src/commands/mod.rs` and `crates/cli/src/main.rs` as a new `Command::Integrate` variant.

Update help text in main.rs. |
| Tests | Smoke: `gramps-gen integrate diff-viz --help` works. Integration: subprocess test using fixture files — run the CLI with known inputs, verify CSV output has expected columns and rows. |

## Resolved Questions

1. **Subcommand name**: `integrate diff-viz` (two-level). Chosen over flat (`integrate --diff`) for extensibility to future modes, and over standalone (`merge-diff-viz`) for consistency with a future `integrate` command family.

2. **CSV reader dependency**: Use the `csv` crate. Adds a new workspace dependency but avoids hand-rolling a fragile parser.

3. **File path metadata flags**: Skipped for now. The JSON output includes the file paths passed to `--diff` and `--selections` directly (no extra CLI flags needed). Optional `--file-a` / `--file-b` flags could be added later to override the display labels if needed.

## Risk Assessment

- **CSV format stability**: The diff CSV format is a hand-written formatter. If columns are reordered or renamed in the future, the integrate crate's parser will break. Mitigation: keep tests that validate the exact CSV header and column ordering, and document the dependency in both crates.
- **Handle collisions**: UUIDv4 handles are astronomically unlikely to collide. Even across two different files, the same UUID appearing by chance is not a practical concern.
- **Large selections**: The visualizer can select many thousands of people. The merge is O(n × log m) where n = diff rows (person field changes) and m = selections — building a HashMap index keeps it fast even at scale.

## Future Work

- [ ] Add a `--side` filter flag to restrict to only A-side or B-side matches
- [ ] Add a `--file-a` / `--file-b` optional flags to enrich JSON output metadata
- [ ] Support integrating visualizer selections with the diff JSON format (not just CSV)
- [ ] Support matching on Gramps ID (`gramps_id`) as a secondary fallback when handle matching fails
- [ ] Add family-group-aware matching: only consider selections from the visualized file's side
