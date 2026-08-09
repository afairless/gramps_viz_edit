# Gramps Data Generator

A tool that generates valid, plausible [Gramps](https://gramps-project.org/) family tree datasets for testing and development.

The tool models the Gramps database as a **typed directed multigraph**, supports both random and configurable scenario-driven generation, applies adversarial transforms for stress-testing, and outputs Gramps XML (`.gramps` format) for direct import into the Gramps desktop application.

## Installation

```bash
# Clone and build
git clone <repo-url> && cd gramps_viz_edit
cargo build --release
```

The binary is at `target/release/gramps-gen`.

Or install globally:

```bash
# Core CLI (gramps-gen)
cargo install --path .

# With visualization (gramps-gen-visualize, requires system deps - see below)
cargo install -p visualize -F visualize --path crates/visualize
```

## Usage

### Basic generation

```bash
# 200 persons, 3 generations
gramps-gen generate --count 200 --output family.gramps

# Reproducible output with a seed
gramps-gen generate --count 1000 --seed 42 --output reproducible.gramps

# Control generation depth
gramps-gen generate --count 500 --depth 5 --output deep-tree.gramps
```

### With optional features (data content)

```bash
gramps-gen generate --count 500 \
  --with-places --with-citations --with-notes \
  --with-media --with-tags
```

### Schema versions

`gramps-gen` supports Gramps 5.1 and 5.2 schema versions out of the box.
Both schemas are compiled in by default for transparent auto-detection —
you don't need to pass `--features` flags at build time. The tool
automatically detects the schema version from each file's XML header.

```bash
# Build (includes both 5.1 and 5.2)
cargo build

# Build with only a specific version (smaller binary)
cargo build --no-default-features --features schema-5-2

# Both versions work without --features flags
gramps-gen diff archive_v5_1.gramps archive_v5_2.gramps
```

### Adversarial datasets

Test your downstream tools against unusual-but-valid family structures:

```bash
# All adversarial strategies
gramps-gen generate --count 100 --adversarial all

# Specific strategies
gramps-gen generate --count 100 \
  --adversarial disconnected,one-parent,double-gender

# Strict mode — promote plausibility warnings to errors
gramps-gen generate --count 100 --adversarial all --strict
```

Available adversarial strategies: `one-parent`, `missing-events`, `solo`, `many-names`, `disconnected`, `deep-nesting`, `max-ref-chains`, `orphaned`, `double-gender`.

### Using a YAML scenario file

```bash
gramps-gen generate --config scenario.yaml
```

Example `scenario.yaml`:

```yaml
person_count: 50
family_count: 20
generations:
  depth: 3
  children_per_family: { min: 1, max: 4 }
date_range:
  start: 1850
  end: 2025
  era: modern
with_citations: true
with_places: true
seed: 42
adversarial:
  enabled: true
  strategies:
    - disconnected
    - double-gender
```

### With a specific schema version

```bash
# Use a specific Gramps schema version
gramps-gen generate --count 200 --schema-version 5.2

gramps-gen generate --count 200 --schema-version 5.1
```

### Schema management

```bash
# List available schemas (local + remote)
gramps-gen schema list

# Download a schema from Gramps GitHub
gramps-gen schema download 5.1

# Download all available schemas
gramps-gen schema download --all
```

### Validate a `.gramps` file

```bash
# Check XML structure
gramps-gen validate output.gramps

# Strict validation
gramps-gen validate output.gramps --strict
```

### Inspect a `.gramps` file

```bash
# Human-readable summary
gramps-gen stats output.gramps

# Machine-readable JSON
gramps-gen stats --json output.gramps
```

### Visualize a `.gramps` file

Requires the `visualize` feature and system dependencies (see [Build Dependencies](#build-dependencies) below).

**Direct binary (recommended for testing):**

```bash
# Open a specific .gramps file (auto-loads on launch)
cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps

# Open without a file (welcome screen with "Open Gramps File" button)
cargo run -p visualize -F visualize

# Skip date imputation (use only explicit dates)
cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps --no-impute

# Custom generation gap for date imputation (default: 25, range: 1-100)
cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps --generation-gap 30

# Flags can appear before or after the file path
cargo run -p visualize -F visualize -- --no-impute ~/Documents/gramps01/exp01.gramps --generation-gap 30
```

**Via the CLI subcommand** (requires both binaries built, see [Two-binary architecture](#two-binary-architecture)):

```bash
cargo build -p cli
cargo build -p visualize -F visualize
cargo run -p cli -- visualize ~/Documents/gramps01/exp01.gramps
```

The file path is a **positional argument** — no `--path` flag needed. The `--no-impute`
and `--generation-gap` flags are forwarded to the visualization backend and sent to
the frontend via Tauri IPC. When launched without arguments, the welcome screen
appears with an "Open Gramps File" button.

See [Visualization](#visualization) for details on the force-directed graph.

| Command | Description |
|---|---|
| `schema list` | List local and available Gramps schemas |
| `schema download` | Download a schema from Gramps GitHub |
| `diff <file_a> <file_b>` | Compare two Gramps XML files and produce a structured diff report |
| `integrate diff-viz --diff <CSV> --selections <JSON>` | Merge diff CSV with visualizer selections (CSV/JSON output) |

The `visualize` subcommand opens a native desktop window with an interactive
force-directed graph of the family tree. It uses **Tauri v2** for the desktop
shell and **D3.js** for graph rendering.

### Features

- **Force-directed graph**: Nodes are persons, edges represent spouse or
  parent-child relationships. The layout uses D3's force simulation with
  collision detection
- **Zoom and pan**: Scroll to zoom, drag to pan
- **Hover tooltips**: 200ms delay tooltip showing name, birth date, and death date
- **Selection and export**: Click to select nodes (Shift-click for multi-select),
  export selections as JSON. Adjacency-based queries (`graph-query.ts`) support
  ancestor/descendant/first-degree/second-degree/indirect selection modes
- **Color gradient**: Nodes are colored by birth year using
  `d3.interpolateViridis` (perceptually uniform, colorblind-friendly).
  Imputed dates shown with dashed borders; undated nodes shown in neutral gray
- **Family group filter**: Dropdown to filter the view to a single connected
  component
- **Force layout tuning**: Sliders adjust `ForceConfig` parameters —
  per-generation Y-field pull (`generationPull`), spouse link strength
  (`spouseStrength`), and parent-child link strength (`parentChildStrength`)
- **Legend**: Color gradient bar showing the year range with labels

### Build Dependencies

Building the full Tauri app requires additional system libraries:

```bash
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config nodejs npm

# Fedora
sudo dnf install webkit2gtk4.1-devel dbus-devel pkgconf pkg-config nodejs npm
```

### Frontend Build

The frontend assets must be built before the Tauri binary:

```bash
cd crates/visualize/frontend
npm install
npm run build
cd ../../..
cargo build -p visualize -F visualize --release
```

### Feature gate

The `visualize` crate is gated behind the `visualize` Cargo feature so the
core CLI can build without system webview dependencies:

```bash
# Build core CLI only (no system deps required)
cargo build --release

# Build with visualization (requires system deps)
cargo build -p visualize -F visualize --release
```

### Two-binary architecture

- `gramps-gen` — the existing CLI, gains a `visualize` subcommand that locates
  and spawns the sibling binary
- `gramps-gen-visualize` — the Tauri desktop app binary

Both binaries are installed to the same directory by:

```bash
cargo install --path .
cargo install -p visualize -F visualize --path crates/visualize
```

### Data Flow

```
.gramps file
    │
    ▼
┌─────────────────────────────┐
│  gramps-reader: XML parse   │  Streaming parser → ParsedPerson[], ParsedFamily[]
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│  visualize: graph_data.rs   │  DSU components, generation layering, gender mapping
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│  visualize: dates.rs        │  Multi-source BFS date imputation
└──────────┬──────────────────┘
           │  GraphData (JSON via Tauri IPC)
           ▼
┌─────────────────────────────┐
│  Frontend: D3.js            │  Force simulation, zoom/pan, tooltips, selection
└─────────────────────────────┘
```

## Pipeline

The tool follows a strict five-stage pipeline with validation gates after every data-altering stage:

```
Generate → Validate (Gate 1) → Adversarial Transform → Validate (Gate 2) → Serialize
```

1. **Generate** — Build a random or scenario-driven family tree graph with procedural names, dates, and places
2. **Validate (Gate 1)** — Check structural integrity (required fields, cardinality) and referential integrity (dangling references)
3. **Adversarial Transform** — Apply post-generation transforms (disconnected subgraphs, deep nesting, ref chains, etc.)
4. **Validate (Gate 2)** — Re-validate after transforms; expected to pass for validity-preserving strategies
5. **Serialize** — Output Gramps XML

## Crate Structure

| Crate | Location | Purpose |
|---|---|---|
| `typed-graph` | `crates/typed-graph/` | Graph model, schema-driven codegen, structural/referential validation, random generation, adversarial strategies, GraphBuilder fluent API |
| `output` | `crates/output/` | Gramps XML serialization with hand-coded `SerializationMap`, streaming `GraphXmlWriter` |
| `gramps-reader` | `crates/gramps-reader/` | Shared library for streaming `.gramps` XML parsing: `FamilyRecord`, `ParsedPerson`, `ParsedFamily`, `Dsu`, `compute_generations`, XML attribute helpers |
| `cli` | `crates/cli/` | CLI binary (`clap`), YAML scenario parsing, pipeline wiring, progress reporting |
| `visualize` | `crates/visualize/` | Tauri v2 desktop app with D3.js force-directed graph visualization (gated behind the `visualize` Cargo feature) |
| `integrate` | `crates/integrate/` | Merge `gramps-gen diff` CSV output with visualizer selection JSON (full outer join by handle) |
| `diff` | `crates/diff/` | Gramps XML diff analyzer: compare two `.gramps` files, produce structured diff report |

## Documentation

- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — Full architecture: codegen, graph model, validation, generation, adversarial strategies, serialization
- **[docs/research/design.md](docs/research/design.md)** — Original design plan and strategy discussion
- **[AGENTS.md](AGENTS.md)** — Instructions for AI coding agents working on this project

## How it works

### Schema-driven codegen

The project uses a schema extraction pipeline:

1. `extract/extract_schema.py` introspects Gramps Python classes to produce `schemas/schema-{version}.json`
2. `typed-graph/build.rs` reads all enabled versioned schema files at compile time and generates Rust types:
   - `Node` enum (10 primary types: Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag)
   - `Edge` enum (~45 edge variants covering handle refs, embedded refs, and mixins)
   - Data structs, ref structs, enum types, and `Schema` runtime metadata
   - Multi-version support via Cargo features (default: `schema-5-2`)

### Graph model

The in-memory graph is a concrete typed directed multigraph:

- Nodes indexed by handle (String, typically UUID v4)
- Edges in insertion order with forward/reverse indexes
- Validation state tracked explicitly (Unvalidated → Valid / Invalid)

### Procedural generation

- **Names**: Markov-chain syllable generation with style support (modern, victorian, nordic)
- **Dates**: `DateValue` structs with quality (Exact/Estimated/Calculated) and modifiers (Before/After/About/Range/Span)
- **Places**: Hierarchical templates (city → county → state → country)
- **Genealogical constraints**: Birth before death, plausible parent ages, generational alignment

### Connection densifier

After generation, a 4-pass post-processing step merges disconnected components into
coherent family structures:

1. **Find components** — Identify weakly connected components (WCC) in the graph
2. **Cross-component marriage** — Merge small components (≤4 persons) into larger ones
   by creating cross-component marriages with shared children
3. **Orphan adoption** — Assign lone persons (no families) to existing families
   as siblings or children
4. **Remarriage** — Convert some single-parent families into two-parent families
   by finding a suitable spouse from the remaining pool

Controlled by `DensifyConfig` with flags for enabling each pass and a merge
probability threshold. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for details.

### Adversarial strategies

Two categories of adversarial strategies:

- **Category A** (generation-time): One-parent families, missing events, solo persons, many alternate names
- **Category B** (post-generation transforms): Disconnected subgraphs, deep place nesting, max ref chains, orphaned references, double gender

All Category B transforms are validity-preserving — they produce graphs that pass structural and referential validation.

## Security

The `extract-schema` command (when fully implemented) imports and executes Python code from the provided path. Only point it at a trusted Gramps source checkout.

## Development

```bash
# Run all tests
cargo test --workspace

# Run clippy linting
cargo clippy --all-targets --all-features -- -D warnings

# Build release
cargo build --release
```
