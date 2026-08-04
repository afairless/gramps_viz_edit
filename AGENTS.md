# AGENTS.md — Agent Instructions for gramps-gen

## Project Overview

**gramps-gen** is a Rust workspace that generates valid, plausible [Gramps](https://gramps-project.org/) family tree datasets. It models Gramps data as a typed directed multigraph, supports both random and configurable scenario-driven generation, and outputs Gramps XML (`.gramps` format).

## Workspace Structure

```
gramps_viz_edit/
├── Cargo.toml                    # Workspace root (resolver = "2")
├── schemas/
│   ├── schema-5.2.json               # Canonical Gramps 5.2 schema (committed artifact)
│   └── schema-5.1.json               # Gramps 5.1 schema in JSON Schema format (auto-converted)
├── extract/
│   └── extract_schema.py         # Python extractor (reads Gramps Python classes)
├── crates/
│   ├── typed-graph/              # Core: graph model, schema codegen, validation, generation
│   │   ├── build.rs              # Reads schema files → generates Rust types at compile time
│   │   ├── build/
│   │   │   └── enum_constants_5_1.json  # Enum integer→name lookup tables for 5.1 conversion
│   │   └── src/
│   │       ├── lib.rs            # Re-exports, unit tests
│   │       ├── schema.rs         # include!($OUT_DIR/generated_schema.rs)
│   │       ├── schema_convert.rs # JSON Schema → flat format converter + unit tests
│   │       ├── graph.rs          # Graph struct, NodeKind, ValidationState, GraphError
│   │       ├── validate.rs       # Structural + referential validation
│   │       ├── date.rs           # DateValue convenience impls (new, new_ymd, display_text, is_valid)
│   │       └── generate/
│   │           ├── mod.rs        # Re-exports
│   │           ├── builder.rs    # GraphBuilder fluent API (separate from Graph)
│   │           ├── random.rs     # Procedural name/date/place generators, generate_random()
│   │           ├── densify.rs    # Connection densifier: component merging, orphan adoption, remarriage
│   │           └── adversarial.rs # Adversarial strategies (Category A + B), apply_adversarial_strategies()
│   │   ├── tests/
│   │   │   ├── schema_convert_tests.rs   # Integration tests for schema_convert via #[path] include
│   │   │   └── merged_schema.rs          # Integration tests for merged 5.1+5.2 schema
│   ├── output/                   # Gramps XML serialization
│   │   └── src/
│   │       ├── lib.rs            # Re-exports
│   │       ├── xml.rs            # GraphXmlWriter (quick-xml based)
│   │       └── serialization_map.rs # Hand-coded mapping to XML element/attribute names
│   ├── gramps-reader/            # Shared library for streaming .gramps XML parsing
│   │   └── src/
│   │       ├── lib.rs            # Library root, re-exports
│   │       ├── types.rs          # FamilyRecord, ParsedPerson, ParsedFamily, Dsu
│   │       ├── graph.rs          # DSU operations, compute_generations
│   │       ├── xml.rs            # XML attribute helpers
│   │       ├── error.rs          # Parser error types
│   │       └── xml/
│   │           ├── count.rs      # Streaming element count
│   │           └── extract.rs    # Streaming person/family extraction
│   ├── visualize/                # Tauri v2 desktop app with D3.js force-directed graph
│   │   ├── tauri.conf.json       # Tauri v2 configuration
│   │   ├── src/
│   │   │   ├── main.rs           # Tauri entry point, IPC commands
│   │   │   ├── lib.rs            # load_graph_data() orchestrator
│   │   │   ├── graph_data.rs     # DSU components, generation layering, gender mapping
│   │   │   ├── dates.rs          # Multi-source BFS date imputation
│   │   │   └── args.rs           # CLI argument parsing
│   │   ├── frontend/
│   │   │   ├── src/
│   │   │   │   ├── main.ts       # Entry point, IPC wiring, filter dropdown, legend
│   │   │   │   ├── graph.ts      # d3.forceSimulation, SVG, zoom/pan
│   │   │   │   ├── graph-query.ts # Adjacency indices, indirect-set queries
│   │   │   │   ├── selection.ts  # Click-to-select, Shift multi-select, export
│   │   │   │   ├── colors.ts     # d3.interpolateViridis, imputed dashed, gray null
│   │   │   │   ├── tooltip.ts    # 200ms hover tooltip
│   │   │   │   └── types.ts      # TypeScript interfaces matching Rust types
│   │   │   ├── styles/
│   │   │   │   └── main.css
│   │   │   ├── tests/            # Vitest frontend tests
│   │   │   ├── test-harness.html # Standalone HTML for D3 rendering tests
│   │   │   ├── package.json
│   │   │   ├── tsconfig.json
│   │   │   └── vite.config.ts
│   │   ├── capabilities/         # Tauri v2 capability manifests
│   │   └── tests/                # Rust integration tests
│   └── cli/                      # CLI binary
│       ├── src/
│       │   ├── main.rs           # Clap parser, command dispatch
│       │   ├── lib.rs            # Library root
│       │   ├── error.rs          # CliError enum
│       │   ├── progress.rs       # ProgressReporter
│       │   ├── scenario.rs       # YAML scenario parsing
│       │   └── commands/
│       │       ├── mod.rs
│       │       ├── generate.rs   # Full 5-stage pipeline
│       │       ├── stats/        # Streaming count & report (stats command)
│       │       ├── validate.rs   # Minimal XML structure check
│       │       └── extract_schema.rs # Stub
│       └── tests/
│           ├── e2e.rs              # Subprocess-based E2E tests
│           └── integration.rs    # Integration tests
```

## Key Design Rules

### 1. Schema-driven codegen

- Schema files (`schemas/schema-{version}.json`) are the **source of truth** for Gramps data types, edges, required fields, and cardinality.
- The project supports multiple Gramps schema versions via Cargo features. Each version has its own file.
- `typed-graph/build.rs` reads all enabled versioned schema files at compile time and generates `$OUT_DIR/generated_schema.rs` containing: `Node` enum, `Edge` enum, all `XxxData` structs, secondary/embedded ref structs, enum types, and `Schema` runtime metadata.
- `typed-graph/src/schema.rs` includes the generated code via `include!`.
- To update the schema: run `gramps-gen schema download <version>` or manually place `schema-{version}.json` and rebuild with the corresponding feature flag.

#### Schema Formats

Two formats are supported:

| Format | Example file | Key indicator |
|---|---|---|
| **Custom flat** | schema-5.2.json | Fields have explicit `type`, `kind`, `required`, `target`, `schema` keys |
| **JSON Schema** | schema-5.1.json | Fields nested under `properties` in a `{type: "object", title: "...", properties: {...}}` envelope |

`schema_convert.rs` detects and automatically converts JSON Schema format to flat
format at build time (in memory, never modifying the file on disk). Enum integer
values are mapped to names using `build/enum_constants_5_1.json`. The extraction
script produces JSON Schema format for Gramps 5.1 and prints a warning directing
users to rely on the build-time converter.

#### Version-specific field helpers

When multiple schema versions are merged, fields that exist in only one version
become `Option<T>`. Helper functions in `graph.rs` abstract over the version-
specific wrapping:

| Helper | Purpose |
|---|---|
| `into_gender_field` / `gender_value` | Gender: `i32` in 5.2, `Option<i32>` in merged |
| `into_event_type_field` / `event_type_eq` | EventType: enum in 5.2, `Option<enum>` in merged |
| `into_source_handle_field` / `get_source_handle` / `is_source_handle_empty` | source_handle: `String` in 5.2, `Option<String>` in merged |
| `make_event_ref` / `make_child_ref` | Ref structs with version-specific optional fields |
| `edge_place_place_ref` | Edge variants with/without metadata |

These helpers use `#[cfg(feature = "schema-5-1")]` to select the right variant
at compile time. Generator and builder code **must** use these helpers instead
of direct struct field access to support merged schemas.

### 2. Graph model invariants

- `Graph` is a **concrete** (not generic) typed directed multigraph.
- Nodes are stored in `HashMap<Handle, Node>`. `Node` is an enum over 10 primary types: Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag.
- Edges are stored in `Vec<Edge>` with forward (`source → [indices]`) and reverse (`target → [indices]`) indexes.
- `add_node` and `add_edge` both reset `validation_state` to `Unvalidated`.
- Handles are plain `String` (type alias, typically UUID v4).

### 3. Five-stage pipeline

```
Generate → Validate (Gate 1) → Adversarial Transform → Validate (Gate 2) → Serialize
```

- Random generation uses `generate_random(config, adversarial_config, densify_config, schema)`.
- Adversarial Category A strategies apply **during** generation via config flags.
- Adversarial Category B strategies apply **after** Gate 1 via `apply_adversarial_strategies()`.
- Validation uses `graph.validate(&schema)` which sets `ValidationState` on the graph.
- Serialization uses `GraphXmlWriter` with a hand-coded `SerializationMap`.

### 4. GraphBuilder is separate from Graph

- `GraphBuilder` takes `&mut Graph`, not an internal `Graph`.
- Builders for each primary type live in `builder.rs` (PersonBuilder, FamilyBuilder, EventBuilder, etc.).
- `build()` returns `Result<Handle, BuilderError>` — it validates required fields and handle references at build time.
- Birth/death dates on `PersonBuilder` auto-create Event nodes and PersonEventRef edges.
- Marriage date on `FamilyBuilder` auto-creates a Marriage event and FamilyEventRef edge.

### 5. Naming conventions

- Generated structs use `XxxData` naming (e.g., `PersonData`, `FamilyData`).
- Ref structs carry metadata: `EventRef` has `ref_field` + `role`; `ChildRef` has `ref_field` + `relation`.
- Edge variants that carry metadata use `Box<RefType>` (e.g., `PersonEventRef { source, target, metadata: Box<EventRef> }`).
- Rust keywords in field names are escaped: `ref` → `ref_field`, `type` → `type_field`.

## Running Tests

```bash
# All tests (workspace)
cargo test --workspace

# Specific crate
cargo test -p typed-graph
cargo test -p output
cargo test -p cli

# E2E tests (need the binary built)
cargo test -p cli --test e2e

# Linting
cargo clippy --all-targets --all-features -- -D warnings
```

## Key Dependencies

| Dependency | Use |
|---|---|
| `serde` / `serde_json` | Schema parsing in build.rs |
| `quick-xml` | XML serialization in output crate |
| `clap` (derive) | CLI argument parsing |
| `serde_yaml` | YAML scenario file parsing |
| `rand` | RNG for procedural generation |
| `uuid` (v4) | Auto-generated handles |

## Code Conventions

- All public types must implement `Debug`, most implement `Clone` and `PartialEq`.
- Error types implement `std::error::Error` and `Display`.
- Unit tests live in `#[cfg(test)] mod tests` at the bottom of each source file.
- Tests cover: normal cases, empty/null edge cases, error conditions, type shape existence, and validation state transitions.
- Doc comments use `///` with `# Examples` for public API, `//` for internal notes.
- Procedural generators (names, places) take `&mut impl Rng` explicitly — no global state.
