# Documentation Update Plan — August 2025 (Comprehensive)

Date: 2025-08-08
Scope: `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`, plus three new `docs/*-tool.md` user guides
Excludes: `docs/research/`

## Background

Since the previous doc audit (2025-08-03) and update plan (2025-08-06), two new
workspace crates — `integrate` and `delete` — have been added alongside new CLI
subcommands. These are entirely absent from all three primary documentation files.
The previous update plan was never fully executed, so its remaining items are
folded into this plan.

This plan covers:

1. **Adding the `integrate` crate to all three docs** — workspace tree, crate table, architecture diagram, CLI commands
2. **Adding the `delete` crate to all three docs** — workspace tree, crate table, architecture diagram, CLI commands
3. **Fixing stale crate counts** — "six crates" → "eight crates" in ARCHITECTURE.md
4. **Updating dependency tables** across AGENTS.md, README.md, ARCHITECTURE.md (`csv` crate, `flate2`, etc.)
5. **Writing three new user guides** in `docs/` following the `diff-tool.md` pattern
6. **Adding tool overviews** to README.md with links to the new user guides
7. **~6 residual gaps** from prior audits that remain unfixed

---

## Gap Inventory

### A. Missing `integrate` crate everywhere

The `integrate` crate (`crates/integrate/`) provides merge logic for combining
`gramps-gen diff --output csv` results with visualizer selection JSON exports.
It has 6 source files, 2 integration test files, and a CLI subcommand
(`gramps-gen integrate diff-viz`).

| # | Doc | Gap |
|---|---|---|
| A1 | AGENTS.md | No `integrate/` crate in workspace structure tree |
| A2 | AGENTS.md | No `integrate.rs` under `cli/src/commands/` |
| A3 | README.md | No `integrate` row in Crate Structure table |
| A4 | README.md | No `integrate` / `integrate diff-viz` in CLI commands |
| A5 | ARCHITECTURE.md | Crate count says "six" → must be "eight" |
| A6 | ARCHITECTURE.md | No `integrate` row in crate overview table |
| A7 | ARCHITECTURE.md | No `integrate` box in ASCII architecture diagram |
| A8 | ARCHITECTURE.md | No `integrate` row in CLI commands table |
| A9 | ARCHITECTURE.md | No `integrate` section describing the crate |

### B. Missing `delete` crate everywhere

The `delete` crate (`crates/delete/`) provides a deletion cascade engine that
computes orphaned dependencies when seed people are removed from a Gramps graph.
It has 5 source files, supports interactive review, save/load manifest, and a
CLI subcommand (`gramps-gen delete`).

| # | Doc | Gap |
|---|---|---|
| B1 | AGENTS.md | No `delete/` crate in workspace structure tree |
| B2 | AGENTS.md | No `delete.rs` under `cli/src/commands/` |
| B3 | README.md | No `delete` row in Crate Structure table |
| B4 | README.md | No `delete` in CLI commands |
| B5 | ARCHITECTURE.md | No `delete` row in crate overview table |
| B6 | ARCHITECTURE.md | No `delete` box in ASCII architecture diagram |
| B7 | ARCHITECTURE.md | No `delete` row in CLI commands table |
| B8 | ARCHITECTURE.md | No `delete` section describing the crate |

### C. Additional residual gaps

| # | Doc | Gap |
|---|---|---|
| C3 | AGENTS.md | Key Dependencies table missing `csv` crate |
| C4 | README.md | First CLI commands table is non-exhaustive — only lists `schema list`, `schema download`, `diff` but omits `generate`, `stats`, `validate`, `visualize`, `delete`, `integrate` |
| C5 | ARCHITECTURE.md | Crate count text still says "six crates" (since diff was added) — now eight |
| C6 | ARCHITECTURE.md | Dependencies table missing `csv`, `flate2` (used by cli, gramps-reader, and the integrate/delete CLI commands) |

### D. New user guides (nonexistent)

The `docs/diff-tool.md` user guide sets the pattern. Following it, three new
guides are needed:

| # | File | Crate/Tool |
|---|---|---|
| D1 | `docs/visualizer-tool.md` | `visualize` — Tauri v2 desktop app with D3.js force-directed graph |
| D2 | `docs/integrate-tool.md` | `integrate` — merge diff CSV with visualizer selections |
| D3 | `docs/delete-tool.md` | `delete` — deletion cascade engine with interactive review |

