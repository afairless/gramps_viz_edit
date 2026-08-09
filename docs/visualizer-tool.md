# Visualizer Tool User Guide

The `gramps-gen visualize` command opens an interactive desktop application
for exploring family trees as a **force-directed graph**. It uses Tauri v2 for
the desktop shell and D3.js for rendering.

## Table of Contents

- [Quick Start](#quick-start)
- [How the Visualizer Works](#how-the-visualizer-works)
- [Understanding the Graph](#understanding-the-graph)
  - [Nodes](#nodes)
  - [Edges](#edges)
- [Command Reference](#command-reference)
  - [Arguments](#arguments)
  - [Options](#options)
- [Interaction Guide](#interaction-guide)
  - [Zoom and Pan](#zoom-and-pan)
  - [Tooltips](#tooltips)
  - [Selection](#selection)
- [Selection Modes](#selection-modes)
- [Family Group Filter](#family-group-filter)
- [Force Layout Tuning](#force-layout-tuning)
- [Legend](#legend)
- [Stats Panel](#stats-panel)
- [Date Imputation](#date-imputation)
- [Build Dependencies](#build-dependencies)
- [Frontend Build](#frontend-build)
- [Feature Gate](#feature-gate)
- [Two-Binary Architecture](#two-binary-architecture)
- [Tips and Common Workflows](#tips-and-common-workflows)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

```bash
# Open a specific .gramps file (auto-loads on launch)
cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps

# Open without a file (welcome screen with "Open Gramps File" button)
cargo run -p visualize -F visualize

# Via the CLI subcommand (requires both binaries built)
cargo build -p cli
cargo build -p visualize -F visualize
cargo run -p cli -- visualize ~/Documents/gramps01/exp01.gramps

# Skip date imputation (use only explicit dates)
cargo run -p visualize -F visualize -- data.gramps --no-impute

# Custom generation gap for date imputation
cargo run -p visualize -F visualize -- data.gramps --generation-gap 30
```

The file path is a **positional argument** — no `--path` flag needed. Flags
can appear before or after the file path.

---

## How the Visualizer Works

The visualizer runs a multi-stage data pipeline before rendering:

```
.gramps file
    │
    ▼
┌────────────────────────────┐
│  gramps-reader (parse)     │  Streaming XML → ParsedPerson[], ParsedFamily[]
└──────────┬─────────────────┘
           │
           ▼
┌────────────────────────────┐
│  graph_data.rs             │  DSU components, generation layering,
│  build_graph_data()        │  gender mapping, FamilyLink construction
└──────────┬─────────────────┘
           │  PersonNode[], FamilyLink[]
           ▼
┌────────────────────────────┐
│  dates.rs                  │  Multi-source BFS date imputation:
│  impute_dates()            │  - Propagate from dated nodes with
│                            │    configurable gap (default 25 years)
│                            │  - Average ties from multiple sources
│                            │  - Null fallback for unreachable nodes
└──────────┬─────────────────┘
           │  GraphData (JSON)
           ▼
┌────────────────────────────┐
│  Tauri IPC bridge          │  #[tauri::command] load_graph()
│  main.rs                   │  → Result<GraphData, String>
└──────────┬─────────────────┘
           │  JSON via invoke()
           ▼
┌────────────────────────────┐
│  Frontend (D3.js + TS)     │  Force simulation, SVG rendering,
│                            │  zoom/pan, tooltips, selection
└────────────────────────────┘
```

### Data model

The pipeline produces a `GraphData` structure with three arrays:

- **nodes**: `PersonNode[]` — each person with name, birth/death dates,
  birth year, gender, family group index, and generation offset
- **links**: `FamilyLink[]` — spouse and parent-child edges between people
- **family_groups**: `FamilyGroupMeta[]` — metadata for each connected component

---

## Understanding the Graph

### Nodes

Each person is rendered as a **circle**:

| Node appearance | Meaning |
|---|---|
| Filled circle, viridis color | Node has a birth year — color maps to year |
| Dashed border | Birth year is **imputed** (not present in the source file) |
| Neutral gray | Birth year is **unknown** (not in source and not reachable for imputation) |

### Edges

| Edge type | Visual style | Meaning |
|---|---|---|
| **Spouse** | Solid line | Two people are married (share a family) |
| **Parent-Child** | Dashed line | Person is a child of a family |

### Color Gradient

Nodes are colored using `d3.interpolateViridis`, a **perceptually uniform,
colorblind-friendly** color scale. Earlier birth years map to purple/blue;
later birth years map to yellow/green. The legend shows the full year range.

---

## Command Reference

```text
gramps-gen visualize [OPTIONS] [FILE]
```

### Arguments

| Argument | Description |
|---|---|
| `FILE` | Path to a `.gramps` file to open on launch. Omit for welcome screen. |

### Options

| Option | Default | Description |
|---|---|---|
| `--no-impute` | `false` | Disable BFS date imputation — only explicit dates are used |
| `--generation-gap <YEARS>` | `25` | Years per generation for date imputation (valid range: 1–100) |

Both flags are forwarded to the visualization backend and sent to the
frontend via Tauri IPC. They can appear before or after the file path.

---

## Interaction Guide

### Zoom and Pan

- **Scroll** to zoom in/out
- **Drag** (click and drag on empty space) to pan the view
- The view auto-centers on the loaded graph

### Tooltips

Hover over any node for **200ms** to see a tooltip showing:

- Full name
- Birth date (with "imputed" label if applicable)
- Death date

The tooltip follows the cursor and disappears on mouse-out.

### Selection

- **Click** a node to toggle its selection (blue highlight)
- **Shift-click** additional nodes for multi-selection
- **Export selections** button saves the current selection as JSON via a
  Tauri native save dialog

---

## Selection Modes

The selection panel provides adjacency-based mass selection modes, all
operating on the **currently selected** nodes as seeds:

| Mode | Selects |
|---|---|
| **Ancestors** | All ancestors (parents, grandparents, etc.) reachable via parent-child links |
| **Descendants** | All descendants (children, grandchildren, etc.) reachable via parent-child links |
| **First-degree** | Spouses and immediate children of selected nodes |
| **Second-degree** | First-degree connections plus their first-degree connections |
| **Indirect connected set** | All nodes reachable from selected nodes through any path of edges |
| **Invert selection** | Swap selection state across all nodes in the current filter view |

Selection operations chain: select a seed node → run ancestors → run
first-degree → you now have a multi-generation family group selected.

---

## Family Group Filter

A dropdown at the top of the window lists all **connected components**
(family groups) in the graph. Selecting one restricts the view to only
that component.

This is useful for large datasets — filter to a single family group to
reduce visual clutter and focus on one lineage.

The filter label shows the component index and its size (e.g., "Group 3
(47 persons)").

---

## Force Layout Tuning

Three sliders below the graph control the D3 force simulation parameters:

| Slider | Force parameter | Effect |
|---|---|---|
| **Generation Pull** | `generationPull` | Pulls nodes toward their generation's Y-position layer. Higher = tighter horizontal bands. |
| **Spouse Strength** | `spouseStrength` | Attraction strength between spouses. Higher = spouses closer together. |
| **Parent-Child Strength** | `parentChildStrength` | Attraction strength between parents and children. Higher = families more clustered. |

Adjust these sliders to change the layout. Changes take effect immediately
— the force simulation re-heats and settles into a new configuration.

A **freeze** checkbox pauses the simulation, freezing node positions for
close inspection or screenshot.

---

## Legend

A color gradient bar at the top (or side) of the window shows:

- The **birth year range** (earliest → latest) with labeled tick marks
- **Undated** caption — gray nodes have no known or imputed birth year
- **Imputed** caption — dashed-border nodes have estimated birth years

---

## Stats Panel

A collapsible **right sidebar** shows file-level statistics:

- Total persons, families, events, places
- Number of sources, citations, repositories
- Media and note counts
- File path and schema version

Click the toggle button to show/hide the panel.

---

## Date Imputation

For nodes without an explicit birth date, the visualizer runs a
**multi-source BFS** to estimate birth years:

1. Initialize a queue with every node that has a known birth year
2. For each visited undated node: `imputed_year = source_year + (gen_diff × gap)`
3. If reached from multiple sources at the same distance, average the
   candidates
4. Nodes with no reachable dated node → `birth_year = None` → neutral gray

The generation gap is configurable via `--generation-gap` (default: 25,
valid range: 1–100). Use `--no-impute` to disable imputation entirely —
only explicit dates from the `.gramps` file are used.

---

## Build Dependencies

Building the Tauri app requires additional system libraries:

**Debian/Ubuntu:**

```bash
sudo apt install libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config nodejs npm
```

**Fedora:**

```bash
sudo dnf install webkit2gtk4.1-devel dbus-devel pkgconf pkg-config nodejs npm
```

---

## Frontend Build

The frontend assets must be built before the Tauri binary:

```bash
cd crates/visualize/frontend
npm install
npm run build
cd ../../..
cargo build -p visualize -F visualize --release
```

The `npm run build` step compiles TypeScript and bundles the D3.js
application into static assets that Tauri serves in its webview.

---

## Feature Gate

The `visualize` crate is gated behind the `visualize` Cargo feature so
the core CLI can build without system webview dependencies:

```bash
# Build core CLI only (no system deps required)
cargo build --release

# Build with visualization (requires system deps)
cargo build -p visualize -F visualize --release
```

This means `gramps-gen` and `gramps-gen-visualize` are separate binaries.

---

## Two-Binary Architecture

| Binary | Purpose | Build command |
|---|---|---|
| `gramps-gen` | Core CLI with `visualize` subcommand that spawns the viz binary | `cargo install --path .` |
| `gramps-gen-visualize` | Tauri desktop app binary | `cargo install -p visualize -F visualize --path crates/visualize` |

Both binaries are installed to the same directory. The CLI `visualize`
subcommand locates and spawns the sibling binary automatically.

---

## Tips and Common Workflows

### Exploring a large dataset

```bash
# Open with imputation for maximum node coloring
cargo run -p visualize -F visualize -- large.gramps --generation-gap 25

# Filter to a single family group using the dropdown
# Then adjust force sliders for a clean layout
```

### Inspecting a specific family

```bash
cargo run -p visualize -F visualize -- data.gramps --no-impute
# Select a person → use "Descendants" to find all children/grandchildren
# Use "Ancestors" to trace lineage upward
```

### Exporting selections for further processing

```bash
# In the visualizer: select nodes of interest → Export button → save as JSON
# Then pass the JSON to the delete or integrate tools:
gramps-gen delete data.gramps --selections exported-selections.json --dry-run
gramps-gen integrate diff-viz --diff changes.csv --selections exported-selections.json
```

### Taking a screenshot of a clean layout

```bash
# 1. Filter to the family group of interest
# 2. Tune force sliders until layout is clean
# 3. Check "freeze" to stop the simulation
# 4. Use your OS screenshot tool
```

---

## Troubleshooting

### Blank screen / white window

The most common cause is a missing or stale frontend build. Rebuild:

```bash
cd crates/visualize/frontend && npm install && npm run build && cd ../../..
cargo build -p visualize -F visualize
```

### "file not found" or parse error

The tool could not read the `.gramps` file. Verify the path and that the
file is valid Gramps XML. The tool handles gzip-compressed `.gramps` files
automatically.

### Missing system libraries

If the build fails with errors about `webkit2gtk-4.1` or `dbus-1`, install
the system dependencies listed in [Build Dependencies](#build-dependencies).

### Graph is all gray (no colors)

All nodes have unknown birth years and imputation is either disabled
(`--no-impute`) or found no reachable dated nodes. Enable imputation or
verify the source file contains date information.

### Force layout is jittery/unstable

Reduce the alpha decay or check the "freeze" checkbox to stop the
simulation. Adjust the force sliders to reduce attraction strength if
nodes are oscillating.

### Visualization doesn't start from CLI subcommand

Both binaries must be built and in the same directory:

```bash
cargo build -p cli
cargo build -p visualize -F visualize
# The visualize subcommand looks for gramps-gen-visualize next to gramps-gen
```

### "npm: command not found"

Install Node.js and npm via your system package manager or
[nvm](https://github.com/nvm-sh/nvm). Required for the frontend build step.
