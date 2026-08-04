# Selection Repel Force — Implementation Plan

## Goal

Give users a slider in the existing Force Controls panel that repels selected
nodes from non-selected nodes (and vice versa) using pairwise repulsion. This
allows strongly separating selected nodes from non-selected nodes by dialing
the repel strength up while dialing the structural forces (spouse, parent-child,
generation pull) down.

## Design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Control | Slider in Force Controls panel | Matches existing UX pattern; fine-grained 0–2× range |
| Algorithm | Pairwise repulsion | Organic separation; O(N·M) per tick is affordable for family-tree scales |
| Symmetry | Symmetric (both groups move) | Feels natural; user can pin one side by dragging unselected nodes |

---

## Implementation steps

### Step 1 — Extend `ForceConfig` and defaults (`types.ts`)

Add `repelStrength: number` to the `ForceConfig` interface and `DEFAULT_FORCE_CONFIG`.

```typescript
// types.ts — ForceConfig
export interface ForceConfig {
  generationPull: number;
  spouseStrength: number;
  parentChildStrength: number;
  repelStrength: number;        // ← new
}

export const DEFAULT_FORCE_CONFIG: ForceConfig = {
  generationPull: 0.30,
  spouseStrength: 0.80,
  parentChildStrength: 0.50,
  repelStrength: 0.00,          // ← new (off by default)
};
```

**How to test:** TypeScript compilation passes; no runtime change yet.

---

### Step 2 — Add the repel slider to `renderForcePanel` (`main.ts`)

In `renderForcePanel()`, add a fourth slider entry for `repelStrength` matching
the pattern of the existing three sliders:

```typescript
{ key: 'repelStrength', label: 'Selection repel' },
```

This slider appears in the collapsible Force Controls panel alongside the
existing generation, spouse, and parent-child sliders. The "Restore defaults"
button resets it to 0.00.

**How to test:** Open the Force Controls panel; the fourth slider is visible and
responds to drag. Clicking "Restore defaults" resets it to 0.00.

---

### Step 3 — Build the `selection-repel` custom D3 force (`graph.ts`)

Add a new exported function `createSelectionRepelForce()` that returns a custom
D3 force.

**Force contract:**

- `force.initialize(nodes)` — capture the node list.
- On each tick, if `strength > 0` and both the selected and unselected subsets
  are non-empty, apply pairwise repulsive impulses:

  ```
  For each selected node s:
    For each unselected node u:
      Vector from u → s determines the push direction
      s is pushed further away from u
      u is pushed further away from s (symmetric)
  ```

**Force interface:**

Export a `SelectionRepelForce` interface so callers can cast to it when
mutating strength at runtime:

```typescript
export interface SelectionRepelForce extends d3.Force<SimNode, undefined> {
  /** Get or set the repel multiplier in [0, 2]. */
  strength(s: number): this;
  strength(): number;
}
```

**Scaling:** The push impulse per selected–unselected pair is

```
impulse = alpha * strength * BASE_REPEL / r²
```

where `r = max(1, distance(s, u))`. Clamping the distance to a minimum of
1px prevents the 1/r² term from blowing up at coincident positions. Each
node's velocity delta is divided by `max(1, selectedCount)` so the total
impulse budget per tick is independent of how many nodes are selected. The
same impulse (opposite sign) is applied to the unselected node, divided by
`max(1, unselectedCount)` for symmetry. A `BASE_REPEL` constant of 500
provides a reasonable default magnitude.

**Degenerate cases handled:**

- Zero or one selected nodes → no-op.
- All nodes selected or no nodes unselected → no-op.
- Nodes at nearly identical positions (distance < 1px) → apply a small random
  offset (±0.5px in both x and y, using `Math.random()`) to break the tie,
  then compute the force normally.

