# Architecture — gramps-gen

## Overview

**gramps-gen** generates valid, plausible [Gramps](https://gramps-project.org/) family tree datasets for testing and development. It models Gramps data as a **typed directed multigraph**, supports both random and scenario-driven generation, applies adversarial transforms for stress-testing, and outputs Gramps XML (`.gramps` format).

The system is a **Rust workspace** with six crates:

| Crate | Purpose |
|---|---|
| `typed-graph` | Core graph model, schema codegen, validation, generation |
| `output` | Gramps XML serialization |
| `gramps-reader` | Shared library for streaming `.gramps` XML parsing, DSU, generation computation, `compute_generation_table` for FamilyGroupGenerationTable |
| `diff` | Gramps XML diff analyzer — compare and match entities across two family trees |
| `cli` | CLI binary (`gramps-gen`), scenario parsing, pipeline wiring |
| `visualize` | Tauri v2 desktop app with D3.js force-directed graph visualization (optional, gated behind `--features visualize`) |

The `gramps-reader` crate's `compute_generation_table` function uses the DSU (disjoint set union) structure
and family/person relationships to assign every person a generation offset relative to their connected
component root, producing a `FamilyGroupGenerationTable` for downstream consumers like the visualizer.

A **Python extractor** (`extract/extract_schema.py`) introspects Gramps Python classes to produce `schemas/schema-5.2.json`, which drives compile-time Rust code generation.

---

## Architecture Diagram

```
┌─────────────────────────────┐
│    extract_schema.py         │  Python — imports gramps.gen.lib
│    (schema extraction)       │  Calls get_schema() / introspects classes
└─────────────┬───────────────┘
              │ emits
              ▼
┌─────────────────────────────┐
│    schemas/schema-5.2.json       │  Committed artifact (Gramps 5.2)
│    schemas/schema-5.1.json       │  Committed artifact (Gramps 5.1)
│    (... more versions)          │
└─────────────┬───────────────┘
              │ build.rs reads at compile time
              ▼
┌─────────────────────────────────────────────────────────────────┐
│  typed-graph  (Rust crate)                                     │
│                                                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
│  │ Schema types  │  │ Graph store  │  │  Validation         │ │
│  │ (codegen)     │  │ (nodes+edges)│  │  (structural +      │ │
│  │ Node, Edge,   │  │ forward/rev  │  │   referential)      │ │
│  │ XxxData,      │  │ indexes      │  │                     │ │
│  │ enums, Schema │  │              │  │                     │ │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬──────────┘ │
│         │                 │                      │            │
│  ┌──────┴─────────────────┴──────────────────────┴──────────┐ │
│  │              Generation Engine                             │ │
│  │  RandomGen (procedural names/dates/places)                │ │
│  │  GraphBuilder (fluent API)                                │ │
│  │  AdversarialStrategies (Category A + B)                   │ │
│  └───────────────────────┬───────────────────────────────────┘ │
└──────────────────────────┼─────────────────────────────────────┘
                           │
┌──────────────────────────┼─────────────────────────────────────┐
│  output  (Rust crate)    │                                      │
│                          ▼                                      │
│  GraphXmlWriter  ──────────── Walks Graph via quick-xml         │
│  SerializationMap  ───────── Hand-coded XML element map         │
└──────────────────────────┬─────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────────────┐
│  gramps-reader  (Rust library crate, shared by cli & visualize)│
│                                                                │
│  ┌──────────────────┐  ┌──────────────────┐                    │
│  │  xml/            │  │  graph.rs         │                    │
│  │  count.rs        │  │  Dsu (disjoint    │                    │
│  │  extract.rs      │  │  set union)       │                    │
│  │  (person/family  │  │  compute_genera-  │                    │
│  │   streaming)     │  │  tions, compute_  │                    │
│  │                  │  │  generation_table │                    │
│  └──────────────────┘  └──────────────────┘                    │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────┐  │
│  │  types.rs        │  │  xml.rs (helpers) │  │  io.rs       │  │
│  │  FamilyRecord,   │  │  strip_prefix,    │  │  gzip detect │  │
│  │  ParsedPerson,   │  │  read_handle_attr,│  │  transparent │  │
│  │  ParsedFamily,   │  │  read_hlink_attr  │  │  decompress  │  │
│  │  ParsedEvent     │  │                    │  │              │  │
│  └──────────────────┘  └──────────────────┘  └──────────────┘  │
└──────────────────────────┬─────────────────────────────────────┘
                           │
┌──────────────────────────┼─────────────────────────────────────┐
│  diff  (Rust crate)      │  gramps-reader for parsing          │
│                          ▼                                     │
│  Entity matching  ────────── matcher.rs, similarity.rs          │
│  Diff comparison  ────────── compare.rs                        │
│  Output  ─────────────────── output.rs (text + JSON)           │
└──────────────────────────┬─────────────────────────────────────┘
                           │
┌──────────────────────────┼─────────────────────────────────────┐
│  cli  (Rust binary)      │                                     │
│                          ▼                                     │
│  gramps-gen generate ──────── 5-stage pipeline                 │
│  gramps-gen validate ──────── XML structure check              │
│  gramps-gen stats ──────────── File summary                    │
│  gramps-gen schema list/download ── Schema management          │
│  gramps-gen visualize ──────── Spawns gramps-gen-visualize     │
│  gramps-gen diff <file_a> <file_b> ── Diff analysis            │
└────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────────────┐
│  visualize  (Rust binary + Tauri app, optional feature)        │
│                                                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
│  │ graph_data.rs │  │ dates.rs     │  │ lib.rs               │ │
│  │ (adapter:     │  │ (multi-source│  │ load_graph_data(     │ │
│  │  ParsedPerson │  │  BFS date    │  │   no_impute,         │ │
│  │  → PersonNode,│  │  imputation) │  │   generation_gap)    │ │
│  │  FamilyLink)  │  │              │  │ (pure function,      │ │
│  │               │  │              │  │  testable)           │ │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘ │
│  ┌──────┴───────┐         │                      │            │
│  │ args.rs       │         │                      │            │
│  │ (CLI flag     │         │                      │            │
│  │  parsing)     │         │                      │            │
│  └──────────────┘         │                      │            │
│                           │  GraphData (JSON via Tauri IPC)    │
│                           ▼                                    │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  Frontend (D3.js + TypeScript)                           │ │
│  │  - forceSimulation, SVG, zoom/pan                       │ │
│  │  - hover tooltips, selection, export                     │ │
│  │  - viridis color gradient, filter, legend                │ │
│  └──────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

---

## Pipeline Model

The tool follows a strict **five-stage pipeline** with validation gates after every data-altering stage:

```
┌──────────────┐    ┌────────────┐    ┌──────────────────────┐    ┌────────────┐    ┌──────────────┐
│  Generation  │───▶│ Validation │───▶│  Adversarial Transform│───▶│ Validation │───▶│ Serialization│
│  (random/    │    │  (Gate 1)  │    │  (Category B only)   │    │  (Gate 2)  │    │  (XML output)│
│   scenario)  │    └────────────┘    └──────────────────────┘    └────────────┘    └──────────────┘
└──────────────┘
```

### Stage 1: Generation

`generate_random(config, adversarial_config, densify_config, schema)` produces a `GenerationResult` containing the `Graph`, seed, warnings, and stats.

- Person generation with procedural names (Markov-chain syllable approach)
- Parent selection with genealogical age constraints
- Child assignment with plausible birth year ranges
- Event generation (Birth, Death, Marriage)
- Optional features: Places, Citations/Sources, Notes, Media, Tags
- Category A adversarial strategies apply **during** generation
- Category B strategies are **not** applied here — they run in Stage 3

### Stage 2: Validation Gate 1

`graph.validate(&schema)` runs two passes:

1. **Structural**: Checks required fields (via `Schema.required_fields`) and cardinality constraints (via `Schema.cardinality_constraints`).
2. **Referential**: Walks every edge and verifies source/target handles exist.

Sets `graph.validation_state` to `Valid` or `Invalid(errors)`.

### Stage 3: Adversarial Transform

`apply_adversarial_strategies(graph, config)` applies **Category B** post-generation transforms:

- **Validity-preserving**: DisconnectedSubgraphs, DeepNesting, MaxRefChains, DoubleGender, OrphanedReferences
- Each transform is `Box<dyn FnOnce(Graph) -> Result<Graph, AdversarialError>>`
- Failed transforms are skipped; the graph reverts to pre-transform state

### Stage 4: Validation Gate 2

Re-runs `graph.validate(&schema)` to catch any issues introduced by transforms.

### Stage 5: Serialization

`GraphXmlWriter.write(graph, writer)` walks the graph by node type, emits XML elements following the `SerializationMap`, and streams directly to a `Write` target for memory efficiency.

---

## Schema Extraction and Codegen

### `schemas/schema-5.2.json` (and other versioned schemas)

A committed JSON artifact describing the Gramps data model for a specific version.

**Multi-version support**: The project supports multiple Gramps schema versions
via Cargo features. All schema files found in `schemas/` are compiled in by
default for transparent auto-detection at runtime. The build script reads all
enabled schemas and generates a **union type** covering all fields and enum
variants across versions.

Each schema file describes:

- **10 primary types**: Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag
- Fields with types, required flags, and cardinality constraints
- Handle references (e.g., `father_handle` → Person)
- Embedded refs with edges (e.g., `event_ref_list` with `EventRef` → Event)
- Mixin bases (CitationBase, NoteBase, MediaBase, TagBase, etc.)
- Secondary types (EventRef, ChildRef, PersonRef, RepoRef, Name, Surname, etc.)
- Enum types (EventType, EventRoleType, ChildRefType, Gender, etc.)

### `build.rs` (typed-graph) — multi-version codegen

Reads all enabled versioned schema files at compile time and generates
`$OUT_DIR/generated_schema.rs`. The build system uses Cargo features to
select which schema versions to compile in:

| Feature | Version | Description |
|---|---|---|
| `schema-5-1` | 5.1 | Gramps 5.1.x support |
| `schema-5-2` | 5.2 | Gramps 5.2.x support (also default) |

Both `schema-5-1` and `schema-5-2` are enabled by default. To build with only
a single version, use `--no-default-features --features schema-5-2`.
| `all-schemas` | all | Enable all available versions |

> **Note:** `schema-5-0` and `schema-6-0` exist as Cargo features for forward
> compatibility, but no corresponding schema JSON files are committed to
> `schemas/` yet. To use these versions, supply your own schema file (e.g.
> `gramps-gen schema download <version>` or manually place `schema-5.0.json` /
> `schema-6.0.json`) and rebuild with the corresponding feature flag.

**Union merge algorithm**: When multiple versions are enabled, the build
script merges them into a single set of Rust types:

1. **Enum types**: Union of all values across all enabled versions
2. **Data structs**: Union of all fields across all enabled versions
3. **Optionality rule**: A field is `Option<T>` if ANY version doesn't require it
4. **Conflict detection**: If a field has different types across versions, a build error is emitted

Each version also gets its own `static LazyLock<Schema>` instance with
version-specific metadata (required fields, cardinality constraints,
valid enum values, field availability).

### Two Schema Formats

The project supports two schema JSON formats:

| Format | Used by | Characteristics |
|---|---|---|
| **Custom flat** | schema-5.2.json | Fields explicitly typed with `type`, `kind`, `required`, `target`, `schema`, `items`, `edges` keys directly on each field entry. This is the canonical format that drives codegen. |
| **JSON Schema** | schema-5.1.json (legacy) | Fields wrapped in a `{"type": "object", "title": "...", "properties": {...}}` envelope. Field types inferred from JSON Schema keywords (`string`, `integer`, `array`, `object`). Enum values are integers, not strings. |

#### Build-time Format Conversion (`schema_convert.rs`)

When `build.rs` detects a JSON Schema format file (by checking for a `"properties"`
key inside the `fields` object), it calls `schema_convert::convert()` to normalize
it to flat format **in memory**. The on-disk file is never modified.

The converter (`crates/typed-graph/src/schema_convert.rs`) handles:

- **Primary/secondary type conversion**: Extracts `properties` → new `fields`,
  maps JSON Schema types to flat types, infers `kind` (handle/handle_ref),
  determines `required` via `oneOf` null-wrapper heuristics.
- **Enum type conversion**: Maps integer values to string names using
  `build/enum_constants_5_1.json` lookup tables.
- **Date normalization**: Converts 5.1's `Date` object (with `dateval` array)
  to 5.2-compatible `DateValue` with `year`/`month`/`day` scalars.
- **Synthetic types**: Generates `PlaceName` and `StyledText` secondary types.
- **Output validation**: `validate_flat_format()` checks structural correctness
  before the merge algorithm runs.

### Generated Code Includes

1. **`Handle` type alias**: `pub type Handle = String`
2. **Enum types**: From `schema.enum_types`, with `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]`
3. **Secondary/embedded structs**: EventRef, ChildRef, PersonRef, RepoRef, Name, Surname, Address, Location, DateValue, etc. — all `#[derive(Clone, Debug, PartialEq, Default)]`
4. **Primary type data structs**: `PersonData`, `FamilyData`, etc.
5. **`Node` enum**: One variant per primary type (e.g., `Node::Person(PersonData)`)
6. **`Edge` enum**: One variant per edge kind:
   - Handle refs: `PersonFamily { source, target }`
   - Embedded refs with metadata: `PersonEventRef { source, target, metadata: Box<EventRef> }`
   - Embedded refs without metadata: `PlacePlaceRef { source, target }`
   - Mixin edges: `CitationRef`, `NoteRef`, `MediaRef`, `TagRef`
   - Deduplicated across types (mixins produce one variant regardless of how many types use them)
