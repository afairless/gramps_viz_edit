# Move Stats Panel to Left Side

## Summary

Move the statistics panel from the right-hand side of the app to the
left-hand side, positioned below the top toolbar (the row of controls
starting with "Mode:"). The legend is relocated to the top-right corner,
and the collapsed "Stats" tab moves to the left edge.

> **Note:** All frontend file paths in this plan are relative to
> `crates/visualize/frontend/` (e.g., `index.html` refers to
> `crates/visualize/frontend/index.html`).

## Current Layout

```
┌─────────────────────────────────────────────────────┐
│  #app                                                │
│  (position: relative; 100% × 100%)                   │
│                                                      │
│  ┌─ Toolbar (abs, top:20, left:20, z:500) ────────┐ │
│  │ Mode: [select] | ☐☐☐ | Group: [▾] [Sel] [Dsl] │ │
│  │ [↺ Reset] [▶ Force Controls ...]               │ │
│  └─────────────────────────────────────────────────┘ │
│                                          ┌─ Legend ─┐│
│                                          │ top:20   ││
│                                          │ right:300││
│                                          └──────────┘│
│                                    ┌─ Stats Panel ──┐│
│                                    │ right:10        ││
│                                    │ top:10          ││
│                                    │ h: calc(100%-20)││
│                                    │ w: 280px        ││
│                                    │ z-index: 400    ││
│                                    └─────────────────┘│
│                                                      │
│               ┌─ Selection Panel ──────────────┐     │
│               │    bottom center                │     │
│               └─────────────────────────────────┘     │
│                                                      │
│  #graph-container fills app (100% × 100%)             │
│  #tooltip (abs, hidden, z:1000)                      │
└─────────────────────────────────────────────────────┘
```

## Target Layout

```
┌─ #app (flex column) ─────────────────────────────────┐
│  ┌─ #toolbar (static top bar, flex-wrap) ──────────┐ │
│  │  Mode: [select] | ☐☐☐ | Group: [▾] [Sel] [Dsl] │ │
│  │  [↺ Reset] [▶ Force Controls ...]               │ │
│  └──────────────────────────────────────────────────┘ │
│  ┌─ #main-row (flex row, flex:1) ───────────────────┐ │
│  │  ┌─ Stats Panel ──┐ ┌─ Graph Area ──────────────┐│ │
│  │  │ w: 280px        │ │  (flex:1)                 ││ │
│  │  │ flex column     │ │                   ┌─ Legend┐││
│  │  │ collapsible     │ │                   │ top:10 │││
│  │  └─────────────────┘ │                   │right:10│││
│  │                      │                   └────────┘││
│  │                      │    ┌─ Selection Panel ────┐ ││
│  │                      │    │   bottom center       │ ││
│  │                      │    └───────────────────────┘ ││
│  │                      │                             ││
│  │                      │  #graph-container fills      ││
│  │                      │  #tooltip (abs, z:1000)      ││
│  └──────────────────────┴─────────────────────────────┘│
└────────────────────────────────────────────────────────┘

  Collapsed state:
┌─ #app ─────────────────────────────────────────────────┐
│  ┌─ #toolbar ─────────────────────────────────────────┐│
│  │  Mode: ...                                          ││
│  └─────────────────────────────────────────────────────┘│
│  ┌─ #main-row ─────────────────────────────────────────┐│
│  │  Stats ─┐  ┌─ Graph Area ──────────────────────────┐││
│  │  (tab)   │  │  (full width)                         │││
│  │  left:0  │  │                                       │││
│  │  z:400   │  │                                       │││
│  └──────────┴──┴───────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
```

## Files to Modify

| # | File | Change |
|---|------|--------|
| 1 | `crates/visualize/frontend/index.html` | Add `#main-row` wrapper around `#graph-container` and overlays |
| 2 | `crates/visualize/frontend/styles/main.css` | Update `#app`, `#toolbar`, `#stats-panel`, `#legend`, `#stats-tab` CSS |
| 3 | `crates/visualize/frontend/src/main.ts` | Update toolbar insertion and stats panel insertion logic |
| 4 | `crates/visualize/frontend/src/stats-panel.ts` | Append tab to `#main-row` instead of `document.body` |
| 5 | `crates/visualize/frontend/tests/main.test.ts` | Update toolbar positioning test |
| 6 | `crates/visualize/frontend/tests/stats-panel.test.ts` | Minor update if tab parent changes |

