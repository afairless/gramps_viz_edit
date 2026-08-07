# Implementation Plan: Invert Selection Button

Source: `docs/research/invert-selection-button.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add invertSelection method to SelectionManager` | SelectionManager.invertSelection + wrapping in createSelectionPanel + unit tests | `crates/visualize/frontend/src/selection.ts`, `crates/visualize/frontend/tests/selection.test.ts` | Unit |
| 2 | `feat: add Invert button to toolbar` | renderToolbar param type update + Invert button in main.ts | `crates/visualize/frontend/src/main.ts` | — |
| 3 | `chore: verify Invert Selection manually` | Manual verification against checklist | — | Manual |

**Step details:**

### Step 1 — `invertSelection` method + wrapping + tests

- Add `invertSelection(handles: Iterable<string>)` method to `SelectionManager` class in `selection.ts`: iterates over handles, toggling each — selected → deselected, unselected → selected. Handles not in the iterable are left unchanged.
- Add wrapping code in `createSelectionPanel` (after `removeAll` wrapping, before `return manager`) so the panel re-renders after inversion, following the same pattern as `addAll` and `removeAll`.
- Add unit tests to `tests/selection.test.ts`: all unselected → all selected; all selected → all deselected; mixed state; empty iterable; handles outside iterable untouched; idempotent double-invert.

### Step 2 — Update renderToolbar + add button

- Add `invertSelection: (handles: Iterable<string>) => void` to the `selectionManager` parameter type in `renderToolbar`'s signature in `main.ts`.
- Add an "Invert" button between the Select All / Deselect All pair and the visual separator (`|`), with matching styling and a click handler that calls `selectionManager!.invertSelection(controller.getVisibleNodes())` and `controller.setHighlighted(new Set(selectionManager!.handles))`.

### Step 3 — Manual verification

- Run `npm run test` from `crates/visualize/frontend/` to confirm all tests pass.
- Follow the manual verification checklist in the plan document.
