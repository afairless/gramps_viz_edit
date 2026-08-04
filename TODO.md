# Implementation Plan: Fix Blank Statistics Panel and Legend Overlap

Source: `docs/research/stats-panel-blank-and-occluded.md`

This is a bug-fix plan for the visualizer's "File Statistics" panel. Two independent
root causes:

1. **Blank panel** — the module-level `statsPanel` variable in `main.ts` is never
   assigned; a local `const statsPanel` in `main()` shadows it, so
   `fetchAndRenderStats` throws on `statsPanel.render(report)`.
2. **Legend occlusion** — `#legend` (right:20px, z-index:500) overlaps the
   `#stats-panel` (right:10px, width:280px, z-index:400); both are anchored
   top-right.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix(visualize): assign module-level statsPanel in main()` | Stats panel wiring fix | `crates/visualize/frontend/src/main.ts` | vitest |
| 2 | `fix(visualize): move legend left of stats panel` | Legend/stats-panel layout fix | `crates/visualize/frontend/styles/main.css` | — |
| 3 | `test(visualize): verify stats panel + legend fixes` | Frontend verification pass | `crates/visualize/frontend/tests/*` (review only) | vitest |

## Step details

### Step 1 — Stats panel wiring fix

**File:** `crates/visualize/frontend/src/main.ts`

In `main()` (around line 404), change the local `const statsPanel` declaration to an
assignment to the module-level variable:

```diff
-  const statsPanel = new StatsPanel();
+  statsPanel = new StatsPanel();
```

**Verification:** `npx vitest run` in `crates/visualize/frontend`. Existing
`stats-panel.test.ts` and `main.test.ts` cover the `StatsPanel` class and toolbar
rendering; the wiring change does not alter the class or DOM structure, so they
should continue to pass.

### Step 2 — Legend/stats-panel layout fix

**File:** `crates/visualize/frontend/styles/main.css`, `#legend` rule

Move the legend to the left of the 280px-wide stats panel (10 + 280 + 10 margin):

```diff
 #legend {
   position: absolute;
   top: 20px;
-  right: 20px;
+  right: 300px;
   ...
 }
```

**Known limitation (accepted for v1):** when the stats panel is collapsed via its
× button, the legend stays at `right: 300px`, leaving a gap where the panel was. The
legend does not overlap anything in either state. Dynamic repositioning on toggle is
deferred.

### Step 3 — Verification pass

Run the full frontend test suite and manually verify the built app:

```bash
cd crates/visualize/frontend
npx vitest run
```

Manual checks (build with `cargo build -p visualize` or `cargo tauri dev`):

1. Open a `.gramps` file — the stats panel populates with data.
2. Legend and stats panel no longer overlap.
3. Collapse the panel (×) — legend sits on the right with a gap; no overlap.
4. Re-expand — no overlap.

## Notes

- The plan document also includes a combined single-commit message for both fixes.
  This plan splits them into two commits (Steps 1 and 2) because they are independent
  one-line changes to different files; both are still covered by the same verification
  in Step 3. If a single combined commit is preferred, merge Steps 1 and 2.
