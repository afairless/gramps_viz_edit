# Force-Freeze Mode for D3 Force-Directed Graph

## Problem

The D3 force simulation continuously repositions nodes in response to any perturbation:

- **Dragging** a node causes all other nodes to adjust to the dragged node's new position via link springs, charge repulsion, etc.
- **Selecting** nodes activates selection forces (repel, selected-attract, unselected-attract) that induce movement in all nodes.

Users need a "force freeze" toggle that suspends all force-based movement: dragged nodes move in isolation, selection changes have no visual effect, and unfreezing resumes normal force layout.

## Proposed Solution

### Core mechanism: stop the simulation

When freeze is enabled, call `simulation.stop()`. No ticks fire, no forces are computed, no nodes move.

When freeze is disabled, call `simulation.alpha(1).restart()` and the simulation resumes from the current node positions (with previously-pinned nodes remaining pinned). We use `alpha(1)` (rather than the `0.3` used for live reconfigurations) because the simulation was completely stopped — this matches `resetLayout`'s behavior and provides enough energy to restart the layout from a cold state.

### Drag during freeze

D3's drag behavior currently calls `simulation.alphaTarget(0.3).restart()` on drag-start, which would defeat freezing. The plan is to make the drag handlers freeze-aware via a `getFrozen()` callback:

| Event | Normal (unfrozen) | Frozen |
|---|---|---|
| `onDragStart` | `simulation.alphaTarget(0.3).restart()`, set `fx/fy`, set cursor to `grabbing` | Set `fx/fy`, set cursor to `grabbing`; no simulation restart |
| `onDrag` | Set `fx/fy` (tick handler updates SVG) | Set `fx/fy` + `x/y`; manually update the node's SVG `transform` via `d3.select(this).attr('transform', 'translate(${d.x ?? 0},${d.y ?? 0})')` |
| `onDragEnd` | Set cursor to `grab`; `simulation.alphaTarget(0)` | Set cursor to `grab`; do NOT call `simulation.alphaTarget(0)`; keep `fx/fy` pinned |

The manual SVG update in `onDrag` (frozen path) replicates exactly the tick handler output:

```typescript
d3.select(this).attr('transform', `translate(${d.x ?? 0},${d.y ?? 0})`);
```

#### Behavior of pinned nodes after unfreeze

When a node is dragged in frozen mode, `onDragEnd` keeps `fx/fy` pinned. After unfreeze, those pinned nodes remain locked — the simulation cannot move them because D3 respects `fx/fy` as explicit constraints. Only a **reset** (which clears all `fx/fy`) releases them. This is the conservative default: nodes dragged to specific positions in frozen mode stay there. A reset is the explicit user action to reflow everything.

### Selection during freeze

Selection-repel, selected-attract, and unselected-attract are simulation forces. When the simulation is stopped, they have no effect — no movement occurs. This is exactly the desired behavior.

When unfrozen, the simulation restarts with the current selection set, and the selection forces resume normally.

### Files to change

| File | Changes |
|---|---|
| `crates/visualize/frontend/src/graph.ts` | Add `frozen` state + `getFrozen` callback; modify `createDragBehavior`, `onDragStart`, `onDrag`, `onDragEnd`; add `setFrozen()` and `isFrozen()` to `GraphController` |
| `crates/visualize/frontend/src/types.ts` | No new types needed — `frozen` is internal state |
| `crates/visualize/frontend/src/main.ts` | Add freeze-toggle button to `renderToolbar()`; add `syncFreezeUI()` helper; wire unfreeze calls before reset/filter actions; add/remove `.force-frozen` class on `#graph-container` |
| `crates/visualize/frontend/styles/main.css` | Add `.force-frozen` rule with inset box-shadow; style freeze button |
| `crates/visualize/frontend/tests/graph.test.ts` | Add tests for freeze-aware drag handlers and `setFrozen`/`isFrozen` |
| `crates/visualize/frontend/tests/main.test.ts` | Add tests for freeze button rendering and unfreeze-on-action behavior |

