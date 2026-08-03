# Link Style Distinction: Vertical vs. Horizontal Relationships

## Problem

In the family-group force-directed graph visualization, all links between person
nodes are drawn identically: a solid gray line (`#999`, 1.5px wide, 0.6 opacity).
This makes it impossible to distinguish parent-child relationships (vertical)
from spouse/civil-union relationships (horizontal/lateral) at a glance.

The data model already carries the distinction — every `FamilyLink` has a
`link_type` field set to `Spouse` or `ParentChild` — but the renderer ignores it.

## Proposed Solution

Style links differently based on `link_type`:

| Relationship | Color  | Stroke style | Width | Opacity |
|---|---|---|---|---|
| Parent-child (vertical) | Orange (`#e67e22`) | Solid (`"none"`) | Thin (1.5px) | 0.8 |
| Spouse / civil union (horizontal) | Blue (`#3498db`) | Dotted (`"4,3"`) | Thick (3px) | 0.8 |

Add a legend entry for both link types so the user can see the mapping.

Note: The stroke-opacity was increased from 0.6 to 0.8 to improve link visibility
against the viridis node fills. See "Why higher stroke-opacity" in Design Decisions.

## Files to Modify

### 1. `crates/visualize/frontend/src/colors.ts` — New link style functions

Add three exported functions parallel to the existing node-styling functions,
using a shared `LinkType` type (imported from `./types` to avoid repeating the
union literal across three files):

```ts
// Link style constants
const LINK_PARENT_CHILD_COLOR = '#e67e22';  // orange
const LINK_SPOUSE_COLOR = '#3498db';         // blue
const LINK_PARENT_CHILD_DASH = 'none';       // solid
const LINK_SPOUSE_DASH = '4,3';              // dotted
const LINK_PARENT_CHILD_WIDTH = 1.5;         // thin
const LINK_SPOUSE_WIDTH = 3;                 // thick

// Default fallback for unknown link types
const LINK_FALLBACK_COLOR = '#999999';
const LINK_FALLBACK_DASH = 'none';
const LINK_FALLBACK_WIDTH = 1.5;

export function getLinkColor(linkType: LinkType): string {
  switch (linkType) {
    case 'ParentChild': return LINK_PARENT_CHILD_COLOR;
    case 'Spouse':      return LINK_SPOUSE_COLOR;
    default:            console.warn('Unknown link type:', linkType);
                        return LINK_FALLBACK_COLOR;
  }
}
export function getLinkStrokeDash(linkType: LinkType): string {
  switch (linkType) {
    case 'ParentChild': return LINK_PARENT_CHILD_DASH;
    case 'Spouse':      return LINK_SPOUSE_DASH;
    default:            console.warn('Unknown link type:', linkType);
                        return LINK_FALLBACK_DASH;
  }
}
export function getLinkStrokeWidth(linkType: LinkType): number {
  switch (linkType) {
    case 'ParentChild': return LINK_PARENT_CHILD_WIDTH;
    case 'Spouse':      return LINK_SPOUSE_WIDTH;
    default:            console.warn('Unknown link type:', linkType);
                        return LINK_FALLBACK_WIDTH;
  }
}
```

Add `LinkType` to the exported types in `types.ts`:

```ts
export type LinkType = 'Spouse' | 'ParentChild';
```

(This replaces the inline union literal `FamilyLink.link_type` and `SimLink.link_type`.)

Also update `renderLegend()` to show two link-type legend items. Each item is a
30px-long inline SVG `<line>` element with the same stroke, dasharray, and width
as the corresponding link type, preceded by a text label. Use `stroke-linecap="butt"`
(default) for both.

Accept new `LegendConfig` fields:

```ts
export interface LegendConfig {
  // ... existing fields ...
  /** Whether to show the Spouse (blue dotted) legend item. */
  hasSpouseLinks?: boolean;
  /** Whether to show the Parent-child (orange solid) legend item. */
  hasParentChildLinks?: boolean;
}
```

If both link-type flags are present, add a small sub-heading "Links" above the
link-type legend items to visually separate them from the "Birth Year" section.
Omit the sub-heading if only one link type is present.

