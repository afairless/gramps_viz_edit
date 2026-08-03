# Multi-Node Selection

**Date:** 2025-01-01  
**Status:** Planned

## Overview

Add multi-node selection capabilities to the family graph visualization. Instead of selecting only one node at a time, users can select nodes via topological relationships (ancestors, descendants, 1st-degree connections, 2nd-degree connections), plus bulk select/deselect all visible nodes or all nodes in a family group.

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Deselection of indirectly-selected nodes | Remove all nodes in the computed indirect set, even if some were selected via other actions | Simple semantics, no need to track selection origin |
| Multi-select click behavior | Always additive — existing selections are preserved | Natural for building complex selections |
| Select All with mode active | Ignore mode; always directly select all visible nodes | Predictable and independent of mode |
| Family-group select/deselect | Buttons in the toolbar next to the filter dropdown, disabled when "All groups" is selected | Discoverable, co-located with the filter |

## Selection Modes

Changing the selection mode does **not** affect already-selected nodes — it only changes the behavior of subsequent clicks.

| Mode | Direct click on node N | Behavior |
|---|---|---|
| **Single** (default) | ✓ | Toggle N only (current behavior) |
| **Ancestors** | ✓ | Toggle N + all ancestors (follow ParentChild links upward) |
| **Descendants** | ✓ | Toggle N + all descendants (follow ParentChild links downward) |
| **1st-degree** | ✓ | Toggle N + spouses, parents, children |
| **2nd-degree** | ✓ | Toggle N + 1st-degree connections + siblings, grandparents, grandchildren, aunts/uncles, nieces/nephews |

### Adding (node not yet selected)

- Compute the **indirect set** for the clicked node in the active mode
- Add the clicked node + all nodes in the indirect set
- Nodes already selected (by any prior action) remain selected

### Removing (node already selected)

- Compute the **indirect set** for the clicked node in the active mode
- Remove the clicked node + all nodes in the indirect set
- This is unconditional — nodes selected via other actions are also removed

### Bulk Operations

| Button | Behavior |
|---|---|
| **Select All** | Select every visible node directly (ignores selection mode) |
| **Deselect All** | Clear all selections (ignores selection mode) |
| **Select Group** | Select every node in the chosen family group from the full dataset (ignores mode and active filter). Only enabled when a specific group is chosen in the filter dropdown. |
| **Deselect Group** | Deselect every node in the chosen family group from the full dataset (ignores mode and active filter). Only enabled when a specific group is chosen in the filter dropdown. |

## Affected Components

```
crates/visualize/frontend/
├── src/
│   ├── types.ts          ✦ Add SelectionMode type, SelectionModeOption
│   ├── graph-query.ts    ★ NEW: graph topology traversal functions
│   ├── selection.ts      ✦ Extend SelectionManager with indirect-set support
│   ├── graph.ts          ✦ No changes (highlighting already uses Set<string>)
│   ├── main.ts           ✦ Add mode selector, select-all, group-select UI
├── styles/
│   └── main.css          ✦ Styles for new toolbar widgets
├── tests/
│   ├── selection.test.ts ✦ Tests for new SelectionManager methods
│   └── graph-query.test.ts ★ NEW: tests for topology traversal functions
└── index.html            ✦ (no changes needed — dynamic DOM)
```

## Implementation Steps

### Step 1: Add `SelectionMode` type and `SelectionModeOption` metadata

**File:** `crates/visualize/frontend/src/types.ts`

Add:

```typescript
export type SelectionMode = 'single' | 'ancestors' | 'descendants' | 'first-degree' | 'second-degree';

export interface SelectionModeOption {
  value: SelectionMode;
  label: string;
  description: string;
}

export const SELECTION_MODES: SelectionModeOption[] = [
  { value: 'single', label: 'Single node', description: 'Select one node at a time' },
  { value: 'ancestors', label: 'Ancestors', description: 'Select node + all ancestors' },
  { value: 'descendants', label: 'Descendants', description: 'Select node + all descendants' },
  { value: 'first-degree', label: '1st-degree', description: 'Select node + spouses, parents, children' },
  { value: 'second-degree', label: '2nd-degree', description: 'Select node + 2-hop connections' },
];
```

