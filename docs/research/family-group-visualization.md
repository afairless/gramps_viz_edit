# Family Group Visualization Tool

## Rust vs TypeScript Assessment

Building an interactive force-directed graph visualization from scratch in
**pure Rust** (egui, iced, bevy) is impractical. No Rust GUI framework ships a
mature force-directed graph renderer with hover tooltips, multi-select, and
smooth animations.  You would be implementing a physics simulation, canvas/SVG
renderer, hit-testing, and zoom/pan from scratch — easily weeks of work for a
suboptimal result.

A **pure TypeScript** (Electron/Node) approach would give you D3.js, vis-network,
and cytoscape.js — all battle-tested for this exact use case — but would require
a separate project, build system, and runtime.  All the Gramps data processing
(XML parsing, DSU component building, generation layering, date handling) would
need to be reimplemented in TypeScript, discarding the existing Rust investment.

**Tauri** is the pragmatic middle ground: Rust handles data processing (reusing
the existing `.gramps` XML parsing, DSU, and generation-layering code
extracted from the `cli` crate into a shared `gramps-reader` library crate)
while the web frontend uses D3.js for visualization.  The result is a native
desktop app integrated into the same Cargo workspace, sharing types and
dependencies.

> **Note on code reuse:** The `cli` crate's streaming XML parser (`strip_prefix`,
> `read_handle_attr`, `read_hlink_attr`), `Dsu` disjoint-set union, and
> `compute_generation_table` layering algorithm currently live as private
> functions in `crates/cli/src/commands/stats/count.rs`.  These will be
> extracted into a new `crates/gramps-reader/` library crate that both `cli`
> and `visualize` depend on — avoiding an architecturally inverted dependency
> where a library crate depends on a binary crate.

| Aspect | Pure Rust | Pure TypeScript | Tauri (Rust + Web) |
|---|---|---|---|
| Force-directed graph | Build from scratch | D3.js, vis-network, cytoscape | D3.js via webview |
| Gramps data parsing | Already done | Reimplement | Reuse `cli` crate |
| Desktop integration | Native | Electron (heavy) | Native webview (light) |
| Workspace integration | Trivial | Separate project | Same workspace |
| Development effort | Very high | Medium | Medium-low |
| Result quality | Mediocre | High | High |

**Recommendation:** Tauri v2 with D3.js frontend.

---

## Architecture

```
crates/gramps-reader/         # NEW: shared .gramps XML parsing (extracted from cli)
├── Cargo.toml                # depends on quick-xml, serde
└── src/
    ├── lib.rs                # Re-exports
    ├── types.rs              # ParsedPerson, ParsedFamily, FamilyRecord structs
    ├── xml.rs                # strip_prefix, read_handle_attr, read_hlink_attr
    ├── xml/                  # XML extraction modules
    │   ├── count.rs          # count_gramps_xml (streaming stats, moved from cli)
    │   └── extract.rs        # Streaming person/family detail extraction (new)
    └── graph.rs              # Dsu, compute_generation_table, cycle detection

crates/visualize/
├── Cargo.toml              # depends on gramps-reader, tauri, serde
├── tauri.conf.json         # Tauri v2 config (see CSP config below)
├── build.rs                # tauri-build (bundles frontend/dist/)
├── src/
│   ├── main.rs             # Tauri entry point: build data, launch window
│   ├── graph_data.rs       # Adapter: ParsedPerson/Family → PersonNode + FamilyLink
│   ├── dates.rs            # Imputed-date algorithm
│   └── lib.rs              # Pure fn load_graph_data (unit-testable, IPC-agnostic)
└── frontend/               # Web frontend (built via npm → dist/)
    ├── index.html
    ├── package.json         # d3 v7, typescript, vitest (dev)
    ├── tsconfig.json
    ├── src/
    │   ├── main.ts         # Entry point, mounts D3
    │   ├── graph.ts        # Force simulation, rendering, zoom/pan
    │   ├── tooltip.ts      # Hover tooltip (name, birth, death)
    │   ├── selection.ts    # Click-to-select, multi-select, export
    │   ├── colors.ts       # Birth-date → color gradient mapping
    │   └── types.ts        # TypeScript interfaces matching Rust GraphData
    ├── tests/              # Frontend unit tests (vitest)
    │   ├── colors.test.ts
    │   ├── selection.test.ts
    │   └── graph.test.ts
    └── styles/
        └── main.css
```

### Data Flow

