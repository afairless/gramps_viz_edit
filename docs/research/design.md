# Gramps Data Generator — Design Plan

## 1. Overview

**What**: A tool that generates valid, plausible Gramps family tree datasets. It models the Gramps database as a typed directed multigraph, supports both scripted and random generation, and outputs Gramps XML for direct import.

**Why this architecture**: Gramps' own data model is an object graph of primary objects (Person, Family, Event, …) linked by handles. A graph abstraction is the natural fit. The typed nature of the graph (each edge has a known source type, link type, and target type) makes generation compositional: you generate a Person, then generate Families they belong to, then generate Events those families reference, and so on — each step constrained by the schema.

**Audience**: Developers building/testing genealogy tools, and anyone who wants repeatable, controllable test datasets for Gramps.

## 2. Architecture at a Glance

```
┌────────────────────┐
│  Schema Extraction │  Python script ← imports gramps.gen.lib
└────────┬───────────┘         │         calls get_schema() on every class
         │                     │         emits schema.json
         ▼                     │
┌────────────────────┐         │
│     schema.json     │◄────────┘  Committed artifact
└────────┬───────────┘
         │  build.rs reads it
         ▼
┌──────────────────────────────────────────────┐
│  typed-graph  (Rust crate)                   │
│                                              │
│  ┌──────────┐  ┌──────────┐  ┌────────────┐ │
│  │  Schema  │  │  Graph   │  │  Validate   │ │
│  │  (types) │  │  (store) │  │  (checks)   │ │
│  └────┬─────┘  └────┬─────┘  └─────┬──────┘ │
│       │             │              │         │
│  ┌────┴─────────────┴──────────────┴───────┐ │
│  │         Generation Engine               │ │
│  │  RandomGen  │  GraphBuilder  │  Config  │ │
│  │  AdversarialStrategies                  │ │
│  └────────────────────┬────────────────────┘ │
└───────────────────────┼──────────────────────┘
                        │
┌───────────────────────┼──────────────────────┐
│  output  (Rust crate) │                      │
│                       ▼                      │
│               XML Serializer                 │
│          (Gramps XML .gramps format)         │
└──────────────────────────────────────────────┘
                        │
┌───────────────────────┼──────────────────────┐
│  cli  (Rust binary)   │                      │
│    gramps-gen generate ──count 2000 --depth 5│
│    gramps-gen extract-schema <gramps-repo>  │
│    gramps-gen validate <input>              │
└──────────────────────────────────────────────┘
```

### Layer Responsibilities

| Layer | Crate | Responsibility |
|---|---|---|
| Schema extraction | `extract/` (Python) | Introspect Gramps Python classes, emit `schema.json` |
| Graph model | `typed-graph` | In-memory typed directed multigraph, schema types, validation |
| Generation | `typed-graph` | Random generation, GraphBuilder API, config-driven scenarios, adversarial strategies |
| Output | `output` | Walk the graph, produce Gramps XML |
| CLI | `cli` | Parse arguments, wire layers together |

### Pipeline Model

The tool follows a strict five-stage pipeline (see Section 11 for details). Validation gates run after every data-altering stage:

```
Generate → Validate → [Adversarially Transform] → Validate → Serialize
```

Each stage has a distinct responsibility, and validation gates run after every data-altering stage.

## 3. Module Map

```
gramps_gen/
├── Cargo.toml                  # Workspace
├── extract/
│   └── extract_schema.py       # Canonical schema extractor
│                                # Requires Gramps source cloned locally
│                                # Run with: PYTHONPATH=<gramps-repo> python extract_schema.py
├── schema.json                 # Committed schema artifact
├── crates/
│   ├── typed-graph/
│   │   ├── Cargo.toml
│   │   ├── build.rs            # Reads schema.json → codegen
│   │   └── src/
│   │       ├── schema.rs        # Schema types (codegen'd)
│   │       ├── graph.rs         # Graph storage (concrete, not generic)
│   │       ├── node.rs          # Node enum + per-type data
│   │       ├── edge.rs          # Edge enum + typed metadata
│   │       ├── validate.rs      # Structural + referential validation
│   │       ├── generate/
│   │       │   ├── mod.rs       # Generate trait, RandomConfig
│   │       │   ├── random.rs    # Property-based random generation
│   │       │   ├── builder.rs   # GraphBuilder fluent API (separate struct)
│   │       │   ├── scenario.rs  # Config-driven scenario runner
│   │       │   └── adversarial.rs # Adversarial strategy transforms
│   │       └── lib.rs
│   ├── output/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── xml.rs           # Gramps XML serializer
│   │       └── lib.rs
│   └── cli/
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
└── tests/
    ├── fixtures/                # Small generated .gramps files
    ├── integration/
    │   └── roundtrip.rs         # Generate → validate → serialize → re-parse
    └── property/
        └── invariants.rs        # Property-based tests for invariants
```

The root `tests/` directory contains integration and property-based tests that span multiple workspace crates. It is linked via a top-level `Cargo.toml` with `[dev-dependencies]` on each workspace crate. Each crate also has its own `#[cfg(test)] mod tests` for unit tests alongside source code (not shown above for brevity).

## 4. Key Design Decisions

### Decision 1: Python extractor + build.rs codegen

- **Context**: Gramps data model lives in Python classes. We need Rust types that mirror it.
- **Decision**: Python script calls `get_schema()` on every Gramps class, producing a `schema.json`. Rust `build.rs` reads it and generates Rust types at compile time.
- **Alternatives considered**:
  - Manual hand-coding: drifts from Gramps over time.
  - RelaxNG parsing: captures XML structure, not the object graph semantics (who-references-what).
  - Proc macros at runtime: more flexible but adds Python as a runtime dependency.
- **Consequences**:
  - (+) Schema stays in sync: one command to update for new Gramps versions.
  - (+) Pure Rust at runtime, no Python dependency.
  - (-) Build-time Python dependency. Acceptable since Gramps source is cloned locally.
  - (-) Codegen adds indirection. Mitigated by committing `schema.json` as an artifact.
  - ⚠ Security: The extractor executes code from a user-supplied `PYTHONPATH`. The user should only point this at a trusted Gramps source checkout. Document this warning.

### Decision 2: Typed directed multigraph as core abstraction

- **Context**: The strategy discussion identifies genealogy data as a graph. Gramps itself is an object graph.
- **Decision**: Model the in-memory state as a concrete `Graph` struct with `HashMap<Handle, Node>` for nodes and `Vec<Edge>` for edges, where `Node` is an enum over the 10 primary object types, and edges capture both ownership (Person contains EventRef) and reference (EventRef points to Event handle).
- **Alternatives considered**:
  - Direct XML generation without intermediate model: fast to build but unusable for the eventual "processing tool" goal.
  - Relational/table model: loses the natural graph structure, makes generation harder.
  - Generic `Graph<N, E>`: adds complexity without benefit since all nodes are `Node` enum variants.
- **Consequences**:
  - (+) Natural fit for generation — you build the graph incrementally.
  - (+) Self-validating — can check referential integrity before serialization.
  - (+) Reusable as a library for downstream genealogy-data processing tools (e.g., graph-based query, analysis, visualization).
  - (-) Graph layer is more code upfront than raw XML generation.

