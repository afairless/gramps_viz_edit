# Per-Generation Field Force in the Force-Directed Layout

## Summary

Add a soft Y-axis field force (one per generation level) and per-type spring
strengths (spouse vs. parent-child) to the D3 force simulation, with three
slider controls and a reset button, so generations naturally separate into
horizontal bands without rigid layering rules.

**Design decisions** (settled via user input):

| Decision | Choice |
|---|---|
| Slider granularity | 3 sliders, each a 0–2× multiplier on a sensible default |
| UI placement | Collapsible panel in the top-left toolbar area, collapsed by default |
| Generation band spacing | Auto-computed from viewport height and family-group span |

---

## Data flow

```
Slider change (main.ts)
  → local ForceConfig object updated (no simulation impact yet)

User clicks "Reset Layout"
  → controller.setForceConfig(config)   // updates stored config + rebuilds force params
  → controller.resetLayout()            // clears pinned positions, resets zoom, reheats α=1

User clicks "Restore defaults"
  → sliders reset to default positions  // updates local ForceConfig
  → controller.setForceConfig(defaults) // auto-applies
  → controller.resetLayout()            // triggers re-layout
```

No Tauri/Rust changes are needed — this is entirely a frontend change. The
`generation` field already exists on every `SimNode` (computed in Rust by
`compute_generations`). The `family_group.span` metadata is also already
available via `FamilyGroupMeta`.

---

## Implementation plan

### Step 1 — Add `ForceConfig` type and defaults

**File:** `crates/visualize/frontend/src/types.ts`

Add a new interface and default constant:

```ts
export interface ForceConfig {
  /** Multiplier for the Y-field pull toward a node's generation band (0–2). */
  generationPull: number;
  /** Multiplier for spouse-link spring stiffness (0–2). */
  spouseStrength: number;
  /** Multiplier for parent-child link spring stiffness (0–2). */
  parentChildStrength: number;
}

export const DEFAULT_FORCE_CONFIG: ForceConfig = {
  generationPull: 0.30,
  spouseStrength: 0.80,
  parentChildStrength: 0.50,
};
```

Export from `types.ts` so both `graph.ts` and `main.ts` can import.

---

### Step 2 — Extract force configuration from `restartSimulation`

**File:** `crates/visualize/frontend/src/graph.ts`

Currently the forces are baked into the `restartSimulation` closure inside
`renderGraph`.  We need to:

1. Lift the force-creation block out of `restartSimulation` into a standalone
   helper function so it can be called on every config change without
   recreating the whole simulation / D3 selections.

2. Split the single `forceLink` into **two named link forces** to support
   different strengths per link type:

   ```
   force('spouse-link',  d3.forceLink(spouseLinks).distance(40).strength(…))
   force('pc-link',      d3.forceLink(pcLinks).distance(120).strength(…))
   ```

3. Add a **generation field force**:

   ```
   force('gen-field', d3.forceY().y(d => generationY(d)).strength(…))
   ```

   where `generationY` is computed from the node's generation number and the
   auto-computed band spacing.

4. Store the active `ForceConfig` in the `renderGraph` closure so the existing
   `restartSimulation` (called on filter changes) uses the current config.

#### Auto-computed generation spacing

A pure helper (exported for testing):

```ts
export function computeGenerationSpacing(
  nodes: SimNode[],
  canvasHeight: number,
): number {
  // Guard: non-positive height means no viewport area — nothing to space.
  if (canvasHeight <= 0) return 0;
  const gens = nodes.map((n) => n.generation);
  if (gens.length === 0) return 0;
  const minGen = Math.min(...gens);
  const maxGen = Math.max(...gens);
  const numGens = maxGen - minGen + 1;
  if (numGens <= 1) return 0;
  // Use ~70 % of the canvas height, leaving top/bottom margin.
  // The Math.max(40, …) floor ensures minimum spacing for very deep
  // trees where (height × 0.7) / (numGens - 1) would otherwise be
  // too tight (e.g. 20 generations in a 600 px canvas → 22 px).
  return Math.max(40, (canvasHeight * 0.7) / (numGens - 1));
}
// Note: generation values are expected to be finite integers; NaN
// generation would produce NaN spacing and is treated as a contract
// violation at the `setForceConfig` call site.  The function itself
// does not guard against NaN — callers must ensure valid input.
```

When spacing is 0 (single generation) the forceY target is a single Y value
for all nodes — effectively a weak horizontal-centering force, harmless.

#### Base distances and fallback