```
.gramps file
    │
    ▼
┌─────────────────────────────┐
│  gramps-reader: xml/        │  Streaming XML parse (shared crate):
│  extract.rs (new)           │  - Person: handle, name, birth, death, gender
│  + xml.rs helpers           │  - Family: father, mother, childref links
│  (strip_prefix,              │  Structural extraction only — no imputation.
│   read_handle_attr,          │  Produces ParsedPerson[], ParsedFamily[].
│   read_hlink_attr)           │
└──────────┬──────────────────┘
           │  ParsedPerson[], ParsedFamily[]
           ▼
┌─────────────────────────────┐
│  gramps-reader: graph.rs    │  DSU → connected components
│  + visualize: graph_data.rs │  Generation layering per component
│  (Dsu, compute_generation   │  Adapter: ParsedPerson/Family → PersonNode + FamilyLink
│   _table from shared crate) │  graph_data.rs is a thin adapter, not a parser.
└──────────┬──────────────────┘
           │  PersonNode[], FamilyLink[]
           ▼
┌─────────────────────────────┐
│  Rust: dates.rs             │  For nodes without birth_date:
│                             │  - Multi-source BFS from all dated nodes
│                             │  - Propagate with configurable generation gap (default: 25 years)
│                             │  - Fallback: neutral null sentinel
└──────────┬──────────────────┘
           │  GraphData (JSON via Tauri command)
           ▼
┌─────────────────────────────┐
│  Tauri IPC bridge           │  serde_json serialization
│                             │  invoke("load_graph", { path }) → GraphData
│                             │  invoke("export_selections", { ... })
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│  Frontend: D3.js            │  Force simulation with collision detection
│                             │  SVG rendering with zoom/pan
│                             │  Hover tooltips on <title> or HTML overlay
│                             │  Click-to-toggle selection with visual highlight
│                             │  Color gradient: birth year → d3.interpolateViridis (perceptually
│                             │  uniform, colorblind-friendly)
└─────────────────────────────┘
```

### Data Model (Rust → JSON → TypeScript)

```rust
// --- gramps-reader types (shared library crate) ---

/// Raw extracted person data from streaming XML parse.
/// Produced by gramps-reader::xml::extract; consumed by visualize::graph_data.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPerson {
    pub handle: String,
    pub given_name: Option<String>,
    pub surname: Option<String>,
    pub birth_date: Option<String>,    // display text from <dateval>
    pub death_date: Option<String>,
    pub birth_year: Option<i32>,       // parsed from dateval year attribute
    pub gender: Option<String>,        // "M", "F", or "U" (raw Gramps text)
}

/// Raw extracted family data from streaming XML parse.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFamily {
    pub handle: String,
    pub father_handle: Option<String>,
    pub mother_handle: Option<String>,
    pub child_handles: Vec<String>,
}

// --- visualize types (serialized to frontend via Tauri IPC) ---

#[derive(Serialize)]
struct GraphData {
    nodes: Vec<PersonNode>,
    links: Vec<FamilyLink>,
    family_groups: Vec<FamilyGroupMeta>,
}

#[derive(Serialize)]
struct PersonNode {
    handle: String,              // Gramps handle (e.g., "p0001")
    name: String,                // Display name (given + surname)
    birth_date: Option<String>,  // Display text (e.g., "1850-03-15")
    death_date: Option<String>,  // Display text
    birth_year: Option<i32>,     // For color gradient (explicit or imputed)
    is_imputed: bool,            // true if birth_year came from imputation
    gender: String,              // "male", "female", or "unknown" (consumer-friendly)
    family_group: usize,         // Which connected component
    generation: usize,           // Generation level within component
}

#[derive(Serialize)]
struct FamilyLink {
    source: String,           // PersonNode.handle
    target: String,           // PersonNode.handle
    link_type: LinkType,      // Spouse | ParentChild
}

#[derive(Serialize)]
enum LinkType { Spouse, ParentChild }

#[derive(Serialize)]
struct FamilyGroupMeta {
    id: usize,
    size: usize,
    span: usize,              // generation span
}

#[derive(Serialize)]
struct SelectionExport {
    exported_at: String,
    file: String,
    selections: Vec<SelectedPerson>,
}

#[derive(Serialize)]
struct SelectedPerson {
    handle: String,
    name: String,
    birth_date: Option<String>,
    death_date: Option<String>,
    gender: String,
    family_group: usize,
}
```

### Imputed Date Algorithm

