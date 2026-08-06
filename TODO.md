# Implementation Plan: Gramps Diff Analyzer (Remaining Steps)

Source: `docs/research/gramps-diff-plan.md`

> Steps 1–10 are complete. The two remaining steps are listed below.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: wire run_diff orchestrator` | Diff orchestrator | `crates/diff/src/lib.rs` — `run_diff(file_a, file_b, config, schema) -> Result<DiffReport, DiffError>` where `config: DiffConfig { thresholds, include_extrinsic, normalize_enabled }`. Orchestrates: parse both files → match (Pass 1) → cascade (Pass 2) → apply thresholds → return report. Parse failures return `DiffError::ParseError` (no partial results — both files must parse). Schema version mismatch: diff proceeds using the higher version's merged schema with a warning in the report. Initial implementation loads both graphs in memory; future optimization can stream one graph while indexing the other. | Integration (fixtures generated via `generate_random()` with fixed seeds: identical → all SAME; add one person → one ADDED; modify note text → one MODIFIED; change handle refs matched as SAME → EXTRINSIC_ONLY) |
| 2 | `feat: add gramps-gen diff CLI subcommand` | CLI subcommand | `crates/cli/src/commands/diff.rs` — `DiffArgs` with `file_a`, `file_b`, `--output`, `--output-file`, `--threshold`, `--no-normalize`, `--include-extrinsic`, `--summary-only`. `run(args)` maps `--no-normalize` to `DiffConfig::normalize_enabled` and threads into `diff::run_diff()`. Writes to stdout/file. Update `crates/cli/src/commands/mod.rs` and `crates/cli/src/main.rs`. | Smoke (compiles, help text works). Integration (subprocess E2E test with two generated files) |
