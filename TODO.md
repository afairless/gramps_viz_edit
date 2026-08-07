# Implementation Plan: Rectangle Multi-Selection in Freeze Mode

Source: `docs/research/rectangle-multi-selection.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add rectangle drawing infrastructure to graph.ts` | Rectangle drawing infrastructure | `graph.ts` (new state, SVG overlay, pointer handlers, membership query, highlight, controller methods, freeze integration), `main.css` (`.selection-rect` styles) | Unit (`graph.test.ts`: drawing state, membership query, highlight layering, freeze integration, cleanup) |
| 2 | `feat: add Rect Select toggle button to toolbar` | Rect Select toggle | `main.ts` (toggle button in `renderToolbar`, `syncFreezeUI` show/hide, return `syncRectSelectUI` callback; update all callers of `renderToolbar` for new return type) | Unit (`main.test.ts`: toggle renders when frozen, hidden when not, text/background toggles, `syncFreezeUI` hides + deactivates) |
| 3 | `feat: add rectangle batch selection and Escape handler` | Rectangle click routing | `main.ts` (modified `onNodeClick` for batch select/deselect with indirect set union; Escape keydown handler; `renderGraphFromData` cleanup of previous listener; dev-console `__GRAPH_CONTROLLER__` export) | Unit (`main.test.ts`: batch click inside rectangle for each mode, outside click normal, mixed selection states, Escape clears rect / deactivates toggle, cleanup on re-render) |
| 4 | `test: add integration tests for rectangle selection` | Integration tests | `tests/graph.test.ts` (drag + membership integration, zoom/coordinate transforms), `tests/main.test.ts` (toggle + click + Escape end-to-end scenarios) | Integration (drag-draw rectangle → click node → verify selection; Shift+drag; unfreeze clears; toggle ON drag draws; toggle OFF Shift+drag draws) |

## Step Details

### Step 1 — Rectangle drawing infrastructure

**File changes:**

- `crates/visualize/frontend/src/graph.ts`:
  - Add state variables inside `renderGraph()`: `rectSelectActive`, `drawingRect`, `rectStartX`, `rectStartY`, `currentRect`, `rectOverlay`
  - Create SVG `<g class="rect-overlay">` in the zoom/pan group, after nodes
  - Add pointer event handlers to `svg`: `pointerdown.rect`, `pointermove.rect`, `pointerup.rect`
    - Only active during freeze + (toggle ON or Shift held)
    - Skip if event target is a node (not canvas background)
    - Tiny drags (< 5px) dismiss the rectangle
  - Implement `getNodesInRectangle()`: iterate visible `simNodes`, filter by `(x,y)` center inside `currentRect` bounds, respecting `currentFilter` (family group filter)
  - Implement `applyRectNodeHighlight()`: blue ring (`#4488cc`) for rectangle members, but NOT if already selected (selection red ring `#ff6b6b` takes priority)
  - Implement `clearRectangle()`: null out `currentRect`, remove overlay children, call `applyHighlight()`
  - Add to `GraphController` interface: `setRectSelectActive`, `isRectSelectActive`, `clearRectangle`, `getNodesInRectangle`, `hasRectangle`
  - Implement controller methods; wire into the controller object
  - Modify `setFrozen(false)`: clear rectangle state + call `clearRectangle()`
  - Modify `applyHighlight()` to also call `applyRectNodeHighlight()`

- `crates/visualize/frontend/styles/main.css`: Add `.selection-rect` styles (fill, stroke, dasharray, pointer-events)

**Tests** (in `tests/graph.test.ts`):

- Pointer down → move → up on SVG creates `.selection-rect` with correct dimensions
- `getNodesInRectangle()` returns correct handles when nodes are inside
- `getNodesInRectangle()` returns empty array when no rectangle drawn
- `getNodesInRectangle()` respects family group filter
- `clearRectangle()` removes rect element and resets `currentRect`
- `setFrozen(false)` clears rectangle and resets `rectSelectActive`
- `setRectSelectActive(true)` + drag draws; `setRectSelectActive(false)` clears
- Tiny drag (< 5px) treated as dismiss (rectangle cleared)
- Click on a node does NOT start rectangle drawing
- Shift+drag draws rectangle even when toggle is off (during freeze)
- Shift+drag does NOT draw when not frozen
- Zoom/pan still works when not frozen and not drawing
- Highlight layering: selected node inside rectangle keeps red ring, not blue
- Node dragged into rectangle after draw → membership includes it at click time
- Node dragged out → membership excludes it at click time
- `destroy()` cleans up rect overlay and state