7. **`Schema` struct**: Runtime metadata with `version`, `required_fields`, `cardinality_constraints`, `valid_enum_values`, and `field_availability`
8. **`Schema::available_versions()`**: Returns all compiled-in versions
9. **`Schema::default_version()`**: Returns the highest compiled-in version
10. **`Schema::for_version(version)`**: Returns the `Schema` for a specific version

---

## Graph Model

### `Graph` struct

```rust
pub struct Graph {
    nodes: HashMap<Handle, Node>,
    edges: Vec<Edge>,                        // insertion-ordered
    forward_edges: HashMap<Handle, Vec<usize>>,  // source → edge indices
    reverse_edges: HashMap<Handle, Vec<usize>>,  // target → edge indices
    validation_state: ValidationState,
}
```

### Key design properties

- **Concrete, not generic**: All nodes are `Node` enum variants, all edges are `Edge` enum variants.
- **Mutable via add/remove**: `add_node`, `add_edge`, `remove_edges`. Mutations reset `validation_state` to `Unvalidated`.
- **Query API**: `get_node`, `iter_nodes`, `nodes_by_kind(NodeKind)`, `edges_from`, `edges_to`, `iter_edges`, `edge_count`, `node_count`.
- **No deletion of individual nodes**: Nodes are never removed (only edges can be removed via `remove_edges`).
- **Edge add validates**: `add_edge` rejects edges to nonexistent source or target nodes.

