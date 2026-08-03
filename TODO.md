# Implementation Plan: Draggable Nodes in the Force-Directed Graph

Source: `docs/research/drag-nodes-in-graph.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add d3.drag behavior to force-directed graph nodes` | Drag behavior + extractable handlers | `crates/visualize/frontend/src/graph.ts` | Unit |
| 2 | `test: add tests for drag handler logic and fx/fy mutation` | Drag handler tests | `crates/visualize/frontend/tests/graph.test.ts` | Unit |
| 3 | `feat: verify drag-nodes feature manually in browser` | Manual verification | — | — |

## Step Details

### Step 1 — Add drag behavior to `graph.ts`

**Code changes:**

- Inside `restartSimulation()`, after the `nodeEnter` chain, define a `d3.drag<SVGGElement, SimNode>()` behavior with `start`/`drag`/`end` handlers.
- In the `start` handler: call `simulation.alphaTarget(0.3).restart()` (guarded by `!event.active`), set `d.fx = d.x` / `d.fy = d.y`, and set cursor to `'grabbing'` via `d3.select(event.sourceEvent.currentTarget).style('cursor', 'grabbing')`.
- In the `drag` handler: convert zoom coordinates to base SVG space via `d3.zoomTransform(svg.node()).invert([event.x, event.y])`, set `d.fx`/`d.fy` to the inverted coordinates.
- In the `end` handler: call `simulation.alphaTarget(0)` (guarded by `!event.active`), set cursor to `'grab'` via `d3.select(event.sourceEvent.currentTarget).style('cursor', 'grab')`. Do **not** clear `fx`/`fy` (pin behavior).
- Apply drag via `nodeGroup.call(drag)` (not `nodeEnter.call(drag)`) to re-bind existing nodes with the current simulation reference.
- Extract drag handler logic into standalone exported functions (`onDragStart`, `onDrag`, `onDragEnd`) for testability. Each function takes `(d: SimNode, event: D3DragEvent, simulation: Simulation, svg: SVGSVGElement)` and returns `{ fx, fy }` or works via mutation.
- Change node cursor from `'pointer'` to `'grab'` in the `nodeEnter` `.attr('cursor', ...)`.

**Files modified:** `crates/visualize/frontend/src/graph.ts`

**Tests:** Unit tests for the extracted handler functions (fx/fy mutation, coordinate conversion, pin behavior). These can be written in the same step or deferred to Step 2.

### Step 2 — Write tests for drag handler logic

**Test coverage:**

- **`onDragStart`**: Given a `SimNode` with `x=100, y=200`, verify `fx=100, fy=200` after calling the handler.
- **`onDrag`** (identity zoom): Given a zoom transform at identity, verify `fx`/`fy` match the event coordinates.
- **`onDrag`** (with zoom): Given a zoom transform of `scale(2)`, verify `fx`/`fy` are correctly inverted from the zoomed coordinates.
- **`onDragEnd`**: Verify `fx`/`fy` remain unchanged (not cleared) — pin behavior.
- **Handler shape**: Verify that the drag behavior has `start`/`drag`/`end` handlers that are functions.

**Files modified:** `crates/visualize/frontend/tests/graph.test.ts`

### Step 3 — Manual verification

Build and run the frontend, then verify:

- Nodes are draggable with `'grab'`/`'grabbing'` cursor feedback
- Other nodes move responsively during drag (simulation reheats)
- Node stays pinned after drag ends
- Filtering preserves pinned positions (fx/fy not cleared on filter change)
- Click (without drag) still triggers selection
- Zoom/pan on background still works
- `npm test` passes
