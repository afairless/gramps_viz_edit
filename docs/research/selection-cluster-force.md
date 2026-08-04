# Selection Cluster Forces — Implementation Plan

## Goal

Add two new custom D3 forces — `selected-attract` and `unselected-attract` —
that pull each subset of nodes toward its own centroid. Together with the
existing `selection-repel` force, this creates two visually distinct clusters:
selected nodes huddle together, unselected nodes huddle together, and the repel
force pushes the two clusters apart.

## Design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Sliders | Separate sliders for selected vs. unselected attract | Finer control; user can dial one side stronger |
| Algorithm | Linear spring toward subset centroid | Nodes far from centroid get the strongest pull, forming a natural cluster. Simple, predictable, no runaway 1/r² behavior |
| Config | Add to existing `ForceConfig` | All force controls stay in one place; no new types needed |

## Force contract

### `selected-attract`

```
On each tick:
  If selected set is empty or has fewer than 2 nodes → no-op.
  If no unselected nodes exist → no-op (nothing to separate from).
  Compute centroid of selected nodes (mean x, mean y).
  For each selected node:
    dx = centroid.x - node.x
    dy = centroid.y - node.y
    impulse = alpha * strength
    node.vx += dx * impulse
    node.vy += dy * impulse
```

### `unselected-attract`

```
On each tick:
  If unselected set is empty or has fewer than 2 nodes → no-op.
  If no selected nodes exist → no-op (nothing to separate from).
  Compute centroid of unselected nodes (mean x, mean y).
  For each unselected node:
    dx = centroid.x - node.x
    dy = centroid.y - node.y
    impulse = alpha * strength
    node.vx += dx * impulse
    node.vy += dy * impulse
```

**Why no-op when the complement set is empty?** When all nodes are selected (or
none are), there is no "other cluster" to separate from. Running the attract
forces in that case would pull all nodes toward a single centroid, competing
with generation bands and link forces with no benefit.

**Why a centroid rather than pairwise?** Pairwise attraction among selected nodes
would be O(S²) per tick. A centroid-based force is O(S) — compute the centroid
once, then apply one impulse per node. For the expected family-tree scale
(hundreds of nodes, dozens selected), the centroid approach is more than
adequate and avoids the O(N·M) pattern of the repel force.

---

## Implementation steps

### Step 1 — Extend `ForceConfig` and defaults (`types.ts`)

Add `selectedAttractStrength` and `unselectedAttractStrength` to the
`ForceConfig` interface and `DEFAULT_FORCE_CONFIG`:

```typescript
export interface ForceConfig {
  generationPull: number;
  spouseStrength: number;
  parentChildStrength: number;
  repelStrength: number;
  selectedAttractStrength: number;     // ← new
  unselectedAttractStrength: number;   // ← new
}

export const DEFAULT_FORCE_CONFIG: ForceConfig = {
  generationPull: 0.30,
  spouseStrength: 0.80,
  parentChildStrength: 0.50,
  repelStrength: 0.00,
  selectedAttractStrength: 0.00,       // ← new (off by default)
  unselectedAttractStrength: 0.00,     // ← new (off by default)
};
```

**How to test:** TypeScript compilation passes. The new required keys will
break every literal `ForceConfig` object in the test files — those must be
**updated in the same commit** (see "Test literal updates" below).

#### Step 1b — Update all literal `ForceConfig` objects in tests

Every test that constructs a `ForceConfig` literal must include the two new
keys. Search for `ForceConfig {` across the test files and add:

```typescript
selectedAttractStrength: 0,
unselectedAttractStrength: 0,
```

Locations in `graph.test.ts`:

- `createSimulationForces` tests (default config, provided-config test)
- `applyForceConfig roundtrip` tests (config1, config2, mutation test)
- `restartSimulation` test (testConfig literal)

Locations in `main.test.ts`:

- All `ForceConfig` object factories passed to `renderToolbar` / `renderForcePanel`
- Slider count assertions: `expect(sliders.length).toBe(4)` → `.toBe(6)` (line 311)
- Slider row count: `expect(sliderRows.length).toBe(4)` → `.toBe(6)` (line 343)
- Value span count: `expect(values.length).toBe(4)` → `.toBe(6)` (line 363)
- Test name: `'has all four keys'` → `'has all six keys'` (line 445)
- Add `expect(cfg).toHaveProperty('selectedAttractStrength')` and
  `expect(cfg).toHaveProperty('unselectedAttractStrength')` to that test

