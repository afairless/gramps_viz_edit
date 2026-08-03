# Draggable Nodes in the Force-Directed Graph

## Problem

After the force-directed graph settles into its final layout, the graph becomes
rigid. Users cannot interactively reshape the graph by clicking and dragging
individual nodes — a capability found in most D3-based force-directed graph
visualizations. This limits exploration: users cannot manually reposition nodes
to untangle overlapping labels, separate dense family groups, or explore
alternative layouts.

## Proposed Solution

Add D3's built-in `d3.drag()` behavior to the node group. When a user clicks
and drags a node:

1. The simulation reheats (`alphaTarget`) to keep other nodes responsive
2. The dragged node's position is fixed (`fx`/`fy`) to follow the cursor
3. On release, the node either stays pinned or settles back into the simulation

## Key Design Decisions

### Click vs. Drag distinction

The node group already has a `click` handler (for selection). A `mousedown`
followed by a `mousemove` should be treated as a drag, not a click. D3's drag
behavior handles this automatically: it fires `start`, `drag`, and `end` events
and suppresses the click event when a drag occurs. The existing click handler
continues to work for clicks without dragging.

### Pin vs. Release on drag end

Two options:

| Option | Behavior | Trade-off |
|---|---|---|
| **Pin on drag end** | Node stays where dropped; `fx`/`fy` remain set | User can reshape graph permanently; manual cleanup needed if layout becomes messy |
| **Release on drag end** | Node settles back into simulation after drop | Cleaner layout, but user's repositioning is temporary |

**Recommendation: pin on drag end.** The user's intent when dragging is to
rearrange the graph. If the node snaps back, the interaction feels unresponsive.
A "Reset Layout" button (future enhancement) can clear all `fx`/`fy` values.

### Zoom conflict

`d3.zoom()` on the SVG intercepts `mousedown` on the background. Node drags
must prevent the zoom gesture from firing when the user starts dragging on a
node. D3's drag behavior handles this automatically when the drag is bound to
the node elements — zoom is only triggered on the SVG background, not on child
elements.

However, there is a subtlety: if the user starts a drag on a node and then the
cursor leaves the node, the drag should still continue. D3's drag behavior
handles this by default (it listens on the SVG for subsequent move/up events
after a drag start on the node).

### Cursor feedback

The node cursor should change from `'pointer'` (current) to `'grab'` when
hovering, and to `'grabbing'` while dragging. This gives clear visual feedback
that the node is draggable.

## Implementation Plan

### 1. Add drag behavior to `graph.ts`

**File:** `crates/visualize/frontend/src/graph.ts`

In `restartSimulation()`, after the `nodeEnter` group is created, add a
`d3.drag<SVGGElement, SimNode>()` behavior:

```ts
// Inside restartSimulation(), after the nodeEnter .on('click') / .on('mouseenter') / .on('mouseleave') chain

const drag = d3
  .drag<SVGGElement, SimNode>()
  .on('start', (event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>, d: SimNode) => {
    if (!event.active) simulation.alphaTarget(0.3).restart();
    d.fx = d.x;
    d.fy = d.y;
    // Visual feedback: change cursor to 'grabbing'
    // NOTE: Use sourceEvent.currentTarget, not `this` — arrow functions
    // don't bind `this` to the DOM element in TypeScript strict mode.
    d3.select(event.sourceEvent.currentTarget as SVGGElement)
      .style('cursor', 'grabbing');
  })
  .on('drag', (event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>, d: SimNode) => {
    // NOTE: event.x/event.y are in the zoom container's coordinate space,
    // but the simulation uses the base SVG coordinate space. Invert the
    // zoom transform to convert to base coordinates.
    const transform = d3.zoomTransform(svg.node());
    const [x, y] = transform.invert([event.x, event.y]);
    d.fx = x;
    d.fy = y;
  })
  .on('end', (event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>, d: SimNode) => {
    if (!event.active) simulation.alphaTarget(0);
    // Pin the node where it was dropped — do NOT clear fx/fy
    // Visual feedback: reset cursor to 'grab'
    d3.select(event.sourceEvent.currentTarget as SVGGElement)
      .style('cursor', 'grab');
  });

// Apply drag to ALL nodes (not just nodeEnter) so that existing nodes
// get the new drag behavior with the current simulation reference.
// This prevents stale closure references to a previous simulation.
nodeGroup.call(drag);
```

**Design notes:**

- `alphaTarget(0.3)` keeps the simulation warm during drag so other nodes
  respond to the movement.
- `fx`/`fy` are set on `start` (not just `drag`) to prevent the node from
  snapping back to the force's preferred position on the first frame.
