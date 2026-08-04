# Bug: Statistics Panel — Blank Content and Legend Overlap

## Summary

The "File Statistics" panel in the visualizer has two distinct bugs:

1. **The panel is blank (white rectangle with only the "File Statistics" label)** — the
   `StatsPanel.render()` method is never called successfully because the module-level
   `statsPanel` variable is never assigned.

2. **The panel is partially occluded by the "Birth Year" legend** — both elements are
   positioned in the top-right corner with conflicting z-indices.

---

## Root Cause Analysis

### Bug 1: Module-level `statsPanel` never assigned

**File:** `crates/visualize/frontend/src/main.ts`

```typescript
// Line 25 — module-level variable, declared but never assigned:
let statsPanel!: StatsPanel;

// Line 47 — fetchAndRenderStats references the module-level variable:
async function fetchAndRenderStats(filePath: string): Promise<void> {
  const tauri = await import('@tauri-apps/api/core');
  try {
    const report: StatsReport = await tauri.invoke('get_stats', { path: filePath });
    statsPanel.render(report);   // ← TypeError: statsPanel is undefined
  } catch (err) {
    console.warn('Failed to load stats:', err);
    statsPanel.renderError('...'); // ← Also crashes
  }
}

// Line 404–410 — main() creates a LOCAL variable that shadows the module-level one:
async function main(): Promise<void> {
  // ...
  const statsPanel = new StatsPanel();  // ← LOCAL, not the module-level var
  const statsPanelEl = statsPanel.create();
  if (appEl) {
    appEl.appendChild(statsPanelEl);
  }
  // ...
}
```

**What happens at runtime:**

1. The `StatsPanel` DOM element is created and appended to `#app` — the panel appears
   in the DOM with the header "File Statistics" but an empty body.
2. When a file is loaded, `renderGraphFromData` calls `fetchAndRenderStats(filePath)`.
3. `fetchAndRenderStats` invokes `statsPanel.render(report)` — but `statsPanel` is
   `undefined` at the module level. A `TypeError` is thrown.
4. The `catch` block tries `statsPanel.renderError(...)` — also on `undefined` —
   throwing a second error, which becomes an unhandled promise rejection.
5. The panel body remains empty (blank white).

**Fix:**

In `main()`, assign to the module-level variable instead of declaring a local:

```typescript
// Change:
const statsPanel = new StatsPanel();
// To:
statsPanel = new StatsPanel();
```

---

### Bug 2: Legend overlaps the statistics panel

**File:** `crates/visualize/frontend/styles/main.css`

Both elements are absolutely positioned in the top-right corner of `#app`:

```css
/* Legend — sits at right:20px, top:20px, z-index 500 */
#legend {
  position: absolute;
  top: 20px;
  right: 20px;
  z-index: 500;
  /* ... */
}

/* Stats panel — a 280px-wide right sidebar at right:10px, z-index 400 */
#stats-panel {
  position: absolute;
  top: 10px;
  right: 10px;
  width: 280px;
  height: calc(100% - 20px);
  z-index: 400;
  /* ... */
}
```

**What happens at runtime:**

- The stats panel occupies pixels `right:10px` to `right:290px` (10 + 280).
- The legend is anchored at `right:20px` — 10 pixels **inside** the stats panel's area.
- The legend has `z-index: 500` vs. the stats panel's `z-index: 400`, so the legend
  paints **on top of** the stats panel header, partially occluding it.

**Fix options:**

1. **Move the legend left of the stats panel** — change `#legend` `right` from `20px`
   to `300px` so it sits to the left of the 280px-wide panel (10 + 280 + 10 margin).

2. **Dynamically adjust** — in JavaScript, set `legendEl.style.right = '300px'`
   (or reset to `20px`) when the stats panel is toggled. This handles the collapsed
   state gracefully but adds JS complexity.

3. **Swap layout sides** — move the legend to the left side (`left: 20px`), but this
   would crowd the toolbar/filter/selection controls already anchored top-left.

**Recommendation:** Option 1 (CSS only, simple).

**Known limitation:** When the stats panel is collapsed via the × button, the
legend sits at `right: 300px` — 272px left of the `.stats-tab` tab (at `right: 0`,
28px wide). This leaves a noticeable gap on the right side while the panel is
hidden. This is an acceptable trade-off for v1: the legend can be repositioned
dynamically in JavaScript in a follow-up, or the user can simply re-expand the
panel. The legend does not overlap anything in either state.

---

## Implementation Plan

### Step 1: Fix the module-level `statsPanel` assignment

**File:** `crates/visualize/frontend/src/main.ts`, lines 404–410

Change the local `const statsPanel` declaration to an assignment to the module-level
variable:

```diff
-  const statsPanel = new StatsPanel();
+  statsPanel = new StatsPanel();
```

### Step 2: Fix legend/stats-panel overlap

**File:** `crates/visualize/frontend/styles/main.css`, `#legend` rule

Move the legend to `right: 300px` so it sits to the left of the 280px-wide stats panel
(10 + 280 + 10 margin).

```diff
 #legend {
   position: absolute;
   top: 20px;
-  right: 20px;
+  right: 300px;
   /* ... */
 }
```

**Collapsed-state behavior:** When the stats panel is toggled off, the legend
remains at `right: 300px` — the gap on the right is larger than ideal but the
legend does not overlap anything. Dynamically adjusting the legend position on
toggle is deferred to a follow-up.

### Step 3: Verify

**Automated tests (run first):**

```bash
# Frontend unit tests (StatsPanel, toolbar, force panel, graph query, etc.)
cd crates/visualize/frontend
npx vitest run
```

These tests cover the `StatsPanel` class in isolation (create, render, toggle,
renderError, destroy, edge cases) and the toolbar/force-panel rendering from
`main.ts`. They should continue to pass after both fixes since:

- The `StatsPanel` class itself is unchanged — only the wiring in `main()` is fixed
- The CSS change does not affect the DOM structure tested by vitest

**Manual verification:**

1. Build with `cargo build -p visualize` (or `cargo tauri dev`).
2. Open a `.gramps` file — confirm the stats panel populates with data.
3. Confirm the legend and stats panel no longer overlap.
4. Click the × button to collapse the stats panel — confirm the legend position
   is on the right side (with a gap, per the known limitation).
5. Re-expand — confirm no overlap.

### Step 4: Commit

Follow [conventional-commit](../../../.pi/agent/skills/conventional-commit/SKILL.md)
conventions:

```
fix(visualize): populate stats panel and fix legend overlap

The stats panel was blank because the module-level `statsPanel` variable
was never assigned — a local variable shadowed it in `main()`.  The panel
also overlapped with the "Birth Year" legend because both were positioned
in the top-right corner with the legend at a higher z-index.

- Assign `statsPanel = new StatsPanel()` instead of `const statsPanel = ...`
  so `fetchAndRenderStats` can call `.render()` on a valid instance.

- Move `#legend` to `right: 300px` so it sits to the left of the 280px-wide
  stats panel.
```