**How to test:** `npm test` passes. TypeScript compilation with `--strict` passes.

---

### Step 2 — Build `createSelectedAttractForce` (`graph.ts`)

Add the exported `AttractForce` interface and a new exported function
`createSelectedAttractForce()`. Both must be exported from the module so
`applyForceConfig` in Step 4 can reference the type.

```typescript
export interface AttractForce extends d3.Force<SimNode, undefined> {
  strength(s: number): this;
  strength(): number;
  initialize(nodes: SimNode[]): void;
}

export function createSelectedAttractForce(
  getSelected: () => Set<string>,
): AttractForce {
  let nodes: SimNode[] = [];
  let strengthValue = 0;

  function force(tickAlpha: number): void {
    const selected = getSelected();
    if (strengthValue === 0 || selected.size < 2) return;

    // Partition nodes
    const selectedNodes: SimNode[] = [];
    let hasUnselected = false;
    for (const n of nodes) {
      if (selected.has(n.handle)) {
        selectedNodes.push(n);
      } else {
        hasUnselected = true;
      }
    }

    if (!hasUnselected) return;

    // Compute centroid
    let cx = 0, cy = 0;
    for (const n of selectedNodes) {
      cx += n.x ?? 0;
      cy += n.y ?? 0;
    }
    cx /= selectedNodes.length;
    cy /= selectedNodes.length;

    const impulse = tickAlpha * strengthValue;
    for (const n of selectedNodes) {
      n.vx = (n.vx ?? 0) + (cx - (n.x ?? 0)) * impulse;
      n.vy = (n.vy ?? 0) + (cy - (n.y ?? 0)) * impulse;
    }
  }

  force.initialize = (nodeList: SimNode[]) => { nodes = nodeList; };
  force.strength = (s?: number) => {
    if (s === undefined) return strengthValue;
    strengthValue = s;
    return force;
  };

  return force as unknown as AttractForce;
}
```

**How to test (unit):** Create nodes with a spread-out selected set (e.g., at
(0,0), (100,0), (0,100)). After one tick with strength=1, alpha=1, all three
should have velocities pointing toward (~33, ~33). The node at (100,0) should
have negative vx (pulled left toward centroid).

---

### Step 3 — Build `createUnselectedAttractForce` (`graph.ts`)

Mirror of `createSelectedAttractForce`, but pulls **unselected** nodes toward
the **unselected** centroid. Only active when there is at least one selected
node (so there is an "other cluster" to separate from).

```typescript
export function createUnselectedAttractForce(
  getSelected: () => Set<string>,
): AttractForce {
  // Same structure, but operates on unselected nodes
  // ...
}
```

**How to test (unit):** Complement of the selected-attract test. Selected nodes
stay still; unselected nodes are pulled toward the unselected centroid.

---

### Step 4 — Register both forces in `createSimulationForces` and update `applyForceConfig` (`graph.ts`)

In `createSimulationForces()`, register the two new forces:

```typescript
.force(
  'selected-attract',
  createSelectedAttractForce(getSelected).strength(config.selectedAttractStrength),
)
.force(
  'unselected-attract',
  createUnselectedAttractForce(getSelected).strength(config.unselectedAttractStrength),
)
```

In `applyForceConfig()`, mutate both forces:

```typescript
const selAttract = simulation.force('selected-attract') as AttractForce | undefined;
if (selAttract) selAttract.strength(config.selectedAttractStrength);

const unselAttract = simulation.force('unselected-attract') as AttractForce | undefined;
if (unselAttract) unselAttract.strength(config.unselectedAttractStrength);
```

Note: `simulation.force()` in D3 v7 returns `Force<SimNode, undefined> | undefined`,
and the constructed forces are registered as that base type. The cast to
`AttractForce` is safe because we created them.

**How to test (unit):** Assert both forces are registered on the simulation
and have the expected strength from config.

---

### Step 5 — Add sliders in `renderForcePanel` (`main.ts`)

Add two new entries to the `sliders` array in `renderForcePanel()`:

```typescript
const sliders: SliderDef[] = [
  { key: 'generationPull', label: 'Generation pull' },
  { key: 'spouseStrength', label: 'Spouse bond' },
  { key: 'parentChildStrength', label: 'Parent-child bond' },
  { key: 'repelStrength', label: 'Selection repel' },
  { key: 'selectedAttractStrength', label: 'Selected attract' },     // ← new
  { key: 'unselectedAttractStrength', label: 'Unselected attract' }, // ← new
];
```

