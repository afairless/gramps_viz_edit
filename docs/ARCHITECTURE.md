# Architecture — gramps-gen

## Overview

**gramps-gen** generates valid, plausible [Gramps](https://gramps-project.org/) family tree datasets for testing and development. It models Gramps data as a **typed directed multigraph**, supports both random and scenario-driven generation, applies adversarial transforms for stress-testing, and outputs Gramps XML (`.gramps` format).

The system is a **Rust workspace** with three crates:

| Crate | Purpose |
|---|---|
| `typed-graph` | Core graph model, schema codegen, validation, generation |
| `output` | Gramps XML serialization |
| `cli` | CLI binary (`gramps-gen`), scenario parsing, pipeline wiring |

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
└─────────────┬───────────────┘
              │ build.rs reads at compile time
              ▼
┌─────────────────────────────────────────────────────────┐
│  typed-graph  (Rust crate)                               │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐ │
│  │ Schema types  │  │ Graph store  │  │  Validation   │ │
│  │ (codegen)     │  │ (nodes+edges)│  │  (structural +│ │
│  │ Node, Edge,   │  │ forward/rev  │  │   referential)│ │
│  │ XxxData,      │  │ indexes      │  │               │ │
│  │ enums, Schema │  │              │  │               │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬────────┘ │
│         │                 │                  │          │
│  ┌──────┴─────────────────┴──────────────────┴────────┐ │
│  │              Generation Engine                       │ │
│  │  RandomGen (procedural names/dates/places)          │ │
│  │  GraphBuilder (fluent API)                          │ │
│  │  AdversarialStrategies (Category A + B)             │ │
│  └───────────────────────┬─────────────────────────────┘ │
└──────────────────────────┼───────────────────────────────┘
                           │
┌──────────────────────────┼───────────────────────────────┐
│  output  (Rust crate)    │                                │
│                          ▼                                │
│  GraphXmlWriter  ──────────── Walks Graph via quick-xml   │
│  SerializationMap  ───────── Hand-coded XML element map   │
└──────────────────────────┬───────────────────────────────┘
                           │
┌──────────────────────────┼───────────────────────────────┐
│  cli  (Rust binary)      │                                │
│                          ▼                                │
│  gramps-gen generate ──────── 5-stage pipeline            │
│  gramps-gen validate ──────── XML structure check         │
│  gramps-gen extract-schema ── Stub                        │
└───────────────────────────────────────────────────────────┘
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

`generate_random(config, adversarial_config, schema)` produces a `GenerationResult` containing the `Graph`, seed, warnings, and stats.

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

### `schemas/schema-5.2.json`

A committed JSON artifact describing:

- **10 primary types**: Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag
- Fields with types, required flags, and cardinality constraints
- Handle references (e.g., `father_handle` → Person)
- Embedded refs with edges (e.g., `event_ref_list` with `EventRef` → Event)
- Mixin bases (CitationBase, NoteBase, MediaBase, TagBase, etc.)
- Secondary types (EventRef, ChildRef, PersonRef, RepoRef, Name, Surname, etc.)
- Enum types (EventType, EventRoleType, ChildRefType, Gender, etc.)

### `build.rs` (typed-graph)

Reads `schema-5.2.json` at compile time and generates `$OUT_DIR/generated_schema.rs`:

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
7. **`Schema` struct**: Runtime metadata with `version`, `required_fields: HashMap<&str, Vec<&str>>`, `cardinality_constraints: HashMap<&str, (Option<u32>, Option<u32>)>`

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

Entry point: `generate_random(config: &RandomConfig, adversarial_config: &AdversarialConfig, schema: &Schema) -> Result<GenerationResult, GenerationError>`

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
| `gramps-gen extract-schema <path>` | Stub (planned: run Python extractor) |

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

---

## Security

The `extract-schema` command (when fully implemented) imports and executes Python code from a user-supplied `PYTHONPATH`. Users should only point this at a trusted Gramps source checkout.
