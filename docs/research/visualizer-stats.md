# Visualizer Stats Panel — Feature Plan

## Summary

Add a collapsible sidebar panel to the Gramps family-group visualizer that
displays summary statistics for the currently loaded `.gramps` file. The
statistics are computed server-side via a new Tauri IPC command and rendered
in a dedicated right-side panel in the frontend.

## Motivation

The CLI already has a `stats` command that produces a rich `StatsReport`
(object counts, family size distribution, family group distribution,
generation-span table, data quality warnings). Users of the visualizer
currently have no way to see these aggregate metrics — they can only explore
the graph node-by-node. Adding a stats panel gives users immediate
quantitative context about their family tree data (e.g., "how many people?",
"how many families?", "are there people not in any family?", "any cycle
warnings?").

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| **Stats UI location** | Right-side collapsible sidebar | Leaves the graph canvas full-height, keeps the toolbar at top-left for graph controls, mirrors the left-aligned toolbar for a balanced layout. |
| **IPC approach** | Separate `get_stats` Tauri command | Modular — stats are fetched independently from graph data, keeping the hot-path `load_graph` payload lean. The file content is re-read from disk, but the streaming pass is fast and the file is already cached by the OS. |
| **Stats sections** | Object counts, family size distribution, family group distribution, data quality warnings | The generation-span table is esoteric and space-intensive; it's omitted from the visualizer for now. |

## Implementation Plan

### Step 1: Verify `StatsReport` re-export from `gramps-reader`

**Status:** ✅ Already done — no changes needed.

**Files checked:**

- `crates/gramps-reader/src/lib.rs`

**Verification:** The following re-exports already exist in the library root:

```rust
pub use xml::count::{count_gramps_xml, PrimaryTypeCounts, StatsReport};
```

Both `StatsReport` and `PrimaryTypeCounts` are already importable by the
`visualize` crate. No action required for this step.

---

### Step 2: Add `get_stats` Tauri IPC command in `crates/visualize`

**Files changed:**

- `crates/visualize/src/lib.rs`
- `crates/visualize/src/main.rs`

**Changes to `lib.rs`:**

- Add a new public function:

  ```rust
  pub fn get_stats(path: &str) -> Result<StatsReport, String> {
      let content = std::fs::read_to_string(path)
          .map_err(|e| format!("Cannot read file '{}': {}", path, e))?;
      gramps_reader::count_gramps_xml(&content)
          .map_err(|e| format!("Failed to parse Gramps XML: {}", e))
  }
  ```

- This reads the file from disk and runs the streaming `count_gramps_xml`
  pass. The file content is already cached by the OS after the initial
  `load_graph_data` call, so the second read is fast (a single streaming
  pass over hot cache).

**⚠️ Known limitation — double file read:** Both `load_graph_data` and
`get_stats` read the entire file into memory independently. For large
`.gramps` files (>100MB), this means ~2x peak memory usage. A future
optimization could share the in-memory content between the two functions
by having `load_graph_data` also expose the raw content string, but this
is out of scope for the initial implementation.

**Changes to `main.rs`:**

- Add a new Tauri command:

  ```rust
  #[tauri::command]
  fn get_stats(path: &str) -> Result<gramps_reader::StatsReport, String> {
      visualize::get_stats(path)
  }
  ```

- Register it in the invoke handler:

  ```rust
  .invoke_handler(tauri::generate_handler![load_graph, export_selections, get_stats])
  ```

**🔐 Tauri capabilities:** The existing `load_graph` and `export_selections`
commands work under `core:default` in `capabilities/default.json`. The new
`get_stats` command reads files from disk using the same pattern, so no
capability changes are expected. **Verify** that `get_stats` doesn't trigger
permission errors at runtime; if it does, add the appropriate permissions to
`capabilities/default.json`.

**Testing:**

- Unit tests in `lib.rs` for `get_stats()`, following the existing pattern
  (using `tempfile::NamedTempFile`):
  - Nonexistent file → returns error with "Cannot read file"
  - Malformed XML → returns error with "Failed to parse Gramps XML"
  - Valid file → returns correctly populated `StatsReport`
  - Empty file → returns zeroed `StatsReport` (matches `count_gramps_xml` behavior)

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn get_stats_nonexistent_file() {
          let result = get_stats("/nonexistent/path.gramps");
          match result {
              Err(msg) => assert!(msg.contains("Cannot read file"), "got: {}", msg),
              Ok(_) => panic!("expected error for nonexistent file"),
          }
      }

      #[test]
      fn get_stats_malformed_xml() {
          let mut tmp = tempfile::NamedTempFile::new().unwrap();
          write!(tmp, "<database><person></database>").unwrap();
          let path = tmp.path().with_extension("gramps");
          std::fs::rename(tmp.path(), &path).unwrap();
          let result = get_stats(path.to_str().unwrap());
          match result {
              Err(msg) => assert!(msg.contains("Failed to parse Gramps XML"), "got: {}", msg),
              Ok(_) => panic!("expected error for malformed XML"),
          }
      }

      #[test]
      fn get_stats_valid_file() {
          let mut tmp = tempfile::NamedTempFile::new().unwrap();
          let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
  <database xmlns="http://gramps-project.org/xml/1.7.2/">
    <people><person handle="p1"/></people>
  </database>"#;
          write!(tmp, "{}", xml).unwrap();
          let path = tmp.path().with_extension("gramps");
          std::fs::rename(tmp.path(), &path).unwrap();
          let report = get_stats(path.to_str().unwrap()).unwrap();
          assert_eq!(report.counts.people, 1);
      }

      #[test]
      fn get_stats_empty_file() {
          let mut tmp = tempfile::NamedTempFile::new().unwrap();
          write!(tmp, "").unwrap();
          let path = tmp.path().with_extension("gramps");
          std::fs::rename(tmp.path(), &path).unwrap();
          let report = get_stats(path.to_str().unwrap()).unwrap();
          assert_eq!(report, StatsReport::default());
      }
  }
  ```

- Add `tempfile` to `[dev-dependencies]` in `crates/visualize/Cargo.toml`
  (it is already present as of the current codebase).

---

### Step 3: Add TypeScript types for `StatsReport`

**Files changed:**

- `crates/visualize/frontend/src/types.ts`

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
  /** Row: group-size, Column: generation-span, Cell: group count. Unused by frontend. */
  family_group_generation_table: Record<string, Record<string, number>>;
  people_not_in_family: number;
  dangling_refs: number;
  warnings: string[];
}
```

