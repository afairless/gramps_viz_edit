# Fix Drag Node Jump on Zoomed/Panned Graph

**Date:** 2025-08-04
**Status:** Planned

## Problem

When a user drags a node after panning or zooming the graph, the node jumps to
a different location the moment the drag begins. While the mouse button is held,
dragging continues from the jumped-to position (not from the original node
position). When the zoom is at its default identity (`translate(0,0) scale(1)`),
dragging works correctly.

### Reproduction

1. Open a `.gramps` file in the visualizer.
2. Pan or zoom the graph (e.g., scroll-wheel zoom to 2×, or drag the background
   to pan).
3. Click and drag a node.
4. **Expected:** the node follows the cursor from its current position.
5. **Actual:** the node jumps to an incorrect location; subsequent dragging
   follows the cursor from that wrong position.

### Root cause

The `onDrag` handler in `graph.ts` (lines 160–169) calls
`d3.zoomTransform(svg).invert([event.x, event.y])` to convert event coordinates.
However, `event.x` and `event.y` from D3's drag behavior are **already in SVG
coordinate space** — the same space the simulation uses. The inversion is
redundant and **double-inverts** the zoom transform, producing wrong coordinates
whenever the zoom transform is non-identity.

#### Detailed trace

D3's drag behavior computes `event.x`/`event.y` internally as follows
(`node_modules/d3-drag/src/drag.js`, the `beforestart` closure):

```javascript
// container = this.parentNode = <g class="nodes"> (inside zoom <g>)
p = pointer(touch || event, container);
// ...
// subject is the SimNode datum (has .x/.y in SVG coords)
dx = s.x - p[0] || 0;   // offset from node center to click point
dy = s.y - p[1] || 0;
// ...
// on each drag tick:
x: p[0] + dx,   // = current pointer (SVG coords) + initial offset
y: p[1] + dy,
```

`d3.pointer(event, container)` (in `node_modules/d3-selection/src/pointer.js`)
converts the mouse screen position to the container's **local coordinate system**
by inverting the container's `getScreenCTM()`:

```javascript
point = point.matrixTransform(node.getScreenCTM().inverse());
```

Since `container` is `<g class="nodes">` — a child of the zoom `<g>` which
carries the `transform` attribute — its `getScreenCTM()` **includes the zoom
transform**. The inverse CTM therefore maps screen coordinates back to **SVG
coordinate space** (the same coordinate space used by the force simulation).

So `event.x`/`event.y` are already in SVG coordinates. Calling
`d3.zoomTransform(svg).invert([event.x, event.y])` applies the inverse zoom
transform a **second time**, producing coordinates that are wrong by the zoom
offset.

#### Concrete example

| Zoom state | Node SVG position | Click screen pos | `event.x`/`event.y` (SVG) | After `invert()` | Jump |
|---|---|---|---|---|---|
| Identity `T(0,0)×S(1)` | (200, 300) | (200, 300) | (200, 300) | (200, 300) | none ✓ |
| Panned `T(100,50)×S(1)` | (200, 300) | (300, 350) | (200, 300) | (100, 250) | −100x, −50y ✗ |
| Zoomed `T(0,0)×S(0.5)` | (200, 300) | (100, 150) | (200, 300) | (400, 600) | +200x, +300y ✗ |
| Both `T(100,50)×S(0.5)` | (200, 300) | (200, 200) | (200, 300) | (200, 500) | +200y ✗ |

The bug is invisible at identity zoom because `invert()` is then a no-op.

> **Note:** The original `drag-nodes-in-graph.md` plan (which introduced the
> `invert()` call) assumed that `event.x`/`event.y` from D3's drag behavior
> were in the zoom container's coordinate space and needed inversion back to
> base SVG space. That assumption was incorrect — D3's `pointer()` helper (used
> internally by `d3-drag`) already maps screen coordinates to the container's
> local coordinate system, which includes the zoom transform. The inversion was
> a double-invert producing wrong coordinates whenever the zoom transform is
> non-identity.

### Why `onDragStart` is correct

`onDragStart` sets `d.fx = d.x` and `d.fy = d.y`, which are the simulation
coordinates in SVG space. No transform inversion is applied — correctly.

