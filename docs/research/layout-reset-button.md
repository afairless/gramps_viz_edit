# Layout Reset Button

## Problem

Users can drag nodes in the force-directed graph, and those nodes stay pinned
at their dropped positions (`fx`/`fy` remain set — see the "Pin on drag end"
design decision in `drag-nodes-in-graph.md`). Over time, repeated dragging can
leave the graph in a tangled or messy state. The only way to recover the
original layout today is to reload the entire file.

The test harness (`test-harness.html`) has a crude workaround: a "Reset layout"
button that calls `controller.updateData(data)`, which rebuilds the entire
simulation data — nodes, links, color scales, everything — just to clear
positions. This is wasteful and resets unrelated state (highlights, the
selection panel, color scale internals).

## Proposed Solution

Add a `resetLayout()` method to the `GraphController` interface that clears
all pinned positions (`fx = null, fy = null`) on every `SimNode` and reheats
the simulation so the force layout re-settles. Then wire a "Reset Layout"
button in the UI, placed near the existing filter dropdown.

## Design

### 1. `resetLayout()` on `GraphController`

The method does three things:

1. Iterates all `simNodes` (the internal array of `SimNode` objects) and sets
   `fx = null` and `fy = null` on each one. This unpins every node.
2. Calls `simulation.alpha(1).restart()` to reheat the simulation to its
   maximum alpha so nodes settle into a fresh layout.
3. Resets the zoom/pan transform to the default identity transform (so the
   viewport matches the initial state).

**Why reset zoom too?** When the user zooms in to drag a specific node and then
resets the layout, the zoomed-in viewport may show only a small portion of the
resettled layout. Resetting the zoom ensures the user sees the full graph in
its fresh layout, matching the initial loading experience.

**Why not rebuild the entire data?** `updateData()` already does that, but it
also re-binds DOM elements, rebuilds color scales, and clears the highlight
set. A `resetLayout()` method is cheaper (O(n) rather than O(n + m) with
re-rendering) and preserves the selection panel, highlights, and filter state.

### 2. User-interface placement

The button goes in a new toolbar container, positioned near the existing
family-group filter dropdown (top-left, `z-index: 500`). The toolbar is a
horizontal row containing the filter dropdown and the reset button, created
by a new `renderToolbar()` function in `main.ts`.

**Layout:**

```
┌──────────────────────────────────┐
│ [Family Group: ▼]  [↺ Reset]    │  ← toolbar (top-left)
│                                  │
│                     ┌──────────┐ │
│                     │ Legend   │ │  ← legend (top-right)
│                     └──────────┘ │
│                                  │
│            ┌──────────────┐      │
│            │ Selection    │      │  ← selection panel (bottom-center)
│            └──────────────┘      │
└──────────────────────────────────┘
```

The toolbar replaces the current standalone filter dropdown. The filter
dropdown's `<div>` is still created by `renderFilterDropdown()`, but it is
placed inside the toolbar instead of directly inside `#app`.

**⚠️ Positioning conflict: `renderFilterDropdown` uses absolute positioning.**
The existing `renderFilterDropdown` function creates a `<div id="filter-container">`
with `position: absolute; top: 20px; left: 20px; z-index: 500`. When placed inside
the toolbar (also `position: absolute`), the filter container's absolute
positioning would be relative to the toolbar, not `#app`, placing it at
`(40px, 40px)` from the app edge. **The fix:** when the filter container is
appended to the toolbar, override its inline positioning to work within the
flex layout. The toolbar handles positioning via its own absolute coordinates
and flexbox.

```ts
// Inside renderToolbar, after appending the filter container:
if (filterDropdown) {
  filterDropdown.style.position = 'relative';
  filterDropdown.style.top = 'auto';
  filterDropdown.style.left = 'auto';
  filterDropdown.style.zIndex = 'auto';
  toolbar.appendChild(filterDropdown);
}
```

This keeps `renderFilterDropdown`'s internal logic unchanged (it still creates
the same `<select>` element) while adapting its placement for the toolbar context.

### 3. Button styling

The reset button uses inline styles, matching the filter dropdown's existing
approach (both use inline styles rather than `styles/main.css`). The button
matches the filter dropdown's font size, padding, border radius, and border
style for visual consistency:

| Property | Value |
|---|---|
| `padding` | `4px 10px` |
| `font-size` | `12px` |
| `border-radius` | `4px` |
| `border` | `1px solid #ccc` |
| `background` | `#fff` |
| `cursor` | `pointer` |
| `color` | `#333` |

On hover: `background: #eee`. Optional: add a subtle icon character such as
`↺` (U+21BA) or `⟳` (U+27F3) before the label text.

**CSS file note:** The project has a `styles/main.css` file, but inline styles
are used here for consistency with the existing filter dropdown — the whole
frontend uses inline positioning and styling for overlay elements. If the
codebase later adopts a CSS architecture, these styles should be moved to
`main.css`.

### 4. GraphController exposure

The `resetLayout()` method is part of the `GraphController` interface, so it
is accessible from `main.ts` and from the test harness. The existing
`window.__GRAPH_CONTROLLER__` assignment in `renderGraphFromData()` already
exposes the controller for console debugging.

## Files to Modify

### 1. `crates/visualize/frontend/src/graph.ts` — Add `resetLayout()` method

**Changes to `GraphController` interface:**

Add a new method signature:

```ts
export interface GraphController {
  // ... existing methods ...
  /** Reset all node positions and re-run the force layout. */
  resetLayout(): void;
}
```

**Implementation in the controller object:**

Inside the `renderGraph()` function, add the `resetLayout` method to the
`controller` object:

```ts
const controller: GraphController = {
  // ... existing methods ...

  resetLayout() {
    // Guard: if the SVG has been removed (e.g. destroy() was called),
    // calling svg.transition() would throw.
    if (svg.node()?.ownerDocument === null) return;

    // Clear all pinned positions
    for (const node of simNodes) {
      node.fx = null;
      node.fy = null;
    }
    // Reset zoom to identity
    svg.transition().duration(500).call(
      zoom.transform,
      d3.zoomIdentity,
    );
    // Reheat the simulation
    simulation.alpha(1).restart();
  },
};
```

Key implementation details:

- The `svg.node()?.ownerDocument === null` guard prevents a crash if
  `resetLayout()` is called after `destroy()` (which removes the SVG element).
  This is a defensive measure — in normal usage the button is removed with the
  DOM before `destroy()` is called, but the guard protects against race
  conditions or console-driven calls.
- Iterating `simNodes` (the unfiltered array) clears positions on ALL nodes,
  not just visible ones. This is correct — when the user switches back to
  "All groups", the previously hidden nodes should also have their positions
  reset.
- The zoom reset uses `svg.transition().duration(500)` for a smooth animated
  zoom-out, rather than an instant snap. This provides visual continuity.
- `simulation.alpha(1)` sets the simulation to maximum alpha, so it runs
  through its full cooling schedule (same as a fresh start).

### 2. `crates/visualize/frontend/src/main.ts` — Add toolbar and button

**New function: `renderToolbar()`**

Create a new function that renders the toolbar container with the filter
dropdown and the reset button:

```ts
function renderToolbar(
  graphData: GraphData,
  controller: GraphController,
): HTMLElement {
  const toolbar = document.createElement('div');
  toolbar.id = 'toolbar';
  toolbar.style.position = 'absolute';
  toolbar.style.top = '20px';
  toolbar.style.left = '20px';
  toolbar.style.zIndex = '500';
  toolbar.style.display = 'flex';
  toolbar.style.alignItems = 'center';
  toolbar.style.gap = '8px';

  // Family group filter dropdown
  const filterDropdown = renderFilterDropdown(graphData, controller);
  toolbar.appendChild(filterDropdown);

  // Reset layout button
  const resetBtn = document.createElement('button');
  resetBtn.textContent = '↺ Reset';
  resetBtn.title = 'Reset node positions to force-directed layout';
  resetBtn.style.padding = '4px 10px';
  resetBtn.style.fontSize = '12px';
  resetBtn.style.borderRadius = '4px';
  resetBtn.style.border = '1px solid #ccc';
  resetBtn.style.background = '#fff';
  resetBtn.style.cursor = 'pointer';
  resetBtn.style.color = '#333';
  resetBtn.addEventListener('mouseenter', () => {
    resetBtn.style.background = '#eee';
  });
  resetBtn.addEventListener('mouseleave', () => {
    resetBtn.style.background = '#fff';
  });
  resetBtn.addEventListener('click', () => {
    controller.resetLayout();
  });
  toolbar.appendChild(resetBtn);

  return toolbar;
}
```

