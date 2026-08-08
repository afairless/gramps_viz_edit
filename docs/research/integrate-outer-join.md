# Plan: Change Integrate from Inner Join to Full Outer Join

Source: existing `crates/integrate/` implementation (committed as of planning)

## Motivation

The `integrate diff-viz` subcommand combines a diff CSV (from `gramps-gen diff`)
with a visualizer selection JSON (from `gramps-gen visualize` Export Selected)
into a single table, matching people by handle.

**Current behavior (inner join):** Only Person rows that appear in **both** the
diff CSV and the visualizer selections are emitted. A person in the diff who
wasn't selected in the visualizer is silently dropped.

**Desired behavior (full outer join):** All unique Person handles from **either**
data source appear in the output:

| Case | Behavior |
|---|---|
| In diff AND in selections | Merged row with both diff fields and viz fields (as today) |
| In diff only | Row with diff fields populated; viz fields are `None`/empty; `side` is `""` |
| In selections only | Row with viz fields populated; diff fields are empty/`None`; `side` is `""` |

## Scope

This plan modifies only the **integrate crate** (`crates/integrate/`). The diff
crate, visualize crate, CLI parsing, and E2E tests all need corresponding updates
to reflect the new semantics, but no structural changes are required outside
`integrate`.

**Note:** The `integrate` crate is not yet listed in `docs/ARCHITECTURE.md` or
`README.md` (which document 6 crates). After implementation, update those files
to include the `integrate` crate in the crate table and architecture diagram.

## Design Changes

### 1. Make viz fields optional in `MergedRow`

Currently three fields are non-optional and cannot represent "no data":

| Field | Current type | New type |
|---|---|---|
| `viz_name` | `String` | `Option<String>` |
| `viz_gender` | `String` | `Option<String>` |
| `viz_family_group` | `usize` | `Option<usize>` |

`viz_birth_date` and `viz_death_date` are already `Option<String>` — no change.

The CSV serializer already handles `Option` fields: `None` → empty cell.
The JSON serializer already handles `Option` fields: `None` → JSON `null`.

### 2. Add `RowKind` discriminator

Introduce a row-kind discriminator so downstream consumers can distinguish the
three cases without pattern-matching on `side` + viz-nullness.

The enum is annotated with `#[serde(rename_all = "snake_case")]` so that
CSV and JSON output use lowercase underscored variant names ("matched",
"diff_only", "viz_only") instead of Rust's default PascalCase:

```rust
/// The provenance of a merged row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RowKind {
    /// Person appears in both diff and selections — full merge.
    Matched,
    /// Person appears only in the diff CSV — viz fields are empty.
    DiffOnly,
    /// Person appears only in the visualizer selections — diff fields are empty.
    VizOnly,
}
```

Add a `row_kind: RowKind` field to `MergedRow` (replacing the previous implicit
semantics). Using the enum directly (rather than `String`) ensures type safety:
the CSV and JSON serializers produce the `#[serde(rename_all = "snake_case")]`
string automatically, and deserialization validates the value at parse time.
This makes the output self-documenting and avoids fragile heuristics like
"side empty means no match".

### 3. Change merge logic from inner join to full outer join

The function `merge_diff_viz()` needs three changes:

**a) Emit diff-only rows** — instead of `continue` when neither handle matches,
emit a `MergedRow` with `row_kind: RowKind::DiffOnly`, `side: ""`, and all viz
fields set to `None`.

**b) Track which selection handles were matched** — maintain a
`HashSet<String>` of selection handles that were consumed during diff-row
iteration.

**c) Emit selection-only rows** — after processing all diff rows, iterate over
selections and emit a `MergedRow` with `row_kind: RowKind::VizOnly`,
`side: ""`, diff fields set to defaults (empty strings, `None` handles,
`0.0` confidence/similarity), and viz fields populated from the selection.

The inlined logic replaces the previous `continue`-on-no-match with explicit
`DiffOnly` row emission, then a second pass emits `VizOnly` rows:

