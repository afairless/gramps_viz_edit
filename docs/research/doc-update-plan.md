# Documentation Update Plan

Date: 2025-08-06
Scope: `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`

## Background

An audit from 2025-08-03 (`doc-audit-plan.md`) identified 29 discrepancies across the three
primary documentation files. The vast majority of those items have since been fixed. This
plan covers:

1. **~4 residual items** from the August audit that were missed
2. **~12 new gaps** created by code changes since the audit (new `diff` crate, new `io.rs`,
   new `stats-panel.ts`, removal of `extract_schema.rs`)

---

## Remaining from August 2025 Audit

| # | Source | Issue | Doc |
|---|---|---|---|
| X1 | A9 | `extract_schema.rs` listed as "Stub" in AGENTS.md workspace tree — the file has been deleted from the codebase. Remove the entry entirely. | AGENTS.md |
| X2 | C9/C14 | `compute_generation_table` appears in the ARCHITECTURE.md ASCII diagram but is not mentioned in prose describing `graph.rs` or the gramps-reader module. Add a brief description. | ARCHITECTURE.md |

---

## New Gaps Since August Audit

### New `diff` crate (workspace member)

The `diff` crate (`crates/diff/`) was added to the workspace. It compares two Gramps XML
files and produces a structured diff report. It has 10 source files plus an integration
test. It is completely absent from all three documentation files.

| # | Issue | Doc |
|---|---|---|
| N1 | No `diff` crate in the Workspace Structure tree | AGENTS.md |
| N2 | No `diff` crate in the Crate Structure table | README.md |
| N3 | ARCHITECTURE.md says **"five crates"** — now six | ARCHITECTURE.md |
| N4 | No `diff` crate in the crate overview table | ARCHITECTURE.md |
| N5 | No `diff` crate section/description in the architecture document | ARCHITECTURE.md |
| N6 | No `diff` crate module in the architecture ASCII diagram | ARCHITECTURE.md |

### New `gramps-gen diff` CLI subcommand

The CLI gained a `diff` subcommand (`crates/cli/src/commands/diff.rs`) that compares two
`.gramps` files.

| # | Issue | Doc |
|---|---|---|
| N7 | `diff.rs` not listed under `cli/src/commands/` in the workspace tree | AGENTS.md |
| N8 | `diff` command not in the CLI commands table | README.md |
| N9 | `diff` command not in the CLI commands table | ARCHITECTURE.md |
| N10 | `diff` command not shown in the architecture ASCII diagram (CLI box) | ARCHITECTURE.md |

### New `io.rs` in gramps-reader (gzip decompression)

`crates/gramps-reader/src/io.rs` was added — it provides transparent gzip decompression
for `.gramps` files (detects gzip magic bytes and decompresses transparently).

| # | Issue | Doc |
|---|---|---|
| N11 | `io.rs` not listed in the gramps-reader subtree of the workspace tree | AGENTS.md |
| N12 | `io.rs` not shown in the gramps-reader box of the architecture diagram | ARCHITECTURE.md |

### New `stats-panel.ts` in visualize frontend

`crates/visualize/frontend/src/stats-panel.ts` was added — a right-side collapsible
sidebar summarizing file statistics. It is missing from both file listings.

| # | Issue | Doc |
|---|---|---|
| N13 | `stats-panel.ts` not in the frontend file listing under `visualize/` | AGENTS.md |
| N14 | `stats-panel.ts` not in the frontend features table | ARCHITECTURE.md |

---

## Implementation Plan

Each step updates all three documents for a related set of changes and should be one
conventional commit.

### Step 1: Remove stale `extract_schema.rs` references

*Removes dead references before adding new content, keeping the diff of later steps clean.*

**Commit message:** `docs: remove stale extract_schema.rs references from AGENTS.md, README.md, and ARCHITECTURE.md`

**AGENTS.md — Workspace Structure tree:**