## Step-by-step Plan

### Step 1 — index.html: Add #main-row wrapper

**What:** Wrap the graph-container and all absolute overlays (tooltip,
selection-panel, legend) in a new `<div id="main-row">`.

**Why:** The flex layout needs a row container to hold the stats panel
(left sidebar) beside the graph area (fills the rest). The overlays
will be positioned relative to `#main-row`, so they sit below the
toolbar naturally.

**Before:**

```html
<div id="app">
  <div id="graph-container"></div>
  <div id="tooltip" class="hidden"></div>
  <div id="selection-panel"></div>
  <div id="legend"></div>
</div>
```

**After:**

```html
<div id="app">
  <div id="main-row">
    <div id="graph-container"></div>
    <div id="tooltip" class="hidden"></div>
    <div id="selection-panel"></div>
    <div id="legend"></div>
  </div>
</div>
```

### Step 2 — CSS: Restructure layout styles

**`#app`:**

- Add `display: flex; flex-direction: column;` (turns into flex column)
- Keep `width: 100%; height: 100%;` (the flex column fills the viewport).
- `position: relative` is no longer needed for the overlay positioning
  context — `#main-row` now serves that role. However, keeping it is
  harmless. Remove it to avoid confusion, or keep it as a safety net.

**`#main-row` (new rule):**

```css
#main-row {
  flex: 1;
  display: flex;
  flex-direction: row;
  position: relative;   /* positioning context for overlays */
  min-height: 0;        /* allow shrinking below content height */
  overflow: hidden;
}
```

**`#graph-container`:**

- Change from `width: 100%; height: 100%` to `flex: 1; position: relative`.
  This removes the explicit `width: 100%; height: 100%` — `flex: 1` makes
  it fill the remaining space in the `#main-row` flex row.
- Keep `position: relative` if the SVG inside uses it for coordinate
  transforms.

**`#toolbar`:**

- Remove `position: absolute; top: 20px; left: 20px; z-index: 500;`.
- Keep `display: flex; align-items: center; gap: 8px; flex-wrap: wrap;`.
- Add `width: 100%;` so it spans the full width as a top bar.
- Add `padding: 10px 12px;` for visual breathing room (optional —
  could use the existing gap).
- Add `background: #fafafa; border-bottom: 1px solid #ddd;` so it
  reads visually as a distinct top bar (optional, but improves the
  "controls row along the top" appearance).

**`#stats-panel`:**

- Remove `position: absolute; top: 10px; right: 10px; height: calc(100% - 20px);`.
- Keep `width: 280px; display: flex; flex-direction: column;`.
- Add `flex-shrink: 0;` so it doesn't shrink when the window is narrow.
- Keep `border`, `border-radius`, `box-shadow`, `z-index`, `overflow`.
- When collapsed via `display: none`, flex will remove it from layout
  and the graph area fills the full width — no JS height measuring
  needed.

**`#legend`:**

- Change `right: 300px` → `right: 10px`.
- Keep `top: 20px` (or `top: 10px` for tighter spacing).
- This places it in the top-right corner of the graph area (no longer
  offset for the old right-side stats panel).

**`#stats-tab` (`.stats-tab` in CSS):**

- Change `right: 0` → `left: 0`.
- Change `border-radius: 6px 0 0 6px` → `border-radius: 0 6px 6px 0`.
- Change `border-right: none` → `border-left: none`.
- Keep `top: 50%; transform: translateY(-50%); z-index: 400;`.
- The tab is positioned relative to `#main-row` (its nearest positioned
  ancestor after the JS change in Step 4).

**`#selection-panel`:**