**Changes to `renderGraphFromData()`:**

Replace the `renderFilterDropdown` call with `renderToolbar`:

```ts
// Before: add filter dropdown separately
const filterDropdown = renderFilterDropdown(graphData, controller);
if (filterDropdown && appEl) {
  appEl.insertBefore(filterDropdown, document.getElementById('legend'));
}

// After: add toolbar (contains filter + reset button)
const toolbar = renderToolbar(graphData, controller);
if (appEl) {
  appEl.insertBefore(toolbar, document.getElementById('legend'));
}
```

**Remove the standalone `renderFilterDropdown` wrapper:**

The `renderFilterDropdown` function itself stays as-is (it creates the `<select>`
element), but it is now called inside `renderToolbar` rather than from
`renderGraphFromData` directly. The `<div>` wrapper it returns is placed inside
the toolbar's flex container.

**Keep the defensive null check on `renderFilterDropdown`:**

`renderFilterDropdown` is typed `HTMLElement | null` and `renderGraphFromData`
already guards with `if (filterDropdown && appEl)` before inserting it.
(Note: in the current implementation the function always returns the
container — even with empty `family_groups` the dropdown shows just the
"All groups" option — so the null path is defensive, not reachable.)
`renderToolbar` should keep the same guard so the reset button is always
shown regardless of the filter dropdown:

```ts
function renderToolbar(graphData, controller): HTMLElement {
  const toolbar = document.createElement('div');
  // ... setup ...

  const filterDropdown = renderFilterDropdown(graphData, controller);
  if (filterDropdown) {
    toolbar.appendChild(filterDropdown);
  }

  // Reset button always shown
  // ...
  return toolbar;
}
```

### 3. `crates/visualize/frontend/src/graph.ts` — (No interface change needed for existing callers)

The `GraphController` interface is used by:

- `renderGraphFromData()` in `main.ts` — will call `resetLayout()` via the button
- `window.__GRAPH_CONTROLLER__` — dev console access
- `GraphController` type in `graph.ts` tests (if any)

All existing callers will continue to work uninterrupted. The new method is
an additive change.

### 4. `crates/visualize/frontend/tests/graph.test.ts` — New tests

Add tests for the `resetLayout` behavior:

**Simulating the controller:**

```ts
describe('GraphController.resetLayout', () => {
  it('clears fx/fy on all simNodes', () => {
    const container = document.createElement('div');
    const controller = renderGraph(container, makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    ));

    // Simulate dragging: pin nodes
    // (Access simNodes indirectly by dragging and checking positions)
    // Then call resetLayout and verify fx/fy are null
    // This requires access to simNodes, which are not exported.
    // Alternative: test via the controller's `resetLayout` method and
    // verify that the simulation is reheated (alpha > 0) and nodes
    // are unpinned.
  });
});
```

**Testing approach:**

The `simNodes` array is not exported from `graph.ts`, so we cannot directly
assert `fx === null` after `resetLayout()`. Instead, test the observable
effects:

- **Simulation reheats**: After `resetLayout()`, `simulation.alpha()` should
  return a positive value (close to 1). This can be verified by checking the
  simulation's alpha property if the simulation is exposed — but it's also
  private. Instead, we can use a spy on the simulation's `alpha` and `restart`
  methods.

**Recommended approach (pragmatic):**

1. Export a `resetLayout` helper that takes `simNodes` and `simulation` as
   parameters, making it a pure-ish function that can be unit-tested. This
   follows the same pattern used for the drag handlers (`onDragStart`,
   `onDrag`, `onDragEnd`).

2. In the main `renderGraph` function, the controller's `resetLayout` method
   delegates to this helper.

3. Test the helper's fx/fy clearing logic directly.

**Helper function:**

```ts
/** Reset all pinned positions and reheat the simulation. */
export function resetNodePositions(
  nodes: SimNode[],
  simulation: d3.Simulation<SimNode, undefined>,
): void {
  for (const node of nodes) {
    node.fx = null;
    node.fy = null;
  }
  simulation.alpha(1).restart();
}
```

**Controller method becomes:**

```ts
resetLayout() {
  resetNodePositions(simNodes, simulation);
  // Zoom reset handled separately (not part of the helper)
  svg.transition().duration(500).call(
    zoom.transform,
    d3.zoomIdentity,
  );
}
```

**Tests:**