`README.md` should gain a brief "Tools" section with one-paragraph overviews of
the diff, visualize, integrate, and delete tools, each linking to its user guide.

---

## Implementation Plan

Each step is a self-contained conventional commit updating 1–3 files.

### Step 1: Add `integrate` crate references to all three docs

**Commit:** `docs: add integrate crate to AGENTS.md, README.md, and ARCHITECTURE.md`

**AGENTS.md — Workspace Structure:**

Add a new `crates/integrate/` tree entry at the same level as other crates (after the `gramps-reader/` tree entry, before the `diff/` tree entry — matching the workspace directory layout):

```
├── integrate/                  # Diff-viz merge: combine diff CSV with visualizer selections
│   ├── src/
│   │   ├── lib.rs              # Library root, re-exports, integrate_diff_viz() orchestrator
│   │   ├── csv_reader.rs       # Parse diff CSV into DiffRow structs
│   │   ├── json_reader.rs      # Parse visualizer selections JSON into Selection structs
│   │   ├── merge.rs            # Full outer join by handle: Matched, DiffOnly, VizOnly rows
│   │   └── output.rs           # CSV + JSON output formatters for merged rows
│   └── tests/
│       ├── integration.rs
│       └── roundtrip.rs
```

Add `integrate.rs` under `cli/src/commands/` (between `generate.rs` and `schema.rs` alphabetically):

```
│       │       ├── integrate.rs # Merge diff CSV with visualizer selections
```

**README.md — Crate Structure table:**

Add a row:

```
| `integrate` | `crates/integrate/` | Merge `gramps-gen diff` CSV output with visualizer selection JSON (full outer join by handle) |
```

**README.md — CLI commands:**

Add to the commands table:

```
| `integrate diff-viz --diff <CSV> --selections <JSON>` | Merge diff CSV with visualizer selections (CSV/JSON output) |
```

**ARCHITECTURE.md — Overview:**

- Change "six crates" → "eight crates"

**ARCHITECTURE.md — Crate overview table:**

Add a row:

```
| `integrate` | Diff-viz merge: full outer join of diff CSV and visualizer selections by handle |
```

**ARCHITECTURE.md — Architecture Diagram:**

