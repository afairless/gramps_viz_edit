# Implementation Plan: Visualizer Stats Panel

Source: `docs/research/visualizer-stats.md`

## Summary

Add a collapsible right-sidebar stats panel to the Gramps family-group
visualizer. Statistics are computed server-side via a new `get_stats` Tauri
IPC command (reusing `gramps_reader::count_gramps_xml`) and rendered in a
vanilla-TS panel component.

**Pre-requisite already done:** `StatsReport` and `PrimaryTypeCounts` are
already re-exported from `gramps_reader` (verified in `crates/gramps-reader/src/lib.rs`).

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add get_stats IPC command to visualize crate` | get_stats IPC command | `crates/visualize/src/lib.rs` (new `get_stats` fn), `crates/visualize/src/main.rs` (new `get_stats` Tauri command + registration) | Unit |
| 2 | `feat: add StatsReport TypeScript types` | StatsReport TS types | `crates/visualize/frontend/src/types.ts` (add `PrimaryTypeCounts`, `StatsReport`, `FamilySizeDistribution`, `FamilyGroupDistribution` interfaces) | — |
| 3 | `feat: create stats panel component with unit tests` | Stats panel component | `crates/visualize/frontend/src/stats-panel.ts` (new `StatsPanel` class), `crates/visualize/frontend/styles/main.css` (stats panel styles) | Unit |
| 4 | `feat: wire stats panel to IPC in main.ts` | Stats IPC wiring | `crates/visualize/frontend/src/main.ts` (add `fetchAndRenderStats`, update `renderGraphFromData` signature, create panel instance) | — |
| 5 | `test: add integration test for get_stats` | Integration test | `crates/visualize/tests/` (new integration test file, e.g. `stats.rs`) | Integration |

---

## Step Details

### Step 1 — Add `get_stats` IPC command

**Files:** `crates/visualize/src/lib.rs`, `crates/visualize/src/main.rs`

**Changes to `lib.rs`:**

- Add a new public function:

  ```rust
  pub fn get_stats(path: &str) -> Result<gramps_reader::StatsReport, String> {
      let content = std::fs::read_to_string(path)
          .map_err(|e| format!("Cannot read file '{}': {}", path, e))?;
      gramps_reader::count_gramps_xml(&content)
          .map_err(|e| format!("Failed to parse Gramps XML: {}", e))
  }
  ```

**Changes to `main.rs`:**

- Register a new `#[tauri::command] fn get_stats(path: &str)` that calls `visualize::get_stats(path)`.
- Add `get_stats` to the `invoke_handler`:

  ```rust
  .invoke_handler(tauri::generate_handler![load_graph, export_selections, get_stats])
  ```

**Dependencies:** No new crate dependencies. `tempfile` is already in `[dev-dependencies]`.

**Unit tests (in `lib.rs`):**

- `get_stats_nonexistent_file` — returns error with "Cannot read file"
- `get_stats_malformed_xml` — returns error with "Failed to parse Gramps XML"
- `get_stats_valid_file` — returns correctly populated `StatsReport`
- `get_stats_empty_file` — returns zeroed `StatsReport`

---

### Step 2 — Add TypeScript types for `StatsReport`

**File:** `crates/visualize/frontend/src/types.ts`

**Additions:**

```typescript
export interface PrimaryTypeCounts {
  people: number;
  families: number;
  events: number;
  places: number;
  sources: number;
  citations: number;
  repositories: number;
  media: number;
  notes: number;
  tags: number;
}

export type FamilySizeDistribution = Record<string, number>;
export type FamilyGroupDistribution = Record<string, number>;

export interface StatsReport {
  file: string;
  counts: PrimaryTypeCounts;
  family_size_distribution: FamilySizeDistribution;
  family_group_distribution: FamilyGroupDistribution;
  family_group_generation_table: Record<string, Record<string, number>>;
  people_not_in_family: number;
  dangling_refs: number;
  warnings: string[];
}
```

**Tests:** Type-only step — no runtime logic to test. The types are verified
by compilation and the integration test in Step 5.

---

### Step 3 — Create stats panel component

