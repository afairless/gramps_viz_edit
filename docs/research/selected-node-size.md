# Selected Node Size Distinction

**Date:** 2025-08-03
**Status:** Planned

## Problem

In the family-group force-directed graph visualization, selected nodes are
currently the **same size** as non-selected nodes. Selection is communicated
only by a red stroke (`#ff6b6b`) and a thicker stroke width (3px vs 1.5px),
applied in `applyHighlight()` in `graph.ts`. This makes the selected state hard
to notice at a glance, especially on small nodes (default radius `r = 8`).

The goal is to make selected nodes **larger** than non-selected nodes so the
selection state is unmistakable.

## Proposed Solution

Selected nodes render with **radius 16 (2× the default 8)**; non-selected nodes
keep radius 8. The size change is **instant** (no transition). The node's text
label moves up along with the radius so it stays above the larger circle. The
force simulation's collision radius stays at 18 — no physics changes.

| Property | Non-selected | Selected |
|---|---|---|
| Circle radius `r` | 8 | 16 |
| Circle stroke | `#fff` (white), 1.5px | `#ff6b6b` (red), 3px (unchanged) |
| Label `dy` offset | `-(8 + 6) = -14` | `-(16 + 6) = -22` |

Selection-driven sizing lives in the existing `applyHighlight()` function —
the single place that already owns the selected-vs-unselected visual state. It
iterates every visible node and sets `r` and `dy` from the highlight set, so it
handles all selection paths uniformly: single click, Shift-click, indirect
(ancestors/descendants/1st/2nd-degree) modes, Select All, Select Group, filter
changes, and deselection.

## Files to Modify

### 1. `crates/visualize/frontend/src/graph.ts` — Size constants + `applyHighlight()`

Add a selected-radius constant next to the existing `NODE_RADIUS` (note: `SELECTED_STROKE_WIDTH` is already declared in the codebase):

```ts
const NODE_RADIUS = 8;
const SELECTED_NODE_RADIUS = 16;
```

The enter-path defaults (`r = NODE_RADIUS`, `dy = -NODE_RADIUS - 6`) stay as
they are — `applyHighlight()` is invoked at the end of every `restartSimulation()`
and on every `setHighlighted()` call, so every node is re-styled from the
highlight set on entry. Extend `applyHighlight()` to set `r` and `dy` in
addition to the existing stroke/fill attributes:

```ts
function applyHighlight() {
  if (!nodeGroup) return;
  nodeGroup.each(function (d: SimNode) {
    const el = d3.select(this);
    const isSelected = highlighted.has(d.handle);
    el.select('circle')
      .attr('r', isSelected ? SELECTED_NODE_RADIUS : NODE_RADIUS)
      .attr('stroke', isSelected ? '#ff6b6b' : '#fff')
      .attr('stroke-width', isSelected ? SELECTED_STROKE_WIDTH : 1.5)
      // Ensure fill/stroke-dasharray/opacity stay correct (no overlapped deselection)
      .attr('fill', getNodeColor(d.birth_year, colorScale))
      .attr('stroke-dasharray', getNodeStrokeDash(d.is_imputed))
      .attr('opacity', getNodeOpacity(d.is_imputed));
    el.select('text').attr(
      'dy',
      isSelected ? -SELECTED_NODE_RADIUS - 6 : -NODE_RADIUS - 6,
    );
  });
}
```

Notes:

- `r` is set on **both** branches (selected and not) so deselection always
  restores radius 8 — no stale large nodes after `setHighlighted(new Set())`.
- The text `dy` update lives in the same loop, so label and circle always move
  together (user requirement).
- No other functions change. `restartSimulation()`, `setHighlighted()`, the
  controller, drag handlers, and force setup are untouched.

### 2. `crates/visualize/frontend/tests/graph.test.ts` — Rendering tests

Add a `describe('selected node sizing')` block using the happy-dom environment
already in place. Render a two-node graph, drive `setHighlighted`, and assert
the SVG attributes:

