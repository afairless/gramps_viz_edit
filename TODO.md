# Implementation Plan: Gramps Diff Analyzer (Steps 13–14)

Source: `docs/research/gramps-diff-plan.md`

**Steps 1–12 are complete.** This plan covers only the remaining two steps.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 13 | `feat: add interactive resolution for ambiguous matches` | Interactive resolution | `crates/diff/src/resolve.rs` (new) — `run_interactive_resolution(cases, resolutions_file) -> ResolvedMatches` with terminal TUI loop presenting side-by-side candidate details and edge context. Saves/loads `.gramps-diff-resolve.json`. `crates/diff/Cargo.toml` — add `resolve` feature, add `crossterm` as optional dep behind the feature. `crates/diff/src/lib.rs` — add `#[cfg(feature = "resolve")] pub mod resolve;` and conditionally re-export `ResolvedMatches`/`run_interactive_resolution`. | Unit (resolution file save/load JSON round-trip), Smoke (crate compiles with `--features resolve`) |
| 14 | `test: add integration tests for diff pipeline` | Diff integration tests | `crates/diff/tests/integration.rs` (extend) — three new test cases: (1) generate two graphs differing by one added person → exactly one ADDED item; (2) generate two graphs with modified note text on an otherwise identical person → one MODIFIED item with `FieldKind::Text` similarity; (3) generate two graphs where only handle references change (extrinsic-only) → one EXTRINSIC_ONLY item. | Integration (three precise end-to-end test cases covering ADDED, MODIFIED, EXTRINSIC classifications) |