## Proposed Fix

### 1. Fix `onDrag` — remove the double-inversion

**File:** `crates/visualize/frontend/src/graph.ts`

Change `onDrag` from:

```ts
export function onDrag(
  d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  _simulation: d3.Simulation<SimNode, undefined>,
  svg: SVGSVGElement,
): void {
  const transform = d3.zoomTransform(svg);
  const [x, y] = transform.invert([event.x, event.y]);
  d.fx = x;
  d.fy = y;
}
```

To:

```ts
export function onDrag(
  d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  _simulation: d3.Simulation<SimNode, undefined>,
): void {
  d.fx = event.x;
  d.fy = event.y;
}
```

### 2. Remove `svg` from `onDragStart` and `onDragEnd`

**File:** `crates/visualize/frontend/src/graph.ts`

`onDrag` was the only handler that actually used the `svg` parameter. The other
two already had it prefixed `_svg` as unused. Remove the parameter from both so
none of the three handlers accept a vestigial argument:

**`onDragStart`:** drop the `_svg: SVGSVGElement` parameter.

**`onDragEnd`:** drop the `_svg: SVGSVGElement` parameter.

### 3. Update `createDragBehavior` — remove `svg` parameter entirely

**File:** `crates/visualize/frontend/src/graph.ts`

Since no drag handler needs `svg` anymore, remove it from
`createDragBehavior`'s signature and stop passing it to all three arrow
functions:

```ts
// Before:
export function createDragBehavior(
  simulation: d3.Simulation<SimNode, undefined>,
  svg: SVGSVGElement,
): d3.DragBehavior<SVGGElement, SimNode, SimNode | d3.SubjectPosition> {
  return d3
    .drag<SVGGElement, SimNode>()
    .on('start', (event, d) => onDragStart(d, event, simulation, svg))
    .on('drag',  (event, d) => onDrag(d, event, simulation, svg))
    .on('end',   (event, d) => onDragEnd(d, event, simulation, svg));
}

// After:
export function createDragBehavior(
  simulation: d3.Simulation<SimNode, undefined>,
): d3.DragBehavior<SVGGElement, SimNode, SimNode | d3.SubjectPosition> {
  return d3
    .drag<SVGGElement, SimNode>()
    .on('start', (event, d) => onDragStart(d, event, simulation))
    .on('drag',  (event, d) => onDrag(d, event, simulation))
    .on('end',   (event, d) => onDragEnd(d, event, simulation));
}
```

Update the call site in `restartSimulation()` from:

```ts
nodeGroup.call(createDragBehavior(simulation, svg.node() as SVGSVGElement));
```

To:

```ts
nodeGroup.call(createDragBehavior(simulation));
```

### 4. Update tests

**File:** `crates/visualize/frontend/tests/graph.test.ts`

The existing test encodes the buggy behavior. Replace:

```ts
it('inverts zoomed event coords back to base SVG space', () => {
  const svg = makeSvg();
  (svg as unknown as { __zoom: d3.ZoomTransform }).__zoom =
    d3.zoomIdentity.scale(2);
  const node = makeSimNode();
  onDrag(node, makeEvent({ x: 100, y: 50 }), mockSimulation, svg);
  expect(node.fx).toBe(50);
  expect(node.fy).toBe(25);
});
```

The `'updates fx/fy to event coords at identity zoom'` test above it also needs
its `onDrag` call updated to drop the `svg` parameter:

```ts
// Before:
onDrag(node, makeEvent({ x: 42, y: 77 }), mockSimulation, svg);
// After:
onDrag(node, makeEvent({ x: 42, y: 77 }), mockSimulation);
```

Since both tests now verify the same behavior (coordinates pass through
directly), the two separate test cases can be **merged into one**. The merged
test explicitly documents that the fix is zoom-independent — `onDrag` no longer
accepts an `svg` parameter, so zoom state cannot affect the result:

```ts
it('sets fx/fy directly from event coordinates (already in SVG space)', () => {
  const node = makeSimNode();
  // After the fix, onDrag no longer references the SVG element or zoom
  // transform at all. event.x / event.y are always in SVG coordinate space
  // regardless of the current zoom/pan state, so coordinates pass through
  // unchanged in every scenario.
  onDrag(node, makeEvent({ x: 100, y: 50 }), mockSimulation);
  expect(node.fx).toBe(100);
  expect(node.fy).toBe(50);
});
```

All `onDragStart` and `onDragEnd` call sites must also drop the final `svg`
argument (mechanical change — remove the 4th argument from every call):

```ts
// onDragStart calls — before:
onDragStart(node, makeEvent(), mockSimulation, makeSvg());
// onDragStart calls — after:
onDragStart(node, makeEvent(), mockSimulation);

// onDragEnd calls — before:
onDragEnd(node, makeEvent(), mockSimulation, makeSvg());
// onDragEnd calls — after:
onDragEnd(node, makeEvent(), mockSimulation);
```

The `createDragBehavior` test must also drop the `svg` argument:

```ts
// Before:
const behavior = createDragBehavior(
  mockSimulation as unknown as d3.Simulation<SimNode, undefined>,
  svg,
);
// After:
const behavior = createDragBehavior(
  mockSimulation as unknown as d3.Simulation<SimNode, undefined>,
);
```

### 5. No logic changes needed in `onDragStart` or `onDragEnd`

Both handlers are already correct — they pin to SVG coordinates directly
without any transform inversion. Only the unused `svg` parameter is removed
(step 2).

## Files to Modify

| File | Change |
|---|---|
| `crates/visualize/frontend/src/graph.ts` | Remove `transform.invert()` from `onDrag`; drop `svg` from all three handlers; remove `svg` from `createDragBehavior` signature and its call site |
| `crates/visualize/frontend/tests/graph.test.ts` | Merge two `onDrag` tests into one; drop `svg` from `onDragStart`/`onDrag`/`onDragEnd`/`createDragBehavior` calls |

## Implementation Order

1. **`graph.ts` and `graph.test.ts` together** — fix `onDrag`, update
   `createDragBehavior`, and update all test call sites to match the new
   function signatures. Commit both files as a single commit — the source
   and test changes are tightly coupled (tests won't compile against the
   old function signatures after the source change).

2. **Build and verify:**

   ```bash
   cd crates/visualize/frontend && npm test
   cargo build -p visualize
   cargo test -p visualize
   ```

3. **Manual smoke test** — run the app, load a `.gramps` file, pan/zoom, then
   drag a node. Verify:
   - Node does not jump on drag start (regardless of zoom/pan state)
   - Node follows cursor from its original position
   - Drag works correctly at identity zoom (no regression)
   - Other nodes respond to simulation during drag
   - Node stays pinned where dropped
   - Filtering (family group selector) continues to work

## Design Decisions

### Why not set `drag.subject` to zero the offset?

The existing `drag.subject` default returns the `SimNode` datum, which has
`.x`/`.y` from the simulation. The offset between the click point and the node
center is preserved throughout the drag — the node does NOT snap to center on
the cursor. This is the standard and expected behavior (like dragging a file
icon by its corner).

Setting `drag.subject(() => ({ x: event.x, y: event.y }))` would snap the node
center to the cursor on drag start, which is a different UX. The current
offset-preserving behavior is correct; the only bug is the double-inversion.

### Why remove `svg` from all three handlers instead of just `onDrag`?

With `onDrag` no longer needing `svg`, the parameter serves no purpose in any
handler. Removing it from all three makes the API consistent and honest: none
of the drag handlers interact with the SVG element. `createDragBehavior` also
sheds the parameter since it was only a pass-through. This is a small, safe
cleanup that leaves no dead parameters behind.

### Why merge the two `onDrag` tests?

After the fix, both tests assert the same thing: that `event.x`/`event.y` map
directly to `fx`/`fy`. The "identity zoom" case and the "scaled zoom" case are
no longer distinguishable — there is no zoom-dependent code path. A single test
case suffices.

## Future Considerations

- **Add a `drag.subject` override for snapping**: Some users might prefer the
  node center to snap to the cursor on drag start. A configuration option could
  support both behaviors.