### `Node` enum

10 variants, each wrapping a `XxxData` struct:
`Person(PersonData)`, `Family(FamilyData)`, `Event(EventData)`, `Place(PlaceData)`, `Source(SourceData)`, `Citation(CitationData)`, `Repository(RepositoryData)`, `Media(MediaData)`, `Note(NoteData)`, `Tag(TagData)`

### `Edge` enum

Generated from schema. Three categories:

| Category | Examples | Shape |
|---|---|---|
| Direct handle ref | `FamilyFather`, `PersonFamily`, `EventPlace`, `CitationSource` | `{ source: Handle, target: Handle }` |
| Embedded ref with metadata | `PersonEventRef` (Box\<EventRef\>), `FamilyChildRef` (Box\<ChildRef\>), `SourceRepoRef` (Box\<RepoRef\>) | `{ source, target, metadata: Box<T> }` |
| Embedded ref without metadata | `PlacePlaceRef`, `PersonMediaRef`, `CitationMediaRef` | `{ source: Handle, target: Handle }` |
| Mixin (deduplicated) | `CitationRef`, `NoteRef`, `MediaRef`, `TagRef` | `{ source: Handle, target: Handle }` |

### `ValidationState`

```rust
pub enum ValidationState {
    Unvalidated,          // initial state, or after mutations
    Valid,                // last validation passed
    Invalid(Vec<ValidationError>),
}
```