- The `event.active` check is the standard D3 pattern: only the first drag
  in a multi-touch scenario should call `alphaTarget`/`restart`.
- The cursor is set to `'grabbing'` during drag and `'grab'` on hover/end.

**Cursor styling on the `<g>` element:**

Change the existing `'pointer'` cursor on the node `<g>` to `'grab'`:

```ts
// Existing line in nodeEnter
.attr('cursor', 'pointer')
// Change to:
.attr('cursor', 'grab')
```

The `'grabbing'` cursor during drag is set via
`d3.select(event.sourceEvent.currentTarget).style('cursor', ...)`
in the drag start/end handlers. Arrow functions are used, so `this`
cannot be used to reference the DOM element — use `event.sourceEvent.currentTarget`
instead.

### 2. Zoom prevention for node drags

D3's drag behavior on child elements already prevents `d3.zoom()` from
intercepting the gesture. No additional code is needed beyond what's described
in Step 1. The zoom continues to work on the SVG background (pan by dragging
empty space, zoom by scroll).

### 3. Update `graph.ts` — apply drag on `restartSimulation()` data re-bind

When `restartSimulation()` is called (e.g., after filtering), the node group
is re-bound with new data. Since `nodeEnter` creates new `<g>` elements, the
`call(drag)` on `nodeEnter` ensures new nodes get the drag behavior.

However, the `nodeEnter` variable is scoped inside `restartSimulation()` — the
drag behavior must be defined inside the function or lifted to a shared scope.
Option A: define the drag behavior inside `restartSimulation()` and apply it
via `nodeGroup.call(drag)` (not `nodeEnter.call(drag)`). Option B: define it
once in the outer scope and re-apply on each `restartSimulation()`.

**Recommendation: Option A** — define the drag inside `restartSimulation()`.
The overhead of creating a `d3.drag()` on each filter change is negligible, and
it keeps the code self-contained.

**⚠️ Important: apply to `nodeGroup`, not `nodeEnter`.**

If you apply drag only to `nodeEnter` (new nodes), existing nodes retain the
old drag behavior from the previous `restartSimulation()` call. Since the old
handlers captured the old `simulation` variable by closure, dragging an
existing node after a filter change would call `alphaTarget` on the old
(stopped) simulation, not the new active one. Using `nodeGroup.call(drag)`
re-binds the drag behavior on ALL visible nodes, ensuring every handler
captures the new `simulation` reference.

### 4. Preserve `fx`/`fy` across filter changes

When the user filters to a single family group and then back to "All groups",
the previously pinned positions should be preserved. This means:

- `fx`/`fy` should NOT be cleared when `restartSimulation()` is called.
- The simulation's `forceCenter` will still apply, but the fixed positions
  override it.

If `fx`/`fy` need to be cleared (for a "Reset Layout" feature), a new method
on `GraphController` can iterate `simNodes` and set `fx = null; fy = null`.

### 5. Cursor style on the SVG during zoom/pan

The SVG's `'grab'` cursor (for pan) is already set. No change needed.

## Files to Modify

### 1. `crates/visualize/frontend/src/graph.ts` — Core drag behavior

- Import `d3.drag` (already available via `import * as d3 from 'd3'`)
- Add drag behavior with `start`/`drag`/`end` handlers inside `restartSimulation()`
- In the `drag` handler, convert zoom coordinates to base SVG space via
  `d3.zoomTransform(svg.node()).invert([event.x, event.y])`
