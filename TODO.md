# Implementation Plan: Fix Drag Node Jump on Zoomed/Panned Graph

Source: `docs/research/fix-drag-node-jump.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix: remove double-inversion in drag handler causing node jump on zoomed/panned graph` | Fix drag node jump | `crates/visualize/frontend/src/graph.ts`, `crates/visualize/frontend/tests/graph.test.ts` | Unit (drag handlers, `createDragBehavior`) |

## Step details

### Step 1 — Fix `onDrag` and clean up unused `svg` parameters

**Changes to `graph.ts`:**

- Remove `transform.invert()` call from `onDrag` — set `d.fx = event.x` and `d.fy = event.y` directly (D3's drag behavior already provides coordinates in SVG space)
- Remove `svg` parameter from `onDrag`, `onDragStart`, and `onDragEnd` (all three had it, but only `onDrag` used it; the other two already had it prefixed `_svg`)
- Remove `svg` parameter from `createDragBehavior` signature and stop passing it to the three arrow functions
- Update the call site in `restartSimulation()` from `createDragBehavior(simulation, svg.node() as SVGSVGElement)` to `createDragBehavior(simulation)`

**Changes to `graph.test.ts`:**

- Merge the two existing `onDrag` tests ("updates fx/fy to event coords at identity zoom" and "inverts zoomed event coords back to base SVG space") into a single test that documents zoom-independent behavior: `onDrag` no longer accepts `svg`, so zoom state cannot affect the result
- Drop `svg` argument from all `onDragStart` calls (3 call sites)
- Drop `svg` argument from all `onDrag` calls (2 call sites, one of which is the merged replacement)
- Drop `svg` argument from all `onDragEnd` calls (4 call sites)
- Drop `svg` argument from `createDragBehavior` call (1 call site)

**Verification:**

- `npm test` passes in `crates/visualize/frontend/`
- `cargo build -p visualize` succeeds
- No TypeScript or lint warnings
