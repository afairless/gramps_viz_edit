# Implementation Plan: Comprehensive documentation update (integrate, delete crates + user guides)

Source: `docs/research/doc-update-plan-2025-08-08.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `docs: add integrate crate to AGENTS.md, README.md, and ARCHITECTURE.md` | `integrate` crate docs | AGENTS.md, README.md, docs/ARCHITECTURE.md | — |
| 2 | `docs: add delete crate to AGENTS.md, README.md, and ARCHITECTURE.md` | `delete` crate docs | AGENTS.md, README.md, docs/ARCHITECTURE.md | — |
| 3 | `docs: fix crate counts, add missing dependencies, update CLI table and Tools section` | Residual gaps | AGENTS.md, README.md, docs/ARCHITECTURE.md | — |
| 4 | `docs: add visualizer-tool.md user guide` | Visualizer user guide | docs/visualizer-tool.md | — |
| 5 | `docs: add integrate-tool.md user guide` | Integrate user guide | docs/integrate-tool.md | — |
| 6 | `docs: add delete-tool.md user guide` | Delete user guide | docs/delete-tool.md | — |

## Verification

After all steps are complete, run the verification checklist from the plan:

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

# 7. No stale crate counts
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