The stats IPC returns the full `StatsReport` from the Rust side (including
`family_group_generation_table`), but the frontend only consumes the four
requested sections. The generation table is included in the type for
accuracy — it faithfully represents the IPC payload.

---

### Step 4: Create stats panel component

**Files changed:**

- `crates/visualize/frontend/src/stats-panel.ts` (new file)

**Contents:**

- `StatsPanel` class with:
  - `create()`: Build the DOM elements for the sidebar panel
  - `render(report: StatsReport)`: Populate the panel with data
  - `renderError(msg: string)`: Show a non-intrusive error message in the
    panel body (e.g., muted red text "Failed to load statistics") when the
    IPC call fails
  - `toggle()`: Show/hide the panel
  - `destroy()`: Clean up DOM elements

**Panel layout (right sidebar, collapsible):**

```
┌─────────────────────────┐
│  File Statistics    [×] │  ← Title bar (click to collapse)
├─────────────────────────┤
│  Object counts          │
│    People:         42   │
│    Families:       10   │
│    Events:         57   │
│    Places:         12   │
│    Sources:         3   │
│    Citations:       9   │
│    Repositories:    1   │
│    Media:           4   │
│    Notes:          15   │
│    Tags:            2   │
│                         │
│  Family size dist.      │
│    size  1: 1 family    │
│    size  2: 2 families  │
│    size  3: 5 families  │
│    size  4: 2 families  │
│                         │
│  Family group dist.     │
│    size  1: 2 groups    │
│    size  3: 1 group     │
│    size  5: 1 group     │
│                         │
│  Data quality           │
│    Not in family:    8  │
│    Dangling refs:    0  │
│    Warnings:     none   │
└─────────────────────────┘
```

**Behavior:**

- Panel is 280px wide, right-aligned, full-height minus a small margin
- Collapsible via the `[×]` button or a keyboard shortcut
- The panel has a CSS `z-index` above the graph but below the toolbar
- When collapsed, only a narrow tab with a "Stats" label remains visible
  on the right edge, which can be clicked to re-open the panel

**CSS additions:**

- `/crates/visualize/frontend/styles/main.css` — new styles for:
  - `#stats-panel` container (right sidebar)
  - `.stats-panel-header` (title bar)
  - `.stats-panel-body` (scrollable content area)
  - `.stats-section` (grouped section with heading)
  - `.stats-section table` (key-value pairs)
  - `.stats-warning` (warning list items)
  - `.stats-tab` (collapsed tab on the right edge)