- No CSS change needed. It stays `position: absolute; bottom: 20px; left: 50%; transform: translateX(-50%)` — still works within `#main-row`.

**`#tooltip`:**

- No CSS change needed. It stays `position: absolute; z-index: 1000; pointer-events: none;` within `#main-row`.

### Step 3 — main.ts: Update toolbar insertion and stats panel insertion

Both changes are in `crates/visualize/frontend/src/main.ts`. They can be
implemented together in one commit since they are small, related changes
in the same file.

**Current toolbar insertion code in `renderGraphFromData()`:**

```ts
const toolbar = renderToolbar(graphData, controller, ...);
if (appEl) {
  appEl.insertBefore(toolbar, document.getElementById('legend'));
}
```

**Change to:**

```ts
const toolbar = renderToolbar(graphData, controller, ...);
// Prepend toolbar to #app so it becomes the top bar above the main-row
appEl?.prepend(toolbar);
```

**`renderToolbar()` function:**

- Remove the inline `position: absolute`, `top`, `left`, `z-index` styles.
  The toolbar is now a static flex item in the column — its positioning
  is handled by the flex layout.
- Keep `display: flex`, `align-items`, `gap`, `flex-wrap`.

**Current stats panel insertion code in `main()`:**

```ts
statsPanel = new StatsPanel();
const statsPanelEl = statsPanel.create();
if (appEl) {
  appEl.appendChild(statsPanelEl);
}
```

**Change to:**

```ts
statsPanel = new StatsPanel();
const statsPanelEl = statsPanel.create();
const mainRow = document.getElementById('main-row');
if (mainRow) {
  // Insert stats panel as the first child of main-row (left sidebar)
  mainRow.prepend(statsPanelEl);
}
```

### Step 5 — stats-panel.ts: Append tab to #main-row

**Current code in `create()`:**

```ts
this.tab = tab;
// Tab is hidden by default
tab.style.display = 'none';

this.panel = panel;
this.expanded = true;
// Append the tab to the document body so it's in the DOM for toggle/destroy
document.body.appendChild(tab);
return panel;
```

**Change to:**

```ts
this.tab = tab;
tab.style.display = 'none';

this.panel = panel;
this.expanded = true;
// Append the tab to #main-row so it's positioned relative to the graph area
const mainRow = document.getElementById('main-row');
(mainRow ?? document.body).appendChild(tab);
return panel;
```

The fallback to `document.body` handles the dev/test-harness case where
`#main-row` may not exist.

### Step 6 — tests/main.test.ts: Update toolbar positioning test

**Current test (line ~99):**

```ts
it('is styled as a flex container with absolute positioning', () => {
  // ...
  expect(toolbar.style.position).toBe('absolute');
  expect(toolbar.style.display).toBe('flex');
  expect(toolbar.style.alignItems).toBe('center');
  expect(toolbar.style.gap).toBe('8px');
  expect(toolbar.style.top).toBe('20px');
  expect(toolbar.style.left).toBe('20px');
  expect(toolbar.style.zIndex).toBe('500');
});
```

**Change to:**

```ts
it('is styled as a flex container with no absolute positioning', () => {
  // ...
  expect(toolbar.style.position).not.toBe('absolute');
  expect(toolbar.style.display).toBe('flex');
  expect(toolbar.style.alignItems).toBe('center');
  expect(toolbar.style.gap).toBe('8px');
});

it('prepends toolbar as first child of #app', () => {
  // Create a fresh #app element
  const appEl = document.createElement('div');
  appEl.id = 'app';
  document.body.appendChild(appEl);

  const data = makeGraph([makeNode('p1')], []);
  const mockController = {
    resetLayout: vi.fn(),
    setForceConfig: vi.fn(),
  } as unknown as GraphController;

  const toolbar = renderToolbar(data, mockController);
  appEl.prepend(toolbar);

  expect(appEl.firstChild).toBe(toolbar);

  document.body.removeChild(appEl);
});
```

### Step 7 — tests/stats-panel.test.ts: Verify tab parent

