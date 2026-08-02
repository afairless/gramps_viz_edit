# Implementation Plan: Family Group Visualization Tool

Source: `docs/research/family-group-visualization.md`

## Design Decisions (baked into this plan)

1. **gramps-reader error type:** `count_gramps_xml` currently returns `CliError::XmlParseError` from the `cli` crate. When extracted into `gramps-reader`, a new `gramps_reader::Error` enum (with `XmlParseError` variant) replaces it. `cli` maps `gramps_reader::Error` to `CliError` via `From`.
2. **Per-node generation exposure:** The existing `compute_generation_table` returns a contingency table, not per-handle generations. A new `pub fn compute_generations(family_records, all_handles) -> HashMap<String, usize>` is added to `gramps-reader::graph` (refactored from the same relaxation loop); `compute_generation_table` uses it internally. This gives `graph_data.rs` the per-node generations it needs for visualization.
3. **Feature gating:** `tauri`/`tauri-build` are optional deps behind the `visualize` feature in `crates/visualize/Cargo.toml`. The `[[bin]] gramps-gen-visualize` has `required-features = ["visualize"]`. Without the feature, the crate's lib compiles fine (no WebKit2GTK needed). Workspace `default-members` excludes `visualize` so `cargo build` at root doesn't require system deps. Full Tauri build: `cargo build -p visualize --features visualize` (requires `libwebkit2gtk-4.1-dev` on Linux).
4. **Frontend dist placeholder:** `crates/visualize/frontend/dist/index.html` (placeholder) is created in the scaffold step so `tauri-build` can find frontend assets when the `visualize` feature is enabled, even before the real frontend is built.
5. **E2E visualize test:** The cli crate's E2E tests verify argument validation and graceful error when the sibling `gramps-gen-visualize` binary doesn't exist (default build). Full Tauri app spawning is not tested headlessly.

