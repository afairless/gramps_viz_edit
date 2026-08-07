# Rectangle Multi-Selection in Freeze Mode

**Date:** 2025-01-01  
**Status:** Planned

## Overview

Add the ability to draw a rectangle on the graph canvas during freeze mode, enabling batch selection or deselection of all nodes within that rectangle. When a node inside the rectangle is clicked, the select/deselect action is applied to every node in the rectangle simultaneously, respecting the active selection mode (Single, Ancestors, Descendants, 1st-degree, 2nd-degree).

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Activation method | Both: toggle button + Shift+drag | Gives users flexibility — toggle for persistent mode, Shift for occasional use |
| Rectangle availability | Only during freeze mode | Per user requirement; avoids conflict with panning when forces are active |
| Rectangle lifecycle | Persists after clicks | User can apply multiple selection-mode changes without redrawing |
| Click outside rectangle | Rectangle stays; normal click applies | Rectangle acts as a shortcut for its interior, not a constraint on outside clicks |
| Node membership | Computed at click time from current positions | Dragged nodes may have moved since rectangle was drawn (freeze permits drag) |
| Indirect set computation | Union of indirect sets for all rectangle nodes | Matches user requirement: "1st-degree nodes of any node in the rectangle" |
| Heterogeneous selection (mixed selected/unselected) | Determined by clicked node's state | Per user requirement: click unselected → select all; click selected → deselect all |

## Interaction Flow

### Activation

1. User enables freeze mode via the ❄ Freeze button.
2. A **"📦 Rect Select"** toggle button appears in the toolbar (only when frozen).
3. The user can enter rectangle-draw mode in two ways:
   - **Toggle ON**: Click the "Rect Select" button to toggle it on; subsequent canvas drags draw rectangles instead of panning.
   - **Shift+drag**: Hold Shift while dragging on empty canvas (regardless of toggle state).

### Drawing a Rectangle

1. User presses mouse button on empty canvas (not on a node).
2. User drags to define a rectangular area.
3. A semi-transparent blue rectangle with a dashed border is drawn in real time.
4. On mouse release, the rectangle is finalized and persists.
5. Nodes whose centers fall within the rectangle are highlighted with a subtle ring (not the full selection highlight — that only appears after a click action).

### Applying a Selection Action

1. User clicks any node whose center lies within the rectangle.
2. The click determines the action:
   - **Clicked node is NOT selected** → **select** all nodes in the rectangle (plus their indirects per mode).
   - **Clicked node IS selected** → **deselect** all nodes in the rectangle (plus their indirects per mode).
3. Nodes already in the target state remain unchanged (idempotent).
4. The rectangle persists — user can click other nodes inside it using different selection modes.

### Clearing the Rectangle

The rectangle is removed when:

- User presses **Escape**.
- User clicks the **Rect Select toggle** off.
- User draws a **new rectangle** (old one is replaced).
- User **unfreezes** the simulation.
- User clicks on **empty canvas** (without dragging — a plain click to dismiss).

### Click Outside Rectangle

When a rectangle exists and the user clicks a node **outside** the rectangle:

- The rectangle **stays**.
- The click is treated as a **normal single-node selection** with the active selection mode.
- This allows the user to use the rectangle for batch operations without losing normal single-node interaction.

## Selection Logic

### Step 1: Determine the action

```
action = clickedNode.isSelected ? DESELECT : SELECT
```

### Step 2: Compute the rectangle set (R)

Let `R` = all **visible** nodes whose `(x, y)` center falls within the rectangle bounds.

Node positions are read from `SimNode.x` / `SimNode.y` at click time (reflects any drags since the rectangle was drawn).

### Step 3: Compute the indirect set (I)

Let `mode` = currently active selection mode.

```
if mode === 'single':
    I = R  (just the rectangle nodes)
else:
    I = R ∪ (union of getIndirectSet(node, mode) for all nodes in R)
```

In other words: compute the indirect set for each node in the rectangle using the existing `getIndirectSet()` function, take the union, and add the rectangle nodes themselves.

### Step 4: Apply the action

```
if action === SELECT:
    for node in I:
        if node not already selected:
            select node
else (DESELECT):
    for node in I:
        if node IS already selected:
            deselect node
```

### Concrete Examples

**Example 1 — Single mode, select:**

- Rectangle contains nodes A (unselected), B (selected), C (unselected).
- User clicks A (unselected) → action = SELECT.
- R = {A, B, C}, mode = 'single', so I = {A, B, C}.
- A is added (was unselected), B stays (was selected), C is added (was unselected).
- Result: A, B, C all selected.

