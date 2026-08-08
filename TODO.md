# Implementation Plan: Integrate — Inner Join to Full Outer Join

Source: `docs/research/integrate-outer-join.md`

## Branch

Create a new branch `agent/integrate-outer-join` from `main`.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `refactor: make viz fields optional in MergedRow, add RowKind, add Default for DiffRow` | Type changes | `crates/integrate/src/merge.rs`: `MergedRow` — `viz_name: Option<String>`, `viz_gender: Option<String>`, `viz_family_group: Option<usize>`; add `RowKind` enum (`Matched`, `DiffOnly`, `VizOnly`) with `#[serde(rename_all = "snake_case")]`; add `row_kind: RowKind` field. `crates/integrate/src/csv_reader.rs`: add `#[derive(Default)]` to `DiffRow`. `crates/integrate/src/output.rs`: update `PropRow` struct and `MergedRow::from(PropRow)` impl to match new fields. | Unit, proptest |
| 2 | `feat: change diff-viz merge from inner join to full outer join` | Merge logic rewrite | `crates/integrate/src/merge.rs`: rewrite `merge_diff_viz()` — emit `DiffOnly` rows for unmatched diff rows, track matched selection handles in `HashSet<String>`, emit `VizOnly` rows for unmatched selections. Remove early returns when selections are empty or diff Person rows are empty. Update unit tests: `no_match_excluded` → asserts `DiffOnly` row; `empty_selections` → asserts `VizOnly` rows (or empty if no selections at all); `person_rows_no_match` → asserts `DiffOnly` rows. Add new tests: `emits_diff_only_row`, `emits_viz_only_row`, `full_outer_join_4_rows`. | Unit |
| 3 | `feat: update format_json for outer join semantics` | JSON output | `crates/integrate/src/output.rs`: add `row_kind: RowKind` to `MatchEntry`; make `selection` field `Option<SelectionView>` with `skip_serializing_if`; rename `JsonOutput.matched_count` → `JsonOutput.row_count`; add `JsonOutput.matched_count` as separate field; update `From<&MergedRow> for DiffRow` (already works — Default used for VizOnly). Update unit tests. | Unit, proptest |
| 4 | `feat: add row_kind column to CSV output` | CSV output | `crates/integrate/src/output.rs`: add `"row_kind"` to `CSV_HEADER` immediately after `"side"` (header becomes 21 columns). `crates/integrate/src/merge.rs`: `MergedRow` already has `row_kind` and the CSV serializer picks it up via `Serialize`. Update unit tests for header length. | Unit, proptest |
| 5 | `test: update integration and E2E tests for full outer join including viz-only rows` | Integration & E2E tests | `crates/integrate/tests/integration.rs`: update `integrate_diff_viz_matches` — 3 matched rows + potential diff-only/viz-only; rename `integrate_diff_viz_no_matches` to `integrate_diff_viz_data_all_unmatched` — now asserts rows > 0; add `integrate_diff_viz_selections_only` — viz-only rows emitted. `crates/cli/tests/e2e.rs`: update `e2e_integrate_diff_viz_csv_output` — check for `row_kind` column; update `e2e_integrate_diff_viz_wrapped_envelope` — abc-3 now appears as diff-only row, update assertion; add `e2e_integrate_diff_viz_unmatched` — test that unmatched diff row still appears. | Integration, E2E |

## Known issues addressed during implementation

- **Old unit tests asserting empty for no-matches** (`no_match_excluded`, `empty_selections`, `person_rows_no_match`) — updated in Step 2 to assert `DiffOnly`/`VizOnly` rows instead.
- **E2E test `e2e_integrate_diff_viz_wrapped_envelope` asserting abc-3 not in output** — updated in Step 5 since abc-3 now appears as a `DiffOnly` row.
- **PropRow in output.rs** — updated in Step 1 to match new `MergedRow` fields, keeping the build compilable after type changes.
- **`IntegrateReport.matched_count`** — renamed to `row_count`; a separate `matched_count` field counts only `RowKind::Matched` rows. Update in Step 3 (JSON) and Step 5 (integration tests checking the report).
- **CLI command** — does not display count in its output message, so no CLI changes needed beyond the struct rename.