```ts
describe('selected node sizing', () => {
  it('renders selected nodes at 2x radius and unselected at default', () => {
    const data = makeGraph(
      [makeNode('p1'), makeNode('p2')],
      [{ source: 'p1', target: 'p2', link_type: 'Spouse' }],
    );
    const container = document.createElement('div');
    container.style.width = '800px';
    container.style.height = '600px';
    document.body.appendChild(container);

    const controller = renderGraph(container, data);
    controller.setHighlighted(new Set(['p1']));

    // DOM join order follows data.nodes order: p1 then p2
    const circles = container.querySelectorAll('circle');
    expect(circles).toHaveLength(2);
    expect(circles[0].getAttribute('r')).toBe('16'); // selected
    expect(circles[1].getAttribute('r')).toBe('8');  // unselected

    // Labels move up with the radius
    const labels = container.querySelectorAll('text');
    expect(labels[0].getAttribute('dy')).toBe('-22');
    expect(labels[1].getAttribute('dy')).toBe('-14');

    controller.destroy();
    document.body.removeChild(container);
  });

  it('restores default size when selection is cleared', () => {
    // render p1/p2, setHighlighted({p1}), then setHighlighted(new Set())
    // → both circles have r="8" and dy="-14"
  });

  it('grows all selected nodes in a multi-node selection', () => {
    // setHighlighted({p1, p2}) → both circles have r="16"
  });

  it('grows only visible nodes when a family-group filter is active', () => {
    // two groups; filter to group 1; setHighlighted includes a group-2 handle
    // → only the visible (group-1) selected node grows; group-2 node not in DOM
  });
});
```

Key assertions per test: `circle.getAttribute('r')` is `'16'` (selected) or
`'8'` (unselected), and `text.getAttribute('dy')` is `'-22'` / `'-14'`. The
"filter" case is worth covering because `restartSimulation()` rebuilds the DOM
on filter change and must re-apply sizes to newly entered nodes.

## Implementation Order

1. **`graph.ts` + `graph.test.ts`** — add `SELECTED_NODE_RADIUS = 16`,
   extend `applyHighlight()` to set `r` and `text dy`, and add the
   `selected node sizing` tests (render + `setHighlighted` + attribute
   assertions). Commit together.
2. **Build and verify** — run the full frontend suite and confirm the Rust
   side still compiles:

   ```bash
   cd crates/visualize/frontend && npm test
   cargo build -p visualize
   cargo test -p visualize
   ```

3. **Manual smoke test** — run the dev harness (`npm run dev`, open
   `test-harness.html` with `window.__GRAPH_DATA__` injected) and verify:
   clicking a node grows it to 2×; clicking again (deselect) shrinks it back;
   Select All grows every node; ancestor/descendant modes grow the whole
   indirect set; labels stay above the larger circles.

## Design Decisions

### Why 2× (radius 16 vs 8)?

The user explicitly chose 2× for maximum visibility. Radius 16 stays within the
existing collision radius of 18, so two selected nodes still cannot overlap in
the force layout — no physics changes required.

### Why instant, not animated?

The user chose instant. Selection state can change in bulk (Select All,
ancestors mode, filter changes), and instant updates avoid queued-transition
complexity in `applyHighlight()`, which runs synchronously after every
`setHighlighted()` call. (A future D3 transition wrapper around the same
attribute writes would be straightforward if desired — see Future
Considerations.)

### Why keep collision radius at 18?

The user chose to leave the force simulation untouched. A dynamic per-node
collision radius would require re-registering `forceCollide` with a radius
accessor on every selection change and reheating the simulation, which couples
visual state to physics and can cause layout jumps mid-interaction. Since
selected radius 16 < collision radius 18, overlaps are already prevented.

### Why put sizing in `applyHighlight()`?

`applyHighlight()` is the existing single code path that maps the highlight set
to per-node visual attributes. Adding `r`/`dy` there means every selection
mutation path (click, indirect modes, Select All/Group, filter rebuilds) gets
consistent sizing with zero new call sites, and the enter/update/exit D3 join
in `restartSimulation()` already invokes it after every rebuild.

### Why move the label with the radius?

The user chose this. The label's `dy` is a negative offset from the node
center; leaving it fixed at `-14` would let the top ~4px of a radius-16 circle
cover the label text. Moving it to `-22` keeps the same 6px clearance.

## Future Considerations

- **Configurable selected size** via a settings panel or a scale factor, if
  users want finer or larger emphasis.
- **Animated transitions** — wrap the `r`/`dy` attribute writes in a short
  D3 transition if smooth growth is ever desired; `applyHighlight()` is the
  single chokepoint to add it.
- **Hover emphasis** — the same mechanism could grow nodes slightly on hover
  (distinct from selection) if a hover ring is insufficient.
- **Dynamic collision radius** — if selected size grows further (e.g. 24px),
  revisit the collision force so dense selections don't overlap.