## Steps

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `refactor: Extract streaming XML stats into gramps-reader crate` | Shared crate extraction | `crates/gramps-reader/` (Cargo.toml, src/lib.rs, src/types.rs, src/error.rs, src/xml.rs, src/xml/count.rs, src/graph.rs); `crates/cli/Cargo.toml` (add dep), `crates/cli/src/commands/stats/count.rs` (trim), `crates/cli/src/commands/stats/mod.rs` (re-import); workspace `Cargo.toml` (add member) | Unit (all moved tests + cli tests pass) |
| 2 | `chore(visualize): Scaffold Tauri v2 crate skeleton, frontend, and workspace gate` | Crate skeleton | `crates/visualize/Cargo.toml` (optional tauri, `visualize` feature, `[[bin]] gramps-gen-visualize`), `tauri.conf.json`, `build.rs` (feature-gated tauri-build), `src/main.rs` (placeholder window); `frontend/package.json` (d3, typescript, vitest), `frontend/tsconfig.json`, `frontend/index.html`, `frontend/dist/index.html` (placeholder); workspace `Cargo.toml` (members + default-members) | Smoke (cargo build -p visualize succeeds; cargo check -p visualize --features visualize if system deps present) |
| 3 | `feat(cli): Add visualize subcommand that spawns gramps-gen-visualize` | CLI passthrough | `crates/cli/src/commands/visualize.rs` (VisualizeArgs, validation, sibling binary lookup, spawn); `crates/cli/src/commands/mod.rs` (add mod); `crates/cli/src/main.rs` (wire Command) | Unit (path/gap validation, binary-not-found); E2E (arg errors, sibling-not-found) |
| 4 | `feat(gramps-reader): Add ParsedPerson and streaming person extraction` | Person detail extraction | `crates/gramps-reader/src/types.rs` (ParsedPerson); `crates/gramps-reader/src/xml/extract.rs` (extract_persons) | Unit (full person, minimal, malformed, namespace-prefixed, gender mapping, birth_year parsing) |
| 5 | `feat(gramps-reader): Add ParsedFamily and family detail extraction` | Family detail extraction | `crates/gramps-reader/src/types.rs` (ParsedFamily); `crates/gramps-reader/src/xml/extract.rs` (extract_families) | Unit (parents+children, empty, childref multi, dangling hlink) |
| 6 | `feat(visualize): Build PersonNode/FamilyLink graph data with per-node generations` | Graph data adapter | `crates/gramps-reader/src/graph.rs` (add `compute_generations` pub fn, refactor `compute_generation_table`); `crates/visualize/src/graph_data.rs` (adapter: ParsedPerson/Family → PersonNode + FamilyLink, DSU components, generation assignment) | Unit (gender mapping, name assembly, component grouping, generation assignment); Property-based (every node generation in [0, MAX_GENERATION]) |
| 7 | `feat(visualize): Implement imputed-date algorithm` | Date imputation | `crates/visualize/src/dates.rs` (multi-source BFS, configurable gap, averaging, null fallback) | Unit (known ancestor, known descendant, multi-source, averaging, no reachable, gap 0); Property-based (ancestor≤descendant, determinism, fully-dated no-op) |
| 8 | `feat(visualize): Wire load_graph_data and Tauri IPC command` | IPC bridge | `crates/visualize/src/lib.rs` (pure fn load_graph_data, GraphData/PersonNode/FamilyLink/FamilyGroupMeta/LinkType/SelectionExport types with Serialize); `crates/visualize/src/main.rs` (#[tauri::command] load_graph, error dialogs) | Unit (valid file, empty file, malformed, not found, gap validation 1..=100) |
| 9 | `chore(frontend): Set up TypeScript/vitest/D3 toolchain` | Frontend toolchain | `crates/visualize/frontend/package.json` (d3 v7, typescript, vitest), `tsconfig.json`, `src/main.ts` (placeholder), `src/types.ts`, `npm install && npm run build` → dist/; build prerequisite documented | Smoke (npx tsc --noEmit, npx vitest run, npm run build) |
| 10 | `feat(frontend): D3 force simulation rendering` | Graph rendering | `crates/visualize/frontend/src/main.ts` (mount point), `src/graph.ts` (forceSimulation, SVG circles+lines, zoom/pan) | Unit (node/link transform, data validation against types.ts) |
| 11 | `feat(frontend): Add hover tooltips` | Tooltips | `crates/visualize/frontend/src/tooltip.ts` (200ms hover, name/birth/death, cursor-follow, mouse-out hide) | — |
| 12 | `feat(frontend): Add click-to-select and export` | Selection & export | `crates/visualize/frontend/src/selection.ts` (toggle, Shift multi-select, highlight, selection panel, Export button → Tauri save dialog) | Unit (toggle, multi-select, export shape, clear, empty) |
| 13 | `feat(frontend): Add color gradient, family-group filter, and legend` | Visual polish | `crates/visualize/frontend/src/colors.ts` (d3.interpolateViridis, imputed dashed border, neutral gray null); `src/graph.ts` (filter dropdown); legend bar | Unit (color mapping, null→gray, single-year range, year 0, negative year) |
| 14 | `test(visualize): Cross-crate integration tests` | Integration tests | `crates/visualize/tests/` (round-trip .gramps → GraphData → JSON, empty file, cycles, missing file error) | Integration (all pass with `cargo test -p visualize`) |
| 15 | `test(cli): Add E2E smoke tests for visualize subcommand` | E2E tests | `crates/cli/tests/e2e.rs` (invalid path error, gap out of range, sibling-binary-not-found graceful error) | E2E (passes without `--features visualize` since sibling-not-found path is tested) |
| 16 | `docs: Document visualize command and new crates` | Documentation | `README.md` (visualize subcommand usage, new build deps: Node.js ≥18, WebKit2GTK, `--features visualize` gate); `docs/ARCHITECTURE.md` (new crates, data flow, Tauri IPC) | — |

## Dependency Graph

```
Step 1 (gramps-reader) ────────────┐
Step 2 (visualize scaffold) ──────┤
Step 3 (CLI passthrough) ─────────┤
Step 4 (person extraction) ───────┤
Step 5 (family extraction) ───────┤
Step 6 (graph data) ──────────────┼── Step 7 (date imputation) ── Step 8 (IPC) ── Step 9 (frontend toolchain) ── Steps 10-13 (frontend features) ── Step 14 (integration tests) ── Step 15 (E2E) ── Step 16 (docs)
                                   │
Steps 1-5 are independent of each other except Step 1 (gramps-reader) is a dependency of Steps 3-6.
Steps 3-5, 9 depend only on 1 and 2 (parallelizable).
Steps 6-8 are sequential (graph data → imputation → IPC).
Steps 10-13 are sequential (simulation → tooltips → selection → colors).
Steps 14-16 are final (integration → E2E → docs).
```