```rust
pub fn merge_diff_viz(diff_rows: Vec<DiffRow>, selections: Vec<Selection>) -> Vec<MergedRow> {
    let person_rows: Vec<DiffRow> = diff_rows.into_iter()
        .filter(|r| r.item_type == "Person").collect();

    // Build selection index
    let selection_map: HashMap<&str, &Selection> = selections.iter()
        .map(|s| (s.handle.as_str(), s)).collect();

    let mut matched_handles: HashSet<String> = HashSet::new();
    let mut merged = Vec::new();

    // --- Phase 1: process diff rows (matched → Matched, unmatched → DiffOnly) ---
    for row in person_rows {
        // Try handle_a first, then handle_b
        let matched = row.handle_a.as_ref()
            .and_then(|h| selection_map.get(h.as_str()))
            .or_else(|| row.handle_b.as_ref()
                .and_then(|h| selection_map.get(h.as_str())));

        if let Some(sel) = matched {
            matched_handles.insert(sel.handle.clone());
            merged.push(MergedRow {
                // diff fields from row, viz fields from sel
                row_kind: RowKind::Matched,
                side: if selection_map.contains_key(
                    row.handle_a.as_deref().unwrap_or("")
                ) { "a".to_string() } else { "b".to_string() },
                viz_name: Some(sel.name.clone()),
                viz_birth_date: sel.birth_date.clone(),
                viz_death_date: sel.death_date.clone(),
                viz_gender: Some(sel.gender.clone()),
                viz_family_group: Some(sel.family_group),
                // ... all diff fields from row ...
            });
        } else {
            // DiffOnly: diff fields populated, viz fields None
            merged.push(MergedRow {
                row_kind: RowKind::DiffOnly,
                side: String::new(),
                viz_name: None,
                viz_birth_date: None,
                viz_death_date: None,
                viz_gender: None,
                viz_family_group: None,
                // ... all diff fields from row ...
            });
        }
    }

    // --- Phase 2: emit selection-only rows ---
    for sel in &selections {
        if !matched_handles.contains(&sel.handle) {
            merged.push(MergedRow {
                row_kind: RowKind::VizOnly,
                side: String::new(),
                handle_a: None,
                gramps_id_a: None,
                display_name_a: None,
                handle_b: None,
                gramps_id_b: None,
                display_name_b: None,
                classification: String::new(),
                item_type: "Person".to_string(),
                confidence: 0.0,
                field_name: String::new(),
                field_kind: String::new(),
                old_value: String::new(),
                new_value: String::new(),
                similarity: 0.0,
                viz_name: Some(sel.name.clone()),
                viz_birth_date: sel.birth_date.clone(),
                viz_death_date: sel.death_date.clone(),
                viz_gender: Some(sel.gender.clone()),
                viz_family_group: Some(sel.family_group),
            });
        }
    }

    merged
}
```

### 4. Update JSON output format

The `format_json` function's `MatchEntry` and `SelectionView` types need
updates:

```rust
#[derive(Serialize)]
struct MatchEntry {
    row_kind: RowKind,  // serialized as "matched" / "diff_only" / "viz_only"
    side: String,
    diff: DiffRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    selection: Option<SelectionView>,
}
```

- For `DiffOnly` rows: `selection` is `None` (omitted from JSON).
- For `VizOnly` rows: `diff` is an all-default `DiffRow` (empty fields).
  `DiffRow` needs `#[derive(Default)]` to support this — add it in Step 1.
- For `Matched` rows: both are populated (as today).

The JSON top-level `matched_count` is renamed to `row_count` to reflect that
it now counts all rows, not just matched ones. A separate `matched_count`
field (counting only `RowKind::Matched` rows) is added alongside it.

### 5. Clarify `IntegrateReport.matched_count` semantics

The `IntegrateReport` struct currently has:

```rust
pub struct IntegrateReport {
    pub matched_count: usize,  // currently = rows.len() (inner join)
    ...
}
```