**How to test (unit test):** Create a small synthetic simulation with known
positions (e.g., one selected node at `{x:0, y:0}`, 3 unselected at
`{x:100,y:0}`, `{x:0,y:100}`, `{x:100,y:100}`), run a few ticks with
`repelStrength = 1`, assert the selected node's velocity points away from the
unselected centroid.

---

### Step 4 — Register the repel force in `createSimulationForces` (`graph.ts`)

In `createSimulationForces()`, register the new force:

```typescript
.force(
  'selection-repel',
  createSelectionRepelForce(highlightedHandles, config.repelStrength),
)
```

**However**, the current signature of `createSimulationForces` does not receive
the selected set. We need to thread it through. Options:

- **A:** Add a `selectedHandles: Set<string>` parameter to `createSimulationForces`
  and `applyForceConfig`.
- **B:** Store the selected set as a mutable ref that `setHighlighted` updates,
  and the custom force reads it on each tick.

**Recommendation: Option B** — pass a getter callback `() => Set<string>` into
the custom force factory. On each tick the force calls the getter to read the
current selection live. This avoids touching `restartSimulation` every time
selection changes.

**Threading the getter through the call chain:**

The `selectedSet` variable lives inside `renderGraph` (a closure in `graph.ts`).
Three functions already inside that closure need the getter:

1. **`restartSimulation`** (closure inside `renderGraph`) — captures `selectedSet`
   and wraps it in a getter arrow `() => selectedSet`.
2. **`createSimulationForces`** (module-level) — gains a new parameter
   `getSelected: () => Set<string>`.
3. **`createSelectionRepelForce`** (module-level) — takes the same getter and
   stores it in the force's closure so `force.tick()` calls it each tick.

The call chain on simulation restart:

```
restartSimulation()
  → createSimulationForces(sim, config, genY, spouseLinks, pcLinks,
                           width, height, () => selectedSet)
    → .force('selection-repel', createSelectionRepelForce(() => selectedSet))
```

**Note for tests:** Existing callers of `createSimulationForces` (unit tests)
must pass a noop getter: `() => new Set<string>()`.

**Implementation sketch:**

```typescript
// In renderGraph(), add alongside the other state variables:
let selectedSet = new Set<string>();

// Module-level factory — captures getSelected in a closure:
export function createSelectionRepelForce(
  getSelected: () => Set<string>,
): SelectionRepelForce {
  // ... implementation stores getSelected, calls it every tick
}

// In createSimulationForces, new parameter:
export function createSimulationForces(
  sim: d3.Simulation<SimNode, undefined>,
  config: ForceConfig,
  genY: (d: SimNode) => number,
  spouseLinks: d3.SimulationLinkDatum<SimNode>[],
  pcLinks: d3.SimulationLinkDatum<SimNode>[],
  width: number,
  height: number,
  getSelected: () => Set<string>,  // ← new
): void {
  // ... existing forces, then:
  sim.force(
    'selection-repel',
    createSelectionRepelForce(getSelected).strength(config.repelStrength),
  );
}
```

**How to test:** Select some nodes. Verify no runtime error (the force is
registered even at strength 0). Increase repel strength slider; selected nodes
should move apart from unselected.

---

### Step 5 — Update `setHighlighted` to notify the force (`graph.ts`)

Modify `setHighlighted` in the controller to update the shared selected set so
the custom force reads it on the next tick. This is a one-line addition:

```typescript
setHighlighted(handles: Set<string>) {
  highlighted = handles;
  selectedSet = handles;  // ← new: update the force's live reference
  applyHighlight();
}
```

No simulation restart is needed — the force picks up the new set on the next
tick automatically.

**How to test:** Select a node; the force begins repelling on the next tick.
Deselect; repulsion stops. No simulation restart is triggered.

---

### Step 6 — Update `applyForceConfig` for live slider feedback (`graph.ts`)

Add a mutation for the `selection-repel` force in `applyForceConfig()` so
moving the repel slider updates the force strength without a simulation restart:

```typescript
const repel = simulation.force('selection-repel') as SelectionRepelForce | undefined;
if (repel) {
  repel.strength(config.repelStrength);
}
```

**How to test:** With nodes selected, drag the repel slider. Nodes visibly
separate in real time. Set slider back to 0; separation stops.

---

## Files changed

| File | Changes |
|---|---|
| `crates/visualize/frontend/src/types.ts` | Add `repelStrength` to `ForceConfig` and `DEFAULT_FORCE_CONFIG` |
| `crates/visualize/frontend/src/graph.ts` | Add `createSelectionRepelForce()` factory; register in `createSimulationForces()`; add live ref in `setHighlighted()`; update `applyForceConfig()` |
| `crates/visualize/frontend/src/main.ts` | Add slider entry to `renderForcePanel()` slider array |

No Rust-side changes are needed — this is a pure frontend feature.

---

## Testing strategy

### Unit tests (`graph.test.ts` or similar)

1. **Force registration & strength default:**
   Create a simulation via `createSimulationForces` with no selected nodes;
   assert `selection-repel` force exists and has strength 0.

2. **Pairwise repulsion math:**
   Manually call the force tick function with hand-crafted node positions:
   - One selected at (0,0), one unselected at (100,0).
   - After one tick with strength=1, alpha=1: the selected node velocity
     vx < 0 (pushed left, away from unselected) and the unselected vx > 0.
   - Verify symmetry: magnitudes of the impulses are equal.

3. **Edge cases:**
   - Empty selected set → no velocity change.
   - All nodes selected → no velocity change.
   - Coincident nodes (same x,y) → no NaN, velocity is finite.
   - Single selected node among many unselected → repelled but does not diverge.

4. **applyForceConfig mutation:**
   Register a simulation with `createSimulationForces` (passing a noop getter),
   call `applyForceConfig` with `repelStrength = 1.5`. Assert the
   `selection-repel` force's strength accessor returns 1.5. (Same pattern as
   existing `applyForceConfig roundtrip` test.)

5. **setHighlighted → force synchronization:**
   Create a simulation with a getter that returns a non-empty selected set;
   tick once with strength=1; assert selected-node velocity is non-zero.
   Swap the getter's backing set to empty; tick again; assert no further
   velocity change.

6. **Slider rendering and restore (main.test.ts):**
   Call `renderForcePanel` and assert a fourth slider with label
   "Selection repel" is present. Click "Restore defaults" and assert
   `repelStrength` resets to 0.00.

### Manual E2E test

1. Load a `.gramps` file with several family groups.
2. Select 2–3 nodes.
3. Open Force Controls panel.
4. Set Spouse bond, Parent-child bond, and Generation pull to 0.10.
5. Increase Selection repel to ~1.50.
6. **Expected:** Selected nodes visibly drift apart from the unselected mass.
7. Drag repel back to 0 → separation stops.
8. Deselect all → no force applied.

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| O(N·M) pairwise loop causes jank on large graphs | Add a guard: if N×M > 10,000, skip the force for that tick. Log a warning at most once per simulation restart (use a boolean sentinel to avoid console spam). Family trees rarely exceed this. |
| Repel + charge forces produce double repulsion on selected nodes | The repel force and the many-body charge both push nodes apart. When using high repel strength, users should reduce the `CHARGE_STRENGTH` constant (currently −300 in `graph.ts`) or dial repel down. Consider noting this trade-off in a tooltip on the repel slider. |
| Nodes pushed off-screen | The weak center force (`CENTER_STRENGTH = 0.05`) still applies; user can also increase it or zoom/pan. |

---

## Future enhancements (out of scope)

- **Repel-selected-only** checkbox to toggle asymmetric mode.
- **Clamp repel** so nodes stop moving once they exceed a configurable distance
  from the unselected cluster (avoid pushing nodes to infinity).
- **Animate** the selection state transition so nodes don't teleport.