| Constant | Value | Rationale |
|---|---|---|
| `SPOUSE_BASE_DISTANCE` | 40 px | Couples should be close |
| `PC_BASE_DISTANCE` | 120 px | Generations need vertical room |
| `CHARGE_STRENGTH` | −300 | Unchanged from current |
| `COLLIDE_RADIUS` | 18 px | Unchanged from current |
| `CENTER_STRENGTH` | 0.05 | Reduced (gen-field handles Y; keep weak X-centering) |

The link-force `strength()` D3 parameter is already a number in [0, 1], so
the slider multipliers map directly: `strength(sliderValue)`.

For the forceY `strength()`, D3 accepts a coefficient controlling the
proportion of velocity applied toward the target per tick. Values in
[0, 2] are valid and more aggressive values accelerate convergence.
`generationPull` maps directly to this coefficient — unlike
`forceLink.strength()` which is a normalized [0, 1] stiffness.

`d3.forceY` is part of the existing `d3` import — no new dependencies
needed.

#### Fix `resize()` to preserve the new `CENTER_STRENGTH`

**Pre-existing bug:** The `resize()` method creates a new `forceCenter`
without specifying strength, which resets it to D3's default of 1.0:

```ts
// BUG: loses the configured CENTER_STRENGTH (currently 0.3, → 0.05 after this change)
simulation.force('center', d3.forceCenter(w / 2, h / 2));
```

Fix by preserving the constant:

```ts
const CENTER_STRENGTH = 0.05;
// … in resize():
simulation.force(
  'center',
  d3.forceCenter(w / 2, h / 2).strength(CENTER_STRENGTH),
);
```

This fix must land in the same commit as the `CENTER_STRENGTH` change —
otherwise a window resize would blow the strength back to 1.0.

#### How `restartSimulation` and `applyForceConfig` relate

There are two distinct code paths that need force configuration:

| Path | Trigger | What happens |
|---|---|---|
| `restartSimulation()` | Filter change, data update, init | Creates a **fresh** simulation via a new helper `createSimulationForces(sim, config, genY)` that registers the four named forces with the current config. |
| `applyForceConfig()` | Slider → Reset Layout click | Mutates forces on the **existing** simulation in place (no D3 selections torn down). |

`setForceConfig` calls `applyForceConfig` for live mutation.
`restartSimulation` calls the lifted helper to build a fresh simulation with
the stored `currentConfig`.  The two paths are mutually exclusive —
`applyForceConfig` is never called on a fresh simulation that hasn't had forces
registered first.

**Config ownership note:** The `renderGraph` closure stores the *applied*
config (used by `restartSimulation`).  `main()` stores a separate *pending*
config (updated by sliders).  They only converge when the user clicks
"Reset Layout", which calls `controller.setForceConfig(pending)` to
promote the pending config to applied.  See the "Unapplied slider state"
edge case for the visual indicator that keeps these states from diverging
silently.

#### Updating forces without recreating the simulation

D3 allows mutating force parameters on a running simulation:

```ts
function applyForceConfig(
  simulation: d3.Simulation<SimNode, undefined>,
  config: ForceConfig,
  genY: (d: SimNode) => number,
): void {
  simulation.force('spouse-link')?.strength(config.spouseStrength);
  simulation.force('pc-link')?.strength(config.parentChildStrength);
  const gf = simulation.force('gen-field');
  if (gf) {
    (gf as d3.ForceY<SimNode>).strength(config.generationPull).y(genY);
  }
}
```

---

### Step 3 — Add `setForceConfig` to `GraphController`

**File:** `crates/visualize/frontend/src/graph.ts`

Extend the `GraphController` interface:

```ts
export interface GraphController {
  // … existing methods …
  /** Update force configuration and reheat the simulation. */
  setForceConfig(config: ForceConfig): void;
}
```

Implementation inside `renderGraph`:

```ts
// Read current SVG dimensions — these can change after window resize,
// so capture at construction time is NOT sufficient for spacing.
function getSvgHeight(): number {
  const el = svg.node();
  if (el) {
    const rect = el.getBoundingClientRect();
    if (rect.height > 0) return rect.height;
  }
  return containerElement.clientHeight || 600;
}
// …
setForceConfig(config: ForceConfig) {
  currentConfig = { ...config };
  // Use the *active* node set so the computed generation range matches
  // what the simulation is actually running on (handles filtered views).
  const activeNodes =
    currentFilter === null
      ? simNodes
      : simNodes.filter((n) => n.family_group === currentFilter);
  if (activeNodes.length === 0) return; // nothing to configure
  const spacing = computeGenerationSpacing(activeNodes, getSvgHeight());
  const minGen = Math.min(...activeNodes.map((n) => n.generation));
  const targetY = (d: SimNode) => (d.generation - minGen) * spacing;
  applyForceConfig(simulation, currentConfig, targetY);
  simulation.alpha(0.3).restart();
}
```