---

## Validation

Two independent passes, both called by `graph.validate(&schema)`:

### Structural validation

- Checks each node's required fields against `Schema.required_fields`
- Required field checking is node-type-specific (e.g., Person requires handle, gender, primary_name)
- Checks array field cardinality against `Schema.cardinality_constraints`

### Referential validation

- Walks every edge, verifies both source and target handles exist in `graph.nodes`

### ValidationError variants

```rust
pub enum ValidationError {
    MissingRequired { node: Handle, field: String },
    CardinalityViolation { node: Handle, field: String, expected: String, actual: usize },
    DanglingReference { source: Handle, link: String, target: Handle },
    PlausibilityWarning { node: Handle, message: String },
}
```

`PlausibilityWarning` is advisory by default; `--strict` promotes it to a blocking error.

---

## Generation

### Random generation (`generate_random`)

Entry point: `generate_random(config: &RandomConfig, adversarial_config: &AdversarialConfig, densify_config: Option<&DensifyConfig>, schema: &Schema) -> Result<GenerationResult, GenerationError>`

**Algorithm:**

1. **Validate config** — reject zero persons, zero generations, inverted date ranges
2. **Generate persons** — procedural names, gender (weighted random), birth/death dates per generation layer
3. **Parent selection** — pair persons into families using age- and layer-matching
4. **Child assignment** — assign children to families with genealogical age constraints
5. **Event generation** — create Birth, Death, Marriage events with consistent dates
6. **Optional features** — places (hierarchical templates), sources/citations, notes
7. **Category A adversarial** — one-parent families, missing events, solo persons, many alternate names

