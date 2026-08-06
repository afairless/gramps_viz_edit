# Implementation Plan: Gramps Diff Analyzer (Steps 9–10)

Source: `docs/research/gramps-diff-plan.md`

> **Prerequisite status:** Steps 1–8 are complete.
>
> - Step 1 (full graph parser) — done via `gramps-reader`
> - Step 2 (diff crate skeleton) — `crates/diff/Cargo.toml`, `crates/diff/src/lib.rs`
> - Step 3 (similarity) — `similarity.rs`
> - Step 4 (normalization) — `normalize.rs`
> - Step 5 (report types) — `report.rs`
> - Step 6 (compare) — `compare.rs`
> - Step 7 (matcher) — `matcher.rs`
> - Step 8 (cascading) — `cascading.rs`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 9 | `feat: add text and JSON output formatters` | Output formatters | `crates/diff/src/output.rs` — `format_text(report, include_extrinsic) -> String` (summary table + per-item details with optional extrinsic filtering) and `format_json(report) -> String` (compact JSON). Update `crates/diff/src/lib.rs` to add `pub mod output;`. | Unit (text output contains expected headings and summary counts; JSON round-trips via serde; empty report renders correctly; extrinsic-only items are omitted from text when `include_extrinsic: false`; all classification labels appear for text output) |
| 10 | `feat: add visualizer index output format` | Visualizer index | `crates/diff/src/visualizer_index.rs` — `format_visualizer(report) -> String` producing compact JSON with `handle_map` (all matched A↔B handle pairs) + per-handle entry `{class, intrinsic_fields?, text_scores?}`. Update `crates/diff/src/lib.rs` to add `pub mod visualizer_index;`. | Unit (output parses as JSON; all 6 `Classification` variants are represented; handle_map keys/values match report items; intrinsic_fields appear for MODIFIED items; text_scores appear for MODIFIED items with text field changes; empty report produces valid JSON) |
