# Implementation Plan: Documentation Update

Source: `docs/research/doc-update-plan.md`

Updates `AGENTS.md`, `README.md`, and `docs/ARCHITECTURE.md` to remove stale
references and add new content for changes since the August 2025 audit.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `docs: remove stale extract_schema.rs references from AGENTS.md, README.md, and ARCHITECTURE.md` | Remove stale `extract_schema.rs` entries | `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md` | — |
| 2 | `docs: add diff crate to AGENTS.md, README.md, and ARCHITECTURE.md` | Add `diff` crate documentation across all three docs | `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md` | — |
| 3 | `docs: add io.rs to AGENTS.md and ARCHITECTURE.md, document compute_generation_table` | Add `io.rs` entries and `compute_generation_table` prose | `AGENTS.md`, `docs/ARCHITECTURE.md` | — |
| 4 | `docs: add stats-panel.ts to AGENTS.md and ARCHITECTURE.md frontend listings` | Add `stats-panel.ts` entry | `AGENTS.md`, `docs/ARCHITECTURE.md` | — |

### Step Details

**Step 1** — Remove stale `extract_schema.rs` references:

- `AGENTS.md`: remove the `extract_schema.rs # Stub` line from workspace tree
- `README.md`: remove the `extract-schema` row from CLI commands table
- `ARCHITECTURE.md`: remove `gramps-gen extract-schema ── Stub` from CLI diagram box
- `ARCHITECTURE.md`: remove the `extract-schema` row from CLI Commands table

**Step 2** — Add `diff` crate:

- `AGENTS.md`: add full `crates/diff/` source tree to workspace structure; add `diff.rs` under `cli/src/commands/`
- `README.md`: add row to Crate Structure table; add row to CLI commands table
- `ARCHITECTURE.md`: update "five crates" → "six crates"; add crate overview row; add diagram box; add `## Diff Analyzer` section; add CLI commands row; add `strsim` dependency row

**Step 3** — Add `io.rs` and `compute_generation_table` prose:

- `AGENTS.md`: add `io.rs` entry under gramps-reader subtree
- `ARCHITECTURE.md`: add `io.rs` to architecture diagram; add sentence about `compute_generation_table` in gramps-reader prose

**Step 4** — Add `stats-panel.ts`:

- `AGENTS.md`: add `stats-panel.ts` entry under visualize/frontend tree
- `ARCHITECTURE.md`: add row to frontend features table