### 2. `crates/visualize/frontend/src/graph.ts` — Apply link styles

In `restartSimulation()`, within the `linkBind.enter().append('line')` chain,
replace the hard-coded `stroke`, `stroke-width`, and `stroke-opacity` attributes
with values returned by the new `getLinkColor`, `getLinkStrokeDash`, and
`getLinkStrokeWidth` functions, keyed on `d.link_type`.

Import the new functions from `colors.ts` (and `LinkType` from `types.ts` if a
shared type alias is used):

```ts
import {
  buildColorScale,
  getNodeColor,
  getNodeStrokeDash,
  getNodeOpacity,
  getLinkColor,
  getLinkStrokeDash,
  getLinkStrokeWidth,
} from './colors';
```

The `linkBind.enter().append('line')` section currently reads:

```ts
const linkEnter = linkBind
  .enter()
  .append('line')
  .attr('stroke', '#999')
  .attr('stroke-width', LINK_STROKE_WIDTH)
  .attr('stroke-opacity', 0.6);
```

Change to:

```ts
const linkEnter = linkBind
  .enter()
  .append('line')
  .attr('stroke', (d: SimLink) => getLinkColor(d.link_type))
  .attr('stroke-width', (d: SimLink) => getLinkStrokeWidth(d.link_type))
  .attr('stroke-dasharray', (d: SimLink) => getLinkStrokeDash(d.link_type))
  .attr('stroke-opacity', 0.8);
```

Note: The `stroke-opacity` was increased from 0.6 to 0.8 across both link types.
See "Why higher stroke-opacity" in Design Decisions.

The constant `LINK_STROKE_WIDTH` at the top of the file (currently `1.5`) can
be removed since it is no longer used uniformly.

### 3. `crates/visualize/frontend/src/main.ts` — Wire up link legend

In `renderGraphFromData()`, after the node-legend call, pass the new
`hasSpouseLinks` and `hasParentChildLinks` flags to `renderLegend()` by
checking whether `graphData.links` contains any of each type.

### 4. `crates/visualize/frontend/tests/graph.test.ts` — New rendering tests

Add tests that verify the data flow from `buildSimLinks` produces the correct
`link_type` mapping. While the pure rendering attributes (stroke, dash, width)
are tested in `colors.test.ts`, the wiring in `restartSimulation()` is not
exported. Verify the data-path at minimum:

- `buildSimLinks` with a mix of Spouse and ParentChild links produces `SimLink`
  objects with the correct `link_type` values
- `buildSimLinks` preserves the `link_type` field through the handle→node mapping

For a more thorough test, use jsdom/happy-dom to instantiate `renderGraph` with
sample data and verify the SVG `<line>` elements have the expected attributes:

- Spouse links have `stroke="#3498db"`, `stroke-dasharray="4,3"`, `stroke-width="3"`
- ParentChild links have `stroke="#e67e22"`, `stroke-dasharray="none"`, `stroke-width="1.5"`

### 5. `crates/visualize/frontend/tests/colors.test.ts` — New tests

Add tests for the new `getLinkColor`, `getLinkStrokeDash`, and
`getLinkStrokeWidth` functions:

- `Spouse` returns blue, `4,3`, 3
- `ParentChild` returns orange, `none`, 1.5
- Both return deterministic values
- Unknown link type returns fallback (`#999999`, `none`, 1.5) with console warning

Add tests for the updated `renderLegend`:

- `renderLegend` with `hasSpouseLinks: true` creates a legend item with a blue
  dotted line labeled "Spouse"
- `renderLegend` with `hasParentChildLinks: true` creates a legend item with an
  orange solid line labeled "Parent-child"
- `renderLegend` with both flags shows both link-type items under a "Links"
  sub-heading
- `renderLegend` with neither flag omits link-type items entirely

## Implementation Order