```ts
describe('resetNodePositions', () => {
  it('clears fx/fy on all nodes', () => {
    const nodes = buildSimNodes(makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [],
    ));
    nodes[0].fx = 100; nodes[0].fy = 200;
    nodes[1].fx = 300; nodes[1].fy = 400;

    const mockSim = { alpha: vi.fn().mockReturnThis(), restart: vi.fn() } as unknown as d3.Simulation<SimNode, undefined>;
    resetNodePositions(nodes, mockSim);

    expect(nodes[0].fx).toBeNull();
    expect(nodes[0].fy).toBeNull();
    expect(nodes[1].fx).toBeNull();
    expect(nodes[1].fy).toBeNull();
  });

  it('reheats the simulation', () => {
    const nodes = buildSimNodes(makeGraph([makeNode('p1')], []));
    const mockAlpha = vi.fn().mockReturnThis();
    const mockRestart = vi.fn();
    const mockSim = { alpha: mockAlpha, restart: mockRestart } as unknown as d3.Simulation<SimNode, undefined>;

    resetNodePositions(nodes, mockSim);

    expect(mockAlpha).toHaveBeenCalledWith(1);
    expect(mockRestart).toHaveBeenCalledTimes(1);
  });

  it('handles empty node list', () => {
    const mockSim = { alpha: vi.fn().mockReturnThis(), restart: vi.fn() } as unknown as d3.Simulation<SimNode, undefined>;
    expect(() => resetNodePositions([], mockSim)).not.toThrow();
  });

  it('is idempotent (second call is a no-op)', () => {
    const nodes = buildSimNodes(makeGraph([makeNode('p1')], []));
    nodes[0].fx = 100; nodes[0].fy = 200;
    const mockAlpha = vi.fn().mockReturnThis();
    const mockRestart = vi.fn();
    const mockSim = { alpha: mockAlpha, restart: mockRestart } as unknown as d3.Simulation<SimNode, undefined>;

    resetNodePositions(nodes, mockSim);
    resetNodePositions(nodes, mockSim); // second call

    expect(nodes[0].fx).toBeNull();
    expect(nodes[0].fy).toBeNull();
    expect(mockAlpha).toHaveBeenCalledTimes(2); // alpha is still called
    expect(mockRestart).toHaveBeenCalledTimes(2);
  });
});
```

**Zoom reset testing note:** The zoom animation (`svg.transition().duration(500)`)
is not tested via the `resetNodePositions` helper — it's handled inline in the
controller method. This is acceptable because zoom transforms are hard to
simulate in happy-dom. The zoom reset behavior is covered by manual verification
(Step 5).

**Toolbar rendering test recommendation:** Add a test in `graph.test.ts` or a
new `main.test.ts` that creates a `renderToolbar` output and verifies:

- The toolbar contains a reset button with text `↺ Reset`
- The toolbar contains a filter dropdown (`<select>` element)
- The reset button's click handler is wired (fire a click and verify the
  controller's `resetLayout` is called via spy)

```ts
// Example sketch (in a suitable test file):
describe('renderToolbar', () => {
  it('includes reset button and filter dropdown', () => {
    const data = makeGraph([makeNode('p1')], []);
    const mockController = { resetLayout: vi.fn(), ... } as unknown as GraphController;
    const toolbar = renderToolbar(data, mockController);
    expect(toolbar.querySelector('button')?.textContent).toContain('↺');
    expect(toolbar.querySelector('select')).toBeTruthy();
  });
});
```

### 5. `crates/visualize/frontend/test-harness.html` — Clean up workaround

The test harness currently has a "Reset layout" button that calls
`controller.updateData(data)`:

```js
document.getElementById('btn-reset')?.addEventListener('click', () => { controller.updateData(data); });
```

After the feature is implemented, this should be changed to:

```js
document.getElementById('btn-reset')?.addEventListener('click', () => { controller.resetLayout(); });
```

**Note:** The test harness has its own independent toolbar (`id="controls"`,
`top: 8px; left: 8px; z-index: 1000`) separate from the production toolbar.
Only the reset button's event handler changes — the test harness keeps its own
UI layout and does not adopt the production `renderToolbar()`. This is
intentional: the test harness needs quick-access filter buttons ("All groups",
"Group 1", "Group 2") and a status display that the production toolbar doesn't
provide.

This is a minor cleanup — the test harness is not part of the production build.

