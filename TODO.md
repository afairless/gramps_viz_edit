# Implementation Plan: Selection Cluster Forces

Source: `docs/research/selection-cluster-force.md`

## Overview

Add two custom D3 forces — `selected-attract` and `unselected-attract` — that
pull each subset of nodes (selected vs. unselected) toward its own centroid.
Together with the existing `selection-repel` force, this creates two visually
distinct clusters. All work is in `crates/visualize/frontend/`; no Rust-side
changes are needed.

**Pre-work verification** (done): `npm test` passes (197 tests, 8 files).

**Per-step verify commands** (run from `crates/visualize/frontend/`):

```bash
npm test                  # vitest run — all tests must pass
npx tsc --noEmit          # strict type-check of src/ (tsconfig excludes tests/)
```

Each step is implemented, tested, verified, and committed before the next
begins (incremental-development workflow). No step may include code belonging
to a later step.

---

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: extend ForceConfig with attract strength fields` | ForceConfig extension | `src/types.ts` — add `selectedAttractStrength`, `unselectedAttractStrength` to `ForceConfig` interface and `DEFAULT_FORCE_CONFIG` (both `0.00`); update the 5 literal `ForceConfig` objects in `tests/graph.test.ts` (`config1`, `config2`, two `config` literals, `testConfig`) so the build stays green | unit |
| 2 | `feat: add selected-attract centroid force` | Selected-attract force | `src/graph.ts` — export `AttractForce` interface (extends `d3.Force<SimNode, undefined>` with `strength()` getter/setter + `initialize()`); export `createSelectedAttractForce(getSelected)` implementing the centroid-spring contract (no-op if strength 0, selected < 2, or no unselected complement) | unit |
| 3 | `feat: add unselected-attract centroid force` | Unselected-attract force | `src/graph.ts` — export `createUnselectedAttractForce(getSelected)`, mirror of selected-attract operating on the unselected complement (no-op if no selected nodes exist) | unit |
| 4 | `feat: register attract forces in simulation config` | Force registration + runtime mutation | `src/graph.ts` — register `'selected-attract'` and `'unselected-attract'` in `createSimulationForces()` with strengths from config; mutate both via `AttractForce` casts in `applyForceConfig()` (guarded by `if (force)`) | unit |
| 5 | `feat: add attract sliders to force panel` | Force panel sliders | `src/main.ts` — append `{ key: 'selectedAttractStrength', label: 'Selected attract' }` and `{ key: 'unselectedAttractStrength', label: 'Unselected attract' }` to the `sliders` array in `renderForcePanel()`; update `tests/main.test.ts` slider-count assertions (4 → 6: `sliders.length`, `sliderRows.length`, `values.length`) and the test name `'has four sliders…'` → `'has six sliders…'` (literal configs there spread `DEFAULT_FORCE_CONFIG`, so no key additions needed) | unit |
| 6 | `test: cover attract forces with unit tests` | Attract-force test suite | `tests/graph.test.ts` — follow the `createSelectionRepelForce` pattern: strength getter/setter + initialize + is-a-`d3.Force`; centroid direction tests (3 selected at (0,0),(100,0),(0,100), tick with strength 1, alpha 1); edge cases (empty set, single selected, all selected, no unselected, strength 0 → no velocity change); unselected-attract mirror; registration in `createSimulationForces` (default strength 0); `applyForceConfig` mutation without restart | unit |

---

## Step detail

### Step 1 — `ForceConfig` extension (types.ts)

- Add to `ForceConfig` interface:
  - `selectedAttractStrength: number;` (doc: multiplier for selected-attract centroid pull)
  - `unselectedAttractStrength: number;` (doc: multiplier for unselected-attract centroid pull)
- Add both to `DEFAULT_FORCE_CONFIG` with value `0.00` (off by default).
- **Same commit:** update the 5 literal `ForceConfig` objects in
  `tests/graph.test.ts` (TypeScript errors otherwise — new keys are required):
  - `applyForceConfig roundtrip`: `config1`, `config2`, and the mutation-test `config`
  - `createSimulationForces` "uses the provided config values": `config`
  - `restartSimulation`: `testConfig`
- Optionally extend the `DEFAULT_FORCE_CONFIG` describe block in `graph.test.ts`
  to assert the two new keys exist (also done in Step 6's key-property test).

### Step 2 — `createSelectedAttractForce` (graph.ts)

- Export `AttractForce` interface (needed by Step 4):
  `extends d3.Force<SimNode, undefined>` with `strength(s: number): this`,
  `strength(): number`, `initialize(nodes: SimNode[]): void`.
- Export `createSelectedAttractForce(getSelected: () => Set<string>): AttractForce`:
  - Closure state: `nodes: SimNode[]`, `strengthValue = 0`.
  - Tick: early-return if `strengthValue === 0`, `selected.size < 2`, or no
    unselected complement. Partition nodes; compute centroid of selected;
    apply `n.vx += (cx - n.x) * tickAlpha * strengthValue` per selected node.
  - `force.initialize` stores node list; `force.strength` getter/setter with
    chaining; return `force as unknown as AttractForce`.
- Tests for this step's behavior live in Step 6 (keep the per-step loop green
  by verifying with existing tests + `tsc`; the full unit suite is step 6).

### Step 3 — `createUnselectedAttractForce` (graph.ts)

- Mirror of Step 2, operating on the unselected complement: partition into
  unselected/selected, early-return if `selected.size === 0` (no "other
  cluster"), compute the **unselected** centroid, apply impulse to unselected
  nodes only.

### Step 4 — Registration + `applyForceConfig` (graph.ts)

- In `createSimulationForces()`, after `'selection-repel'`:

  ```typescript
  .force('selected-attract', createSelectedAttractForce(getSelected).strength(config.selectedAttractStrength))
  .force('unselected-attract', createUnselectedAttractForce(getSelected).strength(config.unselectedAttractStrength))
  ```

- In `applyForceConfig()`:

  ```typescript
  const selAttract = simulation.force('selected-attract') as AttractForce | undefined;
  if (selAttract) selAttract.strength(config.selectedAttractStrength);
  const unselAttract = simulation.force('unselected-attract') as AttractForce | undefined;
  if (unselAttract) unselAttract.strength(config.unselectedAttractStrength);
  ```

- Note: D3 v7 `simulation.force()` returns the base `Force` type; the cast is
  safe because `createSimulationForces` created these forces.

### Step 5 — Force panel sliders (main.ts) + main.test.ts assertions

- Append the two entries to the `sliders` array in `renderForcePanel()` (the
  existing slider loop and Restore-defaults button handle them automatically;
  both default to 0.00).
- **Same commit:** update `tests/main.test.ts`:
  - `expect(sliders.length).toBe(4)` → `.toBe(6)` (restore-defaults test)
  - `expect(sliderRows.length).toBe(4)` → `.toBe(6)` (six-sliders test)
  - `expect(values.length).toBe(4)` → `.toBe(6)` (six-sliders test)
  - Test name `'has four sliders with labels and value spans'` →
    `'has six sliders with labels and value spans'`; extend label assertions
    to cover `Selected attract` / `Unselected attract`.

### Step 6 — Attract-force unit tests (graph.test.ts)

Follow the `createSelectionRepelForce` suite:

1. **Properties:** strength defaults to 0; setter chains and getter returns
   the set value; `initialize` and tick are callable (`is a d3.Force`).
2. **Centroid direction:** selected nodes at (0,0), (100,0), (0,100); tick
   with strength 1, alpha 1; centroid ≈ (33.3, 33.3):
   - (100,0): `vx < 0`, `vy > 0`
   - (0,100): `vx > 0`, `vy < 0`
   - (0,0): `vx > 0`, `vy > 0`
3. **Edge cases:** empty selected set; single selected; all nodes selected;
   no unselected complement; strength 0 — all no-ops (velocities unchanged).
4. **Unselected mirror:** selected nodes stay stationary; unselected cluster
   toward their centroid.
5. **Registration:** both forces present after `createSimulationForces` with
   default strength 0.
6. **`applyForceConfig` mutation:** config change updates both forces' strength
   without a simulation restart.
7. Extend the `DEFAULT_FORCE_CONFIG` describe block: `'has all four keys'` →
   `'has all six keys'` (this test lives in **graph.test.ts**, not main.test.ts),
   asserting `selectedAttractStrength` / `unselectedAttractStrength` exist;
   both default to `0.00` (mirroring the `repelStrength` default test).

---

## Notes / plan corrections

- The research plan attributes the `'has all four keys'` test to
  `tests/main.test.ts`; it actually lives in `tests/graph.test.ts`. Corrected above.
- `tests/main.test.ts` contains no literal `ForceConfig` objects (all use
  `{ ...DEFAULT_FORCE_CONFIG }` spread), so it needs **no** key additions in
  Step 1 — only the slider-count changes in Step 5.
- No Rust-side changes; no schema changes; `docs/ARCHITECTURE.md` unaffected
  (frontend-only feature — no update required per project doc-sync rule since
  no crate/module/CLI surface changes).