The generation gap is **configurable** (default 25 years, set via
`--generation-gap`).  This is an approximation that works well for typical
genealogical data but may be inaccurate for real datasets with unusual
parent-child age gaps.

```

For each family group (connected component):

  1. Assign generation numbers using the existing longest-path layering
     (reuse compute_generation_table's algorithm).
  2. Multi-source BFS from all dated nodes simultaneously (O(V+E)):
     a. Initialize a queue with every node that has a known birth_year.
     b. For each visited undated node, compute:
        imputed_year = source_dated_year + (gen_diff × gap)
        where gen_diff = dated_node.generation - undated_node.generation
        and gap is the configurable years-per-generation (default 25).
        (-gap years per generation going backward in time, +gap going forward).
     c. If a node is reached from multiple dated sources at the same distance,
        average their imputed years.
  3. Nodes with no reachable dated node → birth_year = None → neutral color.

```

### Color Gradient

- Use `d3.interpolateViridis` mapped over the range of birth years
  in the graph.  Viridis is perceptually uniform and colorblind-friendly,
  making it the better default for data visualization.  (A `--color-scheme`
  CLI flag can offer `d3.interpolateWarm` as an alternative for users who
  prefer an intuitive cool→warm aging gradient.)
- Undated-imputed nodes use the same gradient but get a subtle visual
  indicator (e.g., dashed border or slightly reduced opacity).
- Completely undated nodes get a neutral gray (`#999`).

### CLI Integration

```bash
gramps-gen visualize <file>              # Open the visualization in a Tauri window
gramps-gen visualize <file> --no-impute  # Skip date imputation; use explicit dates only
gramps-gen visualize <file> --generation-gap 30  # Custom years-per-generation
```

| Flag | Default | Description |
|---|---|---|
| `<file>` | *(required)* | Path to a `.gramps` file (canonicalized, must have `.gramps` extension) |
| `--no-impute` | false | Skip the imputed-date algorithm; undated nodes get neutral color |
| `--generation-gap` | 25 | Years per generation for date imputation (validated: 1–100) |

The `visualize` subcommand in the existing `cli` crate:

1. Canonicalizes and validates the file path (exists, readable, `.gramps` extension).
2. Locates the `gramps-gen-visualize` binary alongside the current executable
   via `std::env::current_exe()` (both installed to the same `bin/` directory).
3. Spawns the Tauri binary with the canonical path and flags as arguments.
4. Validates `--generation-gap` is in range 1–100 before passing it through.

Since Tauri apps must own `main()`, we adopt a two-binary approach:

- `gramps-gen` (existing CLI) gains a `visualize` subcommand that
  locates and spawns the `gramps-gen-visualize` sibling binary.
- `gramps-gen-visualize` is the Tauri app binary.

Both binaries appear under `[[bin]]` in the workspace root `Cargo.toml`,
so `cargo install --path .` installs both to `~/.cargo/bin/`.

For development, run the visualize binary directly:
`cargo run -p visualize -- <file> [flags]`.

**Workspace change:** `Cargo.toml` must add `"crates/visualize"` and
`"crates/gramps-reader"` to the `members` list, plus `[[bin]]` entries
for both binaries.

### Build Dependencies (new)

| Dependency | Version | Purpose |
|---|---|---|
| **Node.js + npm** | ≥18 | Frontend build (TypeScript compilation, D3 bundling) |
| **WebKit2GTK** | (system) | Tauri v2 webview on Linux (`libwebkit2gtk-4.1-dev`) |
| **WebView2** | (system) | Tauri v2 webview on Windows (pre-installed on Win 10+) |

### Rust Dependencies (new)

| Crate | Purpose |
|---|---|
| `tauri` v2 | Desktop app shell, webview, IPC |
| `tauri-build` v2 | Build-time resource bundling (reads `frontend/dist/`) |
| `serde` / `serde_json` | Data serialization (already in workspace) |
| Frontend: `d3` v7 | Force simulation, SVG rendering, colors |
| Frontend: `typescript` | Type-safe frontend code |
| Frontend: `vitest` (dev) | Frontend unit test runner |

The `visualize` crate depends on `gramps-reader` (path) to reuse:

- `strip_prefix`, `read_handle_attr`, `read_hlink_attr` (XML helpers)
- `Dsu` (disjoint-set union for connected components)
- `compute_generation_table` (longest-path generation layering)
- `MAX_GENERATION` constant and cycle detection
- `ParsedPerson`, `ParsedFamily` (data structs, defined in `types.rs`)
- `FamilyRecord` (moved from `cli::commands::stats::count`)