- Use `event.sourceEvent.currentTarget` instead of `this` to access the DOM
  element (arrow functions don't bind `this`)
- Apply drag via `nodeGroup.call(drag)` (not `nodeEnter.call(drag)`) to
  re-bind existing nodes with the current simulation reference
- Change node cursor from `'pointer'` to `'grab'` in `nodeEnter`
- Set `'grabbing'` cursor via `d3.select(event.sourceEvent.currentTarget).style('cursor', 'grabbing')` in drag start
- Set `'grab'` cursor via `d3.select(event.sourceEvent.currentTarget).style('cursor', 'grab')` in drag end

### 2. `crates/visualize/frontend/tests/graph.test.ts` — New tests

Add tests for:

- **Drag behavior shape**: Verify that `d3.drag().on('start', ...)` etc. can be
  created and that the handlers are functions. (This is a compile-time/type-level
  check, but an explicit test ensures the drag behavior is wired.)

- **`fx`/`fy` mutation on drag start**: Simulate a drag start event on a
  `SimNode` and verify that `fx` and `fy` are set to the node's current `x`/`y`.

- **`fx`/`fy` update on drag**: Simulate a drag event and verify `fx`/`fy`
  update to the event coordinates.

- **`fx`/`fy` preserved on drag end**: Verify that `fx`/`fy` are NOT cleared
  after drag end (pin behavior).

- **Click handler still fires** after a click without drag motion.

- **Cursor changes**: Verify that the cursor attribute is set correctly on the
  node group during drag states (this is a DOM attribute check).

Note: Testing D3 drag behavior in jsdom/happy-dom is notoriously tricky because
D3's drag behavior uses `pointer-events` and DOM event simulation. The tests
should focus on the **handler logic** (pure function tests) rather than the
full DOM interaction. If DOM simulation is too brittle, testing the state
transitions on `SimNode` objects (fx/fy mutation) is sufficient.

**Recommended testing approach:**

1. Extract the drag handler logic into standalone exported functions that take
   `(d: SimNode, event: D3DragEvent, simulation: Simulation, svg: SVGSVGElement)`
   and return `{ fx, fy }` or similar. This makes the logic testable without
   any DOM simulation.

2. For the zoom coordinate conversion, test that `d3.zoomTransform(svg).invert()`
   is called with the correct event coordinates. This can be tested by mocking
   `d3.zoomTransform` to return a known transform.

3. Test the `simulation` re-binding by verifying that `alphaTarget` is called
   on the correct simulation object after a filter change. This requires
   storing the simulation reference and checking it in the handler.

4. For `fx`/`fy` mutation tests:
   - **Start**: Call the handler with a `SimNode` that has `x=100, y=200`.
     Verify `fx=100, fy=200`.
   - **Drag**: Call the handler with a zoom transform at identity. Verify
     `fx`/`fy` match the event coordinates.
   - **Drag with zoom**: Call the handler with a zoom transform of `scale(2)`.
     Verify `fx`/`fy` are correctly inverted from the zoomed coordinates.
   - **End**: Verify `fx`/`fy` remain unchanged (not cleared).

5. Click-vs-drag and cursor tests are better covered by manual verification
   (Step 3) since they require real browser event handling.

### 3. `crates/visualize/frontend/src/main.ts` — No changes expected

The `renderGraph` function is called from `main.ts` and returns a
`GraphController`. The drag behavior is entirely internal to `graph.ts` — no
additional wiring is needed in `main.ts`.

## Implementation Order

1. **Add drag behavior to `graph.ts`** — define the drag behavior inside
   `restartSimulation()`, change cursor to `'grab'`/`'grabbing'`, wire it via
   `nodeGroup.call(drag)` (not `nodeEnter.call(drag)`) to ensure the new
   simulation reference is captured by all nodes. Commit.

2. **Write tests in `graph.test.ts`** — test the drag handler logic (fx/fy
   mutation, cursor changes, click vs. drag distinction). Commit.

3. **Manual verification** — build the frontend, run the app, and verify:
   - Nodes are draggable with `'grab'`/`'grabbing'` cursor feedback
   - Other nodes move responsively during drag (simulation reheats)
   - Node stays pinned after drag ends
   - Filtering preserves pinned positions
   - Click (without drag) still triggers selection
   - Zoom/pan on background still works
   - `npm test` passes

## Future Considerations

- **Reset Layout button**: A method on `GraphController` that clears all `fx`/`fy`
  values and reheats the simulation, allowing the layout to re-settle.
- **Multi-select drag**: Drag multiple selected nodes at once (requires
  additional logic to translate all selected nodes by the drag delta).
- **Touch support**: D3's drag behavior supports touch events by default, but
  it should be tested on touch devices.
- **Force-directed layout controls**: Sliders for charge strength, link distance,
  and collision radius to let users tweak the layout interactively.
- **Snap-to-grid**: Optional grid snapping for neat layouts.

## Design Decisions

### Why pin on drag end instead of release?

Pinning gives the user permanent control over the layout. If the node snapped
back, the user would feel like their action was undone. A "Reset Layout" button
can restore the default layout if the user wants to start over.

### Why define drag inside `restartSimulation()` instead of once?

Simplicity. The drag behavior is re-created on each filter change, but the
overhead is negligible (a few function calls). Keeping it inside
`restartSimulation()` avoids the need to manage a shared drag reference across
function calls.

### Why not add `d3.drag()` to the SVG as a whole?

The `d3.drag()` behavior is bound to the node `<g>` elements, not the SVG
itself. The SVG already has `d3.zoom()` for pan/zoom. Binding drag to the SVG
would conflict with zoom. By binding drag to the node elements only, zoom
continues to work on the background.

### Why `'grab'`/`'grabbing'` instead of `'move'`?

`'grab'`/`'grabbing'` are the standard cursor pair for draggable elements in
modern UIs (maps, image viewers, etc.). `'move'` implies the element will move
on its own, not that the user is dragging it.