`restartSimulation` likewise computes `targetY` from `getSvgHeight()` and
the filtered node set when creating a fresh simulation.  The
`createSimulationForces` helper signature is:

```ts
function createSimulationForces(
  sim: d3.Simulation<SimNode, undefined>,
  config: ForceConfig,
  genY: (d: SimNode) => number,
): void {
  sim
    .force('spouse-link', d3.forceLink(spouseLinks).distance(SPOUSE_BASE_DISTANCE).strength(config.spouseStrength))
    .force('pc-link', d3.forceLink(pcLinks).distance(PC_BASE_DISTANCE).strength(config.parentChildStrength))
    .force('gen-field', d3.forceY<SimNode>().y(genY).strength(config.generationPull))
    .force('charge', d3.forceManyBody().strength(CHARGE_STRENGTH))
    .force('collision', d3.forceCollide(COLLIDE_RADIUS))
    .force('center', d3.forceCenter(getSvgWidth() / 2, getSvgHeight() / 2).strength(CENTER_STRENGTH));
}
```

The existing `resetLayout` stays as-is (clear pins, reset zoom, α=1 reheat)
because the forces are already mutated to the current config by
`setForceConfig`.

> **Note on double reheat:** When the user clicks "Reset Layout,"
> `setForceConfig` reheats α=0.3 and `resetLayout` immediately reheats
> α=1. The first reheat is overwritten — harmless, just a small wasted
> tick. If profiling shows it matters, skip the reheat in `setForceConfig`
> and let `resetLayout` handle it; but the simpler code keeps both.

> **Note on resize + spacing:** After a window resize, generation spacing
> becomes stale until the user clicks "Reset Layout" again (which
> recomputes via `getSvgHeight()`).  For a tall→short resize,
> generation-band Y targets may fall outside the visible viewport.  This
> is a known v1 limitation — a future enhancement could call
> `setForceConfig` automatically on resize when a config is active.

---

### Step 4 — Build the collapsible force-control panel

**File:** `crates/visualize/frontend/src/main.ts`

Add a new function `renderForcePanel(config: ForceConfig, onChange: (c: ForceConfig) => void): HTMLElement`
that builds the DOM. Export it so tests can import it (or keep it module-private
and test via integration harness).

#### Structure

```
┌─ Force Controls ───────────────────────── [▼] ─┐
│                                                 │
│  Generation pull   ─────────●──────  1.20       │
│  Spouse bond       ────────●───────  0.80       │
│  Parent-child bond ────────●───────  0.50       │
│                                                 │
│  [Restore defaults]                             │
└─────────────────────────────────────────────────┘
```

- **Collapsed state** (default): only the header row is visible. Clicking the
  header or the `▼`/`▲` toggle expands/collapses the body.
- **Expanded state**: three `<input type="range">` sliders plus the restore
  button.
- Each slider's `min="0" max="200" step="1"` maps to 0.00–2.00 (divide raw
  value by 100).
- A `<span class="value">` next to each label shows the current number with
  two decimal places.
- `onChange` fires on every `input` event on any slider — the caller
  (`main.ts`) stores the config but does **not** call the controller until
  "Reset Layout" is clicked.
- "Restore defaults" resets all sliders to the values in `DEFAULT_FORCE_CONFIG`,
  calls `onChange` with the defaults, and (the caller) also calls
  `controller.setForceConfig(defaults)` + `controller.resetLayout()`.

#### Integration into `renderToolbar`

The existing `renderToolbar` function returns a toolbar containing the
family-group filter dropdown and the reset button.  The force panel will be
appended to this same toolbar, right after the reset button.

To give the reset button access to the pending `ForceConfig`, change
`renderToolbar`'s signature to:

```ts
export function renderToolbar(
  graphData: GraphData,
  controller: GraphController,
  forceConfig?: ForceConfig,
  onForceConfigChange?: (c: ForceConfig) => void,
): HTMLElement
```

When `forceConfig` and `onForceConfigChange` are provided, `renderToolbar`
appends `renderForcePanel(forceConfig, onForceConfigChange)` inside the
toolbar after the reset button.  The reset button click handler is wired
to call `controller.setForceConfig(forceConfig)` before
`controller.resetLayout()` — reading the live `forceConfig` reference
(not a snapshot).

The existing `renderGraphFromData` already calls `renderToolbar` — the force
panel piggybacks on that flow.

#### Reset button wiring change