The `cli` crate is also refactored to depend on `gramps-reader` instead of
keeping these functions private in `stats/count.rs`.

**Optional feature:** The `visualize` crate is gated behind a Cargo feature
(`--features visualize`) so the core `gramps-gen` CLI can build without
Tauri's system dependencies (WebKit2GTK / WebView2). The workspace
`default-members` excludes `visualize`; CI builds it explicitly.

### Error Handling

| Layer | Error | User-Visible Behavior |
|---|---|---|
| File I/O | File not found, permission denied | Tauri dialog with error message |
| File I/O | Path not a `.gramps` file, path traversal attempt | Tauri dialog: "Invalid file path" |
| File I/O | `--generation-gap` out of range (not 1–100) | CLI error message before spawning Tauri binary |
| XML parse | Malformed `.gramps` | Tauri dialog: "Not a valid Gramps XML file" + details |
| XML parse | Empty file (0 people) | Graph renders with empty canvas + "No people found" message |
| DSU / layering | Cycles in family graph | Warning logged; layering clamped at `MAX_GENERATION` (same as `stats` command) |
| Date imputation | No reachable dated node | `birth_year = None` → neutral color (graceful degradation) |
| Tauri IPC | Serialization failure | Frontend catches error, shows toast notification |
| Frontend | D3 simulation error | Error boundary in UI, graph area shows error message |

### Testing Strategy

Following the project's existing conventions (unit tests in `#[cfg(test)] mod tests`,
integration tests in `tests/`):

**Unit tests — `crates/gramps-reader/`**

| Module | What to test |
|---|---|
| `xml.rs` | `strip_prefix` with/without prefix, `read_handle_attr` with plain and namespace-prefixed attributes, `read_hlink_attr` same, self-closing elements |
| `xml/extract.rs` | Parse person with all fields, person with minimal fields (no name/date), family with parents + children, empty family, namespace-prefixed elements, malformed XML, missing gender, unparseable birth year |
| `graph.rs` | DSU singleton, DSU union, component grouping, generation layering for chains/isolates/cycles, cycle detection, dangling ref handling |

**Unit tests — `crates/visualize/`**

| Module | What to test |
|---|---|
| `graph_data.rs` | Adapter: ParsedPerson → PersonNode (gender mapping M→male/F→female/null→unknown, name assembly from given+surname, handle empty name), FamilyLink construction from ParsedFamily, connected component grouping, generation assignment |
| `dates.rs` | Imputation with known ancestor, imputation with known descendant, multi-source BFS from all dated nodes, averaging multiple candidates, no reachable dated node, custom generation gap, all-nodes-dated (no-op), generation gap 0 (all equal within component) |
| `lib.rs` | `load_graph_data(path, no_impute, gap) → Result<GraphData>` pure function: valid file, empty file, malformed file, file not found, generation gap validation |

**Property-based tests — `crates/visualize/`**

| Module | Invariant |
|---|---|
| `dates.rs` | Ancestor imputed year ≤ descendant imputed year for any parent→child edge |
| `dates.rs` | Imputation is deterministic (same input → same output) |
| `dates.rs` | Fully-dated graph is unchanged by imputation (all `is_imputed: false`) |
| `graph_data.rs` | Every PersonNode has a generation within `[0, MAX_GENERATION]` |

**Frontend unit tests — `crates/visualize/frontend/tests/`**

Run via `npx vitest run` (no browser needed — logic-only tests):

| File | What to test |
|---|---|
| `colors.test.ts` | Birth year range → viridis color mapping, null year → neutral gray, single-year range (all same color), edge cases (year 0, negative year) |
| `selection.test.ts` | Toggle select/deselect, Shift-click multi-select, export data shape, clear selection, empty selection export |
| `graph.test.ts` | Node/link transform functions, force simulation configuration shape, graph data validation against `types.ts` interface |

**Integration tests — `crates/visualize/tests/`**

- Round-trip: known `.gramps` file → parse → build GraphData → serialize to JSON → verify shape
- Empty `.gramps` file produces empty nodes/links arrays
- `.gramps` with cycles produces valid (capped) generation data + warning
- GraphData JSON schema matches TypeScript `types.ts` interface (structural shape check)
- `load_graph_data()` with missing file returns error, not panic

**E2E tests**

- Subprocess-based: `gramps-gen visualize --no-impute <fixture.gramps>` locates sibling
  binary and spawns it without error (binary existence check).