### Decision 3: Three generation entry points

- **Context**: Users need both convenience (quick random trees) and control (exact family structures).
- **Decision**: Three generation entry points, all producing the same `Graph`:
  1. **Random/property-based**: `generate_random(config)` — size, depth, date ranges, probability of citations/notes.
  2. **Fluent builder API** (separate `GraphBuilder` struct): `GraphBuilder::new(&mut graph).add_person("p1").with_name("John")...` — programmatic control.
  3. **Declarative scenario**: YAML/JSON scenario files that drive the `GraphBuilder` under the hood.
- **Consequences**:
  - (+) Covers the spectrum from quick fuzz testing to precise scenario authoring.
  - (+) All three produce the same `Graph` type, so they compose and can be tested with the same validation code.
  - (-) Three APIs to document. Mitigated by shared underlying `Graph` type.

### Decision 4: Procedurally generated source data

- **Context**: Names, places, and dates are needed for realistic-looking datasets.
- **Decision**: Use procedurally generated strings rather than bundled real name lists or place hierarchies. Names are built from a set of syllable/letter patterns producing plausible-but-fictional names (e.g., Markov-chain generation). Dates are drawn from configurable ranges with realistic distributions (adult ages 18-90, mother's age at childbirth 16-45, etc.). Place names follow hierarchical templates ("{prefix}ton", "{prefix}ville", "{county} County, {state}").
- **Alternatives considered**:
  - Bundled real name/place data: realistic but adds a curated data maintenance burden, and real names risk generating real people.
  - Fully random strings: fast but produces obviously fake output that's harder to visually inspect.
- **Consequences**:
  - (+) No data curation burden. No risk of generating real-person lookalikes.
  - (+) Configurable era/style via procedural parameters (Victorian-era names vs. modern names).
  - (-) Names won't match real-world distributions exactly. Acceptable for testing/example data.

### Decision 5: RelaxNG-driven serialization mapping

- **Context**: The serializer needs to map schema types to XML element/attribute names.
- **Decision**: Extract the `SerializationMap` from the Gramps RelaxNG schema (`grampsxml.rng`) as a build-time artifact, rather than hand-coding it. The extractor produces a `serialization_map.json` that maps schema types to XML element names, attribute names, and ordering constraints. If the RelaxNG schema is not available, the map can be hand-coded as a fallback with a prominent note.
- **Alternatives considered**:
  - Hand-coded table: simple but drifts from the RelaxNG schema over time.
  - Runtime RelaxNG parsing: adds a dependency and complexity.
- **Consequences**:
  - (+) Serialization stays in sync with the XML schema.
  - (+) Changes to the RelaxNG schema are automatically reflected.
  - (-) One more build-time artifact to maintain.

### Decision 6: Generate, then validate, then transform, then validate, then serialize

- **Context**: A generated graph may contain errors (dangling references, invalid dates). Each stage of data alteration must be followed by a validation step.
- **Decision**: Strict five-stage pipeline: **Generate → Validate → [Adversarially Transform] → Validate → Serialize**. Generation produces a Graph; the first validation pass checks structural and referential integrity before any transforms; the adversarial stage operates on a known-valid graph; a second validation pass verifies the transformed graph (adversarial transforms may optionally produce structurally invalid graphs for testing); serialization only runs on valid graphs. Adversarial transforms are post-generation graph modifications applied to a known-valid graph.
- **Consequences**:
  - (+) Clear separation of concerns with validation gates after every data-altering stage.
  - (+) Validation errors surface early with clear messages.
  - (+) Adversarial transforms are explicit, composable, and testable independently on a known-good baseline.
  - (+) A second validation pass catches any issues introduced by transforms.
  - (-) Slightly slower for very large graphs. Acceptable — validation is O(n) in graph size.
  - (+) Two categories make the contract clear: validity-preserving passes through the second validation; validity-breaking is expected to fail the second validation.
  - (-) Some strategies need both generation-time and post-generation code paths.

### Decision 7: Adversarial generation as a co-designed feature

- **Context**: The strategy discussion identifies adversarial datasets — valid but unusual structures — as critical for testing downstream tools.
- **Decision**: Adversarial generation is designed alongside the base generation engine, not bolted on after. Each adversarial strategy is a composable graph transform applied between generation and validation. Strategies are split into **two categories**:
  - **Category A — Generation-time strategies** (affect how generation works, gated by config flags): one-parent families, missing events, solo persons, many alternate names.
  - **Category B — Post-generation transforms** (composable functions that modify an already-generated graph): disconnected subgraphs, orphaned references, deep nesting, maximum ref chains.
  
  Within Category B, strategies are further split by their effect on validity:
  - **Validity-preserving transforms**: Produce graphs that remain structurally valid (e.g., disconnected subgraphs, deep nesting). These test downstream tool robustness.
  - **Validity-breaking transforms**: Deliberately produce graphs with structural errors (e.g., orphaned references — removes edges without removing the source references). These test that the validation stage correctly rejects invalid graphs.

  See Section 7.4 for the full catalog and mechanism.
- **Consequences**:
  - (+) The generation API can express adversarial constraints from the start.
  - (+) Strategies are explicit, testable, and independently selectable.
  - (+) Two clear sub-categories clarify which transforms preserve vs. break validity, resolving ambiguity about the validation contract.
  - (-) Some strategies need both generation-time and post-generation code paths.

## 5. Schema Extraction Design

### What the Python extractor does

```python
# extract/extract_schema.py pseudocode
#
# Usage: PYTHONPATH=/path/to/gramps/repo python extract_schema.py
# This imports gramps.gen.lib from a local Gramps source clone.
# ⚠ WARNING: Only point PYTHONPATH at a trusted Gramps source checkout.
#   The extractor imports and executes Python code from that path.

PRIMARY_TYPES = ["Person", "Family", "Event", "Place", "Source",
                 "Citation", "Repository", "Media", "Note", "Tag"]

for clsname in PRIMARY_TYPES:
    cls = getattr(gramps.gen.lib, clsname)

    # get_schema() — Gramps provides this on most primary objects.
    # It returns a JSON Schema dict describing the class fields.
    # If get_schema() is not available, fall back to introspection:
    #   - Inspect __init__ signature for parameter names and types
    #   - Walk MRO (method resolution order) for mixin base classes
    #     (e.g., CitationBase, NoteBase, MediaBase, AttributeBase)
    #   - Check field types: Handle (for handle_ref), list (for arrays),
    #     or embedded secondary objects (EventRef, ChildRef, etc.)
    schema = cls.get_schema() if hasattr(cls, 'get_schema') else introspect(cls)

    # Determine which fields are handle references and to which target types.
    # Heuristic: fields ending in "_handle" or "_list" containing Handle values
    # are cross-object references. Embedded *Ref objects (EventRef, ChildRef, etc.)
    # carry their own handle + metadata.
    for field_name, field_type in get_type_hints(cls).items():
        if is_handle_ref(field_type):
            record_edge(field_name, clsname, resolve_target_type(field_type))

    # Also record which mixin bases the class inherits from
    # (e.g., CitationBase → citation_list maps to Citation target)
    for base in cls.__mro__:
        if base.__name__.endswith("Base"):
            record_mixin(clsname, base.__name__)

    # Output merged record for this type
```

### What `schema.json` looks like

```json
{
  "version": "5.2",
  "primary_types": {
    "Person": {
      "fields": {
        "handle": { "type": "string", "kind": "handle", "required": true },
        "gramps_id": { "type": "string", "required": false },
        "gender": { "type": "enum", "values": [0, 1, 2, 3], "required": true },
        "primary_name": { "type": "embedded", "schema": "Name", "required": true },
        "alternate_names": { "type": "array", "items": "Name", "required": false, "cardinality": { "min": 0, "max": null } },
        "event_ref_list": {
          "type": "array",
          "items": {
            "embedded": "EventRef",
            "edges": [{ "link": "EventRef.ref", "target": "Event" }]
          },
          "required": false,
          "cardinality": { "min": 0, "max": null }
        },
        "family_list": {
          "type": "array",
          "items": {
            "kind": "handle_ref",
            "target": "Family"
          },
          "required": false,
          "cardinality": { "min": 0, "max": null }
        },
        "parent_family_list": {
          "type": "array",
          "items": {
            "kind": "handle_ref",
            "target": "Family"
          },
          "required": false,
          "cardinality": { "min": 0, "max": null }
        }
      },
      "inherit_mixins": [
        "CitationBase", "NoteBase", "MediaBase", "AttributeBase",
        "AddressBase", "UrlBase", "LdsOrdBase", "TagBase"
      ]
    }
  },
  "secondary_types": {
    "EventRef": {
      "fields": {
        "ref": { "type": "handle_ref", "target": "Event", "required": true },
        "role": { "type": "enum_ref", "target": "EventRoleType", "required": false },
        "citation_list": { "type": "array", "items": { "kind": "handle_ref", "target": "Citation" }, "required": false }
      }
    }
  },
  "enum_types": {
    "EventType": { "values": ["Birth", "Death", "Marriage", "..."] },
    "EventRoleType": { "values": ["Primary", "Family", "Witness", "..."] }
  }
}
```

Key additions versus the initial sketch:

- Every field now has `"required": true/false` — the validator uses this to check `MissingRequired`.
- Array fields have `"cardinality": { "min": ..., "max": ... }` — the validator uses this for `CardinalityViolation`.
- `null` means unbounded (no maximum).
- **Enum value discovery**: During schema extraction, for each field typed as a Gramps enum (e.g., `EventType`, `EventRoleType`, `Gender`), the extractor introspects the Python enum class to enumerate all member values. For Gramps classes that define static type constants (e.g., `EventType.BIRTH = 'Birth'`), the extractor iterates over these class attributes. For fields using integer-keyed enumerations (e.g., `Gender.MALE = 0`), both the integer value and the symbolic name are recorded.

### What `build.rs` generates

From `schema.json`, `build.rs` generates:

1. `Node` enum — one variant per primary type
2. `Edge` enum — one variant per edge kind, parameterized by source/target types
3. Per-type data structs (PersonData, FamilyData, …)
4. Ref structs (EventRef, ChildRef, …) — including all metadata fields from the schema
5. Enum types (EventType, EventRoleType, …)
6. `Schema` runtime metadata — for use by generators and validators (required/cardinality info)

**Handle-reference resolution in codegen:**

- Fields with `"kind": "handle_ref"` generate a `Handle` (String) field in the data struct AND an Edge from the source node to the target node.
- Fields with `"embedded"` items containing `"edges"` generate an Edge from the source node to the referenced target node, plus an inline metadata struct.
- Mixin fields (`CitationBase`, `NoteBase`, etc.) generate edges to the respective target types.

## 6. Graph Model Design

### Core types

```rust
// crates/typed-graph/src/graph.rs (sketch)

use std::collections::HashMap;

/// Handle uniquely identifies a primary object in the graph.
/// Matches Gramps' own handle semantics (UUID v4 string, 36 chars).
/// Generated by the RNG; all handles are unique within the graph.
pub type Handle = String;

/// The typed directed multigraph.
/// Concrete (not generic) — all nodes are Node enum variants,
/// all edges are Edge enum variants.
pub struct Graph {
    /// All primary nodes, keyed by handle.
    nodes: HashMap<Handle, Node>,
    /// Edge index: source → (link_type, target)
    edges: Vec<Edge>,
    /// Reverse edge index: target → [edge_index].
    reverse_edges: HashMap<Handle, Vec<usize>>,
    /// Validation state set by the last validation pass.
    /// Serialization checks this and returns an error if it is not `Valid`.
    validation_state: ValidationState,
}

pub enum ValidationState {
    /// Graph has not been validated yet.
    Unvalidated,
    /// Last validation passed with no errors (warnings may remain).
    Valid,
    /// Last validation failed with the given errors.
    Invalid(Vec<ValidationError>),
}

pub enum Node {
    Person(PersonData),
    Family(FamilyData),
    Event(EventData),
    Place(PlaceData),
    Source(SourceData),
    Citation(CitationData),
    Repository(RepositoryData),
    Media(MediaData),
    Note(NoteData),
    Tag(TagData),
}

/// Each edge variant carries the metadata appropriate to its kind.
/// No catch-all `EdgeMetadata` — every variant is explicit.
pub enum Edge {
    /// Person.family_list → Family
    PersonFamily { source: Handle, target: Handle },
    /// Person.parent_family_list → Family
    PersonParentFamily { source: Handle, target: Handle },
    /// Person.event_ref_list → Event (with role metadata)
    PersonEventRef { source: Handle, target: Handle, role: EventRoleType },
    /// Family.father_handle → Person
    FamilyFather { source: Handle, target: Handle },
    /// Family.mother_handle → Person
    FamilyMother { source: Handle, target: Handle },
    /// Family.child_ref_list → Person (with relation metadata)
    FamilyChildRef { source: Handle, target: Handle, relation: ChildRefType },
    /// Event.place_handle → Place
    EventPlace { source: Handle, target: Handle },
    /// Citation.source_handle → Source
    CitationSource { source: Handle, target: Handle },
    /// Mixin: citation_list → Citation
    CitationRef { source: Handle, target: Handle },
    /// Mixin: note_list → Note
    NoteRef { source: Handle, target: Handle },
    /// Mixin: media_list → Media
    MediaRef { source: Handle, target: Handle },
    /// Mixin: tag_list → Tag
    TagRef { source: Handle, target: Handle },
    /// Mixin: attribute_list → Attribute (embedded, not a primary object)
    AttributeRef { source: Handle, target: Handle },
    /// Mixin: url_list → Url (embedded, not a primary object)
    UrlRef { source: Handle, target: Handle },
    /// Mixin: lds_ord_list → LdsOrd (embedded)
    LdsOrdRef { source: Handle, target: Handle },
    /// Person.person_ref_list → Person (peer references)
    PersonRef { source: Handle, target: Handle },
    /// Repository.ref_list → Repository (e.g., Media → Repository)
    RepositoryRef { source: Handle, target: Handle },
    /// Place.place_ref_list → Place (hierarchical place references)
    PlaceRef { source: Handle, target: Handle },
    // ... additional edge kinds as discovered by schema extraction
}
```

### Edge classification

Edges fall into three categories, derived from the schema:

| Category | Examples | Mechanism |
|---|---|---|
| **Direct handle ref** | `Person.family_list → Family`, `Family.father_handle → Person` | Node field holds handle; edge links source→target |
| **Embedded ref** | `EventRef.ref → Event`, `ChildRef.ref → Person` | The *Ref struct holds both the handle and metadata (role, relation, etc.) |
| **Mixins** | CitationBase.citation_list → Citation, NoteBase.note_list → Note | Reusable across most primary types |

This classification is extracted automatically from the Python schema — the extractor identifies which fields are handle references and to which target types.

### Validation layers

The validator runs three passes over the graph:

```rust
pub enum ValidationError {
    /// An edge references a handle that doesn't exist in the graph
    DanglingReference { source: Handle, link: &'static str, target: Handle },
    /// A required field is missing (e.g., Person with no primary_name)
    /// Determined from schema.json's "required": true on each field
    MissingRequired { node: Handle, field: &'static str },
    /// A cardinality constraint is violated (e.g., Family with 3 parents)
    /// Determined from schema.json's "cardinality": { "min": ..., "max": ... }
    CardinalityViolation { node: Handle, field: &'static str, expected: String, actual: usize },
    /// Genealogical plausibility: birth after death, parent younger than child, etc.
    PlausibilityWarning { node: Handle, message: String },
}
```

Validation is always run before serialization. Plausibility warnings are advisory (non-blocking) by default; the `--strict` flag promotes them to errors.

**How the validator determines required/cardinality:**
The `Schema` runtime metadata (generated by `build.rs` from `schema.json`) includes a `required_fields: Vec<&'static str>` and `cardinality_constraints: HashMap<&'static str, (Option<usize>, Option<usize>)>` for each node type. The validator reads these to check `MissingRequired` and `CardinalityViolation`. After a successful validation pass, `Graph.validation_state` is set to `ValidationState::Valid`.

## 7. Generation Design

### 7.1 Procedural data sources

Names, dates, and places are generated procedurally — no bundled real-world data sets.

- **Names**: Markov-chain syllable generation producing plausible-but-fictional names with era-appropriate style. A trigram Markov chain is built from syllable tables keyed by style (e.g., `"victorian"` draws from 19th-century English syllable patterns; `"nordic"` draws from Scandinavian patterns). Each style swap replaces both the syllable inventory and transition probabilities. The RNG is seeded and passed explicitly through all generators. All strings are UTF-8. The name generator currently targets Latin-character patterns; future work may add generators for other scripts.
- **Dates**: Represented as `DateValue` structs matching Gramps' `DateVal`:

  ```rust
  struct DateValue {
      year: u16,          // Calendar year (1-9999). Note: Gramps supports BC dates via a calendar field; this initial implementation targets AD dates only. BC support may be added in the future.
      month: Option<u8>,  // 1-12, or None if year-only
      day: Option<u8>,    // 1-31, or None if month-only
      quality: DateQuality, // Exact, Estimated, Calculated
      modifier: DateModifier, // None, Before, After, About, Range, Span
      text: Option<String>, // Free-form date string (Gramps date text)
  }

  enum DateQuality { Exact, Estimated, Calculated }
  enum DateModifier { None, Before, After, About, Range { start: DateValue, end: DateValue }, Span { start: DateValue, end: DateValue } }
  ```

  Dates are drawn from configurable ranges. Genealogical constraints enforced at generate time: birth before death, mother's age at childbirth 16-45, father's age at childbirth 16-70, marriage after age 16.

  The `text` field is synthesized from the structured fields: e.g., `"about 1870"` for `DateModifier::About`, `"between 1890 and 1900"` for `Range`, `"estimated 1805"` for `Quality::Estimated`. When no modifier is present and quality is `Exact`, the text is a standard YYYY-MM-DD or YYYY-MM or YYYY representation.
- **Places**: Hierarchical template system: `"{prefix}{suffix}, {county} County, {state}"` where `{prefix}` and `{suffix}` are drawn from procedurally generated tables. Place hierarchies build recursively (city → county → state → country). Place count scales with person count (roughly 1 place per 5-10 events). Events in the same geographic region and era share place nodes where plausible. Hierarchy depth is configurable via `place_depth` (default 3). Top-level places (countries) are drawn from a small fixed set of procedurally named countries to simulate realistic geographic clustering.

**Data contracts for procedural generators**:

- Names: 1-40 characters, UTF-8, Latin script. Must not be empty. Must differ from other generated names within the same graph to avoid ambiguous references.
- Dates: Year in [1, 9999]. If month is `Some`, it must be in [1, 12]. If day is `Some`, it must be valid for the given month/year (handling leap years).
- Places: Depth in [1, `config.place_depth`] (default 3). Each level's name is non-empty. Top-level places are reused within a graph for geographic coherence.
- Generators must not panic on valid input. If constraints make generation impossible (e.g., all name combinations exhausted for a given style), return a `GenerationError` that includes the seed for reproducibility.

### 7.2 Random generation algorithm

The RNG is seeded from a configurable `--seed` value (or randomly generated if not provided). The seed is recorded in the output metadata for reproducibility.

```
function generate_random(config: Config, rng: &mut Rng) -> Graph:
    graph = new Graph()

    // 1. Create N Person nodes with procedural names
    for i in 1..config.person_count:
        p = Person(procedural_name(config.name_style, rng),
                   random_gender(rng),
                   random_dates(config.date_range, rng))
        graph.add_node(p)

    // 2. Pair up parents, create Family nodes.
    //    Parent selection algorithm:
    //    a. Sort persons by birth year (ascending). Persons without a birth
    //       date are assigned a default birth year based on their generation
    //       layer (layer 0: 1970-2000, layer 1: 1940-1970, etc.)
    //    b. For each family, select a father and mother from the pool
    //       whose birth years are within a plausible age difference (0-20 years)
    //       and who are not already siblings or parent-child
    //    c. Track assigned parent roles to avoid reusing a person as
    //       both parent and child in the same generation layer
    //    d. If no eligible parents are found in the current generation
    //       layer, expand to adjacent layers (±1 generation). Eligible
    //       pools are: same-generation pool (ideal), then one generation
    //       older (plausible), then one generation younger (less common
    //       but valid). If all pools are exhausted, emit a plausibility
    //       warning and create a family with a single parent.
    for i in 1..config.family_count:
        (father, mother) = select_parents(graph, config, rng)
        f = Family(father_handle, mother_handle)
        graph.add_node(f)

    // 3. Assign children to families.
    //    Children are drawn from the next generation of persons.
    //    Age constraint: child's birth year > max(father's birth year + 16,
    //    mother's birth year + 16) and < mother's birth year + 50
    //    (outside this range: plausibility warning, not rejection)
    for each family:
        children = assign_children(graph, family, config, rng)
        for child in children:
            graph.add_edge(ChildRef { family, child })

    // 4. Create Event nodes and link via EventRef
    for each person:
        e_birth = Event(Birth, person.birth_date)
        graph.add_node(e_birth)
        graph.add_edge(PersonEventRef { person, e_birth, role: Primary })

    // 5. Optionally add Places, Sources, Citations, Notes, Media, Tags
    if config.with_citations:
        ...

    return graph
```

### 7.3 Fluent builder API (GraphBuilder)

The builder is a **separate `GraphBuilder` struct** (not methods on `Graph`). It takes a `&mut Graph` reference and adds nodes/edges to it:

```rust
let mut graph = Graph::new();

let mut builder = GraphBuilder::new(&mut graph);

let p1 = builder.add_person("p1")
    .with_name("John", "Smith")
    .with_gender(Gender::Male)
    .with_birth_date(DateValue::new(1870))
    .with_death_date(DateValue::new(1945))
    .build();  // returns Handle

let p2 = builder.add_person("p2")
    .with_name("Mary", "Smith")
    .with_gender(Gender::Female)
    .with_birth_date(DateValue::new(1872))
    .build();

let family = builder.add_family("f1")
    .with_father(&p1)
    .with_mother(&p2)
    .with_marriage_date(DateValue::new(1895))
    .build();

builder.add_person("p3")
    .with_name("Robert", "Smith")
    .with_parent_family(&family)
    .with_birth_date(DateValue::new(1898))
    .build();
```

Key design points:

- `add_person("p1")` takes a string handle (or auto-generates a UUID v4 if omitted).
- `build()` returns a `Handle` (the handle string), not a reference.
- Time-based methods like `with_birth_date()` take a `DateValue` struct, not a plain integer.
- The builder guarantees that graphs it produces pass structural and referential validation: it checks required fields at build time (e.g., at minimum a person must have a name) and ensures all handle references resolve to existing nodes. This means the builder API guarantees structural and referential validity at build time (required-field and handle-resolution checks). Random generation (Section 7.2) does NOT use this guarantee — it relies on the pipeline Validator. All graphs pass through the validation gates described in Section 11 regardless of how they were constructed.

### 7.4 Adversarial generation

Adversarial strategies are designed alongside the generation engine. Strategies fall into two categories:

**Category A: Generation-time strategies** (affect how the base generation algorithm works, gated by config flags):

| Strategy | Mechanism | Tests for |
|---|---|---|
| **One-parent families** | Generator skips father or mother assignment for a configurable fraction of families | Tools that assume both parents always present |
| **Missing events** | Generator skips birth/death events for a configurable fraction of persons | Tools that assume every person has events |
| **Solo persons** | Generator creates persons with no families, no events, just a name | Tools that assume every person belongs to a family |
| **Many alternate names** | Generator adds 5-20 alternate names to a configurable fraction of persons | Tools that display only primary names |

**Category B: Post-generation transforms** (composable functions that modify an already-generated graph):

| Strategy | Mechanism | Tests for |
|---|---|---|
| **Disconnected subgraphs** | Splits the graph into multiple unrelated clusters by deleting cross-cluster family edges | Tools that assume a single connected tree |
| **Orphaned references** | Removes some edges from citation/note/media references while keeping the target nodes | Tools that scan reference counts unevenly |
| **Deep nesting** | Replaces place hierarchies with 5-10 level deep chains | Tools with recursion limits or fixed-depth assumptions |
| **Maximum ref chains** | Creates legal maximum-length reference chains (Event → Citation → Source → Repository → Note → ...) | Stack overflow or O(n²) traversal bugs |

Both categories are applied before the second validation gate. The pipeline is:

```
Generate (with Category A flags) → Validate (Gate 1) → Category B transforms → Validate (Gate 2) → Serialize
```

Within Category B, strategies are further divided:

- **Validity-preserving** (disconnected subgraphs, deep nesting, max ref chains): produce graphs that pass Validation Gate 2.
- **Validity-breaking** (orphaned references): produce graphs that are expected to fail Validation Gate 2. The errors are reported and inspected, but the graph is not serialized.

Adversarial strategies are selectable individually (`--adversarial disconnected,one-parent`) or as a full suite (`--adversarial all`).

### 7.5 Configuration schema

```yaml
# example scenario: three-generation tree
name: "modest-family-tree"
person_count: 50
family_count: 20
generations:
  depth: 3
  children_per_family: { min: 1, max: 4 }
date_range:
  start: 1850
  end: 2025
  era: modern          # affects name distributions
with_citations: true
with_places: true
with_media: false
seed: 42               # optional, for reproducible generation
adversarial:
  enabled: false       # set true for unusual but valid structures
  strategies:          # when enabled, specific strategies to apply
    - disconnected
    - one_parent
```

## 8. XML Serialization Design

The serializer walks the graph and emits Gramps XML following the RelaxNG schema (`grampsxml.rng`).

### Serialization Map

The mapping from schema types to XML element/attribute names is driven by a `SerializationMap` extracted from the Gramps RelaxNG schema at build time (see Decision 5). If the RelaxNG schema is unavailable, a hand-coded fallback map is used.

The `SerializationMap` is defined as:

```rust
struct SerializationMap {
    /// Maps primary type name to its XML element info
    type_map: HashMap<String, XmlTypeInfo>,
    /// Maps edge variant to the XML nesting and attributes
    edge_map: HashMap<String, XmlEdgeInfo>,
    /// Order in which type sections appear in the XML output
    section_order: Vec<String>,
}

struct XmlTypeInfo {
    element_name: String,            // e.g., "person"
    section_name: String,            // e.g., "people"
    attributes: Vec<XmlAttribute>,   // e.g., ("handle", "handle"), ("id", "gramps_id")
    children: Vec<XmlChild>,         // embedded elements and their ordering
}

struct XmlEdgeInfo {
    /// The parent XML element this edge nests inside
    parent_element: String,
    /// The XML element name for this edge (e.g., "eventref" for PersonEventRef)
    element_name: String,
    /// Attribute mappings (field name → XML attribute name)
    attributes: Vec<(String, String)>,
}

struct XmlAttribute {
    field: String,      // field name in the data struct
    attr_name: String,  // XML attribute name (e.g., "hlink" for handle refs)
}

struct XmlChild {
    element_name: String,
    source: XmlChildSource,   // inline struct, array, or edge
}
```

**RelaxNG extraction algorithm**:

1. Parse the RelaxNG compact schema (`grampsxml.rng`) and locate top-level element definitions corresponding to primary types (`person`, `family`, `event`, etc.)
2. For each element, extract its attributes and their names, mapping them to schema field names using the following heuristic (tried in order):
   - Exact name match (case-sensitive)
   - snake_case → lowerCamelCase conversion
   - Manual override from a committed `name_overrides.json` table
3. Locate child element patterns (e.g., `<eventref>` inside `<person>`) and identify which are inline secondary objects vs. handle references. Handle RelaxNG structural patterns:
   - `<optional>` → `Option<T>` or cardinality `{ min: 0, max: 1 }`
   - `<zeroOrMore>` → `Vec<T>` or cardinality `{ min: 0, max: null }`
   - `<oneOrMore>` → `Vec<T>` or cardinality `{ min: 1, max: null }`
   - `<choice>` → Enum or union type; if the choices are element names, treat as inline polymorphic children
4. Map handle-reference child elements to the corresponding `Edge` variant
5. Extract section ordering from the `<database>` element's children sequence
6. Output `serialization_map.json`

If the RelaxNG schema is unavailable (e.g., user doesn't have the Gramps source), a hand-coded fallback `SerializationMap` is used as a Rust constant. This map covers the most common types and is annotated with a prominent note that it may drift from the actual schema.

### Output ordering

Gramps XML groups objects by type:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2026-07-27" version="5.2"/>
    <researcher>
      <resname>Generated by gramps-gen</resname>
    </researcher>
  </header>
  <tags>       ... </tags>
  <events>     ... </events>
  <people>     ... </people>
  <families>   ... </families>
  <citations>  ... </citations>
  <sources>    ... </sources>
  <places>     ... </places>
  <objects>    ... </objects>
  <repositories> ... </repositories>
  <notes>      ... </notes>
</database>
```

### Serialization strategy

```rust
impl Graph {
    fn to_xml(&self, serialization_map: &SerializationMap, writer: &mut impl std::io::Write) -> Result<(), SerializationError> {
        // Walk nodes by type, emit in Gramps' expected order
        // Use the SerializationMap to determine element/attribute names
        // Handle references emit as `hlink` attributes
        // Embedded refs emit inline elements
        // Streams directly to writer for memory efficiency on large graphs
        // Returns Err for I/O errors or unsupported types
    }
}
```

### Error handling during serialization

The serializer returns `Result<(), SerializationError>` where `SerializationError` covers:

- I/O errors (disk full, permission denied)
- Unsupported node types (should not happen if validation passed)
- Missing required fields that weren't caught by validation (defensive check)

**XML escaping**: All string output is properly escaped for XML (handled by `quick-xml`). If using a hand-coded serialization fallback, ensure proper escaping of `&`, `<`, `>`, `"`, `'` per the XML specification.

## 9. CLI Interface

```
gramps-gen 0.1.0
Generate valid Gramps family tree datasets

USAGE:
    gramps-gen <COMMAND>

COMMANDS:
    generate     Generate a random family tree dataset
    extract-schema  Extract the schema from a Gramps installation
    validate     Validate a schema.json or .gramps file
    help         Print this message

OPTIONS (generate):
    --count, -n <N>          Number of persons to generate [default: 200]
    --depth, -d <N>          Number of generations [default: 3]
    --output, -o <FILE>      Output .gramps file [default: output.gramps]
    --seed <N>               RNG seed (u64 integer) for reproducible generation [default: random]
    --strict                 Promote plausibility warnings to errors
    --adversarial <LIST>     Comma-separated adversarial strategies, or "all"
    --progress-interval <N>  How often to report generation progress [default: 100]
    --save-intermediate <DIR>  Save intermediate graph state after each pipeline stage
    --config, -c <FILE>      YAML scenario file (overrides other options)

EXAMPLES:
    gramps-gen generate --count 2000 --depth 5 --output family.gramps
    gramps-gen generate --config scenario.yaml --output stress.gramps
    gramps-gen generate --count 100 --adversarial all --strict
    gramps-gen extract-schema ~/src/gramps
    gramps-gen validate family.gramps

⚠ SECURITY WARNING for `extract-schema`: The extractor imports and executes
  Python code from the path provided via `PYTHONPATH`. Only point this at
  a trusted Gramps source checkout. Do NOT use untrusted or third-party
  code as the source.
```

## 10. Implementation Plan

### Phase 1: Schema extraction (2 days)

1. Write `extract/extract_schema.py` that imports `gramps.gen.lib` and outputs `schema.json`
   - Include `required`, `cardinality` in schema output
   - Include fallback introspection for classes without `get_schema()`
   - Include edge-kind discovery (handle_ref vs embedded ref)
2. Write `build.rs` that reads `schema.json` and generates Rust types
   - Generate `Node` enum, `Edge` enum, per-type data structs, ref structs, enum types
   - Generate `Schema` runtime metadata with required/cardinality info
3. Verify against Gramps 5.2 classes
4. Write unit tests: schema.json parsing, edge classification, type generation

### Phase 2: Graph core (2.5 days)

1. Implement `Graph`, `Node`, `Edge` types (concrete, not generic)
   - Include `validation_state: ValidationState` field on Graph
2. Implement graph construction/query API (add node, add edge, iterate by type)
3. Implement validation pass (structural + referential), using Schema metadata
   - Validation sets `graph.validation_state = ValidationState::Valid` on success
4. Write unit tests: add/query nodes/edges, validate required fields, detect dangling references

*Note: Phase 2 depends on Phase 1's `build.rs` codegen. Phases 1 and 2 may require iteration: if codegen doesn't produce the right `Schema` metadata shape, the validation code will need adjustments.*

### Phase 3: GraphBuilder API (1 day)

1. Implement `GraphBuilder` struct (separate from `Graph`)
2. Implement per-type builder methods (`add_person`, `add_family`, etc.)
3. Implement `with_*` chainable setters using `DateValue` for dates
4. Write unit tests: builder produces correct Graph, builder validates required fields

### Phase 4: XML serializer (2 days)

1. Extract `SerializationMap` from RelaxNG schema (or hand-code fallback)
2. Implement the XML writer with a `Write`-based API, verified against the RelaxNG schema
3. Write unit tests: serializer with known inputs, edge cases (empty graph, special characters)
4. *Round-trip test deferred to Phase 7 integration tests (requires builder + random generator + serializer)*

### Phase 5: Random generation (3 days)

1. Implement procedural name/date/place generators (seeded RNG, `DateValue` structs)
2. Implement the random generation algorithm with configurable parameters
3. Add genealogical plausibility constraints (age ordering, parent selection algorithm)
4. Add `--seed` flag for reproducible generation
5. Write property-based tests: generate → validate always passes, same seed → same graph

### Phase 6: Adversarial generation (2 days)

1. Implement generation-time adversarial strategies (Category A: one-parent, missing events, solo persons, many names)
2. Implement post-generation adversarial transforms (Category B: disconnected, orphaned, deep nesting, max ref chains)
   - Split Category B into validity-preserving and validity-breaking sub-categories
3. Add `--adversarial` flag and config support
4. Write property-based tests:
   - Every validity-preserving adversarial strategy produces a graph that passes structural + referential validation
   - Validity-breaking strategies produce graphs with the expected error types

### Phase 7: Scenario runner + CLI (2 days)

1. Implement YAML scenario parsing and translation to builder calls
2. Wire everything into the CLI (clap)
3. Add `--strict` flag for plausibility warnings
4. Add progress reporting for large generations
5. Add error handling for I/O failures, config parsing errors, validation failures
6. Documentation and README

### Phase 1-2 iteration notes

Phases 1 and 2 are interdependent: the codegen from `build.rs` must produce `Schema` metadata that the validation code in Phase 2 consumes. The recommended workflow:

1. Implement the extractor and `build.rs` with a minimal schema (one primary type).
2. Generate types, verify they compile.
3. Implement `Graph` + validation for that one type.
4. Expand to full schema.

This iterative loop avoids building the full codegen pipeline before discovering that the metadata shape doesn't match the validation code's needs.

### Revised timeline

| Phase | Duration | Dependencies |
|---|---|---|
| 1. Schema extraction | 2 days | Gramps 5.2 source clone |
| 2. Graph core | 2.5 days | Phase 1 |
| 3. GraphBuilder API | 1 day | Phase 2 |
| 4. XML serializer | 2 days | Phase 2, Phase 3, RelaxNG schema |
| 5. Random generation | 3 days | Phase 2, Phase 3 |
| 6. Adversarial generation | 2 days | Phase 5 |
| 7. Scenario runner + CLI | 2 days | Phase 3, Phase 4, Phase 5, Phase 6 |
| **Total** | **14.5 days** | | |

## 11. Pipeline Model (Data Processing Stages)

The tool follows a strict five-stage pipeline, aligned with the data-pipeline-architecture model (ingestion → transformation → output) with an explicit adversarial stage and validation gates after every data-altering stage:

```
┌──────────────┐    ┌────────────┐    ┌──────────────────────┐    ┌────────────┐    ┌──────────────┐
│  Generation  │───▶│ Validation │───▶│  Adversarial Transform│───▶│ Validation │───▶│ Serialization│
│  (Ingestion) │    │  (Gate 1)  │    │  (Transformation)    │    │  (Gate 2)  │    │  (Output)    │
└──────────────┘    └────────────┘    └──────────────────────┘    └────────────┘    └──────────────┘
```

### Stage 1: Generation (data production)

**Responsibility**: Create a `Graph` from configuration parameters. This stage produces data from scratch — it does not ingest external input. No validation or transformation happens here.

- Input: `Config` (size, depth, date ranges, flags)
- Output: `Graph` (may be structurally valid or not)
- Generators do NOT perform structural validation — they just produce data
- Generators may enforce genealogical plausibility heuristics (birth before death, age ranges) as part of producing sensible data; these are not structural constraints
- RNG is seeded and passed explicitly for reproducibility

### Stage 2: Validation Gate 1

**Responsibility**: Check structural and referential integrity of the generated graph before any transforms are applied.

- Input: `Graph` (from Stage 1)
- Output: `Result<(), Vec<ValidationError>>`
- Three passes: structural (required fields, cardinality), referential (dangling handles), plausibility (warnings)
- If validation fails, the user is shown errors and generation stops (no transforms are applied to an invalid graph)
- Plausibility warnings are non-blocking (blocking with `--strict`)

### Stage 3: Adversarial Transform (Transformation)

**Responsibility**: Apply composable graph transforms that produce unusual or deliberately invalid graph structures.

- Input: `Graph` (known-valid from Stage 2)
- Output: `Graph` (modified; may be valid or invalid depending on transform type)
- Each strategy is a pure function: `fn transform(Graph) -> Graph`
- Strategies are applied in sequence, each independently testable
- Two sub-categories:
  - **Validity-preserving** transforms: produce structurally valid graphs (e.g., disconnected subgraphs, deep nesting)
  - **Validity-breaking** transforms: deliberately produce structurally invalid graphs (e.g., orphaned references)
- This stage is skipped when adversarial mode is disabled

### Stage 4: Validation Gate 2

**Responsibility**: Re-check structural and referential integrity after adversarial transforms.

- Input: `Graph` (from Stage 3, or directly from Stage 2 if adversarial mode is disabled)
- Output: `Result<(), Vec<ValidationError>>`
- Same checks as Stage 2
- For validity-preserving transforms: all errors must be absent
- For validity-breaking transforms: expected errors are reported and can be compared against an expected error set
- Must pass validation before serialization (validity-breaking transforms intentionally fail this gate; their output is inspected but not serialized)

### Stage 5: Serialization (Output)

**Responsibility**: Convert the validated Graph to Gramps XML format.

- Input: `Graph` (guaranteed valid by Stage 4)
- Output: `Result<(), SerializationError>` (Gramps XML written to the provided `Write` target)
- Pure format conversion — no business logic
- Internal graph types are not leaked to the output

### Stage decoupling

- Stages are currently in-memory (no intermediate storage), which is acceptable for the generation use case.
- Each stage is independently testable with mocked inputs.
- A failure in Serialization does not require re-running Generation.
- For debugging and replay, the `--save-intermediate <DIR>` flag writes the graph state after Stage 1 (Generation) and Stage 3 (Adversarial Transforms) as intermediate artifacts. If an adversarial transform panics, the original valid graph is preserved.

### Data contracts

| Contract boundary | Guarantee | Breach handling |
|---|---|---|
| Generation → Validation Gate 1 | Graph is structurally complete (all nodes have handles, no missing fields that would cause undefined behavior) | Validation catches and reports all errors |
| Validation Gate 1 → Adversarial | Graph is fully valid (no structural or referential errors) | Adversarial transforms assert this as a precondition |
| Adversarial → Validation Gate 2 | Graph is structurally sound enough for transform processing; validity-preserving transforms maintain validity; validity-breaking transforms may introduce known errors | Validation Gate 2 catches all errors; validity-breaking errors are expected and reported separately |
| Validation Gate 2 → Serialization | Graph is fully valid (no errors, warnings may remain) | Serialization checks `graph.validation_state` (must be `Valid`) and returns an error if not set |
| Serialization → File | Valid Gramps XML | CLI reports I/O errors to user |

## 12. Error Handling, Logging & Observability

### Error handling strategy

- **Error message format**: Every error reported to the user includes: (1) the affected handle or resource, (2) the violated constraint or operation, and (3) a suggested fix where applicable. Example: `Person p-abc123: missing required field 'primary_name'. Add a name to this person.`
- **Validation errors**: Collected into a `Vec<ValidationError>`, all reported to the user at once (not fail-fast). Validation errors are written to stderr as JSON Lines for machine consumption (one error per line). Exit code: 0 for valid, 1 for validation errors, 2 for I/O errors.
- **I/O errors**: Propagated via `Result<_, io::Error>` from the CLI/Serializer; reported with context (file path, operation).
- **Config parsing errors**: Reported with line/column information from serde_yaml.
- **Generation errors**: Should not occur under normal operation; if they do, the RNG seed is printed so the user can reproduce the exact failure.

### Logging and progress

- **Progress reporting**: Controlled by `--progress-interval <N>` (default: 100). For generation runs where person count > progress interval, print periodic progress: "Generated 500/2000 persons..." every N persons. Set to 0 to disable.
- **Logging**: Use the `log` crate with controlled verbosity via `-v` flag. Not to stdout by default.
- **Seed recording**: The seed used for generation is printed to stderr at the start and embedded in the output XML metadata.

### Performance considerations

- **Memory**: Each `Handle` is a String (typically 36 chars, 36 bytes). For 100k persons with ~500k edges, estimated memory: ~50-100 MB for handles + nodes + edges. Acceptable for a CLI tool.
- **Validation**: O(n) in graph size. For 100k nodes, should complete in < 1 second.
- **Serialization**: O(n) in output size. Large graphs may produce multi-MB XML files — the `Write`-based API streams directly to the output file to avoid building the full XML in memory.
- **Optimization deferred**: If memory becomes a concern, switch to `smol_str` or interned strings for handles. Not needed for initial implementation.

## 13. Dependencies

| Crate | Dependency | Purpose |
|---|---|---|
| `typed-graph` | `rand` (0.8+) | RNG for procedural generation |
| `typed-graph` | `serde`, `serde_json` | Schema parsing (build-time) |
| `typed-graph` | `uuid` (1.x) | UUID v4 generation for handles |
| `output` | `quick-xml` | XML writing |
| `cli` | `clap` (4.x) | CLI argument parsing |
| `cli` | `serde_yaml` | YAML scenario file parsing |
| `cli` | `log` + `env_logger` | Logging and progress output |
| Workspace | `serde` | Common serialization |

### Security notes

- **YAML deserialization**: `serde_yaml` targets a concrete `Config` struct with no `#[serde(untagged)]` or trait-object deserialization. Only the expected config shape is deserializable.
- **XML parsing (round-trip tests)**: Any XML parser used in the round-trip or `validate` command must disable external entity resolution (XXE protection).
- **Build script**: The committed `schema.json` is trusted input. In CI, verify that re-running the extractor against a known-good Gramps checkout produces identical output.

## 14. Resolved Decisions

1. **Name/date source data**: Procedurally generated strings. No bundled real name lists or place hierarchies. See Decision 4 for details.

2. **Adversarial generation**: Designed alongside the base generation engine, not bolted on after. See Decision 7 for the two-category strategy model.

3. **Gramps XML version targeting**: Only the latest stable Gramps release (currently 5.2, XML namespace `1.7.2`). The schema extractor is locked to the corresponding `maintenance/gramps52` branch.

4. **Gramps source access**: Clone the Gramps repository locally. The extractor imports directly from the source tree via `PYTHONPATH`. No pip install required. The extractor script is run manually when upgrading to a new Gramps version; the resulting `schema.json` is committed to the repo.

5. **Builder API**: Separate `GraphBuilder` struct (not methods on `Graph`). See Decision 3 and Section 7.3.

6. **Graph type**: Concrete (not generic). See Decision 2.

7. **Serialization mapping**: Extracted from the RelaxNG schema at build time. See Decision 5.

8. **Pipeline model**: Five-stage pipeline: Generate → Validate → Adversarially Transform → Validate → Serialize. See Section 11.

9. **Date representation**: Full `DateValue` structs (matching Gramps `DateVal`), not plain integers. See Section 7.3.

10. **Seed-based reproducibility**: All RNG operations take an explicit `&mut Rng` parameter. The seed is user-configurable via `--seed`. See Section 7.2.

## 15. Testing Strategy

### 15.1 Unit tests (per component)

| Component | What to test | None-One-Many |
|---|---|---|
| Schema parser | Parse valid/invalid/missing schema.json | Empty file, single type, full schema |
| Graph | Add/query nodes, add/query edges, reverse edge index, validation_state flag | Empty graph, one node, many nodes |
| Validation | Detect missing required fields, dangling refs, cardinality violations; verify validation_state transitions | No errors, one error, many errors |
| Procedural names | Generate names with different styles and seeds | Empty seed, one name, many names |
| Date generation | Enforce age constraints, date ranges; test boundary cases (month=13, day=32, year=0) | No dates, one date range, overlapping ranges, boundary values |
| XML serializer | Serialize known graph structures | Empty graph, one node, complex graph |
| GraphBuilder | Build graph via fluent API | Single person, family with child, complex tree |
| Adversarial transforms | Apply each strategy to known graphs; test adversarial-disabled (default) produces normal output | No-op, single transform, all transforms |
| Schema extractor | Run against mock Gramps-like classes, verify JSON output; golden-file regression test against pinned Gramps version | Single class, multiple classes, mixins |

### 15.2 Property-based tests

| Invariant | Description |
|---|---|
| **Round-trip** | For any valid Graph: `generate → serialize → parse XML → re-validate` produces the same structure |
| **Idempotency** | `generate(config, seed)` == `generate(config, seed)` (same output for same seed) |
| **Full-seed determinism** | For a fixed seed, verify exact same graph structure and all string values match (validates that all RNG sources share the seeded RNG) |
| **Validation** | Every non-adversarial generated graph passes structural + referential validation |
| **Cardinality enforcement** | For 10,000 random generations with default config, no cardinality violations are found (e.g., no Family with >1 father) |
| **Empty graph validation** | Validating an empty graph succeeds with zero errors |
| **Adversarial validity** | Every validity-preserving adversarial strategy produces a graph that passes structural + referential validation; validity-breaking strategies produce expected errors |
| **Builder consistency** | Building a graph via the builder API produces the same structure as generating it with equivalent config |
| **Error messages** | Validation errors, I/O errors, and config parsing errors produce user-actionable messages |

### 15.3 Integration tests

| Test | Description |
| --- | --- |
| Roundtrip | Generate → validate → serialize → import to Gramps (manual verification) |
| CLI end-to-end | Run `gramps-gen generate --count 10 --output test.gramps`, verify file exists and is valid XML |
| CLI adversarial all | Run `gramps-gen generate --count 10 --adversarial all`, verify correct behavior for both validity-preserving and validity-breaking strategies |
| CLI adversarial individual | Run each adversarial strategy in isolation via CLI flag, verify expected behavior |
| Config loading | Load YAML scenario file, verify CLI produces same output as equivalent flags |
| Error reporting | Test CLI with invalid config, missing file, validation errors; verify JSON Lines error output on stderr |
| Serializer edge cases | Test serializer with empty graph, special characters in strings, graphs with all optional fields absent |

### 15.4 Test organization

```text
│   └── simple-family.yaml
├── integration/
│   └── roundtrip.rs         # Generate → validate → serialize → re-parse
├── property/
│   └── invariants.rs        # Property-based tests for invariants
├── cli/
│   └── e2e.rs               # CLI end-to-end tests
├── bench/
│   └── performance.rs       # Benchmarks for validation and serialization at scale
└── extract/
    └── test_extractor.py    # Tests for the Python schema extractor
```

Each crate also has its own `#[cfg(test)] mod tests` for unit tests alongside the source code.

## 16. Future Considerations (not in scope)

- **JSON output format**: Exporting to JSON instead of XML for non-Gramps consumers.
- **GEDCOM export**: Supporting the legacy GEDCOM format.
- **Graph visualization**: Generating Graphviz DOT files for visual inspection of small graphs.
- **Schema migration**: Handling forward-compatibility when a newer Gramps version adds fields.
- **Incremental generation**: Resuming generation from an existing graph (for very large datasets).
- **Web UI**: Browser-based interface for interactively building family trees.
