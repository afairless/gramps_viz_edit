# Implementation Plan: Layout Reset Button

Source: `docs/research/layout-reset-button.md`

## Overview

Add a `resetLayout()` method to the `GraphController` interface that clears all pinned node positions (`fx`/`fy`) and reheats the force simulation, then wire a "Reset Layout" button in the toolbar next to the existing filter dropdown. The toolbar is a new container that groups the filter dropdown and reset button together.

## Step ordering

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add resetNodePositions helper and resetLayout controller method` | Reset helper + controller method | `crates/visualize/frontend/src/graph.ts` | Unit |
| 2 | `feat: add toolbar with reset button and integrate filter dropdown` | Toolbar UI wiring | `crates/visualize/frontend/src/main.ts` | Unit |
| 3 | `chore: update test harness to use resetLayout` | Test harness cleanup | `crates/visualize/frontend/test-harness.html` | — |
| 4 | `docs: add manual verification instructions` | Manual verification | _(manual — no files changed)_ | — |

## Step details

### Step 1 — `resetNodePositions` helper + `resetLayout()` controller method

**What:** Add an exported `resetNodePositions(nodes, simulation)` pure function that clears `fx`/`fy` on every `SimNode` and calls `simulation.alpha(1).restart()`. Add a `resetLayout()` method to the `GraphController` interface and the controller object that delegates to the helper and also resets the zoom transform with a 500ms animated transition.

**Files to modify:**

- `crates/visualize/frontend/src/graph.ts`

**Changes to `graph.ts`:**

1. Add `resetNodePositions` export — iterates `SimNode[]`, sets `fx = null, fy = null`, calls `simulation.alpha(1).restart()`
2. Add `resetLayout(): void` to `GraphController` interface
3. Add `resetLayout` method to the controller object:
   - Guard: `if (svg.node()?.ownerDocument === null) return;`
   - Call `resetNodePositions(simNodes, simulation)`
   - Animate zoom reset: `svg.transition().duration(500).call(zoom.transform, d3.zoomIdentity)`

**Tests (in `crates/visualize/frontend/tests/graph.test.ts`):**

- `resetNodePositions` clears `fx`/`fy` on all nodes
- `resetNodePositions` calls `simulation.alpha(1)` and `.restart()`
- `resetNodePositions` handles empty node list (no panic)
- `resetNodePositions` is idempotent (second call is a no-op)

### Step 2 — Toolbar + reset button in `main.ts`

**What:** Create a `renderToolbar()` function that renders a horizontal toolbar container (`position: absolute`, top-left) containing the filter dropdown and the reset button. Replace the standalone `renderFilterDropdown` call in `renderGraphFromData()` with `renderToolbar()`. The reset button calls `controller.resetLayout()` on click, styled consistently with the filter dropdown.

**Files to modify:**

- `crates/visualize/frontend/src/main.ts`

**Changes to `main.ts`:**

1. Add `renderToolbar(graphData, controller): HTMLElement` function:
   - Creates a `<div id="toolbar">` with `position: absolute; top: 20px; left: 20px; z-index: 500; display: flex; align-items: center; gap: 8px`
   - Calls `renderFilterDropdown(graphData, controller)`, overrides its inline positioning to work within flex layout, appends to toolbar
   - Creates a `<button>` with text `↺ Reset`, title `Reset node positions to force-directed layout`
   - Button styles: padding `4px 10px`, font-size `12px`, border-radius `4px`, border `1px solid #ccc`, background `#fff`, cursor `pointer`, color `#333`
   - Hover: background `#eee`
   - Click handler: `controller.resetLayout()`
   - Returns the toolbar element
2. In `renderGraphFromData()`, replace:

   ```ts
   const filterDropdown = renderFilterDropdown(graphData, controller);
   if (filterDropdown && appEl) {
     appEl.insertBefore(filterDropdown, document.getElementById('legend'));
   }
   ```

   with:

   ```ts
   const toolbar = renderToolbar(graphData, controller);
   if (appEl) {
     appEl.insertBefore(toolbar, document.getElementById('legend'));
   }
   ```

**Tests (in `crates/visualize/frontend/tests/graph.test.ts`):**

- `renderToolbar` returns a toolbar element with a reset button containing `↺`
- `renderToolbar` includes a `<select>` element (filter dropdown)
- Reset button click handler calls `controller.resetLayout()` (via spy)

### Step 3 — Update test harness

**What:** Change the test harness's "Reset layout" button to call `controller.resetLayout()` instead of `controller.updateData(data)`.

**Files to modify:**

- `crates/visualize/frontend/test-harness.html`

**Change:**

```js
// Before:
document.getElementById('btn-reset')?.addEventListener('click', () => { controller.updateData(data); });
// After:
document.getElementById('btn-reset')?.addEventListener('click', () => { controller.resetLayout(); });
```

### Step 4 — Manual verification

**What:** Build the frontend and verify the following in the browser:

- "Reset Layout" button is visible in the toolbar (top-left, next to the filter dropdown)
- Dragging a node pins it; clicking "Reset Layout" unpins all nodes
- Simulation visibly re-settles into a fresh layout
- Zoom is reset smoothly (animated zoom-out over ~500ms)
- Filter state is preserved (active filter still applies after reset)
- Selection / highlights are preserved (not cleared by reset)
- `npm test` passes