### Procedural name generation

Markov-chain syllable approach using static syllable tables:

- **Given names**: Style-specific tables (modern, victorian, nordic)
- **Surnames**: Shared table across styles
- Names are 1–40 characters, unique within a graph (with fallback numeric suffixes)
- `generate_given_name(style, used_names, rng)` and `generate_surname(style, used_names, rng)`

### Procedural place generation

Hierarchical template system:

- `generate_place(depth, used_place_names, rng)` → `GeneratedPlace { city, county, state, country }`
- Cities: prefix + suffix combinations (30 prefixes × 20 suffixes)
- States: 15 procedurally-named states
- Countries: 8 procedurally-named countries
- Depth controls hierarchy: 1 = country only, 2 = state + country, 3 = city + county + state + country

### Date generation

`DateValue` struct with:

- `year: i32`, `month: Option<i32>`, `day: Option<i32>`
- `quality: Option<DateQuality>` (Exact, Estimated, Calculated)
- `modifier: Option<DateModifier>` (None, Before, After, About, Range, Span)
- Generation layers determine birth years, death years calculated from life expectancy
- Date quality randomized: ~80% Exact, ~15% Estimated, ~5% Calculated

### Connection Densifier

After generation (and before adversarial transforms), the connection densifier
post-processes the graph to reduce disconnected components.

Entry point: `densify_connections(graph: &mut Graph, config: &DensifyConfig, rng: &mut impl Rng) -> Result<(), GenerationError>`

Controlled by `DensifyConfig`:

```rust
pub struct DensifyConfig {
    pub enabled: bool,
    pub cross_component_marriage: bool,
    pub orphan_adoption: bool,
    pub remarriage: bool,
    pub merge_probability: f64,
}
```

**4-pass algorithm:**

1. **Find components** — Compute weakly connected components (WCC) over the
   undirected graph (Person ↔ Family via spouse/child edges).
2. **Cross-component marriage** — For each component with ≤4 persons, attempt
   to merge it into a larger component by creating a marriage between a person
   in the small component and a person in a larger one, then adding shared
   children. Controlled by `merge_probability`.
3. **Orphan adoption** — For lone persons (no family associations), find a
   suitable family and add them as a sibling or child. Families are prioritized
   by generation proximity.
4. **Remarriage** — For single-parent families (one spouse), find a suitable
   spouse candidate from the remaining unpartnered pool and create a marriage
   event. Prioritizes candidates within the same generation.

### Configuration

`RandomConfig` controls: `person_count`, `family_count`, `generations`, `children_per_family` (range), `start_year`/`end_year`, `name_style`, feature flags (`with_places`, `with_citations`, etc.), `seed`, `place_depth`.

### GraphBuilder (fluent API)

Separate from `Graph`. Takes `&mut Graph` on construction. Each primary type has a sub-builder:

- `PersonBuilder` — `with_name`, `with_gender`, `with_birth_date`, `with_death_date`, `with_parent_family`, `with_family`, `add_alternate_name`, `build() -> Result<Handle, BuilderError>`
- `FamilyBuilder` — `with_father`, `with_mother`, `add_child`, `with_marriage_date`, `build()`
- `EventBuilder`, `PlaceBuilder`, `SourceBuilder`, `CitationBuilder`, `NoteBuilder`, `MediaBuilder`, `RepositoryBuilder`, `TagBuilder`