**Example 2 — Single mode, deselect:**

- Rectangle contains nodes A (selected), B (unselected), C (selected).
- User clicks A (selected) → action = DESELECT.
- I = {A, B, C}.
- A is removed (was selected), B stays (was unselected), C is removed (was selected).
- Result: none selected.

**Example 3 — Ancestors mode, select:**

- Rectangle contains child D (unselected). Ancestors of D = {B, A}.
- User clicks D (unselected) → action = SELECT.
- R = {D}, mode = 'ancestors', getAncestors(D) = {B, A}.
- I = {D} ∪ {B, A} = {D, B, A}.
- Result: D, B, A all selected.

**Example 4 — 1st-degree mode, select, multiple rectangle nodes:**

- Rectangle contains nodes X (unselected) and Y (unselected).
- 1st-degree of X = {P, Q}. 1st-degree of Y = {Q, R}.
- User clicks X (unselected) → action = SELECT.
- I = {X, Y} ∪ {P, Q} ∪ {Q, R} = {X, Y, P, Q, R}.
- Result: all five nodes selected.

**Example 5 — 2nd-degree mode, deselect:**

- Rectangle contains node Z (selected). 2nd-degree of Z = {M, N, O}.
- User clicks Z (selected) → action = DESELECT.
- I = {Z} ∪ {M, N, O} = {Z, M, N, O}.
- Z is removed (was selected). M, N, O are removed if selected, no-op if not.
- Result: Z, M, N, O all deselected.

## Affected Components

```
crates/visualize/frontend/
├── src/
│   ├── types.ts             ✦ No changes needed (no new types for rectangle state)
│   ├── graph.ts             ★ Changes: rectangle drawing SVG overlay, node membership query, rectangle state
│   ├── graph-query.ts       ✦ No changes (reuses existing getIndirectSet)
│   ├── selection.ts         ✦ No changes (reuses existing clickWithIndirect, addAll, removeAll)
│   ├── main.ts              ★ Changes: Rect Select toggle button, toolbar wiring, lifecycle management
│   ├── tooltip.ts           ✦ No changes
├── styles/
│   └── main.css             ★ Changes: rectangle visual styles, toggle button styles
├── tests/
│   ├── graph.test.ts        ★ Changes: rectangle membership query tests, rectangle drawing state tests
│   └── main.test.ts         ★ Changes: toggle button rendering, Escape handler tests
└── index.html               ✦ No changes (dynamic DOM)
```

## Implementation Steps

### Step 1: Add rectangle drawing infrastructure to `graph.ts`

Add rectangle-drawing state and behavior inside `renderGraph()`. The rectangle is rendered as an SVG `<rect>` element inside the zoom/pan `<g>` group so it transforms correctly with zoom/pan.

#### New state variables (inside `renderGraph` closure)

```typescript
// Rectangle selection state
let rectSelectActive = false;     // toggle button state
let drawingRect = false;         // mid-drag flag
let rectStartX = 0;              // drag start in SVG coordinates
let rectStartY = 0;
let currentRect: { x: number; y: number; w: number; h: number } | null = null;

// SVG group for rectangle overlay (rendered above nodes)
let rectOverlay: d3.Selection<SVGGElement, unknown, HTMLElement, unknown>;
```

#### Rectangle overlay setup

Create a dedicated `<g>` element for the rectangle overlay, added to the zoom/pan `g` after the node group so it renders on top:

```typescript
rectOverlay = g.append('g').attr('class', 'rect-overlay');
```

#### Rectangle drawing handlers

Add pointer event handlers to the SVG for drawing. Because D3 zoom already captures pointer events on the SVG, we need to check if the event target is a node or empty canvas:

```typescript
// Pointer down on SVG (canvas background)
svg.on('pointerdown.rect', (event: PointerEvent) => {
  // Only active during freeze + (toggle ON or Shift held)
  if (!frozen) return;
  if (!rectSelectActive && !event.shiftKey) return;
  // Don't start rectangle on node clicks
  const target = event.target as Element;
  if (target.closest('.nodes g')) return;

  drawingRect = true;
  const coords = d3.pointer(event, g.node());
  rectStartX = coords[0];
  rectStartY = coords[1];

  // Create a new rect element (will be resized on drag)
  rectOverlay.selectAll('*').remove();
  rectOverlay.append('rect')
    .attr('class', 'selection-rect')
    .attr('x', rectStartX)
    .attr('y', rectStartY)
    .attr('width', 0)
    .attr('height', 0);

  event.stopPropagation();
  event.preventDefault();
});

svg.on('pointermove.rect', (event: PointerEvent) => {
  if (!drawingRect) return;
  const coords = d3.pointer(event, g.node());
  const x = Math.min(rectStartX, coords[0]);
  const y = Math.min(rectStartY, coords[1]);
  const w = Math.abs(coords[0] - rectStartX);
  const h = Math.abs(coords[1] - rectStartY);

  rectOverlay.select('.selection-rect')
    .attr('x', x)
    .attr('y', y)
    .attr('width', w)
    .attr('height', h);
});

svg.on('pointerup.rect', (event: PointerEvent) => {
  if (!drawingRect) return;
  drawingRect = false;

  const rect = rectOverlay.select('.selection-rect');
  if (rect.empty()) return;

  const x = parseFloat(rect.attr('x'));
  const y = parseFloat(rect.attr('y'));
  const w = parseFloat(rect.attr('width'));
  const h = parseFloat(rect.attr('height'));

  // Ignore tiny drags (< 5px) — treat as a click to dismiss
  if (w < 5 && h < 5) {
    clearRectangle();
    return;
  }

  currentRect = { x, y, w, h };
  applyRectNodeHighlight();
});
```

#### Rectangle membership query

```typescript
/** Return handles of visible nodes whose centers fall within the current rectangle. */
function getNodesInRectangle(): string[] {
  if (!currentRect) return [];
  const filtered =
    currentFilter === null
      ? simNodes
      : simNodes.filter((n) => n.family_group === currentFilter);
  return filtered
    .filter((n) => {
      const nx = n.x ?? 0;
      const ny = n.y ?? 0;
      return (
        nx >= currentRect!.x &&
        nx <= currentRect!.x + currentRect!.w &&
        ny >= currentRect!.y &&
        ny <= currentRect!.y + currentRect!.h
      );
    })
    .map((n) => n.handle);
}
```

#### Visual feedback for rectangle membership

```typescript
function applyRectNodeHighlight(): void {
  if (!nodeGroup) return;
  const inRect = new Set(getNodesInRectangle());
  nodeGroup.each(function (d: SimNode) {
    const inRectangle = inRect.has(d.handle) && !highlighted.has(d.handle);
    d3.select(this).select('circle')
      .attr('stroke', inRectangle ? '#4488cc' : highlighted.has(d.handle) ? '#ff6b6b' : '#fff')
      .attr('stroke-width', inRectangle ? 2 : highlighted.has(d.handle) ? SELECTED_STROKE_WIDTH : 1.5);
  });
}
```

Call `applyRectNodeHighlight()` from the existing `applyHighlight()` as well (so it refreshes when selections change).

> **Layering behavior:** Selected nodes inside the rectangle keep their red selection ring (`#ff6b6b`) and are NOT overridden by the blue rectangle-membership ring. The `applyRectNodeHighlight` guard `!highlighted.has(d.handle)` ensures selection highlighting always takes priority. This means a selected node inside the rectangle shows its selection state, not its rectangle membership.

#### Clear rectangle function

```typescript
function clearRectangle(): void {
  currentRect = null;
  rectOverlay.selectAll('*').remove();
  applyHighlight(); // restore normal highlighting
}
```

#### Expose rectangle state via GraphController

```typescript
export interface GraphController {
  // ... existing methods ...

  /** Enable or disable rectangle-selection toggle. */
  setRectSelectActive(active: boolean): void;
  /** Query whether rect-select toggle is on. */
  isRectSelectActive(): boolean;
  /** Clear the current selection rectangle (e.g., on Escape or unfreeze). */
  clearRectangle(): void;
  /** Get handles of nodes currently inside the drawn rectangle. */
  getNodesInRectangle(): string[];
  /** Query whether a rectangle is currently drawn. */
  hasRectangle(): boolean;
}
```

Implementation inside the controller:

```typescript
setRectSelectActive(active: boolean) {
  rectSelectActive = active;
  if (!active) clearRectangle();
},
isRectSelectActive() { return rectSelectActive; },
clearRectangle() { clearRectangle(); },
getNodesInRectangle() { return getNodesInRectangle(); },
hasRectangle() { return currentRect !== null; },
```

#### Modify `setFrozen` to clear rectangle on unfreeze

