# Invert Selection Button

## Summary

Add an "Invert Selection" button to the visualizer toolbar. When clicked, the
selection state of every **visible** node is flipped: selected nodes become
deselected, and unselected nodes become selected. The operation is scoped to the
set of nodes currently displayed (respecting the family-group filter dropdown).

## Motivation

- Users frequently need to select everything *except* a few specific nodes. The
  current workflow requires either laborious manual toggling or using
  Select All then manually deselecting the exceptions.
- When viewing a single family group, being able to invert the selection within
  that group is a natural complement to the existing "Select Group" /
  "Deselect Group" buttons.

## Detailed Behavior

### When all family groups are displayed (filter = "All groups")

Every visible node in the graph gets its selection state flipped:

| Before                            | After                                  |
|-----------------------------------|----------------------------------------|
| 0 selected out of 100             | 100 selected (everything)              |
| 100 selected out of 100           | 0 selected (nothing)                   |
| 30 selected (set A), 70 not (set B) | 70 selected (set B), 30 not (set A) |

### When a single family group is displayed (e.g., group 3 of 8)

Only the nodes belonging to that family group are affected. Nodes from hidden
family groups are **not** touched, even if they were previously selected.

Example: 8 family groups exist, user filters to group 3 (5 people).
2 of those 5 are currently selected, plus 3 nodes from other groups were
selected earlier (still in the SelectionManager but invisible).

| State component            | Before | After  |
|----------------------------|--------|--------|
| Visible, selected          | 2      | 0      |
| Visible, unselected        | 3      | 3 (now selected) |
| Hidden, selected           | 3      | 3 (untouched)    |
| Total selected             | 5      | 6      |

The inversion operates strictly on `controller.getVisibleNodes()` — the same
set used by the existing "Select All" and "Deselect All" buttons. The
`GraphController.getVisibleNodes()` method already respects the family-group
filter, returning only nodes whose `family_group` matches the current filter
(or all nodes when the filter is `null`).

### Relationship to existing controls

| Button          | Scope                | Action                          |
|-----------------|----------------------|---------------------------------|
| Select All      | visible nodes        | Adds all to selection           |
| Deselect All    | visible nodes        | Removes all from selection      |
| Select Group    | current group (visible if filtered) | Adds group to selection  |
| Deselect Group  | current group (visible if filtered) | Removes group from selection |
| **Invert Selection** | visible nodes   | Flips each node's state         |

## Implementation Plan

### 1. Add `invertSelection` method to `SelectionManager` (`selection.ts`)

The `SelectionManager` class in `crates/visualize/frontend/src/selection.ts`
already has `addAll` and `removeAll` for batch operations. The new method
iterates over the provided handles and toggles each one individually:

```typescript
/**
 * Invert selection for the given set of handles.
 * Selected handles become deselected; unselected handles become selected.
 * Handles not in the provided iterable are left unchanged.
 */
invertSelection(handles: Iterable<string>): void {
  for (const h of handles) {
    if (this.selected.has(h)) {
      this.selected.delete(h);
    } else {
      this.selected.add(h);
    }
  }
}
```

The method operates only on the supplied handles — it does not iterate over
the entire selection set. This matches the requirement that hidden-group
selections are left undisturbed.

Because `createSelectionPanel` wraps `SelectionManager` methods with
`render()` calls (to update the selection-counter and export-button state),
the new method must also be wrapped so the panel re-renders after inversion.
Follow the existing pattern used for `addAll` and `removeAll`.

Insert this wrapping code **after the `removeAll` wrapping block** (which ends
`manager.removeAll = ...;`) and **before `return manager;`**:

```typescript
const origInvert = manager.invertSelection.bind(manager);
manager.invertSelection = (handles: Iterable<string>) => {
  origInvert(handles);
  render();
};
```

> **Note**: The wrapping code goes inside `createSelectionPanel`, not inside
> the `SelectionManager` class itself. The class method is added directly to
> the class body.

### 2. Update `renderToolbar` parameter type and add button (`main.ts`)