- Tauri window rendering is not tested headlessly (requires Xvfb/Wayland); data processing
  is fully covered up to the IPC boundary by unit and integration tests.

### Known Limitations

- **IPC payload size:** The entire `GraphData` (nodes + links) is sent as one JSON blob
  over Tauri IPC.  For very large datasets (10,000+ people) this may cause multi-second
  serialization latency and WebView memory pressure.  The target scale is <10,000 persons;
  progressive loading via Tauri events is a future enhancement.
- **Not a Gramps editor:** This tool is read-only.  Selections can be exported but
  changes are not written back to the `.gramps` file.
- **Date imputation is approximate:** The 25-year generation gap is a heuristic;
  real genealogical data may have significant variance.
- **Platform dependencies:** Tauri v2 requires WebKit2GTK on Linux (`libwebkit2gtk-4.1-dev`)
  and WebView2 on Windows (pre-installed on Win 10+).  The `visualize` crate is gated
  behind an optional Cargo feature so the core CLI remains buildable without these.
- **Content Security Policy:** The Tauri webview CSP is set in `tauri.conf.json`:
  `"csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'"`.
  D3 may inject inline styles for node positioning; `'unsafe-inline'` on styles is
  required for correct rendering.  All scripts and resources are loaded from the
  bundled `frontend/dist/` directory only.
- **Frontend build required before `cargo build`:** The Tauri build expects compiled
  frontend assets in `frontend/dist/`.  Run `npm install && npm run build` in
  `crates/visualize/frontend/` before building the Rust crate.

### Frontend Interaction Design

1. **Load:** Window opens, graph renders with force simulation.  Simulation
   settles over ~2 seconds.

2. **Pan/Zoom:** Mouse drag to pan, scroll wheel to zoom.  SVG `viewBox`
   transform.

3. **Hover:** When cursor rests on a node (≥200ms), a tooltip appears showing:

   ```
   John Smith
   Born: 1850-03-15
   Died: 1920-07-01
   ```

   Tooltip follows cursor, hides on mouse-out.

4. **Select:** Click a node to toggle selection.  Selected nodes get a
   highlighted ring (stroke width +3, contrasting color).  Click again to
   deselect.  Hold Shift to add to selection without toggling others.

5. **Selection panel:** A sidebar (or bottom bar) shows count of selected
   nodes and an "Export Selected" button.

6. **Export:** Clicking "Export" opens a native save dialog (Tauri API).
   Writes JSON with full details for each selected person.

7. **Family group filter:** Dropdown to filter the view to a single family
   group (connected component).  "All groups" is the default.

8. **Legend:** Color gradient bar showing year range, with labels.  Dashed
   border indicator for imputed dates explained in a caption.

### Implementation Order

Each step is a single logical, testable unit following the incremental-development
workflow (implement → test → verify → commit).  Steps marked with `[GR]` touch the
new `gramps-reader` crate; steps marked with `[CLI]` touch the existing `cli` crate.

