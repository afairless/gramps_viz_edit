# Implementation Plan: Architecture Cleanup (3 Issues)

Source: `docs/research/cleanup-plan.md` (based on `docs/research/architecture-cli-review.md`)

Three independent cleanup issues in the gramps-gen workspace:

- **#2** — Duplicate `strategy_from_name` private functions in two CLI files; deduplicate
  into a single `AdversarialStrategy::from_name` constructor on the enum in `typed-graph`.
- **#4** — Dead `extract-schema` CLI stub; remove from the CLI surface entirely.
- **#5** — `visualize` reads the `.gramps` file twice (`load_graph` + `get_stats`); read
  once and return both `GraphData` and `StatsReport` in a combined `LoadedGraph`.

All three are independent. Implementation order = easiest first, per the plan.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `refactor(cli): remove extract-schema stub command` | Remove dead CLI stub | `crates/cli/src/commands/extract_schema.rs` (del), `crates/cli/src/commands/mod.rs`, `crates/cli/src/main.rs`, `crates/cli/tests/e2e.rs` | integration |
| 2 | `feat(typed-graph): add AdversarialStrategy::from_name` | `from_name` constructor | `crates/typed-graph/src/generate/adversarial.rs` | unit |
| 3 | `refactor(cli): deduplicate strategy_from_name onto from_name` | Replace both call sites | `crates/cli/src/commands/generate.rs`, `crates/cli/src/scenario.rs` | unit |
| 4 | `feat(visualize): add LoadedGraph and load_graph_data_with_stats` | Combined loader | `crates/visualize/src/lib.rs` | unit |
| 5 | `feat(visualize): return LoadedGraph from load_graph IPC` | Tauri IPC wiring | `crates/visualize/src/main.rs` | smoke |
| 6 | `feat(visualize): consume LoadedGraph stats in frontend` | Frontend consumes stats | `crates/visualize/frontend/src/types.ts`, `crates/visualize/frontend/src/main.ts` | vitest |
| 7 | `test(visualize): cover load_graph_data_with_stats` | Test pass | `crates/visualize/src/lib.rs` | unit |

## Step details

### Step 1 — Remove `extract-schema` stub (Issue #4)

Delete `crates/cli/src/commands/extract_schema.rs`. Remove the `pub mod extract_schema;`
line from `crates/cli/src/commands/mod.rs`. In `crates/cli/src/main.rs`, remove the
`use cli::commands::extract_schema;` import, the `ExtractSchemaArgs` type alias, the
`Command::ExtractSchema` enum variant, and the `Command::ExtractSchema(args) => ...`
match arm. Users must see no mention of `extract-schema` in `--help`.

Add a regression test in `crates/cli/tests/e2e.rs` asserting `extract-schema` and
`extract_schema` do not appear in `--help` output.

**Verify:** `cargo test -p cli`, `cargo build --release`, then
`./target/release/gramps-gen --help | grep -q extract-schema && echo FAIL || echo OK`.

### Step 2 — Add `AdversarialStrategy::from_name` (Issue #2, part 1)

Add `pub fn from_name(name: &str) -> Option<Self>` to the `impl AdversarialStrategy`
block in `crates/typed-graph/src/generate/adversarial.rs`. Accepts hyphenated and
underscored aliases; returns `None` for unrecognized names. This is purely additive —
the two private CLI functions remain in place until Step 3.

Add unit tests (None-One-Many): unrecognized/empty → `None`; single hyphenated name;
single underscored name; aliases mapping to the same variant; default fraction
parameters preserved.

**Verify:** `cargo test -p typed-graph`.

### Step 3 — Replace both call sites (Issue #2, part 2)

In `crates/cli/src/commands/generate.rs`, delete the private `fn strategy_from_name`
(~line 398) and replace its call in `parse_adversarial_flag` with
`AdversarialStrategy::from_name(s)` (including the `ok_or_else` error closure). In
`crates/cli/src/scenario.rs`, delete the private `fn strategy_from_name` (~line 116)
and replace its call in `Scenario::to_adversarial_config` with
`AdversarialStrategy::from_name(s)`.

**Verify:** `cargo test -p cli` (existing `parse_adversarial_flag` and scenario tests
catch regressions).

### Step 4 — Add `LoadedGraph` + `load_graph_data_with_stats` (Issue #5a)

In `crates/visualize/src/lib.rs`, add a `LoadedGraph` struct (with `serde` derives)
holding `graph_data: GraphData` and `stats: gramps_reader::StatsReport`, and a
`load_graph_data_with_stats(path, no_impute, generation_gap)` function that reads the
file **once**, runs `count_gramps_xml` + the existing extraction pipeline on the same
in-memory `&str`, and returns `LoadedGraph`. Make `load_graph_data` delegate to it
(discarding `stats`) to preserve the public API and eliminate pipeline duplication.

**Verify:** `cargo test -p visualize` (existing `load_graph_data` tests must still pass).

### Step 5 — Wire into Tauri `load_graph` IPC (Issue #5b)

In `crates/visualize/src/main.rs`, change the `load_graph` Tauri command's return type
from `Result<GraphData, String>` to `Result<visualize::LoadedGraph, String>` and call
`load_graph_data_with_stats` instead of `load_graph_data`. Keep `get_stats` unchanged
for standalone use.

**Verify:** compile check (`cargo check -p visualize` / `cargo build --release --features visualize`).

### Step 6 — Frontend consumes `LoadedGraph` (Issue #5c)

In `crates/visualize/frontend/src/types.ts`, add an `LoadedGraph` interface wrapping
`graph_data: GraphData` and `stats: StatsReport`. In `main.ts`, update
`openAndRenderFile` and `openAndRenderFileFromPath` to destructure the `LoadedGraph`
response and pass `stats` to `renderGraphFromData`. Update `renderGraphFromData` to
accept an optional `StatsReport` and render it directly (falling back to
`fetchAndRenderStats` when absent, e.g. dev mode). Keep `fetchAndRenderStats` for
standalone `get_stats` use.

**Verify:** `npx vitest run` in `crates/visualize/frontend`.

### Step 7 — Test `load_graph_data_with_stats` (Issue #5d)

Add unit tests in `crates/visualize/src/lib.rs`: valid file (graph data matches
`load_graph_data` output, stats populated), nonexistent file ("Cannot read file"),
malformed XML ("Failed to parse Gramps XML"), empty file ("No people found"), and
backward-compatibility delegation (`load_graph_data` == `load_graph_data_with_stats().graph_data`).

**Verify:** `cargo test -p visualize`, `cargo test -p gramps-reader`, `cargo test -p cli`,
`cargo build --release --features visualize`. Manual smoke test of the visualizer.

## Notes

- No new dependencies; no `Cargo.toml` changes required.
- Issue #5's IPC contract change (returning `LoadedGraph` instead of `GraphData`) is
  the only breaking change; the visualizer should be manually smoke-tested after Step 7.
- The deeper DSU/generation duplication between `count_gramps_xml` and
  `build_graph_data` is explicitly deferred to a future pass.