### Step 2: Create graph topology query module

**File:** `crates/visualize/frontend/src/graph-query.ts` (NEW)

Pure functions that operate on `GraphData` to compute indirect sets. Each takes the graph data links and a starting handle, returns a `Set<string>` of indirectly selected handles (excluding the starting handle).

#### Data structure

Build adjacency indices for efficient traversal:

```typescript
interface Adjacency {
  parents: Set<string>;      // ParentChild links where target is this node
  children: Set<string>;     // ParentChild links where source is this node
  spouses: Set<string>;      // Spouse links (undirected)
  siblings: Set<string>;     // Derived: shared parents
  allNeighbors: Set<string>; // Union of the above
}

function buildAdjacency(data: GraphData): Map<string, Adjacency>;
```

`buildAdjacency` adds an entry for **every** node in `data.nodes`, even isolated ones with no edges. This ensures all query functions can safely index the map without null checks.

**Lifecycle:** Adjacency is built once in `renderGraphFromData` after receiving `GraphData`, stored in a closure-level `let` binding (same scope as `currentMode`), and rebuilt only when `updateData` is called or a new file is loaded. Filter changes do **not** rebuild adjacency — the full graph topology is always available for queries regardless of the active filter.

#### Query functions

```typescript
function getAncestors(adj: Map<string, Adjacency>, handle: string): Set<string>;
function getDescendants(adj: Map<string, Adjacency>, handle: string): Set<string>;
function getFirstDegree(adj: Map<string, Adjacency>, handle: string): Set<string>;
function getSecondDegree(adj: Map<string, Adjacency>, handle: string): Set<string>;

/** Dispatch to the correct query function based on mode.
    Returns an empty set for 'single' mode (caller need not branch). */
function getIndirectSet(
  adj: Map<string, Adjacency>,
  handle: string,
  mode: SelectionMode,
): Set<string> {
  switch (mode) {
    case 'single':       return new Set();
    case 'ancestors':    return getAncestors(adj, handle);
    case 'descendants':  return getDescendants(adj, handle);
    case 'first-degree': return getFirstDegree(adj, handle);
    case 'second-degree':return getSecondDegree(adj, handle);
  }
}
```

**Traversal algorithms:**

- **Ancestors:** BFS/DFS walking `parents` upward until no more parents. Includes grandparents, great-grandparents, etc.
- **Descendants:** BFS/DFS walking `children` downward until no more children. Includes grandchildren, great-grandchildren, etc.
- **1st-degree:** Union of `parents`, `children`, `spouses`.
- **2nd-degree:** All nodes reachable in exactly 1 or 2 hops through `allNeighbors`. This naturally includes: 1st-degree connections, siblings (via shared parents), grandparents (via parents-of-parents), grandchildren (via children-of-children), aunts/uncles (via parents-of-siblings), nieces/nephews (via children-of-siblings).

#### Build order

`buildAdjacency` executes in this order:

1. Create an `Adjacency` entry for **every** node in `data.nodes` (all sets initially empty).
2. Iterate all links: populate `parents`, `children`, `spouses`.
3. Derive siblings from shared parents (see below).
4. Compute `allNeighbors` as the union of `parents`, `children`, `spouses`, `siblings`.

Step 4 must run **after** sibling derivation so `allNeighbors` includes siblings for 2nd-degree queries.

#### Sibling derivation

Building siblings requires checking shared parents. For each pair of nodes that share at least one parent (and are not the same node), add to each other's `siblings` set. This is done during `buildAdjacency`. To avoid O(n²) on every parent set, iterate through each parent's children list and add all pairs.