`build()` validates required fields, checks that referenced handles exist, creates nodes and edges atomically. Birth/death dates auto-create Event nodes with `PersonEventRef` edges. Marriage date auto-creates a Marriage event with `FamilyEventRef` edge.

---

## Adversarial Generation

Strategies fall into two categories:

### Category A — Generation-time

Applied during `generate_random()` via config flags. These modify generation behavior:

| Strategy | Parameter | Effect |
|---|---|---|
| `OneParentFamilies` | fraction (0.0–1.0) | Skip father or mother assignment |
| `MissingEvents` | fraction | Skip birth/death event creation |
| `SoloPersons` | fraction | Create persons with no families/events |
| `ManyAlternateNames` | fraction | Add 5–20 alternate names |

### Category B — Post-generation transforms

Applied after Gate 1 via `apply_adversarial_strategies()`. Composable `GraphTransform` functions:

| Strategy | Validity | Effect |
|---|---|---|
| `DisconnectedSubgraphs` | Preserving | Partition graph into k clusters by removing cross-cluster family edges |
| `DeepNesting` | Preserving | Create 5–10 level deep Place→Place parent chains |
| `MaxRefChains` | Preserving | Build Event→Citation→Source→Repository→Note→Tag chains |
| `OrphanedReferences` | Preserving | Remove soft (citation/note/media/tag) edges |
| `DoubleGender` | Preserving | Swap Male↔Female for a fraction of persons |

---

## XML Serialization

### `GraphXmlWriter`

Walks the graph grouped by node type, emits XML via `quick-xml`:

1. XML declaration + `<database>` root + namespace
2. `<header>` with creation date and version
3. Sections in Gramps order: tags, events, people, families, citations, sources, places, objects, repositories, notes
4. Empty sections are omitted
5. Handle refs write as `hlink` attributes
6. Streams directly to `Write` — no intermediate DOM

### `SerializationMap`

Hand-coded mapping from Rust types to XML element/attribute names:

```rust
pub struct SerializationMap {
    pub type_map: HashMap<String, XmlTypeInfo>,       // "Person" → <person>
    pub edge_map: HashMap<String, XmlEdgeInfo>,       // Edge variant → XML nesting
    pub section_order: Vec<String>,                   // Output order
}
```

Future: plan describes extracting this from the Gramps RelaxNG schema at build time.

---

## CLI Interface

### Commands

| Command | Description |
|---|---|
| `gramps-gen generate` | Full 5-stage pipeline → `.gramps` output |
| `gramps-gen validate <file>` | Minimal XML structure check (well-formedness, root element, namespace) |
| `gramps-gen stats <file>` | Streaming count and summary of a `.gramps` file (text or JSON) |
| `gramps-gen visualize <file>` | Open a Tauri desktop window with force-directed graph visualization |
| `gramps-gen schema list` | List local and available Gramps schemas |
| `gramps-gen schema download [VERSION]` | Download a schema from Gramps GitHub |
| `gramps-gen diff <file_a> <file_b>` | Compare two Gramps XML files and produce a structured diff report |

### Generate flags

```
-n, --count <N>          Persons to generate [default: 200]
-d, --depth <N>          Generations [default: 3]
-o, --output <FILE>      Output file [default: output.gramps]
--seed <N>               RNG seed for reproducibility
--strict                 Promote plausibility warnings to errors
--adversarial <LIST>     Comma-separated strategies or "all"
-c, --config <FILE>      YAML scenario file
--with-places / --with-citations / --with-notes / --with-media / --with-tags
--schema-version <VERSION>  Schema version to use [default: highest installed]
```

### YAML scenario format

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
    - one_parent
```

---

## Visualization Architecture

The `visualize` crate (optional, gated behind `--features visualize`) provides a
desktop application for interactively exploring family trees as a force-directed
graph. It uses **Tauri v2** for the desktop shell, **D3.js** for rendering, and
the **gramps-reader** shared crate for data parsing.

### Two-binary approach

- `gramps-gen` (CLI binary) gains a `visualize` subcommand that locates the
  sibling `gramps-gen-visualize` binary and spawns it with the file path and
  flags
- `gramps-gen-visualize` (Tauri binary) owns `main()` and opens the native
  window

### Data Flow

```
.gramps file
    │
    ▼