## Implementation Order

1. **Add `resetNodePositions` helper and `resetLayout()` to `graph.ts`** —
   Export the helper function for testing. Add the `resetLayout` method to
   the `GraphController` interface and the controller object. Include the
   zoom-reset animation. Commit.

2. **Write tests in `graph.test.ts`** — Test `resetNodePositions` for fx/fy
   clearing, simulation reheating, and empty-node-list handling. Commit.

3. **Add toolbar and button in `main.ts`** — Create `renderToolbar()` function
   that combines the filter dropdown and the reset button. Replace the
   standalone filter dropdown insertion in `renderGraphFromData()`. Style the
   button consistently. Commit.

4. **Update test harness** — Change the test harness's "Reset layout" button
   to call `controller.resetLayout()` instead of `controller.updateData(data)`.
   Commit.

5. **Manual verification** — Build the frontend, run the app, and verify:
   - "Reset Layout" button is visible in the toolbar (top-left, next to filter)
   - Dragging a node pins it; clicking "Reset Layout" unpins all nodes
   - Simulation visibly re-settles into a fresh layout
   - Zoom is reset smoothly (animated zoom-out)
   - Filter state is preserved (active filter still applies)
   - Selection / highlights are preserved (not cleared by reset)
   - `npm test` passes

## Design Decisions

### Why reset zoom as part of layout reset?

The initial rendering centers the graph in the viewport. After the user zooms
in and drags nodes, the viewport may be focused on a small area. If we only
reset node positions without resetting zoom, the freshly laid-out nodes may be
off-screen (outside the zoomed viewport). Resetting zoom ensures the user sees
the full graph in its new layout, matching the initial experience.

### Why animate the zoom reset (500ms transition)?

An instant zoom reset is disorienting — the viewport jumps. A 500ms animated
zoom-out provides visual continuity: the user sees the graph zoom out and the
nodes float back into position. This matches the behavior of "Reset View"
buttons in map applications.

### Why put the button in a toolbar instead of standalone?

The filter dropdown and the reset button are both layout controls that operate
on the same graph. Grouping them into a toolbar establishes a clear visual
hierarchy: "these are controls for the graph layout." It also makes room for
future layout controls (e.g., a "Re-center" button, charge-strength slider)
without cluttering the UI.

### Why not trigger a full `updateData()` to reset positions?

`updateData()` rebuilds all simulation data from scratch:

1. Rebuilds `simNodes` array from `GraphData` (O(n))
2. Rebuilds color scale (O(n))
3. Rebuilds `simLinks` from `GraphData` (O(m))
4. Re-binds all DOM elements (enter/exit/merge for nodes and links)
5. Creates a new simulation (discarding the old one)
6. Clears highlights and selection state

A `resetLayout()` method is O(n) — it only clears `fx`/`fy` on existing
`SimNode` objects and reheats the existing simulation. It preserves all UI
state (selection, highlights, filter, tooltip). And it's visually seamless:
the DOM elements are not removed and re-added.

### Why preserve filter state during reset?

If the user has filtered to a single family group and then clicks "Reset
Layout", they expect the visible nodes to re-settle within that group, not
for the filter to be cleared. Clearing the filter would be surprising and
would require the user to re-select the group.

### Why preserve selection/highlights during reset?

The drag-nodes feature already preserves selections across filter changes.
Resetting the layout is a visual operation — it should not affect which nodes
are selected. If the user has selected several nodes for export, resetting the
layout should not deselect them.

## Future Considerations

- **Re-center button**: A separate "Re-center" button that only resets the zoom
  without clearing node positions. Useful when the user zooms in and loses
  orientation.
- **Undo / redo**: A history of layout states, so users can undo a reset or
  revert a drag.
- **Auto-layout options**: Different force configurations (e.g., "tidy" layout
  with stronger charge, "compact" layout with shorter link distances) selectable
  from a dropdown.
- **"Reset Layout" confirmation**: If the user has invested significant effort
  in arranging nodes, a confirmation dialog ("Reset all node positions?") could
  prevent accidental loss of work. This is probably overkill for the current
  use case but could be added later.
- **Keyboard shortcut**: `Ctrl+R` or `Cmd+R` to reset layout without clicking
  the button. (Note: browsers already use `Ctrl+R` for reload, so this would
  need to be intercepted via `event.preventDefault()`.)