```typescript
// During buildAdjacency, after populating parents/children:
for (const [parentHandle, adjEntry] of adj) {
  const childList = [...adjEntry.children];
  for (let i = 0; i < childList.length; i++) {
    for (let j = i + 1; j < childList.length; j++) {
      const a = adj.get(childList[i])!;
      const b = adj.get(childList[j])!;
      a.siblings.add(childList[j]);
      b.siblings.add(childList[i]);
    }
  }
}
```

#### Edge cases

- Node not in adjacency map → return empty set (guard: `if (!adj.has(handle)) return new Set()`)
- Node with no edges → return empty set (all adjacency sets are empty)
- Graph with 0 links → all queries return empty set
- Cycles (unlikely in family trees but possible with data errors) → use visited set to prevent infinite loops; a 3-node cycle A→B→C→A returns all 3 nodes for ancestors/descendants of any node in the cycle

#### Tests (co-located with implementation)

**File:** `crates/visualize/frontend/tests/graph-query.test.ts` (NEW)

Write alongside this step:

- Empty graph (0 nodes, 0 links) → empty set for all modes
- Single node, no links → empty set for all modes
- Node not in graph → empty set
- Two spouses → each sees the other as 1st-degree and 2nd-degree
- Parent-child → parent sees child as descendant and 1st-degree; child sees parent as ancestor and 1st-degree
- Three-generation chain (grandparent → parent → child):
  - Ancestors of child = {parent, grandparent}
  - Descendants of grandparent = {parent, child}
  - 1st-degree of parent = {grandparent, child}
  - 2nd-degree of child = {parent, grandparent}
- Siblings: two children of same parents see each other as 2nd-degree
- Cycle: A→B→C→A returns finite set (3 nodes), does not infinite-loop
- `getIndirectSet` with 'single' mode returns empty set regardless of graph
- Property-based invariant: `getAncestors(adj, h)` never contains `h`
- Property-based invariant: `getDescendants(adj, h)` never contains `h`

### Step 3: Extend SelectionManager and update SelectionPanel wrapping

**File:** `crates/visualize/frontend/src/selection.ts`

The `SelectionManager` class needs two new capabilities:

1. **Indirect-set toggle:** A method that takes a pre-computed indirect set and applies add/remove logic.
2. **Bulk add/remove:** Methods for select-all and deselect-all operations.

```typescript
class SelectionManager {
  // ... existing methods ...

  /**
   * Click with indirect selection support.
   * - If handle is NOT selected: add handle + all indirectHandles
   * - If handle IS selected: remove handle + all indirectHandles
   */
  clickWithIndirect(handle: string, indirectHandles: Set<string>): void;

  /** Add multiple handles at once (no toggle — pure add). */
  addAll(handles: Iterable<string>): void;

  /** Remove multiple handles at once (no toggle — pure remove). */
  removeAll(handles: Iterable<string>): void;
}
```

**`clickWithIndirect` semantics:**

```
if selected.has(handle):
    // DESELECT: remove handle + all indirects
    selected.delete(handle)
    for each h in indirectHandles: selected.delete(h)
else:
    // SELECT: add handle + all indirects
    selected.add(handle)
    for each h in indirectHandles: selected.add(h)
```

This unconditionally removes all indirects (matching the design decision: "Deselect all in the indirect set" even if selected via other means).

#### Update SelectionPanel wrapping

The `createSelectionPanel` function wraps `SelectionManager` methods to trigger DOM re-renders. The new methods (`clickWithIndirect`, `addAll`, `removeAll`) need similar wrapping. Follow the existing pattern:

```typescript
// NOTE: when adding new public mutation methods to SelectionManager,
// they must be wrapped here to trigger re-renders.
const origClickWithIndirect = manager.clickWithIndirect.bind(manager);
manager.clickWithIndirect = (handle, indirectHandles) => {
  origClickWithIndirect(handle, indirectHandles);
  render();
};
const origAddAll = manager.addAll.bind(manager);
manager.addAll = (handles) => {
  origAddAll(handles);
  render();
};
const origRemoveAll = manager.removeAll.bind(manager);
manager.removeAll = (handles) => {
  origRemoveAll(handles);
  render();
};
```

