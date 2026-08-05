# Implementation Plan: Include `.xml` Extension in Visualizer File Browser

Source: `docs/research/include-xml-extension.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: accept .xml extension in CLI visualize path guard` | CLI extension guard | `crates/cli/src/commands/visualize.rs` | Unit |
| 2 | `feat: accept .xml extension in frontend file dialog and welcome screen` | Frontend dialog filter + welcome text | `crates/visualize/frontend/src/main.ts` | Unit |
| 3 | `test: add tests for .xml extension acceptance across frontend, CLI, and backend` | Cross-cutting .xml acceptance tests | `crates/visualize/frontend/tests/main.test.ts`, `crates/cli/src/commands/visualize.rs`, `crates/visualize/src/lib.rs` | Unit |
| 4 | `docs: update doc comments to reference .xml extension alongside .gramps` | Doc comment and error message updates | `crates/visualize/src/main.rs`, `crates/visualize/src/lib.rs`, `crates/visualize/src/args.rs`, `crates/cli/src/commands/visualize.rs`, `crates/gramps-reader/src/lib.rs`, `crates/gramps-reader/src/io.rs`, `crates/cli/src/commands/stats/mod.rs`, `crates/cli/src/commands/validate.rs` | — |