```typescript
setFrozen(f: boolean) {
  frozen = f;
  if (frozen) {
    simulation.stop();
  } else {
    // Clear rectangle when unfreezing
    rectSelectActive = false;
    clearRectangle();
    simulation.alpha(1).restart();
  }
  nodeGroup.call(createDragBehavior(simulation, () => frozen));
},
```

#### Modify zoom behavior when rectangle drawing is active

When `rectSelectActive` is true or Shift is held during freeze, suppress the zoom pan behavior so the drag draws a rectangle instead:

```typescript
// The pointerdown handler calls event.stopPropagation() which prevents
// D3 zoom from receiving the event. No zoom filter modification needed.
```

#### Tests (`graph.test.ts`)

- Rectangle drawing: mousedown + mousemove + mouseup on SVG creates `.selection-rect` with correct dimensions.
- `getNodesInRectangle()` returns correct handles when nodes are inside the rectangle.
- `getNodesInRectangle()` returns empty array when no rectangle is drawn.
- `getNodesInRectangle()` respects the active family group filter.
- `clearRectangle()` removes the rect element and resets `currentRect`.
- `setFrozen(false)` clears the rectangle and sets `rectSelectActive = false`.
- `setRectSelectActive(true)` followed by drag draws; `setRectSelectActive(false)` clears.
- Tiny drag (< 5px) is treated as a dismiss-click (rectangle cleared).
- Click on a node does NOT start rectangle drawing (event target check).
- Shift+drag draws a rectangle even when `rectSelectActive` is false (during freeze).
- Shift+drag does NOT draw when not frozen.
- Zoom/pan continues to work when not frozen and not drawing.
- `applyHighlight()` correctly layers rectangle membership highlight WITH selection highlight (selected nodes inside rectangle keep red ring, not blue).
- Node dragged into rectangle after draw → membership includes it at click time.
- Node dragged out of rectangle after draw → membership excludes it at click time.
- `controller.destroy()` removes the rectangle overlay and cleans up rect state.

### Step 2: Modify click routing in `main.ts`

When a rectangle exists and a node is clicked inside it:

- Compute the rectangle set.
- Determine the action (select/deselect) from the clicked node's state.
- Apply the action to the rectangle set + indirect set union.
- When a node outside the rectangle is clicked, act normally.

The existing click wiring in `renderGraphFromData`:

```typescript
controller.onNodeClick((handle: string) => {
  const indirect = getIndirectSet(adjacency, handle, currentMode);
  selectionManager.clickWithIndirect(handle, indirect);
  controller.setHighlighted(new Set(selectionManager.handles));
});
```

New click wiring:

```typescript
controller.onNodeClick((handle: string) => {
  if (controller.hasRectangle()) {
    const nodesInRect = controller.getNodesInRectangle();
    if (nodesInRect.includes(handle)) {
      // Clicked node is inside the rectangle → batch operation
      const selecting = !selectionManager.has(handle);

      if (currentMode === 'single') {
        // All nodes in the rectangle
        if (selecting) {
          selectionManager.addAll(nodesInRect);
        } else {
          selectionManager.removeAll(nodesInRect);
        }
      } else {
        // Union of indirect sets for all nodes in the rectangle
        const indirectUnion = new Set<string>();
        for (const h of nodesInRect) {
          indirectUnion.add(h);
          for (const ih of getIndirectSet(adjacency, h, currentMode)) {
            indirectUnion.add(ih);
          }
        }
        if (selecting) {
          selectionManager.addAll(indirectUnion);
        } else {
          selectionManager.removeAll(indirectUnion);
        }
      }
    } else {
      // Clicked node is outside the rectangle → normal single-node behavior
      const indirect = getIndirectSet(adjacency, handle, currentMode);
      selectionManager.clickWithIndirect(handle, indirect);
    }
  } else {
    // No rectangle → normal single-node behavior
    const indirect = getIndirectSet(adjacency, handle, currentMode);
    selectionManager.clickWithIndirect(handle, indirect);
  }

  controller.setHighlighted(new Set(selectionManager.handles));
});
```

#### Escape key handler

Add a keydown listener to clear the rectangle on Escape and to toggle rect-select off.
The handler is registered in `renderGraphFromData` (where both `controller` and the
`syncRectSelectUI` callback are in scope) rather than inside `renderToolbar`.

To bridge the scope gap, `renderToolbar` returns the `syncRectSelectUI` function so
the caller (`renderGraphFromData`) can store it and wire the Escape handler after
the toolbar is created:

```typescript
// In renderGraphFromData, after toolbar is created:
const toolbar = renderToolbar(graphData, controller, ...);
appEl?.prepend(toolbar);

// Store the sync callback returned by renderToolbar
let _syncRectSelectUI: ((active: boolean) => void) | null = null;

// --- Escape handler ---
const onKeyDown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') {
    if (controller.hasRectangle()) {
      controller.clearRectangle();
    } else if (controller.isRectSelectActive()) {
      controller.setRectSelectActive(false);
      _syncRectSelectUI?.(false);
    }
  }
};
document.addEventListener('keydown', onKeyDown);
```

The handler reference `onKeyDown` is stored so it can be removed when the graph is
torn down (see cleanup section).

#### Tests (`main.test.ts`)

- Clicking a node inside the rectangle with modes 'single', 'ancestors', 'descendants', 'first-degree', 'second-degree' dispatches the correct batch action (select or deselect).
- Clicking a node OUTSIDE the rectangle falls through to normal single-node behavior.
- Click routing handles mixed selection states: when rectangle contains selected + unselected nodes, clicking an unselected node selects all, clicking a selected node deselects all.
- The indirect-set union for multiple rectangle nodes in expanded modes (ancestors, 1st-degree, etc.) correctly merges overlapping sets.
- Escape key clears the rectangle when one exists.
- Escape key toggles off rect-select mode when no rectangle exists (but toggle is active).

### Step 3: Add Rect Select toggle button to toolbar in `main.ts`

Add the toggle button to `renderToolbar`. The button is only visible/active when freeze is active.

```typescript
// ---- rect-select toggle button (only during freeze) ----
const rectSelectBtn = document.createElement('button');
rectSelectBtn.textContent = '📦 Rect Select';
rectSelectBtn.title = 'Toggle rectangle selection mode (or hold Shift while dragging)';
rectSelectBtn.style.padding = '4px 10px';
rectSelectBtn.style.fontSize = '12px';
rectSelectBtn.style.borderRadius = '4px';
rectSelectBtn.style.border = '1px solid #ccc';
rectSelectBtn.style.background = '#fff';
rectSelectBtn.style.cursor = 'pointer';
rectSelectBtn.style.color = '#333';
rectSelectBtn.style.display = 'none'; // hidden until frozen

function syncRectSelectUI(active: boolean): void {
  rectSelectBtn.textContent = active ? '📦 Rect Select (ON)' : '📦 Rect Select';
  rectSelectBtn.style.background = active ? '#e8f0fe' : '#fff';
  rectSelectBtn.style.borderColor = active ? '#2266aa' : '#ccc';
}

rectSelectBtn.addEventListener('click', () => {
  const next = !controller.isRectSelectActive();
  controller.setRectSelectActive(next);
  syncRectSelectUI(next);
});

toolbar.appendChild(rectSelectBtn);
```

Update `syncFreezeUI` to show/hide the rect-select button:

```typescript
function syncFreezeUI(frozen: boolean): void {
  // ... existing freeze button updates ...

  // Show/hide rect-select toggle
  rectSelectBtn.style.display = frozen ? '' : 'none';
  if (!frozen) {
    controller.setRectSelectActive(false);
    syncRectSelectUI(false);
  }
}
```

Return `syncRectSelectUI` from `renderToolbar` so the Escape handler in `renderGraphFromData` can access it. The caller captures the returned function:

```typescript
// renderToolbar now returns { toolbar: HTMLElement; syncRectSelectUI: (active: boolean) => void }
const { toolbar, syncRectSelectUI: updateRectSelectUI } = renderToolbar(graphData, controller, ...);
appEl?.prepend(toolbar);

// The Escape handler uses updateRectSelectUI via the _syncRectSelectUI variable
```

#### Tests (`main.test.ts`)

- Rect Select toggle button renders (when frozen UI is synced).
- Rect Select toggle is hidden when not frozen.
- Toggle button text updates on click (toggles between "📦 Rect Select" and "📦 Rect Select (ON)").
- `syncFreezeUI(false)` hides the toggle button and calls `setRectSelectActive(false)`.
- Escape key clears rectangle when one exists (via the stored handler reference).
- Escape key toggles off rect-select mode when no rectangle exists.
- Unfreezing clears rectangle and hides the toggle.

### Step 4: Add CSS styles for rectangle

