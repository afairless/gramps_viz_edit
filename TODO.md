# Implementation Plan: Selection Repel Force

Source: `docs/research/selection-repel-force.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add repelStrength to ForceConfig and defaults` | Types & defaults | `crates/visualize/frontend/src/types.ts` | Unit |
| 2 | `feat: add Selection repel slider to Force Controls panel` | Slider UI | `crates/visualize/frontend/src/main.ts` | Unit |
| 3 | `feat: build custom selection-repel D3 force` | Repel force function | `crates/visualize/frontend/src/graph.ts`, `crates/visualize/frontend/tests/graph.test.ts` | Unit, property-based (symmetry) |
| 4 | `feat: register selection-repel force in simulation and thread selected set` | Force wiring | `crates/visualize/frontend/src/graph.ts`, `crates/visualize/frontend/tests/graph.test.ts` | Unit |
| 5 | `feat: sync selected set to repel force on highlight change` | Selection sync | `crates/visualize/frontend/src/graph.ts` | Unit |
| 6 | `feat: live-update repel strength in applyForceConfig` | Live slider feedback | `crates/visualize/frontend/src/graph.ts`, `crates/visualize/frontend/tests/graph.test.ts` | Unit |

---

## Step details

### Step 1 — Types & defaults (`types.ts`)

**Deliverables:**

- Add `repelStrength: number` to `ForceConfig` interface
- Add `repelStrength: 0.00` to `DEFAULT_FORCE_CONFIG`

**Test updates:**

- `DEFAULT_FORCE_CONFIG has all three keys` → change to four keys
- Add `repelStrength` default value and range check

**How to test:** TypeScript compilation passes; `DEFAULT_FORCE_CONFIG.repelStrength === 0.00`.

---

### Step 2 — Slider UI (`main.ts`)

**Deliverables:**

- Add `{ key: 'repelStrength', label: 'Selection repel' }` to the `sliders` array in `renderForcePanel()`
- "Restore defaults" button already iterates `sliders` array, so it automatically resets `repelStrength` to 0.00

**Test updates:**

- `has three sliders` → assert four sliders exist
- `restore defaults` test already iterates all sliders — it should still pass (fourth slider is reset to 0.00)
- Restore defaults test's `sliders.length === 3` assertion → change to 4

**How to test:** Open Force Controls panel; fourth slider "Selection repel" is visible and responds to drag. Clicking "Restore defaults" resets it to 0.00.

---

### Step 3 — Repel force function (`graph.ts`)

**Deliverables:**

- Export `SelectionRepelForce` interface extending `d3.Force<SimNode, undefined>` with `strength()` getter/setter
- Export `createSelectionRepelForce(getSelected: () => Set<string>): SelectionRepelForce`
- Implement pairwise repulsion: for each selected↔unselected pair, compute impulse `alpha * strength * BASE_REPEL / r²` where `r = max(1, distance)`
- Divide impulse by `max(1, selectedCount)` for selected nodes, `max(1, unselectedCount)` for unselected nodes (symmetric)
- **O(N·M) guard:** If `selectedCount * unselectedCount > 10_000`, skip the force for that tick. Log a `console.warn` at most once per simulation restart (use a boolean sentinel).
- **Tie-breaking:** If distance < 1px, apply a small random offset (±0.5px in both x/y) then compute force normally
- `BASE_REPEL = 500` constant (module-level, not exported)
- Degenerate cases: zero or one selected nodes → no-op; all nodes selected or none unselected → no-op

**Test updates:**

- New tests: force registration & strength default, pairwise repulsion math (direction + symmetry), edge cases (empty selected set, all selected, coincident nodes, single selected node among many unselected)

**How to test:** Unit tests with synthetic simulation and known positions. Verify velocity direction and symmetry.

---

### Step 4 — Force wiring (`graph.ts`)

**Deliverables:**

- Add `selectedSet` variable to `renderGraph` closure alongside `highlighted` (line ~477)
- Add `getSelected: () => Set<string>` parameter to `createSimulationForces()`
- In `restartSimulation()`, pass `() => selectedSet` as the getter to `createSimulationForces()`
- Register the `selection-repel` force: `.force('selection-repel', createSelectionRepelForce(getSelected).strength(config.repelStrength))`
- Export `createSelectionRepelForce` from `graph.ts` (already exported in Step 3)

**Test updates:**

- Update all existing `createSimulationForces` callers in `graph.test.ts` (lines 545, 577, 596) to pass a noop getter `() => new Set<string>()`
- `registers all six named forces` → change to seven forces and assert `selection-repel` exists with strength 0

**How to test:** Select some nodes. Verify no runtime error (the force is registered even at strength 0). Increase repel slider; selected nodes should move apart from unselected.

---

### Step 5 — Selection sync (`graph.ts`)

**Deliverables:**

- In the `setHighlighted` method of the `GraphController`, add `selectedSet = handles;` alongside the existing `highlighted = handles;`

**How to test:** Select a node; the force begins repelling on the next tick. Deselect; repulsion stops. No simulation restart is triggered.

---

### Step 6 — Live slider feedback (`graph.ts`)

**Deliverables:**

- In `applyForceConfig()`, look up the `selection-repel` force via `simulation.force('selection-repel')`, cast to `SelectionRepelForce`, and call `.strength(config.repelStrength)`

**Test updates:**

- Add test: register simulation with `createSimulationForces`, call `applyForceConfig` with `repelStrength = 1.5`, assert `selection-repel` force's strength returns 1.5

**How to test:** With nodes selected, drag the repel slider. Nodes visibly separate in real time. Set slider back to 0; separation stops.

---

## Files changed

| File | Changes |
|---|---|
| `crates/visualize/frontend/src/types.ts` | Add `repelStrength` to `ForceConfig` and `DEFAULT_FORCE_CONFIG` |
| `crates/visualize/frontend/src/main.ts` | Add slider entry to `renderForcePanel()` |
| `crates/visualize/frontend/src/graph.ts` | `createSelectionRepelForce()`, `createSimulationForces()` getter param, `setHighlighted()` sync, `applyForceConfig()` mutation |
| `crates/visualize/frontend/tests/graph.test.ts` | Existing callers updated, new tests for repel force math, wiring, and live feedback |
| `crates/visualize/frontend/tests/main.test.ts` | Slider count assertions updated (3→4) |

## Reference

- Full design rationale: `docs/research/selection-repel-force.md`
- Existing force simulation API: `crates/visualize/frontend/src/graph.ts` — `createSimulationForces`, `applyForceConfig`, `renderGraph`
- Existing test patterns: `crates/visualize/frontend/tests/graph.test.ts`, `crates/visualize/frontend/tests/main.test.ts`
