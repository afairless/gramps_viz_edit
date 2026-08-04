# Implementation Plan: Documentation Audit & Update

Source: `docs/research/doc-audit-plan.md`

## Summary

Audit (`docs/research/doc-audit-plan.md`) found **29 discrepancies** across `AGENTS.md`, `README.md`, and `docs/ARCHITECTURE.md`. These 7 steps fix every discrepancy in commit-sized batches, ordered by impact.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `docs: add gramps-reader and visualize crates to doc trees` | Missing crate documentation | `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md` (workspace/file trees + fix `typed-graph/tests/` indentation) | — |
| 2 | `docs: document connection densifier across all docs` | Connection densifier docs | `AGENTS.md` (§3 `generate_random` signature, `generate/` tree), `README.md` (How it works), `docs/ARCHITECTURE.md` (new subsection, `generate_random` signature) | — |
| 3 | `docs: document schema and visualize CLI commands` | CLI command documentation | `AGENTS.md` (commands tree, deps table), `README.md` (commands table, dedupe section, install path), `docs/ARCHITECTURE.md` (CLI diagram, deps table) | — |
| 4 | `docs: document frontend and visualization files` | Visualization frontend docs | `README.md`, `docs/ARCHITECTURE.md` (`graph-query.ts`, `args.rs`, `test-harness.html`, `ForceConfig`, `ParsedEvent`, `compute_generation_table`) | — |
| 5 | `docs: note schema-5-0/6-0 forward-compat caveat` | Schema feature table caveat | `docs/ARCHITECTURE.md` (§Schema Extraction) | — |
| 6 | `docs: confirm extract-schema stub status` | extract_schema stub verification | `AGENTS.md` (verify stub label; no change expected — confirmed accurate) | — |
| 7 | `docs: add doc-sync convention to AGENTS.md` | Docs-drift mitigation | `AGENTS.md` §Code Conventions (add note to update docs when adding/removing crates, modules, or CLI commands) | — |

---

## Step Details

### Step 1 — Add missing crates to all doc workspace trees

**Files:** `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

**Discrepancies:** A1, A2, A6

- **AGENTS.md** — Add `gramps-reader/` crate subtree (`src/lib.rs`, `src/types.rs`, `src/graph.rs`, `src/xml.rs`, `src/xml/count.rs`, `src/xml/extract.rs`, `src/error.rs`). Add `visualize/` crate subtree (`src/main.rs`, `src/lib.rs`, `src/graph_data.rs`, `src/dates.rs`, `src/args.rs`, `frontend/` subtree, `tauri.conf.json`, `capabilities/`, `tests/`). Fix indentation of `typed-graph/tests/` (currently a sibling of `crates/typed-graph/`, should be a child).
- **README.md** — Add `gramps-reader` and `visualize` to the Crate Structure table.
- **ARCHITECTURE.md** — Ensure both crates appear in the Architecture Diagram and overview.

### Step 2 — Document the connection densifier

**Files:** `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

**Discrepancies:** A3, A8, C1, C2, R5

- **AGENTS.md** (§3 Five-stage pipeline): Update `generate_random` signature from 3 params to 4 (`config, adversarial_config, densify_config, schema`). Add `densify.rs` to the `generate/` tree listing.
- **README.md** (How it works): Add a paragraph describing the 4-pass densifier post-processing: find components, cross-component marriage, orphan adoption, remarriage. Link to ARCHITECTURE.md for details.
- **ARCHITECTURE.md** (Generation section): Add a new "Connection Densifier" subsection covering `DensifyConfig`, the 4-pass algorithm (`find_components`, `densify_connections`), and public API. Update `generate_random` signature to 4 params.

### Step 3 — Document missing CLI commands and update diagrams

**Files:** `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

**Discrepancies:** A4, A5, A7, R1, R2, R3, C4, C7

- **AGENTS.md** (Workspace Structure): Add `schema.rs` and `visualize.rs` to `cli/src/commands/` tree. (Key Dependencies): Add rows for `log`/`env_logger`, `ureq`; note `quick-xml` also used by `cli` and `gramps-reader`.
- **README.md** (Usage): Add `schema list` and `schema download` to the CLI commands table. Remove the duplicate "Inspect a .gramps file" section (the `cargo run` variant). Fix the `cargo install -p visualize` path from `--path .` to `--path crates/visualize`.
- **ARCHITECTURE.md** (CLI diagram): Add `gramps-gen schema list/download` and `gramps-gen visualize` to the CLI box. (Dependencies): Add `ureq` row.

### Step 4 — Document missing frontend and visualization files

**Files:** `README.md`, `docs/ARCHITECTURE.md`

**Discrepancies:** R4, R6, C5, C6, C8, C9, C10, C11, C12, C13, C14

- **Both docs:** Add `graph-query.ts` to frontend files tables (adjacency indices, `buildAdjacency`, `getIndirectSet` for selection modes). Add `ForceConfig` to frontend features tables (per-generation Y-field pull, spouse/parent-child link strengths, UI sliders).
- **ARCHITECTURE.md only:** Add `args.rs` to the visualize module listing. Add `test-harness.html` to the frontend listing. Add `ParsedEvent` type and `compute_generation_table` function to the gramps-reader module description. Update the data flow diagram to note `load_graph_data()` takes `no_impute` and `generation_gap` params.

### Step 5 — Fix schema version feature table caveat

**File:** `docs/ARCHITECTURE.md`

**Discrepancies:** C3

- Add a note that `schema-5-0` and `schema-6-0` exist as Cargo features (forward compatibility stubs) but no corresponding schema JSON files are committed yet. Users must supply their own schema files for those versions.

### Step 6 — Verify extract_schema.rs stub status

**File:** `AGENTS.md`

**Discrepancies:** A9

- **Verification:** `crates/cli/src/commands/extract_schema.rs` is still a stub (confirmed: prints "stub" message and returns `Ok(())`). The "Stub" label in AGENTS.md is accurate — no change needed. If future verification finds it's no longer a stub, update the label accordingly.

### Step 7 — Add doc-sync convention to AGENTS.md

**File:** `AGENTS.md`

**Mitigation for risk of docs drift (from plan's Risk Assessment):**

- Add a line in §Code Conventions: "Update `AGENTS.md`, `README.md`, and `docs/ARCHITECTURE.md` when adding/removing crates, modules, or CLI commands."

---

## Verification

After all steps are committed, run:

```bash
# Verify all referenced files exist
grep -oP 'crates/[a-z0-9_/-]+\.(rs|json|toml|ts|html|css|py)' \
  AGENTS.md README.md docs/ARCHITECTURE.md | sort -u | while read f; do
  [ -f "$f" ] || echo "MISSING: $f"
done

# Verify all crate names match Cargo.toml
cargo metadata --format-version=1 --no-deps | jq '.packages[].name'
```