The test currently creates the panel and appends it to `document.body`.
The `create()` method in Step 5 will try to find `#main-row` and fall
back to `document.body`. In the test environment (happy-dom), there is
no `#main-row`, so the fallback applies. The existing tests should
pass unchanged. We should add tests that verify the tab is appended
and the panel is inserted into the correct parent when `#main-row`
exists:

```ts
it('appends tab to #main-row when available', () => {
  // Create a #main-row element
  const mainRow = document.createElement('div');
  mainRow.id = 'main-row';
  document.body.appendChild(mainRow);

  const newPanel = new StatsPanel();
  const el = newPanel.create();
  document.body.appendChild(el);

  const tab = document.querySelector('.stats-tab')!;
  expect(tab.parentElement).toBe(mainRow);

  newPanel.destroy();
  document.body.removeChild(mainRow);
});

it('inserts panel into #main-row when available', () => {
  const mainRow = document.createElement('div');
  mainRow.id = 'main-row';
  document.body.appendChild(mainRow);

  const newPanel = new StatsPanel();
  const el = newPanel.create();
  mainRow.prepend(el);

  expect(document.getElementById('stats-panel')!.parentElement).toBe(mainRow);

  newPanel.destroy();
  document.body.removeChild(mainRow);
});
```

## Edge Cases

| Case | Expected behavior |
|------|-------------------|
| **Narrow viewport** | Toolbar wraps via `flex-wrap`; stats panel stays 280px fixed width, graph area shrinks to fill remaining space. Stats panel can be collapsed to reclaim space. |
| **Force panel expanded** | Force panel sliders appear below the toolbar controls, increasing toolbar height. Stats panel top is unaffected (it's aligned to the top of `#main-row`, which starts below the toolbar). |
| **No graph data loaded** (welcome screen) | `#main-row` exists but has no toolbar above it (toolbar is only added in `renderGraphFromData`). The welcome screen content is in `#graph-container`. The stats panel is hidden initially (empty) — it's created in `main()` but only populated when data loads. |
| **Dev mode / test harness** | `test-harness.html` has its own layout and doesn't use `#main-row`. The stats panel tab falls back to `document.body`. The harness doesn't include the stats panel, so this is safe. |
| **Stats panel collapsed, then window resized** | The collapsed tab stays on the left edge. The graph fills the full width. No drift. |
| **Stats panel toggled** | `display: none` on the panel removes it from the flex row; graph takes full width. `display: flex` restores it at 280px. The tab shows/hides as before. |

## Test Plan

| Test file | What to test |
|-----------|-------------|
| `crates/visualize/frontend/tests/main.test.ts` | Update "is styled as a flex container with absolute positioning" → check no absolute positioning. Add "prepends toolbar as first child of #app" test. All other tests (click handlers, mode selector, etc.) unaffected. |
| `crates/visualize/frontend/tests/stats-panel.test.ts` | Add "appends tab to #main-row when available" and "inserts panel into #main-row when available" tests. Existing tests (create, render, toggle, destroy) should pass unchanged. |
| `crates/visualize/frontend/tests/graph.test.ts` | No changes needed — graph rendering is container-relative, not affected by parent layout. |

> **Note on CSS testing:** The flex layout changes in Step 2 are verified
> only via the Manual QA Checklist below. CSS layout testing is difficult
> in happy-dom (which doesn't implement computed layout). Adding
> integration-level visual regression tests is left as a future
> improvement.

## Manual QA Checklist

- [ ] Stats panel is on the left, below the toolbar
- [ ] Toolbar controls are functional (Mode, Select All, Group filter, Reset)
- [ ] Force panel expands/collapses without affecting stats panel position
- [ ] Legend is at top-right (not offset by 300px)
- [ ] Stats panel toggle shows/hides panel and the collapsed tab appears on the left edge
- [ ] Collapsed tab click re-expands the panel
- [ ] Window resize: toolbar wraps, stats panel stays positioned correctly
- [ ] Welcome screen (no data loaded) still renders correctly
- [ ] No visual regressions in the D3 graph area (zoom, pan, node selection)
