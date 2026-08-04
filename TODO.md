# Implementation Plan: Selected Node Size Distinction

Source: `docs/research/selected-node-size.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: grow selected nodes to 2x radius in force graph` | Selected node sizing | `src/graph.ts` (add `SELECTED_NODE_RADIUS`, extend `applyHighlight()` to set `r`/`dy`), `tests/graph.test.ts` (add `selected node sizing` tests) | Unit |

## Step Details

### Step 1 — Selected node sizing

**Files:** `src/graph.ts`, `tests/graph.test.ts`

- Add `SELECTED_NODE_RADIUS = 16` constant next to `NODE_RADIUS = 8` (line 68)
- Extend `applyHighlight()` to set circle `r` and text `dy` in addition to existing stroke/fill/opacity attributes:
  - Selected: `r = SELECTED_NODE_RADIUS` (16), `dy = -(SELECTED_NODE_RADIUS + 6)` = `-22`
  - Non-selected: `r = NODE_RADIUS` (8), `dy = -(NODE_RADIUS + 6)` = `-14`
  - Both branches are explicit (selected and not) so deselection always restores defaults
- Add `describe('selected node sizing')` block in `tests/graph.test.ts` with four test cases:
  1. **Selected node at 2x, unselected at default** — render two nodes, `setHighlighted({p1})`, assert `r="16"` / `r="8"`, `dy="-22"` / `dy="-14"`
  2. **Restores default size when selection cleared** — `setHighlighted({p1})` then `setHighlighted(new Set())`, both circles have `r="8"` and `dy="-14"`
  3. **Grows all selected nodes in multi-node selection** — `setHighlighted({p1, p2})`, both circles have `r="16"`
  4. **Grows only visible nodes when a family-group filter is active** — filter to one group, `setHighlighted` includes a hidden-group handle, only the visible selected node grows

**Dependencies:** None — `NODE_RADIUS`, `applyHighlight()`, `renderGraph`, `makeGraph`, `makeNode` all exist.

## Build & Verify

After the step is committed, run:

```bash
cd crates/visualize/frontend && npm test
cargo build -p visualize
cargo test -p visualize
```

## Manual Smoke Test

After the build passes, run the dev harness and verify:

- Clicking a node grows it to 2×; clicking again (deselect) shrinks it back
- Select All grows every node; Deselect All shrinks all
- Ancestor/descendant/1st/2nd-degree modes grow the whole indirect set
- Labels stay above the larger circles
- Filter change + selection interaction works correctly
