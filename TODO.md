# Implementation Plan: Integrate Diff Results with Visualizer Selections

Source: `docs/research/diff-viz-integration.md`

## Design Notes

- The `integrate` crate is a new library crate under `crates/integrate/`.
- New workspace deps: `csv`. Also `log` added to workspace deps (currently a per-crate dep).
- The `crates/cli/src/error.rs` gains an `IntegrateFailed(String)` variant + `From` impl.
- `crates/integrate` added to `default-members` in root `Cargo.toml`.
- Step 4 uses `proptest` for property-based CSV↔JSON round-trip tests (dev-dep in integrate crate).

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: scaffold integrate crate with diff CSV parser` | Scaffold + CSV reader | `crates/integrate/Cargo.toml`, `crates/integrate/src/lib.rs` (IntegrateError, parse_diff_csv), `crates/integrate/src/csv_reader.rs` (DiffRow, parse_diff_csv), root `Cargo.toml` (workspace dep `csv`, default-members) | Unit: valid CSV (3 Person rows → 4 diff rows), empty CSV, no-Person CSV, special characters, invalid CSV → DiffReadError. Integration: round-trip via `diff::output::format_csv()` |
| 2 | `feat: add visualizer selections JSON parser` | JSON reader | `crates/integrate/src/json_reader.rs` (Selection, SelectionExport, parse_selections_json) | Unit: valid selections (3 people), empty array, invalid JSON → SelectionsReadError, missing field |
| 3 | `feat: implement diff-viz handle matching and row merging` | Merge logic | `crates/integrate/src/merge.rs` (MergedRow, merge_diff_viz) | Unit: match handle_a → side="a", match handle_b → side="b", Added/Removed matching, no match → excluded, same-handle-both-sides → side="a", empty selections, empty diff Person rows, zero Person rows |
| 4 | `feat: add CSV and JSON output for merged results` | Output formatters | `crates/integrate/src/output.rs` (format_csv, format_json) | Unit (CSV): header, one row, empty input, special chars, None fields. Unit (JSON): valid JSON, matches count, empty matches. Property-based: random MergedRow round-trip via proptest |
| 5 | `feat: add public integrate_diff_viz orchestrator function` | Orchestrator | `crates/integrate/src/lib.rs` (integrate_diff_viz, IntegrateReport) | Integration: temp files with known CSV+JSON, verify matched count and rows; mismatch → 0 matches |
| 6 | `feat: add gramps-gen integrate diff-viz CLI subcommand` | CLI wiring | `crates/cli/src/commands/integrate.rs` (IntegrateArgs, IntegrateMode, DiffVizArgs, run), `crates/cli/src/error.rs` (IntegrateFailed variant, From impl), `crates/cli/src/commands/mod.rs`, `crates/cli/src/main.rs` | Smoke: `--help` works. Integration: subprocess test with fixture files, verify CSV output columns/rows |