With full outer join this field is ambiguous. It is **renamed to `row_count`**
(to match the JSON rename) and counts all rows. A separate `matched_count`
field is added that counts only `RowKind::Matched` rows:

```rust
pub struct IntegrateReport {
    pub row_count: usize,       // all rows (Matched + DiffOnly + VizOnly)
    pub matched_count: usize,   // only Matched rows
    pub diff_path: String,
    pub sel_path: String,
    pub rows: Vec<MergedRow>,
}
```

The orchestrator in `lib.rs` computes both counts after calling `merge_diff_viz()`.
The CLI command output message ("Integrated report written to ...") should report
`row_count` instead of the old `matched_count`.

### 6. Update stale warning message in `integrate_diff_viz()`

The current orchestrator warns:

```rust
if rows.is_empty() {
    log::warn!("no matches found between diff and selections");
}
```

This message is misleading with full outer join: empty rows only occur when
*both* the diff CSV has no Person rows *and* the selections array is empty.
Update it to:

```rust
if rows.is_empty() {
    log::warn!("no data to integrate: diff CSV contains no Person rows and selections are empty");
}
```

### 7. CSV output format

The CSV header gets one new column:

```
classification,item_type,handle_a,...,side,row_kind,viz_name,viz_birth_date,viz_death_date,viz_gender,viz_family_group
```

`row_kind` is inserted immediately after `side` and before the viz columns.
This positions the discriminator in a logical place — grouping metadata
columns together.

For `DiffOnly` rows: `viz_name` etc. are empty cells.
For `VizOnly` rows: `classification` etc. are empty cells.
For `Matched` rows: all cells populated (as today).

## Implementation Plan

### Step 1 — Make viz fields optional + add RowKind + add Default for DiffRow

| Commit | `refactor: make viz fields optional in MergedRow, add RowKind, add Default for DiffRow` |
|---|---|
| Files | `crates/integrate/src/merge.rs` (part 1), `crates/integrate/src/csv_reader.rs` |
| Changes | **In `MergedRow`**: change `viz_name: String` → `viz_name: Option<String>`, `viz_gender: String` → `viz_gender: Option<String>`, `viz_family_group: usize` → `viz_family_group: Option<usize>`. Add `RowKind` enum with `#[serde(rename_all = "snake_case")]`, variants `Matched`, `DiffOnly`, `VizOnly`. Add `row_kind: RowKind` field to `MergedRow`. **In `DiffRow`**: add `#[derive(Default)]` so it can serve as the all-empty diff for `VizOnly` JSON entries. |
| Tests | Update existing tests to use `Some(...)` for viz fields in matched rows. No behavioral change yet — `merge_diff_viz` still emits only matched rows, so all tests still pass. |

### Step 2 — Change merge logic to full outer join

| Commit | `feat: change diff-viz merge from inner join to full outer join` |
|---|---|
| Files | `crates/integrate/src/merge.rs` (part 2) |
| Changes | Rewrite `merge_diff_viz()` to: (1) emit diff-only rows instead of skipping unmatched diff rows, (2) track matched selection handles, (3) emit viz-only rows for unmatched selections. |
| Tests | Add test cases: diff-only row emitted with `RowKind::DiffOnly` and `None` viz fields; viz-only row emitted with `RowKind::VizOnly` and empty diff fields; matched row still produces `RowKind::Matched` with populated fields. Test full outer join: 2 diff rows (1 matched, 1 unmatched) + 2 selections (1 matched, 1 unmatched) → 4 rows total. |

### Step 3 — Update JSON output format