The `renderToolbar` function in `crates/visualize/frontend/src/main.ts`
already accepts a `selectionManager` parameter, but its type only exposes
`addAll`, `removeAll`, `clear`, and `handles`. The new `invertSelection`
method must be added to this parameter type so the button click handler
compiles.

**Update the `selectionManager` parameter type in `renderToolbar`'s signature**
to include `invertSelection`:

```typescript
selectionManager?: {
  addAll: (handles: Iterable<string>) => void;
  removeAll: (handles: Iterable<string>) => void;
  invertSelection: (handles: Iterable<string>) => void;
  clear: () => void;
  handles: string[];
}
```

A new "Invert Selection" button should be placed adjacent to the Select All /
Deselect All buttons for logical grouping.

Add the button between the existing Select All / Deselect All pair and the
separator that precedes the family-group filter:

```typescript
const invertBtn = document.createElement('button');
invertBtn.textContent = 'Invert';
invertBtn.title = 'Invert selection on visible nodes';
invertBtn.style.padding = '4px 10px';
invertBtn.style.fontSize = '12px';
invertBtn.style.borderRadius = '4px';
invertBtn.style.border = '1px solid #ccc';
invertBtn.style.background = '#fff';
invertBtn.style.cursor = 'pointer';
invertBtn.style.color = '#333';
invertBtn.addEventListener('mouseenter', () => {
  invertBtn.style.background = '#eee';
});
invertBtn.addEventListener('mouseleave', () => {
  invertBtn.style.background = '#fff';
});
invertBtn.addEventListener('click', () => {
  selectionManager!.invertSelection(controller.getVisibleNodes());
  controller.setHighlighted(new Set(selectionManager!.handles));
});
container.appendChild(invertBtn);
```

All styling matches the existing Select All / Deselect All buttons — no new
CSS rules are required.

### 3. Add unit tests for `invertSelection` (`tests/selection.test.ts`)

Tests go in `crates/visualize/frontend/tests/selection.test.ts` within the
`SelectionManager` describe block. Test cases:

| Test case | Input | Expected |
|-----------|-------|----------|
| all unselected | `['p1','p2']`, none selected | both become selected |
| all selected | `['p1','p2']`, both selected | both become deselected |
| mixed | `['p1','p2','p3']`, p1 selected, p2/p3 not | p1 deselected, p2/p3 selected |
| empty iterable | `[]`, p1 selected | p1 remains selected (no-op) |
| handles outside iterable untouched | invert `['p1']`, p2 selected | p1 toggled, p2 unchanged |
| idempotent double-invert | invert twice on same set | returns to original state |

### 4. Verify with `npm run test`

From the `crates/visualize/frontend/` directory, run:

```bash
npm run test
```

Ensure all existing tests still pass and the new tests succeed.

### 5. Manual verification checklist

- [ ] Open a `.gramps` file with multiple family groups
- [ ] "All groups" view: click Invert → all nodes become selected
- [ ] Click Invert again → all nodes become deselected
- [ ] Select 2 nodes manually → click Invert → those 2 become deselected, all others selected
- [ ] Filter to a single family group
- [ ] With no selections in that group, click Invert → all group nodes selected, other groups' selections untouched
- [ ] Click Invert again → group nodes deselected, other groups' selections still untouched
- [ ] Verify the selection counter in the sidebar updates correctly
- [ ] Verify that "Export Selected" is enabled/disabled correctly
- [ ] Verify the button styling is consistent (hover state)

## Files Changed

| File | Change |
|------|--------|
| `crates/visualize/frontend/src/selection.ts` | Add `invertSelection` method to `SelectionManager`; wrap in `createSelectionPanel` |
| `crates/visualize/frontend/src/main.ts` | Add `invertSelection` to `renderToolbar` parameter type; add "Invert" button |
| `crates/visualize/frontend/tests/selection.test.ts` | Add `invertSelection` unit tests |

## Risk Assessment

- **Risk**: Low. The change is additive, scoped to a single well-understood
  subsystem, and follows existing patterns exactly.
- **Breaking changes**: None. No existing API is modified.
- **Performance**: O(n) in visible node count per click, same as Select All /
  Deselect All. The operation is trivially fast even for thousands of nodes.
