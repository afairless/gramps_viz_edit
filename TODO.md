# Implementation Plan: Family-Size × Generations Contingency Table

Source: `docs/research/family-generation-table.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat(cli): Replace family_members with FamilyRecord for parent-child separation` | FamilyRecord struct and streaming pass update | `crates/cli/src/commands/stats/count.rs` | Unit |
| 2 | `feat(cli): Implement connected-component generation layering` | `compute_generation_table()` with DSU and longest-path layering | `crates/cli/src/commands/stats/count.rs` | Unit |
| 3 | `feat(cli): Add FamilyGenerationTable to StatsReport` | Wired generation table into stats report | `crates/cli/src/commands/stats/count.rs` | Unit, Integration |
| 4 | `feat(cli): Render generation table in text output` | `render_generation_table()` with Unicode box-drawing and `--no-unicode` flag | `crates/cli/src/commands/stats/mod.rs` | Unit |
| 5 | `test(cli): Update E2E tests for generation table` | E2E assertions for text and JSON output | `crates/cli/tests/e2e.rs` | Integration |
| 6 | `test(cli): Update integration tests for generation table` | Integration test for known graph | `crates/cli/tests/integration.rs` | Integration |
| 7 | `chore: Run full test suite` | Verify all tests pass and clippy is clean | — | — |

---

## Step 1 — FamilyRecord struct and streaming pass update

**Commit:** `feat(cli): Replace family_members with FamilyRecord for parent-child separation`

**Files:** `crates/cli/src/commands/stats/count.rs`

Replace `family_members: Vec<HashSet<String>>` with `family_records: Vec<FamilyRecord>` where `FamilyRecord` has:

- `size: usize` — distinct count of handles across both parents and children
- `parent_handles: Vec<String>` — handles from `<father>` and `<mother>` refs
- `child_handles: Vec<String>` — handles from `<childref>` refs

In the streaming pass, separate parent refs (`father`/`mother`) from child refs (`childref`) when recording family data. For self-closing `<family/>`, push a `FamilyRecord { size: 0, parent_handles: vec![], child_handles: vec![] }`. Keep `histogram: HashMap<usize, usize>` logic unchanged.

Also add optional `person_parent_families: HashMap<String, Vec<usize>>` and `person_child_families: HashMap<String, Vec<usize>>` for debugging.

**Tests:** Existing tests must still pass. The family size distribution is unchanged.

**Test verification:**

- `cargo test -p cli --lib -- commands::stats::count`
- All existing count tests pass with the new struct

---

## Step 2 — Connected-component generation layering

**Commit:** `feat(cli): Implement connected-component generation layering`

**Files:** `crates/cli/src/commands/stats/count.rs`

Define `pub type FamilyGenerationTable = BTreeMap<String, BTreeMap<String, usize>>` at the top of `count.rs`.

Implement `fn compute_generation_table(family_records: &[FamilyRecord], all_handles: &HashSet<String>) -> FamilyGenerationTable`:

1. Build a `Dsu` (Disjoint Set Union) over person handles that exist in `all_handles`. Union all members of each family.
2. For each DSU connected component, build parent→child edges and assign generation numbers via longest-path layering from roots (people with no parents).
3. Handle cycles: detect via visited-set, cap at 50, emit warning.
4. For each nuclear family, compute `family_size` (distinct handles) and `family_gen_span` (component span of any member). Increment table cell.
5. Return the nested `BTreeMap` representation.

**New tests:**

- `generation_table_empty` — no families → empty table
- `generation_table_single_family_no_children` — size 2, 1 gen
- `generation_table_single_family_with_children` — size 3, 2 gens
- `generation_table_two_family_chain` — two families forming a parent→child chain → 3 gens
- `generation_table_three_generation_chain` — grandparent→parent→child→grandchild → 4 gens
- `generation_table_isolated_person` — single person, no families → empty table
- `generation_table_disconnected_components` — two independent components
- `generation_table_pedigree_collapse` — cousins marry; DAG preserved
- `generation_table_cycle` — artificial cycle → warning, layering caps at 50
- `generation_table_duplicate_handles` — same person in multiple families
- `generation_table_single_parent_family` — single parent + child → size 2, 2 gens
- `generation_table_child_only_family` — only children, no parents → all gen 0, 1 gen
- `generation_table_single_member_family` — family with one parent, no children → size 1, 1 gen

---

## Step 3 — FamilyGenerationTable in StatsReport

**Commit:** `feat(cli): Add FamilyGenerationTable to StatsReport`

**Files:** `crates/cli/src/commands/stats/count.rs`

- Add `pub family_generation_table: FamilyGenerationTable` field to `StatsReport`
- Type alias already defined in Step 2; `BTreeMap` derives `Serialize`/`Deserialize` by default
- Update `StatsReport` construction in `count_gramps_xml()` to call `compute_generation_table()` and populate the field
- Add warnings from cycle detection to `report.warnings`
- Update `StatsReport::default()` if needed (empty `BTreeMap` is default, so no change)

**Tests:**

- `json_output_contains_generation_table` — JSON round-trip includes the new field
- `report_default_empty_table` — default report has empty table
- `generation_table_integration` — generate a known graph (via `GraphBuilder` + `GraphXmlWriter`), count via `count_gramps_xml`, verify the table matches expected values (existing `stats_count_known_graph` test in integration.rs can be extended)

---

## Step 4 — Text table rendering

**Commit:** `feat(cli): Render generation table in text output`

**Files:** `crates/cli/src/commands/stats/mod.rs`

Add `render_generation_table(table: &FamilyGenerationTable) -> String`:

- Unicode box-drawing (`│`, `─`, `┼`) for terminal output
- Auto-sized column widths
- Header: "Family size × generation table" then column labels
- Row labels: "# people" on first row, numeric size thereafter
- Marginal sums (row totals and column totals)
- Fallback to ASCII (`|`, `-`, `+`) when `--no-unicode` is set

Add `--no-unicode` flag to `StatsArgs`.

Call `render_generation_table` from `format_text_report` and append after "Family size distribution" section.

Add "Warnings" section at end of text report (renders `report.warnings`).

**Tests:**

- `render_generation_table_empty` — no rows → "No data" or empty section
- `render_generation_table_single_row` — one row
- `render_generation_table_multi_row` — multiple rows, column widths
- `render_generation_table_unicode_ascii` — both modes produce correct alignment
- `format_text_report_contains_generation_table_section` — full report includes the new section
- `format_text_report_warnings` — report with cycle warnings renders them in text output

---

## Step 5 — E2E tests

**Commit:** `test(cli): Update E2E tests for generation table`

**Files:** `crates/cli/tests/e2e.rs`

- `e2e_stats_text_output` — add assertion that the generation table section appears in stdout
- `e2e_stats_json_output` — add assertion that `family_generation_table` is present in parsed JSON
- Existing assertions must still pass

---

## Step 6 — Integration tests

**Commit:** `test(cli): Update integration tests for generation table`

**Files:** `crates/cli/tests/integration.rs`

- `stats_count_known_graph` — add assertions for the generation table (the existing test builds a 3-person family; the generation table should have one entry: size 3, span 2 generations)

---

## Step 7 — Full test suite verification

**Commit:** `chore: Run full test suite`

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

No code changes. Verify everything passes.