| Commit | `feat: update format_json for outer join semantics` |
|---|---|
| Files | `crates/integrate/src/output.rs` (JSON part) |
| Changes | Update `MatchEntry` to include `row_kind: RowKind`. Make `selection` field `Option<SelectionView>` with `skip_serializing_if = Option::is_none`. Change `matched_count` → `row_count` in `JsonOutput`, add `matched_count` as a separate field. |
| Tests | JSON: diff-only row has no `selection` field; viz-only row has diff fields but null handles; matched row has both. Update existing JSON tests to use new field names. **Also update `PropRow` struct and proptest strategies** in `output.rs` to include `row_kind: RowKind` and the new optional viz fields. Ensure the `prop_csv_roundtrip` and `prop_json_structure` property-based tests still pass after field changes. |

### Step 4 — Update CSV output format

| Commit | `feat: add row_kind column to CSV output` |
|---|---|
| Files | `crates/integrate/src/output.rs` (CSV part) |
| Changes | Add `row_kind` to `CSV_HEADER` immediately after `side`. |
| Tests | CSV header length becomes 21. Round-trip tests: verify `row_kind` column round-trips correctly. Viz-only row: diff columns are empty, viz columns populated. Diff-only row: viz columns empty, diff columns populated. |

### Step 5 — Update integration tests for full outer join

| Commit | `test: update integration tests for full outer join including viz-only rows` |
|---|---|
| Files | `crates/integrate/tests/integration.rs` |
| Changes | Rewrite `integrate_diff_viz_matches` to verify: 3 matched rows + any diff-only + any viz-only. Add `integrate_diff_viz_selections_only` test: selections with handles not in diff → viz-only rows emitted. Rename `integrate_diff_viz_no_matches` to `integrate_diff_viz_data_all_unmatched` — now rows ARE emitted (as diff-only or viz-only), so test asserts >0 rows. |
| E2E files | `crates/cli/tests/e2e.rs` — update `e2e_integrate_diff_viz_csv_output` to check for `row_kind` column. Add `e2e_integrate_diff_viz_unmatched`: test that a diff row without matching selection still appears. |

## Backward Compatibility

### CSV format

- **New column added:** `row_kind` inserted between `side` and `viz_name`.
  Parsers that use header names (like the `csv` crate) will ignore unknown
  columns but will still get the new column if they deserialize a compatible
  struct. Parsers that use positional indexing will break — this is acceptable
  because the integrate CSV is an output format, not an interchange format
  consumed by other gramps-gen tools.

- **Viz fields now `Option`:** The CSV serializer already maps `None` to
  empty cells, so existing tools that read the integrate CSV output will
  see the same values for matched rows. Diff-only rows will have empty viz
  columns (expected new behavior).

### JSON format

- **`matched_count` → `row_count`:** Breaking rename. Both fields are
  included for one release to ease migration, then `matched_count` can be
  deprecated in a follow-up.

- **`selection` is now nullable:** `null` for diff-only rows, omitted
  (via `skip_serializing_if`) for cleaner output.

### CLI API

- No CLI flags change. The `integrate diff-viz` subcommand has the same
  arguments (`--diff`, `--selections`, `--output`, `--format`).

## Risk Assessment

- **Large unmatched sets:** If the diff CSV has many Person rows and only
  a few are selected in the visualizer, the output will be large. This is
  the desired behavior — it's a reporting tool, and the user expects all
  diff rows to appear. No performance concern (O(n × log m) unchanged).

- **Viz-only rows lack handle context:** A viz-only row has no `handle_a`
  or `handle_b`. The selection's handle is stored in the selection data
  (it's part of the `Selection` struct), but does not appear in the
  current `MergedRow` output columns. Consider adding the selection handle
  as a new viz column (`viz_handle`) in a follow-up if users need it.

- **Classification of viz-only rows:** The diff `classification` column is
  empty. Downstream consumers must check `row_kind` instead. This is
  clearly documented in the column description.

## Future Work

- [ ] Add `viz_handle` column to expose the selection's handle for viz-only rows
- [ ] Add `--show diff-only` / `--show viz-only` / `--show matched` filter flags
- [ ] Support a `--merge-handle` flag to copy the matching selection handle into `handle_a` or `handle_b` for easier downstream processing
