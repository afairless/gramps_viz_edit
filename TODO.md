# Implementation Plan: Move Stats Panel to Left Side

Source: `docs/research/move-stats-panel-to-left.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add #main-row wrapper to index.html` | HTML layout wrapper | `crates/visualize/frontend/index.html` | — |
| 2 | `feat: restructure CSS for left-side stats panel layout` | Flex layout CSS | `crates/visualize/frontend/styles/main.css` | — |
| 3 | `feat: update toolbar and stats panel insertion in main.ts` | JS insertion logic | `crates/visualize/frontend/src/main.ts`, `crates/visualize/frontend/tests/main.test.ts` | Unit |
| 4 | `feat: append stats tab to #main-row` | Tab parent change | `crates/visualize/frontend/src/stats-panel.ts`, `crates/visualize/frontend/tests/stats-panel.test.ts` | Unit |

## Step Details

### Step 1 — index.html: Add #main-row wrapper

**What:** Wrap `#graph-container` and all absolute overlays (tooltip, selection-panel, legend) in a new `<div id="main-row">`.

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

**No tests needed** — structural HTML change only. Verify by checking index.html parses correctly.

---

### Step 2 — CSS: Restructure layout styles

**Changes:**

| Selector | Change |
|---|---|
| `#app` | Add `display: flex; flex-direction: column;` |
| `#main-row` (new) | `flex: 1; display: flex; flex-direction: row; position: relative; min-height: 0; overflow: hidden;` |
| `#graph-container` | Remove `width: 100%; height: 100%;` → add `flex: 1; position: relative;` |
| `#toolbar` | Remove `position: absolute; top: 20px; left: 20px; z-index: 500;`. Add `width: 100%; padding: 10px 12px; background: #fafafa; border-bottom: 1px solid #ddd;` |
| `#stats-panel` | Remove `position: absolute; top: 10px; right: 10px; height: calc(100% - 20px);`. Add `flex-shrink: 0;` |
| `#legend` | Change `right: 300px` → `right: 10px` |
| `.stats-tab` | Change `right: 0` → `left: 0`; `border-radius: 6px 0 0 6px` → `border-radius: 0 6px 6px 0`; `border-right: none` → `border-left: none` |
| `#selection-panel` | No change needed |
| `#tooltip` | No change needed |

**No tests needed** — CSS layout cannot be tested in happy-dom. Verification via manual QA checklist.

---

### Step 3 — main.ts: Update toolbar and stats panel insertion

**Changes to `renderToolbar()`:**

- Remove inline `position: absolute`, `top: 20px`, `left: 20px`, `z-index: 500` styles from the toolbar element.

**Changes to `renderGraphFromData()`:**

- Change `appEl.insertBefore(toolbar, document.getElementById('legend'))` → `appEl.prepend(toolbar)`.

**Changes to `main()`:**

- Change `appEl.appendChild(statsPanelEl)` → `mainRow.prepend(statsPanelEl)` where `mainRow = document.getElementById('main-row')`.

**Test changes (`main.test.ts`):**

- Update "is styled as a flex container with absolute positioning" test to check that toolbar does NOT have absolute positioning, and still has `display: flex`, `align-items: center`, `gap: 8px`.
- Add a new test "prepends toolbar as first child of #app".

---

### Step 4 — stats-panel.ts: Append tab to #main-row

**Changes to `create()`:**

- Change `document.body.appendChild(tab)` → `(document.getElementById('main-row') ?? document.body).appendChild(tab)`.

**Test changes (`stats-panel.test.ts`):**

- Add test "appends tab to #main-row when available" — creates #main-row, calls create(), verifies tab parent is #main-row.
- Add test "inserts panel into #main-row when available" — creates #main-row, calls create(), verifies panel parent is #main-row.
- Existing tests must pass unchanged (fallback to `document.body` in test environment).

---

## Manual QA Checklist

After all steps are implemented:

- [ ] Stats panel is on the left, below the toolbar
- [ ] Toolbar controls are functional (Mode, Select All, Group filter, Reset)
- [ ] Force panel expands/collapses without affecting stats panel position
- [ ] Legend is at top-right (not offset by 300px)
- [ ] Stats panel toggle shows/hides panel and the collapsed tab appears on the left edge
- [ ] Collapsed tab click re-expands the panel
- [ ] Window resize: toolbar wraps, stats panel stays positioned correctly
- [ ] Welcome screen (no data loaded) still renders correctly
- [ ] No visual regressions in the D3 graph area (zoom, pan, node selection)