| Step | Crate | Description |
|---|---|---|
| 1 | workspace | **Extract shared code.** Move `strip_prefix`, `read_handle_attr`, `read_hlink_attr`, `FamilyRecord`, `Dsu`, `compute_generation_table`, `MAX_GENERATION`, cycle detection, and `count_gramps_xml` from `cli::commands::stats::count` into new `crates/gramps-reader/`.  `gramps-reader/src/xml.rs` holds attribute helpers; `gramps-reader/src/xml/count.rs` holds the streaming counter (moved verbatim); `gramps-reader/src/graph.rs` holds DSU + layering; `gramps-reader/src/types.rs` holds `FamilyRecord`.  Fix the `"handle` suffix check in `read_handle_attr` (it is unreachable dead code).  Update `cli` to depend on `gramps-reader`.  All existing tests must pass. |
| 2 | workspace | **Scaffold `crates/visualize/`.** Create Tauri v2 crate skeleton: `Cargo.toml` (depends on `gramps-reader`, `tauri`, `serde`; gated behind Cargo feature `visualize`), `tauri.conf.json` (CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'`, `frontendDist: "../frontend/dist"`), `build.rs` (tauri-build), `src/main.rs` with placeholder window.  Add both new crates to workspace `members`.  Set up `frontend/` directory with `package.json` (d3 v7, typescript, vitest), `tsconfig.json`, and build script.  Tauri window opens and closes without crashing. |
| 3 | visualize | **Wire CLI passthrough.** Add `visualize` subcommand to `cli` (`gramps-gen visualize <file> [flags]`).  CLI canonicalizes path, validates file existence + `.gramps` extension, validates `--generation-gap` ∈ [1, 100], locates sibling `gramps-gen-visualize` binary via `current_exe()`, spawns it with canonical path + flags.  Add `[[bin]]` entry for `gramps-gen-visualize` in workspace root `Cargo.toml`. |
| 4 | gramps-reader | **Add person detail extraction.** Define `ParsedPerson` in `gramps-reader/src/types.rs` (handle, given_name, surname, birth_date, death_date, birth_year, gender).  Implement new streaming extractor in `gramps-reader/src/xml/extract.rs`: reads `<person>` elements fully — extracts `<name>/<first>`, `<name>/<surname>`, `<birth>/<dateval>`, `<death>/<dateval>`, `<gender>` text.  This is *new* parser logic (the existing `count.rs` only reads attributes); reuse `strip_prefix`/`read_handle_attr` helpers. |
| 5 | gramps-reader | **Add family detail extraction.** Define `ParsedFamily` in `gramps-reader/src/types.rs` (handle, father_handle, mother_handle, child_handles).  Extend `xml/extract.rs` to read `<family>` elements: father/mother `hlink` attributes, `childref` `hlink` attributes.  Reuses `read_hlink_attr` helper. |
| 6 | visualize | **Build graph data.** `graph_data.rs`: thin adapter that converts `ParsedPerson[]`/`ParsedFamily[]` → `Vec<PersonNode>` (gender mapping M→"male"/F→"female"/null→"unknown", name assembly from given+surname), `Vec<FamilyLink>` (spouse + parent-child).  Run DSU over parsed families → connected components, run generation layering via `gramps-reader::graph`. |
| 7 | visualize | **Implement date imputation.** `dates.rs`: for each family group, multi-source BFS from all dated nodes simultaneously (O(V+E)), propagate with configurable gap, average ties, null fallback.  Include property-based tests for ancestor≤descendant invariant and determinism. |
| 8 | visualize | **Wire Tauri IPC.** Implement pure function `load_graph_data(path, no_impute, generation_gap) -> Result<GraphData>` in `lib.rs` (unit-testable, IPC-agnostic).  Define thin `#[tauri::command] fn load_graph(...)` wrapper in `main.rs` that delegates to `load_graph_data` and handles errors → Tauri dialogs.  Frontend calls `invoke("load_graph", ...)` and logs the result. |
| 9 | frontend | **Set up frontend toolchain.** Verify `npm install` succeeds, `npx tsc --noEmit` passes, `npm run build` produces `frontend/dist/` with bundled JS.  Add `npm run build` as a build prerequisite documented in the visualize crate README.  Verify `npx vitest run` executes the placeholder test file. |
| 10 | frontend | **HTML scaffold + D3 force simulation.** `index.html`, `main.ts` mount point, `graph.ts` with `d3.forceSimulation()` over received nodes/links.  Nodes render as circles, links as lines.  Frontend unit tests in `tests/graph.test.ts` for node/link transform and data validation. |
| 11 | frontend | **Add hover tooltips.** `tooltip.ts`: HTML overlay tooltip on 200ms hover showing name, birth date, death date.  Follows cursor, hides on mouse-out. |
| 12 | frontend | **Add click-to-select.** `selection.ts`: click toggles node highlight (stroke), Shift-click for multi-select.  Selection panel shows count + Export button.  Export opens Tauri save dialog.  Frontend unit tests in `tests/selection.test.ts` for toggle/add/remove/export logic. |
| 13 | frontend | **Add color gradient, filter, legend.** `colors.ts`: `d3.interpolateViridis` over birth year range.  Imputed nodes get dashed border.  Family group dropdown filter.  Legend bar with year labels.  Frontend unit tests in `tests/colors.test.ts` for color mapping and edge cases. |
| 14 | workspace | **Cross-crate integration tests.** `crates/visualize/tests/`: round-trip from `.gramps` file → `load_graph_data` → JSON → verify shape.  Empty file, cycles, schema match against TypeScript `types.ts`.  IPC error handling: missing file, malformed XML. |
| 15 | workspace | **E2E smoke test + documentation.** Subprocess test: `gramps-gen visualize --no-impute <fixture.gramps>` locates sibling binary and spawns without error.  Update `README.md` with visualize subcommand usage, new build dependencies (Node.js, WebKit2GTK), and `--features visualize` gate.  Update `docs/ARCHITECTURE.md` with new crates. |