1. **Add link style functions to `colors.ts` + `types.ts`** — add `LinkType` type alias
   to `types.ts`, add style constants and functions with defensive defaults to
   `colors.ts`. **Write tests** in `colors.test.ts` for `getLinkColor`,
   `getLinkStrokeDash`, `getLinkStrokeWidth` (all three link types + unknown fallback).
   Commit.

2. **Update `renderLegend` in `colors.ts`** — accept `hasSpouseLinks`/
   `hasParentChildLinks` flags, render SVG line legend items under a "Links"
   sub-heading. **Write tests** in `colors.test.ts` for the new legend items.
   Commit.

3. **Update `graph.ts`** — import new functions, apply link styles in
   `restartSimulation()`, remove `LINK_STROKE_WIDTH` constant. **Write tests**
   in `graph.test.ts` for data-path verification (buildSimLinks preserves
   link_type) and optionally jsdom-based rendering verification. Commit.

4. **Update `main.ts`** — pass `hasSpouseLinks`/`hasParentChildLinks` flags to
   `renderLegend` by checking `graphData.links`. Commit.

5. **Build and verify** — run all test suites:

   ```bash
   (cd crates/visualize/frontend && npm test)          # TypeScript tests
   cargo build -p visualize                             # Rust build (needs frontend/dist/ up to date)
   cargo test -p visualize                              # Rust integration tests
   ```

   Note: The frontend must be built (`npm run build`) before `cargo build -p visualize`
   since the Rust build script reads from `frontend/dist/`.

## Design Decisions

### Why functions in `colors.ts` instead of inline in `graph.ts`?

The existing pattern keeps node-styling logic (`getNodeColor`, `getNodeStrokeDash`,
`getNodeOpacity`) in `colors.ts` and calls them from `graph.ts`. Following the
same pattern for link styling keeps the rendering code clean and the styling
logic testable and reusable.

### Why orange and blue?

- **Orange** (`#e67e22`) is a warm, earthy color that conveys "rootedness" —
  appropriate for the vertical lineage of parent-child relationships.
- **Blue** (`#3498db`) is a calm, neutral color often used for partnerships —
  appropriate for spouse/civil-union relationships.
- Both have good contrast against the white node stroke (`#fff`) and the
  viridis-based node fill colors.
- Both are distinguishable by color-blind users (blue-orange is a common
  colorblind-friendly pair).

### Why solid vs. dotted?

Solid lines for parent-child (vertical) relationships help the eye follow
generational flow. Dotted lines for spouse relationships visually distinguish
the "bond" from the "lineage" without requiring the user to read a legend.

### Why thin vs. thick?

Thicker lines for spouse relationships (3px vs. 1.5px) make the marriage
connection visually prominent, which is important for understanding family
structure — marriages are the "glue" that connects otherwise separate
bloodlines.

### Why higher stroke-opacity?

The stroke-opacity was increased from 0.6 (the current uniform gray) to 0.8 for
both link types. This serves two purposes:

1. **Compensates for thinner lines**: The 1.5px orange parent-child line at 0.6
   opacity appears too faint against the viridis-colored nodes. At 0.8 it remains
   visible but still subordinate to the node fills.
2. **Uniform opacity keeps the visual hierarchy clean**: Both link types share the
   same opacity so the viewer perceives them as part of the same layer (links),
   distinguished only by stroke, dash, and width — not by transparency.

### What about civil unions and other non-marriage partnerships?

Gramps data models all spousal relationships (marriage, civil union, etc.)
through the same family structure (`<family>` with `<father>` and `<mother>`
or `<childref>` elements). The reader converts all of them to `LinkType::Spouse`.
Thus, all non-parent-child relationships are styled identically (blue, dotted,
thick), which is a reasonable default. If finer-grained distinction is needed
later (e.g., a Gramps attribute on the family record), it can be added as a
future enhancement.

## Future Considerations

- **Configurable colors/styles** via CLI flags or a settings panel, if users
  want to customize the link appearance.
- **Directional arrows** on parent-child links to indicate the flow from parent
  to child (currently the links are undirected in rendering).
- **Half-sibling distinction** — if a parent has children with multiple spouses,
  the visual distinction between full-sibling and half-sibling links could be
  shown with a different link style.
