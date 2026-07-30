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
cargo install --path .
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

### With optional features

```bash
gramps-gen generate --count 500 \
  --with-places --with-citations --with-notes \
  --with-media --with-tags
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

### Validate a `.gramps` file

```bash
# Check XML structure
gramps-gen validate output.gramps

# Strict validation
gramps-gen validate output.gramps --strict
```

## Commands

| Command | Description |
|---|---|
| `generate` | Generate a random family tree dataset in `.gramps` format |
| `validate` | Validate the XML structure and namespace of a `.gramps` file |
| `extract-schema` | Extract the Gramps schema from a local Gramps source checkout (stub) |

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
| `cli` | `crates/cli/` | CLI binary (`clap`), YAML scenario parsing, pipeline wiring, progress reporting |

## Documentation

- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — Full architecture: codegen, graph model, validation, generation, adversarial strategies, serialization
- **[docs/research/design.md](docs/research/design.md)** — Original design plan and strategy discussion
- **[AGENTS.md](AGENTS.md)** — Instructions for AI coding agents working on this project

## How it works

### Schema-driven codegen

The project uses a schema extraction pipeline:

1. `extract/extract_schema.py` introspects Gramps Python classes to produce `schemas/schema-5.2.json`
2. `typed-graph/build.rs` reads `schema-5.2.json` at compile time and generates Rust types:
   - `Node` enum (10 primary types: Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag)
   - `Edge` enum (~45 edge variants covering handle refs, embedded refs, and mixins)
   - Data structs, ref structs, enum types, and `Schema` runtime metadata

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