### Detailed implementation

#### 1. `graph.ts` — add frozen state and modify drag

```typescript
// New internal state in renderGraph()
let frozen = false;

// Modified createDragBehavior — second parameter is NEW.
// This is a BREAKING SIGNATURE CHANGE. All call sites must be updated:
//   - restartSimulation() in graph.ts
//   - setFrozen() in graph.ts
//   - tests/graph.test.ts (createDragBehavior test)
export function createDragBehavior(
  simulation: d3.Simulation<SimNode, undefined>,
  getFrozen: () => boolean,
): d3.DragBehavior<SVGGElement, SimNode, SimNode | d3.SubjectPosition> {
  return d3
    .drag<SVGGElement, SimNode>()
    .on('start', function(event, d) { onDragStart(d, event, simulation, getFrozen); })
    .on('drag',  function(event, d) { onDrag(d, event, simulation, getFrozen); })
    .on('end',   function(event, d) { onDragEnd(d, event, simulation, getFrozen); });
}
```

Each handler's frozen path:

```typescript
export function onDragStart(
  d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  simulation: d3.Simulation<SimNode, undefined>,
  getFrozen: () => boolean,
): void {
  d.fx = d.x ?? null;
  d.fy = d.y ?? null;
  d3.select(event.sourceEvent.currentTarget as SVGGElement).style('cursor', 'grabbing');
  if (getFrozen()) return;  // frozen: no simulation restart
  if (!event.active) simulation.alphaTarget(0.3).restart();
}

export function onDrag(
  d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  _simulation: d3.Simulation<SimNode, undefined>,
  getFrozen: () => boolean,
): void {
  d.fx = event.x;
  d.fy = event.y;
  if (getFrozen()) {
    // Manually update SVG transform: the tick handler is not firing.
    d.x = event.x;
    d.y = event.y;
    d3.select(event.sourceEvent.currentTarget as SVGGElement)
      .attr('transform', `translate(${d.x ?? 0},${d.y ?? 0})`);
  }
}

export function onDragEnd(
  _d: SimNode,
  event: d3.D3DragEvent<SVGGElement, SimNode, SimNode>,
  simulation: d3.Simulation<SimNode, undefined>,
  getFrozen: () => boolean,
): void {
  d3.select(event.sourceEvent.currentTarget as SVGGElement).style('cursor', 'grab');
  if (getFrozen()) return;  // frozen: keep fx/fy pinned, no alphaTarget reset
  if (!event.active) simulation.alphaTarget(0);
}
```

#### 2. `GraphController` — expose `setFrozen` and `isFrozen`

```typescript
export interface GraphController {
  // … existing methods …
  /** Enable or disable force freeze. When frozen, only explicitly dragged
   *  nodes move; all other forces are suspended. */
  setFrozen(frozen: boolean): void;
  /** Query whether freeze is currently active. */
  isFrozen(): boolean;
}
```

Implementation inside `renderGraph`:

```typescript
setFrozen(f: boolean) {
  frozen = f;
  if (frozen) {
    simulation.stop();
  } else {
    // alpha(1) because the simulation was completely stopped — needs a
    // strong kick, matching resetLayout's behavior (vs. alpha(0.3) for
    // live reconfigurations like force-config changes).
    simulation.alpha(1).restart();
  }
  // Rebind drag so getFrozen closure sees updated value.
  nodeGroup.call(createDragBehavior(simulation, () => frozen));
},
isFrozen() { return frozen; },
```

#### 3. `main.ts` — freeze toggle button in toolbar

`renderToolbar` owns the freeze button and all handlers that may need to unfreeze.
It exposes a `syncFreezeUI(frozen: boolean)` helper used by every code path that
calls `controller.setFrozen(…)`. This prevents the button appearance from drifting
out of sync when unfreeze is triggered implicitly (reset, filter, slider).

