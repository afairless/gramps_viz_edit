# Documentation Audit & Update Plan

Audit date: 2025-08-03
Scope: `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

## Summary

A full audit of the three primary documentation files against the current
codebase (commit state as of 2025-08-03) found **29 discrepancies** across the
three documents. The root cause is that documentation was not updated in sync
with code changes — the `gramps-reader` and `visualize` crates were added, the
connection densifier module was introduced, the `schema` subcommand was added,
and `generate_random` gained a fourth parameter, none of which were reflected
in the docs.

---

## Per-File Findings

### AGENTS.md — 9 discrepancies

| # | Severity | Issue | Location | Fix |
|---|---|---|---|---|
| A1 | **High** | Workspace tree missing `gramps-reader` crate entirely | Workspace Structure | Add `crates/gramps-reader/` with its full src/ subtree |
| A2 | **High** | Workspace tree missing `visualize` crate entirely | Workspace Structure | Add `crates/visualize/` with src/, frontend/, and Tauri config |
| A3 | **Medium** | `densify.rs` not listed under `typed-graph/src/generate/` | Workspace Structure | Add `densify.rs` entry |
| A4 | **Medium** | `schema.rs` not listed under `cli/src/commands/` | Workspace Structure | Add `schema.rs` entry (list/download subcommands) |
| A5 | **Medium** | `visualize.rs` not listed under `cli/src/commands/` | Workspace Structure | Add `visualize.rs` entry |
| A6 | **Low** | `typed-graph/tests/` indented as sibling of `crates/typed-graph/` rather than child | Workspace Structure | Fix tree indentation |
| A7 | **Medium** | Dependencies table missing `log`/`env_logger`, `ureq`, `quick-xml` (cli crate deps) | Key Dependencies | Add rows for `log`/`env_logger`, `ureq`; note `quick-xml` is also used by cli and gramps-reader |
| A8 | **High** | `generate_random` signature documented as 3 params; actual is 4 (adds `densify_config`) | Key Design Rules §3 | Update to `generate_random(config, adversarial_config, densify_config, schema)` |
| A9 | **Low** | `extract_schema.rs` still labeled "Stub" — verify if still accurate | Workspace Structure | Confirm stub status; update description if needed |

### README.md — 6 discrepancies

| # | Severity | Issue | Location | Fix |
|---|---|---|---|---|
| R1 | **High** | `cargo install -p visualize -F visualize --path .` uses wrong path | Installation | Change to `--path crates/visualize` |
| R2 | **Medium** | Duplicate "Inspect a .gramps file" section — appears twice with different commands | Usage section | Remove the second instance (the `cargo run` variant) |
| R3 | **Medium** | `schema` subcommand (list/download) not in the CLI commands table | Usage → CLI table | Add `schema list` and `schema download` rows |
| R4 | **Medium** | Frontend files table missing `graph-query.ts` | Visualization section | Add `graph-query.ts` row (adjacency indices, indirect-set queries) |
| R5 | **Medium** | No mention of connection densifier in pipeline or generation docs | Pipeline / How it works | Add densifier overview: component merging, orphan adoption, remarriage |
| R6 | **Low** | `forceConfig` controls (per-generation Y-field, link strengths) not documented | Visualization section | Add `ForceConfig` to the frontend features table |

### docs/ARCHITECTURE.md — 14 discrepancies

| # | Severity | Issue | Location | Fix |
|---|---|---|---|---|
| C1 | **High** | `generate_random` signature shows 3 params; actual takes 4 | Generation §Random generation | Update signature and parameter docs |
| C2 | **High** | Connection densifier not documented at all — no mention of `densify.rs`, `DensifyConfig`, or densification passes | Generation section | Add new subsection: Connection Densifier (4-pass post-processing) |
| C3 | **Medium** | Schema feature table includes `schema-5-0` and `schema-6-0` rows but no corresponding schema files exist in `schemas/` | Schema Extraction §Multi-version | Add note that these features exist as compile-time stubs for forward compatibility |
| C4 | **Medium** | Dependencies table missing `ureq` (HTTP client for schema download) | Dependencies | Add `ureq` → `cli` → "HTTP requests for `schema download`" |
| C5 | **Medium** | Missing `visualize/args.rs` in visualize module description | Visualization Architecture | Add `args.rs` to data flow diagram or module listing |
| C6 | **Medium** | Missing `graph-query.ts` in frontend files table | Visualization → Frontend features | Add `graph-query.ts` row (adjacency index, indirect-set queries for selection modes) |
| C7 | **Medium** | CLI diagram doesn't show `schema` subcommand or `visualize` subcommand | Architecture Diagram (CLI box) | Add `gramps-gen schema list/download` and `gramps-gen visualize` to diagram |
| C8 | **Medium** | Missing `ParsedEvent` type in gramps-reader types description | Architecture Diagram (gramps-reader box) | Add `ParsedEvent` type with description |
| C9 | **Medium** | Missing `compute_generation_table` function in gramps-reader description | Architecture Diagram (gramps-reader box) | Add function to the listing |
| C10 | **Low** | Missing `test-harness.html` in visualize frontend listing | Visualization → Frontend | Add `test-harness.html` entry |
| C11 | **Low** | Missing `graph.ts`, `selection.ts` node-splitting into `graph-query.ts` | Visualization → Frontend features | Update table to reflect current file structure |
| C12 | **Low** | `load_graph_data()` now takes `no_impute` and `generation_gap` params | Visualization §Data Flow | Verify parameter signature is documented correctly |
| C13 | **Low** | `ForceConfig` (per-generation Y-field, link strengths) not mentioned | Visualization → Frontend features | Document the `ForceConfig` interface and sliders |
| C14 | **Low** | `compute_generation_table` function signature not documented | gramps-reader section | Add brief description |

---

## Implementation Plan

The fixes are organized into commit-sized batches, ordered by impact. Each
batch updates all three documents for a related set of changes and should be a
single conventional commit.

### Step 1: Add missing crates to all doc workspace trees

**Files:** `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