#### Tests (co-located with implementation)

**File:** `crates/visualize/frontend/tests/selection.test.ts` (ADDITIONS)

Add alongside this step:

- `clickWithIndirect` with empty indirects behaves like `click` (toggle)
- `clickWithIndirect` adds node + indirects when node unselected
- `clickWithIndirect` removes node + indirects when node selected, even if some indirects were selected via other means
- `addAll` adds multiple handles
- `removeAll` removes multiple handles
- `addAll` with already-selected handles is idempotent (no double-counting)
- `removeAll` with non-selected handles is idempotent
- `addAll([])` is a no-op (empty iterable)
- `removeAll([])` is a no-op (empty iterable)

### Step 4: Add UI controls to main.ts

**File:** `crates/visualize/frontend/src/main.ts`

All new controls integrate into the existing `renderToolbar` function. The toolbar already contains the filter dropdown, reset button, and force-control panel. Adding a mode selector + 2-4 buttons may crowd the toolbar on smaller windows — use `flex-wrap: wrap` or a visual separator (via CSS) between selection controls and existing widgets.

#### 4a. Mode selector

Add a `<select>` dropdown to the toolbar for choosing the selection mode. Position it next to the existing toolbar widgets.

```typescript
function renderModeSelector(onChange: (mode: SelectionMode) => void): HTMLElement {
  // Renders a <select> with options from SELECTION_MODES
  // Default value: 'single'
  // Calls onChange on user interaction
}
```

#### 4b. Select All / Deselect All buttons

Two buttons in the toolbar, between the mode selector and the filter dropdown.

```typescript
function renderSelectAllButtons(
  onSelectAll: () => void,
  onDeselectAll: () => void,
): HTMLElement {
  // Two <button> elements: "Select All" and "Deselect All"
  // Styled consistently with existing toolbar buttons
}
```

#### 4c. Family group Select/Deselect buttons

Two small buttons added inside `renderToolbar`, next to the filter dropdown. They are enabled only when a specific group (not "All groups") is selected in the dropdown. When "All groups" is selected, the buttons are disabled (no specific group to target).

```
[Family Group: ▼ Group 1 (42 people, 5 gen.)] [Select Group] [Deselect Group]
```

These operate on the **full dataset** for the chosen family group, not just currently visible nodes — so "Select Group" works even if the filter is set to a different group. The group's node handles are obtained from `graphData.nodes.filter(n => n.family_group === selectedGroupId).map(n => n.handle)`, then passed to `selectionManager.addAll()` / `selectionManager.removeAll()`.

Click wiring for bulk buttons (added in `renderToolbar` or `renderGraphFromData`):

```typescript
// Select All: use controller.getVisibleNodes() so it respects the active filter
onSelectAll: () => {
  selectionManager.addAll(controller.getVisibleNodes());
  controller.setHighlighted(new Set(selectionManager.handles));
}

// Deselect All
onDeselectAll: () => {
  selectionManager.clear();
  controller.setHighlighted(new Set());
}
```

#### 4d. Revised click wiring

The onNodeClick callback in `renderGraphFromData` changes from:

```typescript
controller.onNodeClick((handle: string) => {
  selectionManager.click(handle, false);
  controller.setHighlighted(new Set(selectionManager.handles));
});
```

To:

```typescript
controller.onNodeClick((handle: string) => {
  const indirect = getIndirectSet(adjacency, handle, currentMode);
  selectionManager.clickWithIndirect(handle, indirect);
  controller.setHighlighted(new Set(selectionManager.handles));
});
```

Where `adjacency` is built from `graphData` (once, in `renderGraphFromData`, same scope as `currentMode`) and `currentMode` is the active selection mode from the mode selector. Both are captured in the closure when rendering the graph. Adjacency is NOT rebuilt on filter changes — the full graph topology is always available for queries.

