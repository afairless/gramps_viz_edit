# Implementation Plan: Family Group Statistics

Source: `docs/research/family-group-stats.md`

This plan reworks the existing `Family size × generation table` (which conflates Gramps families with family groups) into a self-consistent `Family group size × generation table` that counts connected components of the person graph. It also adds a new `Family group distribution` section.

**Branch:** `agent/family-generation-table` (the existing feature branch for the generation table)

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `refactor(cli): Rename FamilyGenerationTable to FamilyGroupGenerationTable and add family_group_distribution` | Type/field renames | `crates/cli/src/commands/stats/count.rs`, `crates/cli/src/commands/stats/mod.rs` | Unit (compile check, default values) |
| 2 | `feat(cli): Rewrite generation table to count family groups (connected components) instead of Gramps families` | Component-level tabulation | `crates/cli/src/commands/stats/count.rs` | Unit (multi-family collapse, isolated persons, disconnected components, empty graph, family group distribution) |
| 3 | `feat(cli): Add "Family group distribution" section and update table rendering in text report` | Text report update | `crates/cli/src/commands/stats/mod.rs` | Unit (new section, updated table title, renamed functions) |
| 4 | `test(cli): Update unit tests for family group statistics` | Unit test updates | `crates/cli/src/commands/stats/count.rs`, `crates/cli/src/commands/stats/mod.rs` | Unit (all existing and new tests pass) |
| 5 | `test(cli): Update integration and E2E tests for family group statistics` | Integration/E2E test updates | `crates/cli/tests/integration.rs`, `crates/cli/tests/e2e.rs` | Integration, E2E (full test suite passes) |

## Step Details

### Step 1 — Rename types and fields

- Rename `FamilyGenerationTable` → `FamilyGroupGenerationTable` (keep the same `BTreeMap` shape)
- Rename `family_generation_table` → `family_group_generation_table` on `StatsReport`
- Add `family_group_distribution: BTreeMap<usize, usize>` field to `StatsReport` (default: empty)
- Update `StatsReport::default()` and `Default` derive
- Update `count_gramps_xml` return to populate `family_group_generation_table` (rename only, behavior unchanged)
- Update `mod.rs` imports and usages of the renamed type/field
- Update doc comments on `StatsReport` to distinguish Gramps families vs. family groups

**Tests:** Compile check; `report_default_empty_table` verifies new field defaults; `json_output_contains_generation_table` updated for field name.

### Step 2 — Rewrite tabulation to count components

- Rewrite the final tabulation loop in `compute_generation_table` (renamed `compute_family_group_table`) to iterate over **components** (including isolated-person components with zero family records) instead of iterating over `family_records`
- Populate `family_group_distribution` in the same component iteration loop: for each component, increment `distribution[component.len()]`
- The DSU and component-building code in the first half of the function remains unchanged
- Keep the same function signature but change the tabulation logic

**Key behavioral changes:**

- Multi-family components collapse to a single row (size = total people in component, span = component's generation span)
- Isolated persons (no family records) appear as size-1, span-1 family groups
- Empty graph still produces empty table

**Tests:**

- `generation_table_multi_family_collapse` — multiple Gramps families in one component → single family-group row
- `generation_table_isolated_person_components` — isolated persons appear as size-1 family groups
- `family_group_distribution_empty` — empty graph → empty map
- `family_group_distribution_single` — single component → one entry
- `family_group_distribution_multiple` — multiple components of varying sizes
- Update existing tests for new semantics (e.g., `generation_table_pedigree_collapse` now produces one row instead of four)

### Step 3 — Update text report rendering

- Add "Family group distribution" section to `format_text_report` (after "Family size distribution", before the generation table)
- Update table title: `"Family size × generation table"` → `"Family group size × generation table"`
- Rename rendering functions: `render_generation_table` → `render_family_group_table`, `render_generation_table_ascii` → `render_family_group_table_ascii`, `render_generation_table_inner` → `render_family_group_table_inner`
- Update field references from `report.family_generation_table` to `report.family_group_generation_table`

**Tests:**

- Update `format_text_report_contains_generation_table_section` for new table title and new section
- Update render table tests for renamed functions
- Add test for "Family group distribution" section rendering

### Step 4 — Update unit tests

- In `count.rs` tests: update `json_output_contains_generation_table` for renamed JSON field; update `generation_table_integration` for renamed field; update all generation table tests for new semantics (component-level counting)
- In `mod.rs` tests: update `format_text_report_expected_output` for new section; update `render_generation_table_*` tests for renamed functions; update `format_text_report_contains_generation_table_section`
- Add new tests from Step 2

**Tests:** All unit tests pass.

### Step 5 — Update integration and E2E tests

- `integration.rs`: Update `stats_count_known_graph` to use new field name `family_group_generation_table` and add `family_group_distribution` assertion
- `e2e.rs`: Rename `family_generation_table` → `family_group_generation_table` in JSON assertions; add `family_group_distribution` assertion; update text output assertions for new table title

**Tests:** `cargo test --workspace` passes; `cargo clippy --all-targets --all-features -- -D warnings` passes.