### Step 2 — Rect Select toggle button

**File changes:**

- `crates/visualize/frontend/src/main.ts`:
  - In `renderToolbar()`: add "📦 Rect Select" button (hidden by default, positioned near freeze button)
  - Add `syncRectSelectUI(active)` helper: updates button text and background
  - Button click handler: toggle `controller.setRectSelectActive()` + `syncRectSelectUI()`
  - Update `syncFreezeUI(frozen)`: show/hide the rect-select button; if unfreezing, deactivate and sync
  - **Change `renderToolbar` return type** from `HTMLElement` to `{ toolbar: HTMLElement; syncRectSelectUI: (active: boolean) => void }`
  - Update all callers of `renderToolbar` (in `renderGraphFromData`, tests) to destructure the return value
  - Export the updated `renderToolbar` signature

**Tests** (in `tests/main.test.ts`):

- Rect Select toggle button renders inside toolbar when freeze UI is synced
- Rect Select toggle is hidden when not frozen
- Toggle button text updates on click (toggles between "📦 Rect Select" and "📦 Rect Select (ON)")
- Toggle button background changes on click (`#e8f0fe` when active)
- `syncFreezeUI(false)` hides the toggle button and calls `setRectSelectActive(false)`
- `renderToolbar` returns the correct shape with `syncRectSelectUI` function

### Step 3 — Rectangle click routing and Escape handler

**File changes:**

- `crates/visualize/frontend/src/main.ts`:
  - **Modified `onNodeClick`** in `renderGraphFromData()`:
    - If `controller.hasRectangle()` and clicked handle is in `getNodesInRectangle()`:
      - Determine action: `selecting = !selectionManager.has(handle)`
      - If mode is `'single'`: call `addAll(nodesInRect)` or `removeAll(nodesInRect)`
      - If mode is expanded: compute union of indirect sets for all rectangle nodes, then `addAll(union)` or `removeAll(union)`
    - If clicked outside rectangle (or no rectangle): normal single-node `clickWithIndirect` path
  - **Escape keydown handler** registered in `renderGraphFromData()`:
    - `Escape` key: if rectangle exists → `controller.clearRectangle()`; else if toggle active → `controller.setRectSelectActive(false)` + call `syncRectSelectUI(false)`
    - Store handler reference for proper cleanup on re-render
    - Remove previous listener before registering new one (at top of `renderGraphFromData`)
  - Ensure the `__GRAPH_CONTROLLER__` dev-console export still works

**Tests** (in `tests/main.test.ts`):

- Click inside rectangle in 'single' mode selects all (unselected → selected)
- Click inside rectangle in 'single' mode deselects all (selected → deselected)
- Mixed selection states: clicking unselected selects all, clicking selected deselects all
- 'ancestors' mode: rectangle node + ancestors selected (indirect set union)
- 'descendants' mode: rectangle node + descendants selected
- 'first-degree' mode: multiple rectangle nodes, union of 1st-degree sets
- 'second-degree' mode: rectangle node + 2nd-degree connections selected
- Click outside rectangle falls through to normal single-node behavior
- Escape clears rectangle when one exists
- Escape deactivates toggle when no rectangle but toggle is on
- Escape handler is removed on re-render (no listener accumulation)

### Step 4 — Integration tests

**File changes:**

- `tests/graph.test.ts`: Add integration-level tests combining drawing + membership + coordinate transforms
- `tests/main.test.ts`: Add scenarios exercising the full toggle + draw + click + Escape lifecycle

**Tests:**

- Drag-draw rectangle → click node inside → verify selection state changes correctly
- Shift+drag (toggle off) draws rectangle; click inside applies batch action
- Toggle ON: drag draws rectangle (no Shift needed)
- Toggle OFF + Shift+drag draws rectangle
- Unfreeze clears rectangle and deactivates toggle
- Select All / Deselect All still work regardless of rectangle state
- Select Group / Deselect Group still work regardless of rectangle state
- Filter to a subset → rectangle only considers visible nodes
