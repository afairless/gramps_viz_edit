# Implementation Plan: Force-Freeze Mode for D3 Force-Directed Graph

Source: `docs/research/force-freeze-mode.md`

## Summary

Add a "force freeze" toggle to the D3 force-directed graph visualization. When frozen, all force-based movement is suspended via `simulation.stop()`. Dragged nodes move in isolation (manual SVG update), selection changes have no visual effect, and unfreezing resumes normal force layout with `simulation.alpha(1).restart()`.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add frozen state and freeze-aware drag handlers to graph module` | Freeze core in graph.ts | `crates/visualize/frontend/src/graph.ts` — `frozen` state, `getFrozen` param on drag handlers, `setFrozen()`/`isFrozen()` on `GraphController`, update `createDragBehavior` call sites | Unit — freeze-aware drag paths (onDragStart/onDrag/onDragEnd), setFrozen calls simulation.stop/alpha(1).restart, isFrozen returns state, createDragBehavior wiring |
| 2 | `feat: add freeze toggle button and toolbar wiring in main.ts` | Freeze UI in toolbar | `crates/visualize/frontend/src/main.ts` — freeze button, `syncFreezeUI()` helper, unfreeze on reset/filter | Unit — freeze button renders/toggles, controller.setFrozen called, syncFreezeUI toggles class, reset unfreezes, filter unfreezes |
| 3 | `feat: add force-frozen CSS class and freeze button styles` | Freeze visual styling | `crates/visualize/frontend/styles/main.css` — `.force-frozen` inset box-shadow, freeze button styles | — |
| 4 | `docs: add manual smoke test results for force-freeze mode` | Manual QA verification | (none — manual verification only) | — |

## Step details

### Step 1 — graph.ts: frozen state + freeze-aware drag handlers + tests

**Changes to `graph.ts`:**

- Add `let frozen = false` variable inside `renderGraph()`.
- Add `getFrozen: () => boolean` parameter to `createDragBehavior`, `onDragStart`, `onDrag`, `onDragEnd`.
- Implement frozen paths in each handler:
  - `onDragStart` (frozen): set `fx/fy`, set cursor to `grabbing`, return early — no `simulation.alphaTarget(0.3).restart()`.
  - `onDrag` (frozen): set `fx/fy` + `x/y`, manually update SVG `transform` attribute on the dragged `<g>` element via `d3.select(event.sourceEvent.currentTarget as SVGGElement).attr('transform', 'translate(${d.x ?? 0},${d.y ?? 0})')`.
  - `onDragEnd` (frozen): set cursor to `grab`, return early — no `simulation.alphaTarget(0)`, keep `fx/fy` pinned.
- Add `setFrozen(f: boolean)` to the `GraphController` interface and implementation:
  - `setFrozen(true)`: calls `simulation.stop()`.
  - `setFrozen(false)`: calls `simulation.alpha(1).restart()`.
  - Rebinds drag behavior so `getFrozen` closure sees updated value.
- Add `isFrozen()` to the `GraphController` interface and implementation.
- Update existing drag call sites in `restartSimulation()`: `nodeGroup.call(createDragBehavior(simulation))` → `nodeGroup.call(createDragBehavior(simulation, () => frozen))`.

**Tests in `tests/graph.test.ts`:**

- `onDragStart` with `getFrozen() === true`: assert `fx/fy` set, cursor set to grabbing, simulation NOT restarted.
- `onDragStart` with `getFrozen() === false`: existing behavior preserved (no regression).
- `onDrag` with `getFrozen() === true`: assert `fx/fy`/`x`/`y` set, SVG transform attribute updated on the dragged `<g>` element.
- `onDrag` with `getFrozen() === false`: existing behavior preserved.
- `onDragEnd` with `getFrozen() === true`: assert cursor set to grab, `alphaTarget` NOT called, `fx/fy` preserved.
- `onDragEnd` with `getFrozen() === false`: existing behavior preserved.
- `createDragBehavior` with `getFrozen` callback: assert callback is wired; assert second-parameter requirement.
- `setFrozen(true)` calls `simulation.stop()`.
- `setFrozen(false)` calls `simulation.alpha(1).restart()`.
- `isFrozen()` returns current state.
- `restartSimulation` rebinds drag with freeze-aware behavior.

### Step 2 — main.ts: freeze toggle button + toolbar wiring + tests

**Changes to `main.ts`:**

- Add freeze button to `renderToolbar()` (inserted after the filter dropdown / separator, before the reset button).
- Add `syncFreezeUI(frozen: boolean)` helper inside `renderToolbar()` to keep button text, background, border color, and `.force-frozen` CSS class on `#graph-container` in sync.
- Modify reset button's click handler: if frozen, call `controller.setFrozen(false)` and `syncFreezeUI(false)` before proceeding with reset.
- Modify family-group filter's change handler: if frozen, call `controller.setFrozen(false)` and `syncFreezeUI(false)` before applying the filter. This requires passing `syncFreezeUI` into the filter handler or wiring the unfreeze after `renderFilterDropdown` returns.

**Tests in `tests/main.test.ts`:**

- Freeze button renders with text "❄ Freeze".
- Clicking freeze button toggles text to "❄ Unfreeze" and back.
- Clicking freeze button calls `controller.setFrozen()`.
- Reset button calls `controller.setFrozen(false)` when frozen.
- `syncFreezeUI` adds/removes `.force-frozen` on `#graph-container`.

### Step 3 — main.css: freeze visual styling

**Changes to `main.css`:**

- Add `.force-frozen` rule: `box-shadow: inset 0 0 0 3px #2266aa` (blue-tinted inset border).
- Optionally add freeze button base + active state styles (inline styles in `main.ts` are primary, but CSS classes can supplement).

### Step 4 — Manual smoke test

Manual QA (no code changes):

- Open a `.gramps` file in the visualization app.
- Toggle freeze: verify button text/color changes and blue border appears.
- Drag a node while frozen: verify only the dragged node moves; others stay still.
- Select nodes while frozen: verify no visual movement (selection forces suspended).
- Unfreeze: verify simulation resumes and nodes settle normally.
- Freeze, then click Reset: verify layout resets and freeze is released.
- Freeze, then change family-group filter: verify filter applies and freeze is released.
- Drag a node while frozen, unfreeze: verify dragged node stays at its dragged position (pinned) until Reset is clicked.