- Remove the line: `│       │       └── extract_schema.rs # Stub`

**README.md — CLI commands table:**

- Remove the row: `| extract-schema <path> | Extract the Gramps schema from a local Gramps source checkout (stub) |`

**ARCHITECTURE.md — CLI box in diagram:**

- Remove `gramps-gen extract-schema ── Stub`

**ARCHITECTURE.md — CLI Commands table:**

- Remove the `extract-schema` row

> **Note:** The Python extractor `extract/extract_schema.py` still exists — this step only removes CLI-level references to the defunct subcommand stub.

---

### Step 2: Add the `diff` crate to all three docs

**Commit message:** `docs: add diff crate to AGENTS.md, README.md, and ARCHITECTURE.md`

**AGENTS.md — Workspace Structure tree:**

- Add `crates/diff/` entry at the same level as other crates, with its full source tree:

  ```
  ├── diff/                       # Gramps XML diff analyzer (compare two .gramps files)
  │   ├── src/
  │   │   ├── lib.rs              # Crate root, re-exports
  │   │   ├── compare.rs          # Diff comparison engine
  │   │   ├── matcher.rs          # Entity matching across files
  │   │   ├── normalize.rs        # XML normalization for comparison
  │   │   ├── cascading.rs        # Cascading/extrinsic resolution
  │   │   ├── resolve.rs          # Interactive conflict resolution (feature-gated)
  │   │   ├── report.rs           # Diff report types and formatting
  │   │   ├── similarity.rs       # String similarity scoring
  │   │   ├── output.rs           # Text + JSON output formatters
  │   │   └── visualizer_index.rs # Index for the visualizer integration
  │   └── tests/
  │       └── integration.rs
  ```

- Add `diff.rs` under `cli/src/commands/`:

  ```
  │       │       ├── diff.rs     # Compare two Gramps XML files
  ```

**README.md — Crate Structure table:**

- Add a row:

  ```
  | `diff` | `crates/diff/` | Gramps XML diff analyzer: compare two `.gramps` files, produce structured diff report |
  ```

**README.md — CLI commands table:**

- Add a row:

  ```
  | `diff <file_a> <file_b>` | Compare two Gramps XML files and produce a structured diff report |
  ```

**ARCHITECTURE.md — Overview:**

- Change "five crates" → "six crates"
- Add diff to the crate overview table:

  ```
  | `diff` | Gramps XML diff analyzer — compare and match entities across two family trees |
  ```

**ARCHITECTURE.md — Architecture Diagram:**

- Add a `diff` crate box (consumes gramps-reader for parsing, produces structured reports)
- Update the CLI box to show `gramps-gen diff <file_a> <file_b>`

**ARCHITECTURE.md — New section:**

- Add a "Diff Analyzer" section under a new `## Diff Analyzer` heading (at the same level as Visualization Architecture), covering:
  - **Purpose:** compare two Gramps XML files, match persons/families, produce structured diff
  - **Architecture:** uses `gramps-reader` for parsing, entity matching via `matcher.rs`,
    cascading resolution, text/JSON output
  - **Feature gate:** The `resolve` feature (default: off) enables interactive conflict
    resolution via `crossterm`. Enable with `--features diff/resolve`.
  - **Integration:** `gramps-gen diff` CLI subcommand

**ARCHITECTURE.md — CLI Commands table:**

- Add row:

  ```
  | `diff <file_a> <file_b>` | Compare two Gramps XML files and produce a structured diff report |
  ```

**ARCHITECTURE.md — Dependencies table:**

- Add the workspace-level `strsim` dependency used by the `diff` crate:

  ```
  | `strsim` | diff | String similarity scoring for entity matching |
  ```

---

### Step 3: Add `io.rs` and `compute_generation_table` prose to gramps-reader docs

**Commit message:** `docs: add io.rs to AGENTS.md and ARCHITECTURE.md, document compute_generation_table`