┌──────────────────────────────┐
│  gramps-reader::xml::extract  │  Streaming parse → ParsedPerson[], ParsedFamily[]
└──────────┬───────────────────┘
           │
           ▼
┌──────────────────────────────┐
│  visualize::graph_data.rs     │  DSU components, generation layering,
│  build_graph_data()           │  gender mapping, FamilyLink construction
└──────────┬───────────────────┘
           │  PersonNode[], FamilyLink[]
           ▼
┌──────────────────────────────┐
│  visualize::dates.rs          │  Multi-source BFS date imputation:
│  impute_dates()               │  - Propagate from dated nodes with
│                                │    configurable gap (default 25 years)
│                                │  - Average ties from multiple sources
│                                │  - Null fallback for unreachable nodes
└──────────┬───────────────────┘
           │  GraphData
           ▼
┌──────────────────────────────┐
│  visualize::lib.rs            │  load_graph_data() — pure function that
│                                │  orchestrates the full pipeline
│                                │  (unit-testable, IPC-agnostic)
│                                │  Params: no_impute, generation_gap
└──────────┬───────────────────┘
           │  GraphData → serde_json
           ▼
┌──────────────────────────────┐
│  Tauri IPC bridge             │  #[tauri::command] load_graph(path, no_impute,
│  src/main.rs                  │    generation_gap) → Result<GraphData, String>
│                                │  #[tauri::command] export_selections(path,
│                                │    selections) → Result<String, String>
└──────────┬───────────────────┘
           │  JSON via invoke()
           ▼
┌──────────────────────────────┐
│  Frontend (D3.js + TypeScript)│
│                                │
│  src/graph.ts                  │  d3.forceSimulation, SVG, zoom/pan
│  src/graph-query.ts            │  Adjacency indices, indirect-set queries
│  src/tooltip.ts                │  200ms hover tooltip (name, birth, death)
│  src/selection.ts              │  Click-to-select, Shift multi-select, export
│  src/colors.ts                 │  d3.interpolateViridis, imputed dashed, gray null
│  src/types.ts                  │  TypeScript interfaces matching Rust types
│  src/main.ts                   │  Entry point, IPC wiring, filter dropdown, legend
└──────────────────────────────┘
```

### Feature gate

The `visualize` feature is optional to avoid requiring WebKit2GTK / WebView2
system dependencies for the core CLI:

```bash
cargo build --release                    # Core CLI only
cargo build --release --features visualize  # With visualization (requires system deps)
```

The `visualize` crate is excluded from `default-members` in the workspace
`Cargo.toml`.

### Data model

```rust
pub struct GraphData {
    pub nodes: Vec<PersonNode>,
    pub links: Vec<FamilyLink>,
    pub family_groups: Vec<FamilyGroupMeta>,
}

pub struct PersonNode {
    pub handle: String,
    pub name: String,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
    pub birth_year: Option<i32>,
    pub is_imputed: bool,
    pub gender: String,
    pub family_group: usize,
    pub generation: usize,
}