Currently the reset button calls `controller.resetLayout()`.  After the
change, the button calls:

```ts
resetBtn.addEventListener('click', () => {
  controller.setForceConfig(currentForceConfig);
  controller.resetLayout();
});
```

where `currentForceConfig` is the mutable object owned by
`renderGraphFromData` / `main()` and passed to `renderToolbar` via its new
`forceConfig` parameter.  The button reads the live reference at click
time, so it always applies the latest slider values.

The button also gets a `data-dirty` attribute (or CSS class) when the
pending slider config differs from the last-applied config — see
"Unapplied slider state" in the edge cases section.

> **Config ownership summary:** `main()` stores the *pending* config
> (updated by sliders, passed to `renderToolbar`).  The `renderGraph`
> closure stores the *applied* config (used by `restartSimulation`).
> The "Reset Layout" click is the moment they converge: pending → applied.

> **Test impact:** The existing `renderToolbar` test mock (`main.test.ts`)
> only stubs `resetLayout`. After this change, the mock must also include
> `setForceConfig: vi.fn()` and both `setForceConfig` and `resetLayout`
> must be asserted in order on the reset-button click test. See Step 6.

---

### Step 5 — Add CSS

**File:** `crates/visualize/frontend/styles/main.css`

New rules for the force panel:

- `#force-panel` — positioned inside the toolbar, `display: flex; flex-direction: column`
- `.force-header` — flex row with title + toggle button, cursor pointer
- `.force-body` — `display: none` when collapsed, flex column when expanded
- `.force-slider` — flex row with label, slider, value span
- Slider width ~120 px, labels 11px font
- "Restore defaults" button styled consistently with the existing reset button

No changes needed to existing rules — the toolbar is already
`position: absolute` with appropriate z-index.

---

### Step 6 — Update tests

**File:** `crates/visualize/frontend/tests/graph.test.ts`

Add tests for:

1. **`DEFAULT_FORCE_CONFIG` shape** — verify all three keys exist and values
   are within [0, 2].

2. **`computeGenerationSpacing`**:
   - Empty nodes → 0
   - Single generation → 0
   - Two generations with height 600 → ~420 px/generation
   - Uniform generation (all same) → 0
   - 5 generations, height 1000 → ~175 px/generation
   - Height ≤ 0 → 0
   - NaN generation values → NaN (contract violation; callers must guard
     before calling this function)