```typescript
export function renderToolbar(
  graphData: GraphData,
  controller: GraphController,
  forceConfig?: ForceConfig,
  onForceConfigChange?: (c: ForceConfig) => void,
  // … existing optional params …
): HTMLElement {
  // … existing toolbar setup …

  // ---- freeze button ----
  const freezeBtn = document.createElement('button');
  freezeBtn.textContent = '❄ Freeze';
  freezeBtn.title = 'Freeze all forces (only dragged nodes move)';
  freezeBtn.style.padding = '4px 10px';
  freezeBtn.style.fontSize = '12px';
  freezeBtn.style.borderRadius = '4px';
  freezeBtn.style.border = '1px solid #ccc';
  freezeBtn.style.background = '#fff';
  freezeBtn.style.cursor = 'pointer';
  freezeBtn.style.color = '#333';

  /** Update freeze button AND #graph-container CSS class in one place. */
  function syncFreezeUI(frozen: boolean): void {
    freezeBtn.textContent = frozen ? '❄ Unfreeze' : '❄ Freeze';
    freezeBtn.style.background = frozen ? '#e8f0fe' : '#fff';
    freezeBtn.style.borderColor = frozen ? '#2266aa' : '#ccc';
    const gc = document.getElementById('graph-container');
    if (gc) gc.classList.toggle('force-frozen', frozen);
    // Update hover style so it differs when highlighted
    if (frozen) {
      freezeBtn.addEventListener('mouseenter', highlightFrozen);
      freezeBtn.addEventListener('mouseleave', unhighlightFrozen);
    } else {
      freezeBtn.removeEventListener('mouseenter', highlightFrozen);
      freezeBtn.removeEventListener('mouseleave', unhighlightFrozen);
    }
  }

  function highlightFrozen() { freezeBtn.style.background = '#d0dff5'; }
  function unhighlightFrozen() { freezeBtn.style.background = '#e8f0fe'; }

  freezeBtn.addEventListener('click', () => {
    const nowFrozen = !controller.isFrozen();
    controller.setFrozen(nowFrozen);
    syncFreezeUI(nowFrozen);
  });

  toolbar.appendChild(freezeBtn);

  // ---- reset button (modified: unfreeze before reset) ----
  const resetBtn = document.createElement('button');
  resetBtn.textContent = '↺ Reset';
  resetBtn.title = 'Reset node positions to force-directed layout';
  // … existing styling …
  resetBtn.addEventListener('click', () => {
    // Unfreeze implicitly so the reset takes visible effect
    if (controller.isFrozen()) {
      controller.setFrozen(false);
      syncFreezeUI(false);
    }
    if (forceConfig) {
      controller.setForceConfig(forceConfig);
    }
    controller.resetLayout();
  });
  toolbar.appendChild(resetBtn);

  // ---- family-group filter: unfreeze on change ----
  // (inside renderFilterDropdown's change handler):
  //   select.addEventListener('change', () => {
  //     if (controller.isFrozen()) {
  //       controller.setFrozen(false);
  //       syncFreezeUI(false);
  //     }
  //     controller.setFamilyGroupFilter(val === '' ? null : Number(val));
  //   });

  // ---- force panel: sliders update config only; unfreeze not needed ----
  // Slider input events only mutate the local forceConfig object.
  // The actual force application happens only on reset (above).
  // No unfreeze is needed for slider changes themselves.

  return toolbar;
}
```

#### 4. Interaction with existing controls

**Decision: implicitly unfreeze on state-altering control actions.** Reset and
family-group filter changes both unfreeze automatically and take effect immediately.
Sliders update config in-memory only — the actual force application happens on reset,
so no unfreeze is needed on slider input.

| Control | Behavior when frozen |
|---|---|
| **Reset button** | Calls `controller.setFrozen(false)`, syncs UI, then resets layout as normal. |
| **Force config sliders** | No immediate unfreeze — sliders only update the local config object; reset applies it. |
| **Family-group filter** | Calls `controller.setFrozen(false)`, syncs UI, then rebuilds simulation. |
| **Freeze button** | Toggles freeze state; syncs its own appearance via `syncFreezeUI`. |