pub struct FamilyLink {
    pub source: String,
    pub target: String,
    pub link_type: LinkType,  // Spouse | ParentChild
}
```

### Date imputation

For nodes without a birth date, a multi-source BFS propagates from all dated
nodes simultaneously:

1. Initialize a queue with every node that has a known birth year
2. For each visited undated node: `imputed_year = source_year + (gen_diff × gap)`
3. If reached from multiple sources at the same distance, average the candidates
4. Nodes with no reachable dated node → `birth_year = None` → neutral gray

The generation gap is configurable (default: 25 years, validated range: 1-100).

### Frontend features

| Feature | File | Description |
|---|---|---|
| Force simulation | `graph.ts` | d3.forceSimulation with collision, zoom/pan, SVG circles+lines |
| Hover tooltips | `tooltip.ts` | 200ms delay, name+birth+death, cursor-follow, mouse-out hide |
| Selection | `selection.ts` | Click toggle, Shift multi-select, Export via Tauri save dialog |
| Graph queries | `graph-query.ts` | Adjacency indices, ancestor/descendant/indirect-set queries for selection modes |
| Stats panel | `stats-panel.ts` | Collapsible right sidebar with file statistics summary |
| Color gradient | `colors.ts` | d3.interpolateViridis, imputed dashed border, undated gray |
| Filter | `main.ts` | Dropdown to filter by family group (connected component) |
| Force layout tuning | `types.ts` | `ForceConfig` with generationPull, spouseStrength, parentChildStrength; UI sliders |
| Legend | `colors.ts` | Gradient bar with year labels, undated/imputed caption items |
| Standalone test page | `test-harness.html` | HTML page for manual D3 rendering tests outside Tauri |

---

## Diff Analyzer

The `diff` crate provides a structured comparison engine for Gramps XML files. It
matches persons, families, and other entities across two `.gramps` files, identifies
additions, deletions, and modifications, and produces a structured diff report in
text or JSON format.

### Architecture

The diff analysis follows a multi-stage pipeline:

1. **Parsing**: Uses `gramps-reader` to stream-parse both `.gramps` files into
   `ParsedPerson` and `ParsedFamily` arrays.
2. **Entity matching** (`matcher.rs`): Matches entities across the two files using
   string similarity scoring from `similarity.rs`.
3. **Comparison** (`compare.rs`): Compares matched entity pairs field-by-field to
   identify differences.
4. **Cascading resolution** (`cascading.rs`): Extrinsic resolution for indirectly
   referenced entities (e.g., events, places) that become relevant after initial
   matching.
5. **Output** (`output.rs`): Formats the diff report as human-readable text or
   machine-readable JSON.

### Feature gate

The `resolve` feature (default: off) enables interactive conflict resolution via
`crossterm`. Enable with `--features diff/resolve`.

### Integration

The `gramps-gen diff <file_a> <file_b>` CLI subcommand wires the full pipeline:
parse → match → compare → resolve (if enabled) → output.

---

## Error Handling

### Error types across layers

| Layer | Error type | Purpose |
|---|---|---|
| Graph | `GraphError` | DuplicateHandle, MissingNode, InvalidEdge |
| Validation | `ValidationError` | MissingRequired, CardinalityViolation, DanglingReference, PlausibilityWarning |
| Builder | `BuilderError` | MissingRequiredField, InvalidHandle, DuplicateHandle |
| Generation | `GenerationError` | InvalidConfig, ConstraintExhausted |
| Adversarial | `AdversarialError` | TransformNotApplicable, MissingRequiredFeature |
| Serialization | `SerializationError` | Io, UnsupportedType, MissingRequiredField |
| CLI | `CliError` | Io, ConfigError, GenerationFailed, ValidationFailed, SerializationFailed, ScenarioError |
| Scenario | `ScenarioError` | Io, ParseError |

`CliError` has `From` impls for all downstream error types.

---

## Testing Strategy

### Unit tests

In `#[cfg(test)] mod tests` at the bottom of each source file. Cover:

- Type shape existence (every variant compiles)
- Normal cases
- Edge cases (empty handles, zero counts, boundary values)
- Error conditions
- Validation state transitions
- Clone/Debug/PartialEq derives

### Integration tests

`crates/cli/tests/integration.rs` — cross-crate integration of generation + validation + serialization.

### E2E tests

`crates/cli/tests/e2e.rs` — runs the compiled binary as a subprocess, verifies exit codes, checks output file content, tests roundtrips.

### Running tests

```bash
cargo test --workspace          # All tests
cargo test -p typed-graph        # Unit tests only
cargo test -p cli --test e2e    # E2E tests
```

---

## Dependencies

| Dependency | Crate | Usage |
|---|---|---|
| `serde` / `serde_json` | typed-graph (build) | Parse schema-5.2.json in build.rs |
| `rand` | typed-graph | RNG for procedural generation |
| `uuid` (v4 feature) | typed-graph | Auto-generated handles |
| `quick-xml` | output | XML serialization |
| `clap` (derive) | cli | CLI argument parsing |
| `serde_yaml` | cli | YAML scenario parsing |
| `log` / `env_logger` | cli | Logging |
| `ureq` | cli | HTTP requests for `schema download` |
| `strsim` | diff | String similarity scoring for entity matching |

---

## Security

The `extract-schema` command (when fully implemented) imports and executes Python code from a user-supplied `PYTHONPATH`. Users should only point this at a trusted Gramps source checkout.
