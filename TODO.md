# Implementation Plan: Per-Generation Field Force

Source: `docs/research/per-generation-field-force.md`

## Plan review notes

- **`getSvgWidth()`:** The plan references `getSvgWidth()` in `createSimulationForces` (Step 2) but only defines `getSvgHeight()` (Step 3). Both need to be defined in Step 2 alongside the helper.
- All other aspects are well-specified and consistent with the codebase.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add ForceConfig type and defaults` | ForceConfig type | `crates/visualize/frontend/src/types.ts` | Unit |
| 2 | `refactor: lift force creation into helpers, split link forces, add gen-field` | Force simulation helpers | `crates/visualize/frontend/src/graph.ts` | Unit |
| 3 | `feat: add setForceConfig to GraphController` | Controller method | `crates/visualize/frontend/src/graph.ts` | Unit |
| 4 | `feat: build collapsible force-control panel` | Force panel UI | `crates/visualize/frontend/src/main.ts` | Unit |
| 5 | `feat: add force-panel CSS` | Force panel styles | `crates/visualize/frontend/styles/main.css` | — |
| 6 | `test: add force-config tests for graph and main` | Force-config tests | `crates/visualize/frontend/tests/graph.test.ts`, `crates/visualize/frontend/tests/main.test.ts` | Unit, Integration |
| 7 | `chore: add manual test sliders to test-harness.html` | Test harness sliders | `crates/visualize/frontend/test-harness.html` | — |

## Step details

### Step 1 — `ForceConfig` type and defaults

**File:** `crates/visualize/frontend/src/types.ts`

Add `ForceConfig` interface and `DEFAULT_FORCE_CONFIG` constant. Export both.

```ts
export interface ForceConfig {
  generationPull: number;
  spouseStrength: number;
  parentChildStrength: number;
}