---

### Step 5: Wire up stats to the IPC and integrate into the UI

**Files changed:**

- `crates/visualize/frontend/src/main.ts`

**Changes:**

- After `renderGraphFromData` succeeds, fetch stats via IPC:

  ```typescript
  async function fetchAndRenderStats(filePath: string): Promise<void> {
    const tauri = await import('@tauri-apps/api/core');
    try {
      const report: StatsReport = await tauri.invoke('get_stats', {
        path: filePath,
      });
      statsPanel.render(report);
    } catch (err) {
      console.warn('Failed to load stats:', err);
      statsPanel.renderError('Failed to load statistics. The file may have been moved or deleted.');
    }
  }
  ```

- Create the `StatsPanel` instance early and append it to `#app`
- Call `fetchAndRenderStats` after graph data is loaded
- Handle the path source: both from `window.__GRAMPS_FILE__` (CLI path)
  and from the file-open dialog (store the path after successful load)

**The file path tracking:**

- In `renderGraphFromData`, we need to know the file path to pass to
  `get_stats`. Either:
  (a) Store the path as a module-level variable (set by the caller)
  (b) Pass it as a parameter to `renderGraphFromData`

  Option (b) is cleaner. The function signature becomes:

  ```typescript
  function renderGraphFromData(
    container: HTMLElement,
    appEl: HTMLElement,
    graphData: GraphData,
    filePath?: string,  // optional — for stats
  ): void
  ```

  When `filePath` is provided, the stats IPC call is made after rendering.

**Callers needing updates:** The function is called from three locations:

  1. `openAndRenderFile()` — has the path from the file dialog `selected`
     variable; pass it to `renderGraphFromData`
  2. `openAndRenderFileFromPath()` — has the `filePath` parameter; pass it
     through
  3. Dev mode fallback in `main()` — no file path available (dev mode uses
     `window.__GRAPH_DATA__`); omit the argument

  The file path is also available via `window.__GRAMPS_FILE__` (set by the
  CLI in `main.rs` setup), but passing it as a parameter is cleaner than
  reading a global.

---

### Step 6: Add unit tests

**Files changed:**

- `crates/visualize/frontend/tests/stats-panel.test.ts` (new file)

**Test coverage:**

- `StatsPanel.create()` produces correct DOM structure
- `StatsPanel.render()` populates sections correctly
  - Object counts: all 10 types shown
  - Family size distribution: empty, single item, multiple items
  - Family group distribution: empty, single item, multiple items
  - Data quality: zero values, non-zero values, warnings present
  - Warnings list: empty warnings shown as "none", non-empty shown as list
- `StatsPanel.toggle()` shows/hides correctly
- `StatsPanel.destroy()` removes elements from DOM

---

### Step 7: Integration test — end-to-end IPC round-trip

**Files changed:**

- `crates/visualize/tests/` (new integration test file, e.g. `stats.rs`)

**Approach:** Test `visualize::get_stats` directly (not via Tauri IPC), using
`tempfile::NamedTempFile` — consistent with the existing 9 integration tests
in `crates/visualize/tests/integration.rs`.

**Test:**

- Write a minimal `.gramps` XML file to a temp file (using the `write_gramps_file`
  helper pattern from `integration.rs`)
- Call `visualize::get_stats(path)` and verify the returned `StatsReport`
  matches the expected counts for the crafted XML
- Verify that the stats are consistent with a known-good `StatsReport`
  (round-trip through serde_json)

---

## Open Questions / Future Work

- **Generation-span table**: Could be added as a 5th section in the future
  if users need it. It was omitted from the initial scope because it's the
  most complex to display and the least commonly needed.
- **Stats refresh on filter change**: Currently stats are computed once per
  file load. If the graph is filtered by family group, the stats could
  optionally be recomputed for the visible subset. Out of scope for now.
- **Export stats**: Could add a "Download stats as JSON" button in the
  panel footer. Out of scope for now.
- **Keyboard shortcut**: `S` key to toggle the stats panel. Nice-to-have.

## Dependencies

- No new Rust crate dependencies
- No new npm dependencies — all DOM manipulation is vanilla
- The `StatsReport` type is already serializable via `serde` (it derives
  `Serialize`/`Deserialize`), so it can be sent directly over Tauri IPC