- Add `gramps-reader` crate to all three workspace/file trees with its
  full structure: `src/lib.rs`, `src/types.rs`, `src/graph.rs`,
  `src/xml.rs`, `src/xml/count.rs`, `src/xml/extract.rs`, `src/error.rs`
- Add `visualize` crate to all three workspace/file trees with its full
  structure: `src/main.rs`, `src/lib.rs`, `src/graph_data.rs`,
  `src/dates.rs`, `src/args.rs`, `frontend/` subtree, `tauri.conf.json`,
  `capabilities/`, `tests/`
- Fix indentation of `typed-graph/tests/` in AGENTS.md tree

### Step 2: Document the connection densifier

**Files:** `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

- AGENTS.md (§3): Update `generate_random` signature to include `densify_config`
  parameter; add `densify.rs` to the generate/ tree listing
- README.md: Add densifier overview paragraph in "How it works" section —
  describe 4-pass post-processing (find components, cross-component marriage,
  orphan adoption, remarriage) with a link to ARCHITECTURE.md
- ARCHITECTURE.md: Add a new subsection "Connection Densifier" under Generation,
  covering `DensifyConfig`, the 4-pass algorithm, and public API
  (`find_components`, `densify_connections`). Update `generate_random`
  signature to 4 params.

### Step 3: Document missing CLI commands and update diagrams

**Files:** `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

- AGENTS.md: Add `schema.rs` and `visualize.rs` to the cli/src/commands/ tree;
  update dependencies table with `log`/`env_logger`, `ureq`, `quick-xml`
- README.md: Add `schema list` and `schema download` to the commands table;
  remove duplicate "Inspect a .gramps file" section; fix install path for
  visualize crate
- ARCHITECTURE.md: Update CLI box in architecture diagram to show
  `schema list/download` and `visualize` subcommands; add `ureq` to
  dependencies table

### Step 4: Document missing frontend and visualization files

**Files:** `README.md`, `docs/ARCHITECTURE.md`

- Both: Add `graph-query.ts` to frontend files tables (adjacency indices,
  `buildAdjacency`, `getIndirectSet` for selection modes)
- ARCHITECTURE.md: Add `args.rs` to visualize module listing; add
  `test-harness.html`; document `ForceConfig` and its UI sliders
  (per-generation Y-field pull, spouse/parent-child link strengths);
  add `ParsedEvent` type and `compute_generation_table` function to
  gramps-reader module description
- README.md: Add `ForceConfig` to frontend features table

### Step 5: Fix schema version feature table caveat

**Files:** `docs/ARCHITECTURE.md`

- Add a note that `schema-5-0` and `schema-6-0` exist as Cargo features
  (forward compatibility) but no corresponding schema JSON files are
  committed yet. Users must supply their own schema files for those versions.

### Step 6: Verify and fix extract_schema.rs stub status

**Files:** `AGENTS.md`

- Check `cli/src/commands/extract_schema.rs` to determine if it's still a
  stub. Update AGENTS.md label if the implementation has progressed.

---

## Verification

After all steps are complete, run:

```bash
# Verify all referenced files exist
grep -oP 'crates/[a-z0-9_/-]+\.(rs|json|toml|ts|html|css|py)' \
  AGENTS.md README.md docs/ARCHITECTURE.md | sort -u | while read f; do
  [ -f "$f" ] || echo "MISSING: $f"
done

# Verify all crate names match Cargo.toml
cargo metadata --format-version=1 --no-deps | jq '.packages[].name'

# Check for dead references (files mentioned but deleted)
# (manual review)
```

---

## Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Docs drift again without process change | High | Add a note in AGENTS.md §Code Conventions: "Update AGENTS.md, README.md, and docs/ARCHITECTURE.md when adding/removing crates, modules, or CLI commands" |
| schema-5-0/6-0 features confuse users | Low | Clearly document as forward-compatibility stubs |
| `generate_random` signature changes again | Low | Docs should link to `rustdoc` output rather than hard-coding full signatures |