```css
/* ---- Rectangle selection ---- */

.selection-rect {
  fill: rgba(68, 136, 204, 0.08);
  stroke: #4488cc;
  stroke-width: 2px;
  stroke-dasharray: 6 3;
  pointer-events: none;
}
```

### Step 5: Cleanup and integration testing

#### Escape listener cleanup

To avoid listener accumulation when the graph is reloaded (opening a new file), the Escape
keydown handler reference must be removed when the graph is torn down. Store the handler and
remove it before re-rendering:

```typescript
// In renderGraphFromData, near the top of the function
let _escapeKeyDown: ((e: KeyboardEvent) => void) | null = null;

// Before rendering a new graph, remove any previous listener
if (_escapeKeyDown) {
  document.removeEventListener('keydown', _escapeKeyDown);
}

// When wiring the Escape handler (after toolbar creation):
_escapeKeyDown = onKeyDown;
```

#### `selection.test.ts` — no new tests needed

The existing `clickWithIndirect`, `addAll`, and `removeAll` methods handle the batch operations. The new logic in `main.ts` composes these correctly.

#### Integration testing

Manual verification checklist:

1. Open a `.gramps` file, freeze the simulation.
2. Verify "📦 Rect Select" button appears in toolbar.
3. Click "📦 Rect Select" to toggle ON.
4. Drag on empty canvas → rectangle appears with dashed blue border.
5. Release mouse → rectangle persists, nodes inside get blue ring.
6. Click an unselected node inside the rectangle in "Single node" mode → all nodes in rectangle become selected. Already-selected nodes inside remain selected.
7. Click a selected node inside the rectangle in "Single node" mode → all nodes in rectangle become deselected. Already-unselected nodes inside remain unselected.
8. Switch mode to "Ancestors", click unselected node inside rectangle → all nodes in rectangle + all their ancestors become selected.
9. Switch mode to "Descendants", click selected node inside rectangle → all nodes in rectangle + all their descendants become deselected.
10. Switch mode to "1st-degree", click unselected node inside rectangle → all nodes in rectangle + all their 1st-degree connections become selected.
11. Switch mode to "2nd-degree", click selected node inside rectangle → all nodes in rectangle + all 2nd-degree connections become deselected.
12. Click a node OUTSIDE the rectangle → normal single-node selection applies; rectangle stays.
13. Press Escape → rectangle clears; nodes lose blue ring.
14. Draw a rectangle, then drag a node into the rectangle area → click another node inside → the dragged node is included (membership computed at click time).
15. Draw a rectangle, drag a node OUT of the rectangle → click a node inside → the dragged-out node is NOT included.
16. Draw a rectangle, then draw another → first is replaced.
17. Enter rectangle mode via toggle ON, then unfreeze → toggle turns off, rectangle clears, button hides.
18. With toggle OFF, hold Shift and drag → rectangle draws. Release Shift, click node inside → action applies.
19. With toggle ON, drag draws rectangle (no Shift needed).
20. With toggle ON, Shift+drag still works identically.
21. Select All / Deselect All buttons continue to work normally regardless of rectangle state.
22. Select Group / Deselect Group buttons continue to work normally regardless of rectangle state.
23. Filter family group to a subset → rectangle only considers visible nodes in membership.

## Performance Considerations

- **Rectangle membership query:** `getNodesInRectangle()` iterates over all visible nodes. For typical family trees (hundreds to low thousands), this is O(N) and negligible.
- **Indirect set union:** For expanded modes, `getIndirectSet` is called once per node in the rectangle. With a reasonable rectangle size (typically < 50 nodes) and O(V+E) traversal per call, total work is manageable.
- **SVG rectangle rendering:** A single `<rect>` element — no performance concern.
- **No new simulation forces:** Rectangle membership highlighting is purely visual, applied via the existing `applyHighlight` path.

## Alternative Considered

**Ephemeral rectangle (auto-clear after action):** Rejected because the user chose persistent rectangle. The persistent model allows trying different selection modes on the same set of nodes without redrawing.

**Rect-select mode independent of freeze:** Rejected per user requirement. During active simulation, the D3 zoom pan behavior on SVG would conflict with rectangle drawing.

## Dependencies

- No Rust crate changes required — purely frontend TypeScript/DOM.
- No Tauri IPC changes required.
- No schema changes required.
- Depends on existing multi-node selection infrastructure (`SelectionManager.clickWithIndirect`, `getIndirectSet`).
- Depends on existing freeze mode infrastructure (`setFrozen`, `isFrozen`, `syncFreezeUI`).