**AGENTS.md — Workspace Structure tree (gramps-reader):**

- Add `io.rs` after `error.rs`:

  ```
  │   │       ├── io.rs           # Gzip detection and transparent decompression
  ```

**ARCHITECTURE.md — Architecture Diagram (gramps-reader box):**

- Add `io.rs` to the diagram — either as a new sub-box or as a note on the types box

**ARCHITECTURE.md — gramps-reader prose:**

- Add a sentence mentioning `compute_generation_table` (the `FamilyGroupGenerationTable`
  returned struct) with a brief description of what it computes

---

### Step 4: Add `stats-panel.ts` to frontend docs

**Commit message:** `docs: add stats-panel.ts to AGENTS.md and ARCHITECTURE.md frontend listings`

**AGENTS.md — Workspace Structure tree (visualize/frontend):**

- Add `stats-panel.ts` to the frontend listing:

  ```
  │   │   │   │   ├── stats-panel.ts  # Collapsible sidebar with file statistics
  ```

**Note:** `styles/main.css` already exists in the AGENTS.md workspace tree with correct indentation. No changes needed.

**ARCHITECTURE.md — Frontend features table:**

- Add a row:

  ```
  | Stats panel | `stats-panel.ts` | Collapsible right sidebar with file statistics summary |
  ```

---

## Verification

After all steps are complete:

```bash
# 1. No stale extract_schema references (except the Python extractor)
grep -ri "extract_schema" AGENTS.md README.md docs/ARCHITECTURE.md
# Expected: only extract_schema.py in extract/ directory

# 2. All workspace members matched to documented crate names
cargo metadata --format-version=1 --no-deps | jq '.packages[].name'
# Expected: typed-graph, output, gramps-reader, cli, visualize, diff
# Verify each appears in docs:
grep "typed-graph" AGENTS.md README.md docs/ARCHITECTURE.md > /dev/null
grep "output" AGENTS.md README.md docs/ARCHITECTURE.md > /dev/null
grep "gramps-reader" AGENTS.md README.md docs/ARCHITECTURE.md > /dev/null
grep "cli" AGENTS.md README.md docs/ARCHITECTURE.md > /dev/null
grep "visualize" AGENTS.md README.md docs/ARCHITECTURE.md > /dev/null
grep "diff" AGENTS.md README.md docs/ARCHITECTURE.md > /dev/null

# 3. All .ts files in frontend/src/ are documented in AGENTS.md
diff <(ls crates/visualize/frontend/src/*.ts | xargs -n1 basename | sort) \
     <(grep -oP '\w+\.ts' AGENTS.md | sort -u)

# 4. All CSS files in frontend/styles/ are documented in AGENTS.md
diff <(ls crates/visualize/frontend/styles/*.css | xargs -n1 basename | sort) \
     <(grep -oP '\w+\.css' AGENTS.md | sort -u)

# 5. Verify ARCHITECTURE.md crate count says "six crates"
grep "six crates" docs/ARCHITECTURE.md

# 6. Verify diff crate feature gate documented
grep -A2 "resolve" docs/ARCHITECTURE.md | head -6

# 7. Verify strsim in ARCHITECTURE.md dependencies
grep "strsim" docs/ARCHITECTURE.md

# 8. Lint check (docs changes should not break code)
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Docs drift again without process change | High | The AGENTS.md §Code Conventions already states: "Update AGENTS.md, README.md, and docs/ARCHITECTURE.md when adding or removing crates, modules, or CLI commands." Enforcement is the real challenge. |
| `diff` crate details become stale | Medium | Keep the ARCHITECTURE.md section high-level; refer to `crates/diff/src/lib.rs` doc comments for details |
| Future crate additions missed | Medium | Add a pre-commit checklist item (or CI script) that cross-references workspace members against documented crates |
| `extract_schema.rs` might be re-added later | Low | If the feature is revived, a new plan doc should be written rather than silently re-adding |