All handlers are inside `renderToolbar`, which owns both the freeze button and
the `syncFreezeUI` helper. No handler needs to reach into another module to
update the button — the toolbar coordinates its own UI.

#### 5. Visual freeze indicator

**Decision: blue-tinted border around the graph canvas.** When frozen, `#graph-container` gets a CSS class `.force-frozen` that adds a 3px `#2266aa` inset `box-shadow`. This provides an unmistakable ambient cue without cluttering the graph area.

The CSS class is toggled by `syncFreezeUI()` (see §3), keeping the canvas border and button text in lockstep.

#### 6. Keyboard shortcut

**Decision: button only.** No keyboard shortcut. The toolbar button is sufficient and avoids accidental key presses.

### Implementation steps

**Step 1 — `graph.ts` core changes + tests**

- Add `let frozen = false` state inside `renderGraph()`.
- Add `getFrozen: () => boolean` parameter to `createDragBehavior`, `onDragStart`, `onDrag`, `onDragEnd`.
- Implement frozen paths in each handler (see §1).
- Add `setFrozen()` / `isFrozen()` to the controller object (see §2).
- Update both call sites of `createDragBehavior`: `restartSimulation()` and `setFrozen()`.
- In `tests/graph.test.ts`: add tests covering all freeze-aware paths:
  - `onDragStart` with `getFrozen() === true`: assert `fx/fy` set, cursor set to grabbing, simulation NOT restarted
  - `onDragStart` with `getFrozen() === false`: assert existing behavior preserved (no regression)
  - `onDrag` with `getFrozen() === true`: assert `fx/fy`/`x`/`y` set, SVG transform attribute updated on the dragged `<g>` element
  - `onDrag` with `getFrozen() === false`: assert existing behavior preserved
  - `onDragEnd` with `getFrozen() === true`: assert cursor set to grab, `alphaTarget` NOT called, fx/fy preserved
  - `onDragEnd` with `getFrozen() === false`: assert existing behavior preserved
  - `createDragBehavior` with `getFrozen` callback: assert callback is wired; assert second-parameter requirement
  - `setFrozen(true)` calls `simulation.stop()`
  - `setFrozen(false)` calls `simulation.alpha(1).restart()`
  - `isFrozen()` returns current state
- Run `cargo test -p visualize` (frontend tests via Vitest), verify all pass.
- Commit with a conventional message.

**Step 2 — `main.ts` freeze button + toolbar wiring + tests**

- Add freeze button to `renderToolbar()` (between reset and force panel).
- Add `syncFreezeUI()` helper to keep button and `.force-frozen` class in sync.
- Modify reset button handler to unfreeze before reset.
- Modify family-group filter handler to unfreeze before rebuilding.
- In `tests/main.test.ts`: add tests:
  - freeze button renders with text "❄ Freeze"
  - clicking freeze button toggles text to "❄ Unfreeze" and back
  - clicking freeze button calls `controller.setFrozen()`
  - reset button calls `controller.setFrozen(false)` when frozen
  - `syncFreezeUI` adds/removes `.force-frozen` on `#graph-container`
- Run `cargo test -p visualize`, verify all pass.
- Commit with a conventional message.

**Step 3 — `main.css` styling**

- Add `.force-frozen` rule: `box-shadow: inset 0 0 0 3px #2266aa`.
- Add freeze button base + active state styles.
- Run `cargo test -p visualize`, verify all pass.
- Commit with a conventional message.

**Step 4 — Manual smoke test**

- Open a `.gramps` file in the visualization app.
- Toggle freeze: verify button text/color changes and blue border appears.
- Drag a node while frozen: verify only the dragged node moves; others stay still.
- Select nodes while frozen: verify no visual movement (selection forces suspended).
- Unfreeze: verify simulation resumes and nodes settle normally.
- Freeze, then click Reset: verify layout resets and freeze is released.
- Freeze, then change family-group filter: verify filter applies and freeze is released.
- Drag a node while frozen, unfreeze: verify dragged node stays at its dragged position (pinned) until Reset is clicked.