**Files:** `crates/visualize/frontend/src/stats-panel.ts` (new), `crates/visualize/frontend/styles/main.css` (additions)

**New file `stats-panel.ts`:**

- `StatsPanel` class with:
  - `create()`: Build DOM elements for the right-side collapsible sidebar
  - `render(report: StatsReport)`: Populate the panel with data sections
    (object counts, family size distribution, family group distribution,
    data quality warnings)
  - `renderError(msg: string)`: Show non-intrusive error message in the
    panel body (muted red text)
  - `toggle()`: Show/hide the panel programmatically
  - `destroy()`: Clean up DOM elements

**Panel layout:**

- 280px wide, right-aligned, full-height minus margin
- Title bar with "File Statistics" label and `[×]` collapse button
- Sections: Object counts (10 types), Family size dist., Family group dist., Data quality
- Collapsed state shows a narrow "Stats" tab on the right edge

**CSS additions (in `main.css`):**

- `#stats-panel` container (right sidebar, z-index above graph, below toolbar)
- `.stats-panel-header` (title bar, click-to-collapse)
- `.stats-panel-body` (scrollable content area)
- `.stats-section` (grouped section with heading)
- `.stats-section table` (key-value pairs)
- `.stats-warning` (warning list items)
- `.stats-tab` (collapsed tab on the right edge)

**Unit tests (in `crates/visualize/frontend/tests/stats-panel.test.ts`):**

- `StatsPanel.create()` produces correct DOM structure
- `StatsPanel.render()` populates all sections correctly
  - Object counts: all 10 types shown
  - Family size distribution: empty, single item, multiple items
  - Family group distribution: empty, single item, multiple items
  - Data quality: zero values, non-zero values, warnings present
  - Warnings list: empty shown as "none", non-empty shown as list
- `StatsPanel.toggle()` shows/hides correctly
- `StatsPanel.renderError()` displays error message
- `StatsPanel.destroy()` removes elements from DOM

---

### Step 4 — Wire stats panel to IPC in main.ts

**File:** `crates/visualize/frontend/src/main.ts`

**Changes:**

- Add `fetchAndRenderStats` async function that invokes the `get_stats` Tauri IPC command:

  ```typescript
  async function fetchAndRenderStats(filePath: string): Promise<void> {
    const tauri = await import('@tauri-apps/api/core');
    try {
      const report: StatsReport = await tauri.invoke('get_stats', { path: filePath });
      statsPanel.render(report);
    } catch (err) {
      console.warn('Failed to load stats:', err);
      statsPanel.renderError('Failed to load statistics. The file may have been moved or deleted.');
    }
  }
  ```

- Create the `StatsPanel` instance early and append it to `#app`
- Update `renderGraphFromData` signature to accept an optional `filePath` parameter:

  ```typescript
  function renderGraphFromData(
    container: HTMLElement,
    appEl: HTMLElement,
    graphData: GraphData,
    filePath?: string,
  ): void
  ```

- When `filePath` is provided, call `fetchAndRenderStats(filePath)` after rendering
- Update callers:
  - `openAndRenderFile()` — pass the `selected` path
  - `openAndRenderFileFromPath()` — pass the `filePath` parameter
  - Dev mode fallback in `main()` — omit the argument

**Tests:** No separate test file — the integration test in Step 5 covers the
end-to-end round-trip. The existing `main.test.ts` tests should continue to
pass (the `filePath` parameter is optional).

---

### Step 5 — Integration test for `get_stats`

**File:** `crates/visualize/tests/stats.rs` (new)

**Test:**

- Write a minimal `.gramps` XML file to a temp file using the `write_gramps_file` helper pattern
- Call `visualize::get_stats(path)` and verify the returned `StatsReport` matches expected counts
- Verify round-trip through serde_json

**No new dependencies:** `tempfile` is already in `[dev-dependencies]`.

---

## Verification

After all 5 steps are committed:

```bash
# Run all Rust tests
cargo test -p visualize

# Run all frontend tests
cd crates/visualize/frontend && npm test

# Verify no lint issues
cargo clippy -p visualize -- -D warnings
```