Add an `integrate` crate box (between `diff` and `cli`), showing it consumes
diff CSV (from the diff crate's CSV output) and selections JSON (from the
visualize crate's export), and produces merged CSV/JSON output.

**ARCHITECTURE.md — New section:**

Add a brief "Integrate Tool" section under a new `## Integrate Tool` heading
(at the same level as Diff Analyzer, after it), covering:

- **Purpose:** merge diff CSV results with visualizer selection JSON
- **Architecture:** full outer join by handle, three row kinds (Matched, DiffOnly, VizOnly)
- **Output:** CSV and JSON formatters
- **Integration:** `gramps-gen integrate diff-viz` CLI subcommand

**ARCHITECTURE.md — CLI Commands table:**

Add row:

```
| `integrate diff-viz` | Merge diff CSV and visualizer selections into a combined report |
```

---

### Step 2: Add `delete` crate references to all three docs

**Commit:** `docs: add delete crate to AGENTS.md, README.md, and ARCHITECTURE.md`

**AGENTS.md — Workspace Structure:**

Add a new `crates/delete/` tree entry (after `cli/`, before `diff/` alphabetically):

```
├── delete/                     # Deletion cascade engine: remove people and orphaned dependencies
│   └── src/
│       ├── lib.rs              # Library root, re-exports
│       ├── types.rs            # DeletePlan, DeleteManifest, DeleteCandidate, TypePlan, ReviewState
│       ├── cascade.rs          # Fixed-point cascade engine (read-only on graph)
│       ├── review.rs           # Interactive terminal review loop
│       └── manifest.rs         # Save/load/validate deletion manifests (JSON)
```

Add `delete.rs` under `cli/src/commands/`:

```
│       │       ├── delete.rs   # Delete selected people and orphaned dependencies
```

**README.md — Crate Structure table:**

Add a row:

```
| `delete` | `crates/delete/` | Deletion cascade engine: remove selected people and compute orphaned dependencies for removal |
```

**README.md — CLI commands:**

Add to the commands table:

```
| `delete <FILE> --selections <JSON>` | Delete selected people and their orphaned dependencies from a .gramps file |
```

**ARCHITECTURE.md — Crate overview table:**

Add a row:

```
| `delete` | Deletion cascade engine — remove seed people and compute orphaned dependencies |
```

**ARCHITECTURE.md — Architecture Diagram:**

Add a `delete` crate box (consumes `typed_graph::Graph` and selections JSON,
produces filtered XML via `output` crate), with the CLI command
`gramps-gen delete` shown in the CLI box.

**ARCHITECTURE.md — New section:**

Add a "Delete Tool" section under a new `## Delete Tool` heading (after
Integrate Tool), covering:

- **Purpose:** remove selected people and compute cascade of orphaned dependencies
- **Architecture:** three-phase fixed-point cascade (pre-connectivity recording,
  fixed-point orphan detection, per-type rules), interactive review, manifest
  save/load
- **Dependency chain:** People → Families → Events → Places → Citations →
  Sources → Repositories → Media → Notes → Tags
- **Per-type orphan rules:** brief overview (families cascade when no remaining
  person connections, events cascade when no remaining eventref edges, etc.)
- **Manifest:** JSON format, version 1, audit trail support
- **Integration:** `gramps-gen delete` CLI subcommand with `--yes`, `--dry-run`,
  `--save-manifest`, `--load-manifest` flags

**ARCHITECTURE.md — CLI Commands table:**

Add row:

```
| `delete <file>` | Remove selected people and orphaned dependencies from a .gramps file |
```

---

### Step 3: Fix residual gaps across all three docs

**Commit:** `docs: fix crate counts, add missing io.rs/stats-panel.ts, update dependency tables`

**AGENTS.md:**

- Add `csv` to Key Dependencies table: `|`csv`| integrate, cli | CSV parsing and serialization for diff-viz merge |`
- Add `flate2` to Key Dependencies table: `|`flate2`| cli, gramps-reader, visualize | Gzip compression/decompression |`
- Add `cargo test -p integrate` and `cargo test -p delete` to the "Running Tests" section

**README.md:**

- Fix the non-exhaustive CLI commands table at the bottom of the Usage section
  (currently only shows `schema list`, `schema download`, `diff`). Replace it
  with a complete table:

```markdown
| Command | Description |
|---|---|
| `generate --count <N>` | Full 5-stage pipeline → `.gramps` output |
| `validate <file>` | Minimal XML structure check |
| `stats <file>` | Streaming count and summary (text or JSON) |
| `visualize <file>` | Open Tauri desktop window with force-directed graph |
| `schema list` | List local and available Gramps schemas |
| `schema download <VERSION>` | Download a schema from Gramps GitHub |
| `diff <file_a> <file_b>` | Compare two Gramps XML files |
| `integrate diff-viz --diff <CSV> --selections <JSON>` | Merge diff CSV with visualizer selections |
| `delete <file> --selections <JSON>` | Delete selected people and orphaned dependencies |
```

- Add a "Tools" section after "Crate Structure" with brief overviews:

```markdown
## Tools

### Diff (`gramps-gen diff`)
Compare two Gramps XML files. Matches people, families, events, and other
entities across files, identifies additions/deletions/modifications, and
produces a structured report in text, JSON, or CSV format. See
[docs/diff-tool.md](docs/diff-tool.md) for the full guide.

### Visualizer (`gramps-gen visualize`)
Interactive desktop app (Tauri v2 + D3.js) for exploring family trees as a
force-directed graph. Zoom, pan, hover tooltips, multi-select, family group
filtering, and force-layout tuning sliders. See
[docs/visualizer-tool.md](docs/visualizer-tool.md) for the full guide.

### Integrate (`gramps-gen integrate`)
Merge a diff CSV report with visualizer selection JSON. Performs a full outer
join by person handle, producing combined rows with both diff field-change
data and visualizer metadata. Useful for cross-referencing who changed vs.
who was selected. Output in CSV or JSON. See
[docs/integrate-tool.md](docs/integrate-tool.md) for the full guide.

### Delete (`gramps-gen delete`)
Safely remove selected people and all orphaned dependencies from a Gramps
file. Uses a fixed-point cascade engine to determine which families, events,
places, sources, citations, repositories, media, notes, and tags become
unreachable. Supports interactive review, dry-run mode, and auditable JSON
manifests. See [docs/delete-tool.md](docs/delete-tool.md) for the full guide.
```

**ARCHITECTURE.md:**

- Update "six crates" → "eight crates" in the overview paragraph
- Add `csv` and `flate2` to the Dependencies table:

```
| `csv` | integrate, cli | CSV parsing and serialization |
| `flate2` | cli, gramps-reader, visualize | Gzip compression/decompression |
```

---

### Step 4: Write `docs/visualizer-tool.md` — visualizer user guide

**Commit:** `docs: add visualizer-tool.md user guide`

Following the `docs/diff-tool.md` pattern (Table of Contents, Quick Start, How
the Pipeline Works, Understanding the Output, Command Reference, How Matching
Works / Architecture deep-dives, Feature details, Troubleshooting), write a
comprehensive user guide covering:

- **Quick Start** — `cargo run -p visualize -F visualize -- file.gramps`,
  `gramps-gen visualize file.gramps`
- **How the Visualizer Works** — Data flow: `.gramps` → gramps-reader → DSU
  - generation layering → date imputation → GraphData → Tauri IPC → D3.js
- **Understanding the Graph** — nodes (circles colored by year), spouse links
  (solid lines), parent-child links (dashed lines), viridis color gradient,
  imputed-date dashed borders, undated gray nodes
- **Command Reference** — all CLI flags: positional file path, `--no-impute`,
  `--generation-gap`
- **Interaction Guide** — zoom/pan, hover tooltips (200ms), click-select,
  Shift multi-select, export selections to JSON
- **Selection Modes** — click to toggle, Shift-click for multi, rectangle
  drag-select, invert selection button; adjacency queries (ancestors,
  descendants, first-degree, second-degree, indirect connected set)
- **Family Group Filter** — dropdown to restrict view to a single DSU
  connected component
- **Force Layout Tuning** — per-generation Y-field pull (`generationPull`),
  spouse link strength (`spouseStrength`), parent-child link strength
  (`parentChildStrength`), freeze mode
- **Legend** — color gradient bar with year range, undated + imputed caption items
- **Stats Panel** — collapsible right sidebar with file statistics
- **Date Imputation** — multi-source BFS algorithm, configurable generation gap
- **Build Dependencies** — system library requirements (Debian/Ubuntu, Fedora)
- **Frontend Build** — `npm install && npm run build` workflow
- **Feature Gate** — `--features visualize`, two-binary architecture
- **Troubleshooting** — common issues (blank screen, missing frontend build,
  "file not found", gzip support)

---

### Step 5: Write `docs/integrate-tool.md` — integrate user guide

**Commit:** `docs: add integrate-tool.md user guide`

Following the `docs/diff-tool.md` pattern, write a comprehensive user guide:

- **Quick Start** — basic `gramps-gen integrate diff-viz --diff diff.csv --selections selections.json`
- **How the Integration Works** — pipeline: parse diff CSV → parse selections
  JSON → full outer join on person handles → format output
- **Understanding the Output** — three row kinds: Matched (diff + viz data),
  DiffOnly (only in diff CSV), VizOnly (only in selections); CSV and JSON
  output formats
- **Command Reference** — `--diff`, `--selections`, `--output`, `--format`
  (csv | json)
- **CSV Output Schema** — all 21 columns (diff fields + side + row_kind +
  viz_name, viz_birth_date, viz_death_date, viz_gender, viz_family_group)
- **JSON Output Schema** — top-level object with diff_file, selection_file,
  row_count, matched_count, matches array
- **How Matching Works** — handle_a first, then handle_b; no fuzzy matching
  (handle-only); unmatched rows become DiffOnly/VizOnly
- **Tips and Common Workflows** — finding overlaps, cross-referencing who
  changed vs who was selected, spreadsheet import
- **Troubleshooting** — "no data to integrate" warning, 0% handle match,
  empty selections

---

### Step 6: Write `docs/delete-tool.md` — delete user guide

**Commit:** `docs: add delete-tool.md user guide`

Following the `docs/diff-tool.md` pattern, write a comprehensive user guide:

- **Quick Start** — `gramps-gen delete data.gramps --selections picks.json`,
  `gramps-gen delete data.gramps --selections picks.json --dry-run`,
  `gramps-gen delete data.gramps --selections picks.json --yes`
- **How the Delete Pipeline Works** — parse → load selections → cascade
  engine → (optional review) → validate → write output with filter
- **How the Cascade Works** — Phase A (record pre-connectivity), Phase B
  (fixed-point orphan detection), Phase C (per-type orphan rules)
- **Dependency Chain** — People → Families → Events → Places → Citations →
  Sources → Repositories → Media → Notes → Tags
- **Per-Type Orphan Rules** — detailed table for all 10 types: what causes
  each type to be considered orphaned
- **Command Reference** — all flags: `--selections`, `--output`, `--yes`,
  `--dry-run`, `--save-manifest`, `--load-manifest`
- **Interactive Review** — type-by-type prompts (People, Families, Events,
  etc.), 6 actions (y/n/l/r/s/q), sample output
- **Manifest Format** — JSON schema, version 1, fields: version, source_file,
  selections_file, created_at, seed_people, plan (per-type to_delete + kept)
- **Save/Load Manifests** — audit trail workflow, cross-reference validation,
  source file mismatch warnings
- **Dry Run Mode** — compute cascades without writing output
- **Tips and Common Workflows** — cleaning up after visualizer pruning,
  reviewing before deletion, saving manifests for audit, re-running from
  a saved manifest
- **Troubleshooting** — 0% handle match between selections and graph,
  manifest version errors, source file mismatch warnings

---

---

## Verification Checklist

After all steps are complete:

```bash
# 1. All workspace crate names appear in all three docs
for crate in typed-graph output gramps-reader cli visualize diff integrate delete; do
  echo "=== $crate ==="
  grep -c "$crate" AGENTS.md README.md docs/ARCHITECTURE.md
done

# 2. ARCHITECTURE.md says "eight crates"
grep "eight crates" docs/ARCHITECTURE.md

# 3. All CLI commands appear in all three docs
for cmd in generate stats validate visualize diff integrate delete schema; do
  echo "=== $cmd ==="
  grep -c "$cmd" AGENTS.md README.md docs/ARCHITECTURE.md
done

# 4. All .rs files in new crates appear in AGENTS.md workspace tree
for f in crates/integrate/src/*.rs crates/delete/src/*.rs; do
  base=$(basename "$f")
  grep -q "$base" AGENTS.md || echo "MISSING in AGENTS.md: $base"
done

# 5. All user guide files exist and are non-empty
for f in docs/visualizer-tool.md docs/integrate-tool.md docs/delete-tool.md; do
  [ -s "$f" ] || echo "MISSING or EMPTY: $f"
done

# 6. README.md has "Tools" section with links to all guides
grep -c 'docs/.*-tool.md' README.md

# 7. No stale stale crate counts
grep -i "five crate\|six crate\|seven crate" docs/ARCHITECTURE.md
# Expected: no output (only "eight crates" should appear)

# 8. Frontend .ts files all documented in AGENTS.md
diff <(ls crates/visualize/frontend/src/*.ts | xargs -n1 basename | sort) \
     <(grep -oP '\w+\.ts' AGENTS.md | sort -u)

# 9. New dependencies in AGENTS.md
grep "csv" AGENTS.md
grep "flate2" AGENTS.md

# 10. Build still passes
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| User guides go stale as features evolve | Medium | Keep guides focused on user-visible behavior, not implementation details. Cross-reference ARCHITECTURE.md for internals. |
| `integrate` tool is narrow (only diff-viz mode) | Low | Structure the guide so new modes can be added as subsections. |
| `delete` tool's cascade rules change | Low | Link to `delete/src/cascade.rs` doc comments as source of truth for per-type rules. |
| Docs drift again without process change | High | AGENTS.md §Code Conventions already mandates doc updates. Add a note that all four user-guide docs (diff-tool.md, visualizer-tool.md, integrate-tool.md, delete-tool.md) must be kept in sync when their crates change. |
| Three user guides are substantial writing effort | Medium | Each guide follows the same proven structure as `diff-tool.md`, which is already well-received. Reuse the same section templates. |
