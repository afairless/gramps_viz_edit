# Implementation Plan: Link Style Distinction — Vertical vs. Horizontal Relationships

Source: `docs/research/link-style-distinction.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat(visualize): add LinkType type and link style functions` | Link style functions + `LinkType` type | `crates/visualize/frontend/src/types.ts`, `crates/visualize/frontend/src/colors.ts`, `crates/visualize/frontend/tests/colors.test.ts` | Unit |
| 2 | `feat(visualize): add link-type legend items to renderLegend` | Legend rendering for link types | `crates/visualize/frontend/src/colors.ts`, `crates/visualize/frontend/tests/colors.test.ts` | Unit |
| 3 | `feat(visualize): apply link styles in force simulation graph` | Graph rendering with distinct link styles | `crates/visualize/frontend/src/graph.ts`, `crates/visualize/frontend/tests/graph.test.ts` | Unit |
| 4 | `feat(visualize): wire up link legend flags in main entry point` | Main entry wiring | `crates/visualize/frontend/src/main.ts` | — |
| 5 | `chore(visualize): build and verify frontend + Rust integration` | Build verification | — | Integration |

## Step Details

### Step 1 — Link style functions + `LinkType` type

- Add `export type LinkType = 'Spouse' | 'ParentChild';` to `types.ts`.
- Replace inline union literal in `FamilyLink.link_type` with `LinkType`.
- In `colors.ts`, add link style constants (`LINK_PARENT_CHILD_COLOR`, `LINK_SPOUSE_COLOR`, etc.) and three exported functions: `getLinkColor`, `getLinkStrokeDash`, `getLinkStrokeWidth`, each taking `LinkType` and returning the appropriate value with a console-warn fallback for unknown types.
- In `colors.test.ts`, add tests for each function covering `Spouse`, `ParentChild`, and unknown fallback (deterministic return values + console.warning).

### Step 2 — Legend rendering for link types

- Add `hasSpouseLinks?: boolean` and `hasParentChildLinks?: boolean` to `LegendConfig` in `colors.ts`.
- Update `renderLegend` to render two SVG `<line>` legend items (30px long, matching stroke/dash/width of link types) under a "Links" sub-heading when both flags are present, or without sub-heading when only one flag is present.
- In `colors.test.ts`, add tests for `renderLegend` with each flag combination (both, one, none).

### Step 3 — Graph rendering with distinct link styles

- Import `getLinkColor`, `getLinkStrokeDash`, `getLinkStrokeWidth` from `./colors` in `graph.ts`.
- Remove the `LINK_STROKE_WIDTH` constant (no longer used uniformly).
- In `restartSimulation()`, update the `linkBind.enter().append('line')` chain to use the new functions keyed on `d.link_type`, and increase `stroke-opacity` from 0.6 to 0.8.
- In `graph.test.ts`, add tests verifying `buildSimLinks` preserves `link_type` through the handle→node mapping.

### Step 4 — Main entry wiring

- In `renderGraphFromData()` in `main.ts`, compute `hasSpouseLinks` and `hasParentChildLinks` by checking `graphData.links` for each type, and pass them to the `renderLegend` call.

### Step 5 — Build verification

- Run `npm test` in `crates/visualize/frontend` to verify all TypeScript tests pass.
- Build the frontend (`npm run build`).
- Run `cargo build -p visualize` to verify the Rust build with the updated frontend.
- Run `cargo test -p visualize` to verify Rust integration tests.
