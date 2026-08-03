# Implementation Plan: Multi-Node Selection

Source: `docs/research/multi-node-selection.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add SelectionMode type and metadata` | Selection mode types | `src/types.ts` (add `SelectionMode`, `SelectionModeOption`, `SELECTION_MODES`) | — |
| 2 | `feat: create graph topology query module` | Topology traversal | `src/graph-query.ts` (NEW), `tests/graph-query.test.ts` (NEW) | Unit, property-based |
| 3 | `feat: extend SelectionManager with indirect-set and bulk methods` | Selection extensions | `src/selection.ts` (add `clickWithIndirect`, `addAll`, `removeAll`, wrapping), `tests/selection.test.ts` (additions) | Unit |
| 4 | `feat: add UI controls for multi-node selection` | Toolbar controls + click wiring | `src/main.ts` (mode selector `<select>`, Select All / Deselect All buttons, Group Select / Deselect buttons, revised `onNodeClick` wiring) | — |
| 5 | `style: add CSS for selection toolbar widgets` | Selection UI styles | `styles/main.css` (mode selector, bulk buttons, group buttons, separators) | — |
| 6 | `test: add smoke tests for selection UI rendering` | Smoke tests for UI widgets | `tests/main.test.ts` (additions) | Unit, smoke |
| 7 | `test: manual integration verification` | Manual verification | — | — |

## Step Details

### Step 1 — Selection mode types

**File:** `src/types.ts`

Add the `SelectionMode` type, `SelectionModeOption` interface, and `SELECTION_MODES` constant array as specified in the source plan. No tests — pure type definition.

### Step 2 — Topology query module (new)

**Files:** `src/graph-query.ts` (NEW), `tests/graph-query.test.ts` (NEW)

- Implement `Adjacency` interface and `buildAdjacency()` — builds parent/children/spouses/siblings/allNeighbors maps from `GraphData`
- Implement `getAncestors()`, `getDescendants()`, `getFirstDegree()`, `getSecondDegree()`, `getIndirectSet()`
- All query functions guard against missing nodes and cycles using visited sets
- Sibling derivation via nested loop over children per parent
- Write tests alongside: empty graph, single node, spouses, parent-child, three-generation chain, siblings, cycle safety, `getIndirectSet` with `'single'` mode, property-based invariants (no self-inclusion)

**Dependencies:** Step 1 (`SelectionMode` import in `getIndirectSet` signature)

### Step 3 — SelectionManager extensions

**Files:** `src/selection.ts`, `tests/selection.test.ts`

- Add `clickWithIndirect(handle, indirectHandles)` — if handle selected, remove handle + all indirects; if not selected, add handle + all indirects
- Add `addAll(handles)` and `removeAll(handles)` for bulk operations
- Wrap all three new methods in `createSelectionPanel` to trigger DOM re-render
- Write tests: `clickWithIndirect` toggle/add/remove semantics, `addAll`/`removeAll` with empty iterables, idempotency

### Step 4 — UI controls

**File:** `src/main.ts`

- Add `renderModeSelector(onChange)` — `<select>` populated from `SELECTION_MODES`, default `'single'`
- Add `renderSelectAllButtons(onSelectAll, onDeselectAll)` — two `<button>` elements
- Add family group Select Group / Deselect Group buttons inside `renderToolbar`, disabled when "All groups" selected
- Revise the `onNodeClick` callback in `renderGraphFromData`:
  - Build `adjacency` from `graphData` once (same closure scope as `currentMode`)
  - Use `getIndirectSet(adjacency, handle, currentMode)` + `selectionManager.clickWithIndirect()`
- Wire Select All → `selectionManager.addAll(controller.getVisibleNodes())`
- Wire Deselect All → `selectionManager.clear()`
- Import `SelectionMode` from types, query functions from graph-query

### Step 5 — CSS styles

**File:** `styles/main.css`

- Style for mode selector `<select>` (match existing filter dropdown)
- Style for Select All / Deselect All buttons (small, toolbar-consistent)
- Style for Group Select / Deselect buttons (compact, inline with filter dropdown)
- Optional visual separator between selection controls and existing toolbar widgets

### Step 6 — Smoke tests for UI

**File:** `tests/main.test.ts`

- `renderModeSelector(onChange)` creates a `<select>` with 5 options
- Mode selector fires `onChange` with correct `SelectionMode` value on user interaction
- Select All / Deselect All buttons appear in toolbar
- Group Select/Deselect buttons disabled when "All groups" selected

### Step 7 — Manual integration verification

Follow the 13-point checklist in the source plan to verify all modes, bulk operations, multi-click additive behavior, filter interaction, and export behavior.