export const DEFAULT_FORCE_CONFIG: ForceConfig = {
  generationPull: 0.30,
  spouseStrength: 0.80,
  parentChildStrength: 0.50,
};
```

**Tests:** Unit test verifying shape and value ranges of `DEFAULT_FORCE_CONFIG`.

---

### Step 2 — Lift force creation into helpers, split link forces, add gen-field

**File:** `crates/visualize/frontend/src/graph.ts`

Changes:

1. **Define helpers** (module-level, not inside `renderGraph`):
   - `computeGenerationSpacing(nodes, canvasHeight)` — pure function; exported for testing.
   - `getSvgHeight()` — reads `svg.node().getBoundingClientRect().height` (closure-bound).
   - `getSvgWidth()` — reads `svg.node().getBoundingClientRect().width` (closure-bound).
   - `applyForceConfig(simulation, config, genY)` — mutates forces on existing simulation.
   - `createSimulationForces(sim, config, genY)` — registers all four named forces.

2. **Add constants** (module-level):
   - `SPOUSE_BASE_DISTANCE = 40`
   - `PC_BASE_DISTANCE = 120`
   - `CHARGE_STRENGTH = -300` (unchanged)
   - `COLLIDE_RADIUS = 18` (unchanged)
   - `CENTER_STRENGTH = 0.05` (reduced from 0.3)

3. **Refactor `restartSimulation`** inside `renderGraph`:
   - Replace the inline force definition with a call to `createSimulationForces`.
   - Split the single `force('link', ...)` into `force('spouse-link', ...)` and `force('pc-link', ...)`.
   - Add `force('gen-field', d3.forceY<SimNode>().y(genY).strength(config.generationPull))`.
   - Pass `currentConfig` (new closure variable) to the helper.

4. **Store `currentConfig`** in the renderGraph closure, initialized to `DEFAULT_FORCE_CONFIG`.

5. **Fix `resize()`:** Preserve `CENTER_STRENGTH` when recreating the center force:

   ```ts
   simulation.force('center', d3.forceCenter(w / 2, h / 2).strength(CENTER_STRENGTH));
   ```

**Note:** `getSvgWidth()` and `getSvgHeight()` both read from the live SVG element, not the captured initial `width`/`height`, so they stay correct after resize.

**Tests:** Unit tests for `computeGenerationSpacing` (empty, single gen, two gen, height ≤ 0, NaN), `applyForceConfig` roundtrip, `restartSimulation` force creation shape.

---

### Step 3 — Add `setForceConfig` to `GraphController`

**File:** `crates/visualize/frontend/src/graph.ts`

1. Add `setForceConfig(config: ForceConfig): void` to the `GraphController` interface.
2. Implement in the controller object inside `renderGraph`:
   - Compute generation spacing from the active (filtered) node set.
   - Compute target Y function: `(d) => (d.generation - minGen) * spacing`.
   - Call `applyForceConfig(simulation, config, targetY)`.
   - Reheat: `simulation.alpha(0.3).restart()`.

**Tests:** Unit test for `setForceConfig` — verify it computes spacing from filtered node set.

---

### Step 4 — Build collapsible force-control panel

**Files:** `crates/visualize/frontend/src/main.ts`

1. Add `renderForcePanel(config, onChange): HTMLElement` function.
   - Collapsible panel (header + body, collapsed by default).
   - Three `<input type="range">` sliders (0–200 → 0.00–2.00).
   - Value display spans next to each slider.
   - "Restore defaults" button.
   - `onChange` fires on every `input` event.

2. Extend `renderToolbar` signature:

   ```ts
   export function renderToolbar(
     graphData: GraphData,
     controller: GraphController,
     forceConfig?: ForceConfig,
     onForceConfigChange?: (c: ForceConfig) => void,
   ): HTMLElement
   ```

   When `forceConfig` and `onForceConfigChange` are provided, append the force panel after the reset button.

3. Update reset button wiring:

   ```ts
   resetBtn.addEventListener('click', () => {
     controller.setForceConfig(currentForceConfig);
     controller.resetLayout();
   });
   ```

4. Update `renderGraphFromData` to create a `ForceConfig` state object and pass it through to `renderToolbar`.

**Tests:** Unit tests for `renderToolbar` — update mock to include `setForceConfig`, verify call order (`setForceConfig` before `resetLayout`). Integration tests for force panel DOM: slider input updates value span, restore defaults resets to `DEFAULT_FORCE_CONFIG`, panel collapsed by default, toggle expands/collapses.

---

### Step 5 — Add force-panel CSS

**File:** `crates/visualize/frontend/styles/main.css`

Add:

- `#force-panel` — `display: flex; flex-direction: column`
- `.force-header` — flex row, title + toggle, cursor pointer
- `.force-body` — `display: none` when collapsed, flex column when expanded
- `.force-slider` — flex row, label + slider + value
- Slider width ~120px, labels 11px font
- Restore button styled consistently with existing reset button

**Tests:** None (CSS-only, no JS logic).

---

### Step 6 — Add force-config tests

**File:** `crates/visualize/frontend/tests/graph.test.ts`

New `describe('force configuration')` block:

1. `DEFAULT_FORCE_CONFIG` shape and value range.
2. `computeGenerationSpacing` — empty, single gen, two gen, uniform gen, 5 gens, height ≤ 0, NaN.
3. `applyForceConfig` roundtrip — apply then read back force parameters.
4. `restartSimulation` force creation — all four named forces registered.
5. Filter + `setForceConfig` interaction — spacing computed from filtered set.

**File:** `crates/visualize/frontend/tests/main.test.ts`

1. Update mock to include `setForceConfig: vi.fn()`.
2. Verify call order: `setForceConfig` before `resetLayout`.
3. Slider `input` event updates value `<span>`.
4. "Restore defaults" resets sliders to `DEFAULT_FORCE_CONFIG`.
5. Panel collapsed by default.
6. Header click toggles collapsed/expanded.

---

### Step 7 — Add manual test sliders to test-harness.html

**File:** `crates/visualize/frontend/test-harness.html`

Add three hard-coded sliders: generation pull, spouse strength, parent-child strength. Wire to `controller.setForceConfig()` + `controller.resetLayout()`. Manual QA only — no automated test.
