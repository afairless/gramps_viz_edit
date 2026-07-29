# Gramps Data Generator

A tool that generates valid, plausible [Gramps](https://gramps-project.org/) family tree datasets for testing and development.

The tool models the Gramps database as a typed directed multigraph, supports both random and configurable scenario-driven generation, and outputs Gramps XML (`.gramps` format) for direct import.

## Installation

```bash
# Clone the repository
git clone <repo-url> && cd gramps_viz_edit

# Build and install
cargo install --path .
```

Or just build:

```bash
cargo build --release
```

The binary is `target/release/gramps-gen`.

## Usage

### Basic generation

```bash
# Generate a family tree with 200 persons across 3 generations
gramps-gen generate --count 200 --output family.gramps

# Generate with a specific seed for reproducibility
gramps-gen generate --count 1000 --seed 42 --output reproducible.gramps
```

### With optional features

```bash
# Include places, citations, notes, media, and tags
gramps-gen generate --count 500 \
  --with-places --with-citations --with-notes \
  --with-media --with-tags
```

### Adversarial datasets

Test your downstream tools against unusual-but-valid family structures:

```bash
# All adversarial strategies
gramps-gen generate --count 100 --adversarial all

# Specific strategies only
gramps-gen generate --count 100 \
  --adversarial disconnected,one-parent

# Strict mode — promote plausibility warnings to errors
gramps-gen generate --count 100 \
  --adversarial all --strict
```

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
| `validate` | Validate the XML structure of a `.gramps` file |
| `extract-schema` | Extract the Gramps schema from a local Gramps source checkout |

## Pipeline

The tool follows a strict five-stage pipeline with validation gates after every data-altering stage:

```
Generate → Validate → [Adversarial Transform] → Validate → Serialize
```

1. **Generate** — Build a random or scenario-driven family tree graph
2. **Validate** (Gate 1) — Check structural and referential integrity
3. **Adversarial Transform** — Apply validity-preserving or -breaking transforms
4. **Validate** (Gate 2) — Re-validate after transforms
5. **Serialize** — Output Gramps XML

## Architecture

```
┌────────────────────┐
│  Schema Extraction │  Python script (extract/extract_schema.py)
└────────┬───────────┘
         │
         ▼
┌──────────────────────────────────────────────┐
│  typed-graph  (Rust crate)                   │
│  - Schema types, Graph storage, Validation   │
│  - Random generation, Adversarial strategies  │
└────────────────────┬──────────────────────────┘
                     │
┌────────────────────┴──────────────────────────┐
│  output  (Rust crate)                         │
│  - XML Serializer (Gramps XML .gramps format) │
└────────────────────┬──────────────────────────┘
                     │
┌────────────────────┴──────────────────────────┐
│  cli  (Rust binary)                           │
│  - gramps-gen generate, validate, extract-schema│
│  - YAML scenario parsing, progress reporting   │
│  - Pipeline wiring                             │
└─────────────────────────────────────────────────┘
```

## Crate Structure

| Crate | Location | Description |
|---|---|---|
| `typed-graph` | `crates/typed-graph/` | Graph model, schema types, validation, random and adversarial generation |
| `output` | `crates/output/` | Gramps XML serialization with `SerializationMap` |
| `cli` | `crates/cli/` | CLI binary, YAML scenario parsing, pipeline wiring |

## Security

The `extract-schema` command imports and executes Python code from the provided path. Only point it at a trusted Gramps source checkout.

## Development

```bash
# Run all tests
cargo test --workspace

# Run clippy linting
cargo clippy --all-targets --all-features -- -D warnings

# Build release
cargo build --release
```

## License

This project is licensed under the MIT License.