The existing slider factory loop (`for (const s of sliders)`) handles these
automatically — no other changes needed. The "Restore defaults" button resets
both to 0.00.

**How to test:** Open the Force Controls panel; two new sliders are visible.
Drag them; values update. Click "Restore defaults"; both reset to 0.00.

---

### Step 6 — Add unit tests for both attract forces (`graph.test.ts`)

Follow the pattern of the existing `createSelectionRepelForce` tests:

1. **Basic properties:** strength getter/setter, initialize, `is a d3.Force`.
2. **Centroid computation:** Three selected nodes at (0,0), (100,0), (0,100).
   Centroid at (~33.3, ~33.3). After tick with strength=1, alpha=1:
   - Node at (100,0): vx < 0 (pulled left), vy > 0 (pulled down).
   - Node at (0,100): vx > 0, vy < 0.
   - Node at (0,0): vx > 0, vy > 0.
3. **Edge cases:**
   - Empty selected set → no-op.
   - Single selected → no-op (need ≥2 to form a cluster).
   - All nodes selected → no-op (no unselected complement).
   - No unselected nodes → no-op.
   - Strength 0 → no velocity change.
4. **Unselected-attract mirror:** Same tests but for unselected nodes. Selected
   nodes should remain stationary while unselected nodes cluster.
5. **Force registration in simulation:** Both `selected-attract` and
   `unselected-attract` forces are present in simulation after
   `createSimulationForces`, default strength 0.
6. **applyForceConfig mutation:** Changing config values updates the forces
   without a simulation restart.

---

## Files changed

| File | Changes |
|---|---|
| `crates/visualize/frontend/src/types.ts` | Add `selectedAttractStrength`, `unselectedAttractStrength` to `ForceConfig` and `DEFAULT_FORCE_CONFIG` |
| `crates/visualize/frontend/src/graph.ts` | Add `AttractForce` interface, `createSelectedAttractForce()`, `createUnselectedAttractForce()`; register in `createSimulationForces()`; update `applyForceConfig()` |
| `crates/visualize/frontend/src/main.ts` | Add two slider entries to `renderForcePanel()` |
| `crates/visualize/frontend/tests/graph.test.ts` | Add unit tests for both new forces; update all literal `ForceConfig` objects |
| `crates/visualize/frontend/tests/main.test.ts` | Update ForceConfig literals, slider count assertions (4→6), key-property test (4→6 keys) |

No Rust-side changes needed.

---

## Force interaction summary

With all three cluster forces active, the expected behavior is:

| Force | Effect |
|---|---|
| `selection-repel` | Pushes selected and unselected nodes apart (pairwise, symmetric) |
| `selected-attract` | Pulls selected nodes toward the selected centroid |
| `unselected-attract` | Pulls unselected nodes toward the unselected centroid |

**Playbook for a user wanting distinct clusters:**

1. Reduce structural forces (generation pull, spouse, parent-child) to low
   values (~0.10).
2. Set Selection repel high (~1.50).
3. Set Selected attract and Unselected attract to moderate values (~0.80–1.20).
4. The result: two tight clusters separated by the repel force, with selected
   nodes in one and unselected in the other.

**Degenerate case:** If both attract forces are 0 but repel is high, selected
and unselected nodes push apart symmetrically but neither side clusters — they
spread out. This is the current behavior the user wants to improve.

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Centroid-based clustering competes with generation bands | The user controls the balance — reduce generation pull when using cluster forces. No code change needed. |
| Attract forces cause nodes to overshoot and oscillate around the centroid | The linear spring force naturally damps as nodes approach the centroid (impulse ∝ distance). D3's built-in alpha cooling further damps oscillation. Acceptable without explicit damping. |
| Six sliders may overwhelm the Force Controls panel | The panel is collapsible and defaults to collapsed. Sliders are labeled clearly. This is a design trade-off the user explicitly chose. |
| Pinned nodes (fx/fy) are not special-cased by attract forces | Pinned nodes still receive velocity from the attract force, but their position doesn't change because D3's simulation applies fx/fy after velocity integration. This is correct behavior — pinned nodes contribute to the centroid but don't move. |

---

## Future enhancements (out of scope)

- **Damping factor** on the attract forces to prevent oscillation at very high strengths.
- **Visual cluster indicator** — a shaded ellipse or convex hull around each cluster.
- **Animate** the transition when selection changes so nodes visibly drift into their clusters rather than jumping.