**Filter + selection interaction:** When a family group filter is active, selected nodes hidden by the filter remain in the `SelectionManager` (they are still "selected") but are not visually highlighted (they aren't in the visible node set). `getVisibleNodes()` on the controller already accounts for the active filter.

### Step 5: Add CSS styles

**File:** `crates/visualize/frontend/styles/main.css`

New styles for:

- Mode selector dropdown (match existing filter dropdown style)
- Select All / Deselect All buttons (small, match toolbar aesthetics)
- Group Select / Deselect buttons (compact, inline with filter dropdown)
- Possibly a separator between toolbar sections

### Step 6: Write remaining tests

Tests for Steps 2 and 3 are written alongside their implementations (see inline test sections above). This step covers any remaining test additions:

#### selection.test.ts (additional edge cases if not already covered)

- `addAll` with empty iterable is a no-op
- `removeAll` with empty iterable is a no-op
- `clickWithIndirect` with empty indirect set is equivalent to `click` (toggle)

#### graph-query.test.ts (property-based invariants)

If not already written with Step 2:

- `getAncestors(adj, h)` never contains `h`
- `getDescendants(adj, h)` never contains `h`

#### Smoke tests for UI rendering (optional, use vitest + happy-dom)

- `renderModeSelector(onChange)` creates a `<select>` with 5 options
- Mode selector fires `onChange` with correct `SelectionMode` value on user interaction

### Step 7: Integration testing

Manual verification checklist:

1. Default mode is "Single node" — clicking a node highlights only that node
2. Switch to "Ancestors" — clicking a child node highlights the child + all ancestors
3. Switch to "Descendants" — clicking a grandparent highlights them + all descendants
4. Switch to "1st-degree" — clicking a married parent highlights spouse + children
5. Switch to "2nd-degree" — clicking a node highlights connections up to 2 hops
6. Clicking an already-selected node in any mode deselects it + its indirects
7. Multi-click is additive — click one node in ancestors mode, then another — both sets are selected
8. "Select All" selects every visible node regardless of mode
9. "Deselect All" clears everything regardless of mode
10. "Select Group" / "Deselect Group" buttons appear and work when a specific family group is chosen in the filter dropdown
11. Filter group dropdown + "Select Group" work together — selecting group 5 highlights all nodes in group 5
12. Export button is enabled when any nodes are selected, exports selected nodes
13. Selection count updates correctly for all operations

## Performance Considerations

- **Adjacency map:** Built once when graph data loads (or changes via `updateData`). Building is O(N + E) where N = nodes, E = links. For typical family trees (hundreds to low thousands of people), this is negligible. The map includes entries for isolated nodes (no edges) so queries never need null checks.
- **Traversal:** Each traversal uses DFS/BFS with visited-set protection, O(V + E) per traversal. Worst case: traversing the entire graph for each click. For thousands of nodes: sub-millisecond.
- **Re-render:** `setHighlighted` triggers D3 selection on all visible nodes. Currently this iterates all nodes already. No new performance concern.
- **Sibling derivation:** The nested loop over children per parent is bounded by the maximum family size (number of children per parent), which in realistic data is small (typically < 20). The total work is O(sum of children² per family), which is small.
- **Filter changes:** Do not trigger adjacency rebuild. The full graph topology is always available for queries regardless of the active filter.

## Alternative Considered

**Tracking selection origin:** We considered tracking which indirect selections belong to which direct selection, so deselecting a node only removes "its" indirects. Rejected because:

- Adds complexity (origin map, cascading cleanup)
- The simple unconditional model matches user expectation: "click node → select its neighborhood, click again → deselect that neighborhood"
- Users can always re-click nodes they want to keep selected

## Dependencies

- None of the other Rust crates are affected — this is purely a frontend (TypeScript/DOM) change
- No Tauri IPC changes required
- No schema changes required