3. **`ForceConfig` roundtrip** — creating a config object, applying to a
   simulation via `applyForceConfig`, then reading back the force parameters
   yields the expected values (this is a unit test against D3's public API).

4. **`restartSimulation` force creation** — verify that after a filter
   change, the fresh simulation registers all four named forces
   (`spouse-link`, `pc-link`, `gen-field`, `charge`, `collision`,
   `center`) with the applied config values.

5. **Filter + setForceConfig interaction** — verify that
   `setForceConfig` computes generation spacing from the *filtered*
   node set, not the full node set.

**File:** `crates/visualize/frontend/tests/graph.test.ts` — no structural
changes needed; new tests go into a new `describe('force configuration')`
block.

**File:** `crates/visualize/frontend/tests/main.test.ts`:

1. **Update existing mock:** The `renderToolbar` test creates a mock
   `GraphController` with only `resetLayout`. After Step 3 adds
   `setForceConfig` to the interface, this mock must also include
   `setForceConfig: vi.fn()`. The reset-button click test must assert
   that `setForceConfig` is called before `resetLayout`:

   ```ts
   const mockController = {
     resetLayout: vi.fn(),
     setForceConfig: vi.fn(),
   } as unknown as GraphController;
   // …
   resetBtn.click();
   expect(mockController.setForceConfig).toHaveBeenCalledTimes(1);
   expect(mockController.resetLayout).toHaveBeenCalledTimes(1);
   // verify ordering:
   expect(mockController.setForceConfig.mock.invocationCallOrder[0])
     .toBeLessThan(mockController.resetLayout.mock.invocationCallOrder[0]);
   ```

2. **Add integration tests for the force panel DOM**. Happy-dom supports
   range inputs and click events, so these must be automated:
   - Slider `input` event updates the adjacent value `<span>`.
   - "Restore defaults" button resets all three sliders to
     `DEFAULT_FORCE_CONFIG` values.
   - Panel is collapsed by default (force body has `display: none`).
   - Clicking the header toggles expanded/collapsed state.

---

### Step 7 — Smoke-test with test-harness.html

**File:** `crates/visualize/frontend/test-harness.html`

The test harness already has a "Reset layout" button and status display.
Extend it with:

1. Three sliders matching the force panel (hard-coded HTML for the harness,
   not the full panel widget).
2. Wire them to `controller.setForceConfig()` + `controller.resetLayout()`.
3. Verify visually that the sliders affect the layout as expected.

This is manual QA — no automated test here, but the harness is valuable for
catching integration issues before they hit the Tauri binary.

---

## File change summary

| Step | File | Change |
|---|---|---|
| 1 | `frontend/src/types.ts` | Add `ForceConfig` + `DEFAULT_FORCE_CONFIG` |
| 2 | `frontend/src/graph.ts` | Lift force creation; split link into two; add gen-field `forceY`; `applyForceConfig` helper; store config in closure; fix `resize()` to preserve `CENTER_STRENGTH` (pre-existing bug) |
| 3 | `frontend/src/graph.ts` | Add `setForceConfig` to `GraphController` interface + implementation |
| 4 | `frontend/src/main.ts` | Add `renderForcePanel`; wire to controller; update reset button wiring; extend `renderToolbar` signature |
| 5 | `frontend/styles/main.css` | Add force-panel styles |
| 6 | `frontend/tests/graph.test.ts` | Add force-config unit tests (spacing, NaN, applyForceConfig roundtrip, restartSimulation force creation) |
| 6 | `frontend/tests/main.test.ts` | Update mock for `setForceConfig`; add panel integration tests (slider→value display, restore defaults) |
| 7 | `frontend/test-harness.html` | Add manual test sliders |

No Rust or schema-side changes.

> **Note on config persistence:** Force config is not persisted across page
> reloads in this version.  The user's slider choices are lost on refresh.
> This is acceptable for v1; localStorage or Tauri store integration can
> be added in a follow-up.

---

## Edge cases and gotchas

### Single-generation or one-person family group

`computeGenerationSpacing` returns 0, so `forceY` becomes a no-op (all nodes
target the same Y). The layout falls back to the current behavior — no
regression.

### Family group filter change

Calling `setFamilyGroupFilter` already calls `restartSimulation()`.  The
restarted simulation must read `currentConfig` from the closure to rebuild
the two link forces and the gen-field force for the new node set.  Step 2
stores config in the closure; `restartSimulation` reads it.

After a filter change, `setForceConfig` must also recompute the generation
bounds from the **filtered** node set — see the `activeNodes` logic in Step 3.
Otherwise a filtered group whose generations don't span the full range will be
offset from its expected Y position.

### Empty filter (all nodes removed)

Already handled by the existing `restartSimulation` — it removes all link/node
elements. Force config updates are harmless when there are no nodes.

### Very deep tree (>20 generations)

With 70 % of height and 20 generations, spacing ≈ (height × 0.7) / 19. For
a 600 px canvas: ~22 px per generation. That's tight but passable. The
`Math.max(40, …)` floor ensures minimum spacing for very deep trees where
the raw calculation would be too compressed. If this proves too tight in
practice, the formula can be tuned later.

### Zoom interaction

`resetLayout` already resets zoom to identity via `zoom.transform`. This
happens after force config changes, so the user sees the new layout at 1×.

### Multiple sliders and performance

Slider `input` events fire rapidly during drag (every pixel).  We only
update local state on `input` — no simulation calls.  Only the "Reset
Layout" button click triggers `setForceConfig` + `resetLayout`.  So slider
dragging has zero performance impact on the simulation.

### Unapplied slider state

Sliders and simulation state can diverge: the slider panel shows new values
after a drag, but the simulation uses the last-applied config until "Reset
Layout" is clicked. To make this visible, the "↺ Reset" button text should
change to a highlighted style (e.g. bold + accent color) when
`currentForceConfig` differs from the displayed slider values.  Exact
styling is left to the implementer; the key property is a `data-dirty`
attribute or class toggle on the reset button.

---

## Default values rationale

| Parameter | Default | Reasoning |
|---|---|---|
| `generationPull` | 0.30 | Gentle nudge. High enough to create visible bands in typical 2–5 generation trees; low enough to let spouse pairs stay close horizontally and allow cross-generation anomalies (e.g. age-gap marriages) to sit between bands. |
| `spouseStrength` | 0.80 | Near-full stiffness. Couples should be visually cohesive. Less than 1.0 gives some give so a spouse pair isn't pulled apart by competing parent-child links. |
| `parentChildStrength` | 0.50 | Half stiffness. Weaker than spouse so that the generation field (not the spring) does most of the vertical positioning. Still strong enough to keep a child recognizably near its parents rather than flying off. |

These defaults were chosen so that the layout "just works" better out of the
box than the current uniform-force layout, while leaving room for users to
dial them up or down.
