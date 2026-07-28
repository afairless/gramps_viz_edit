# Implementation Plan: Phase 1 — Schema Extraction

Source: `docs/research/design.md`

## Phase 1 Scope

Phase 1 builds the foundation for the entire project: a Python script that introspects Gramps' Python data model and emits a `schema.json` artifact, plus a Rust `build.rs` codegen that reads that artifact and generates Rust types (Node/Edge enums, data structs, enum types, Schema metadata) at compile time.

**Phase 1-2 iteration note** (from design §10): The codegen output shape must match what the validation code in Phase 2 expects. The recommended workflow is to iterate with a minimal single-type schema first, then expand. This plan assumes a full-schema approach for the committed steps; the iteration loop can be applied during implementation.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: scaffold Cargo workspace and typed-graph crate` | Workspace skeleton | `Cargo.toml` (workspace root), `crates/typed-graph/Cargo.toml`, `crates/typed-graph/src/lib.rs` | — |
| 2 | `feat: add Python schema extractor script` | Schema extractor | `extract/extract_schema.py` | Unit (Python) |
| 3 | `feat: add schema.json as committed artifact` | Schema artifact | `schema.json` | — |
| 4 | `feat: implement build.rs codegen that reads schema.json and generates Rust types` | Codegen pipeline | `crates/typed-graph/build.rs`, `crates/typed-graph/src/schema.rs` | Unit |
| 5 | `test: add unit tests for schema parsing, edge classification, and type generation` | Schema pipeline tests | Tests in `crates/typed-graph/src/` modules | Unit |

### Step Details

#### Step 1 — Workspace skeleton

- Create Cargo workspace root `Cargo.toml` with `typed-graph` as the sole member crate.
- Create `crates/typed-graph/Cargo.toml` with dependencies: `serde`, `serde_json`, `uuid` (1.x), `rand` (0.8+).
- Create minimal `crates/typed-graph/src/lib.rs` with a placeholder.
- Verify `cargo build` passes.

#### Step 2 — Python schema extractor

- Write `extract/extract_schema.py` that:
  - Imports primary types from `gramps.gen.lib` (Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag).
  - Calls `get_schema()` on each class; falls back to introspection for classes without it.
  - Discovers handle-reference fields, embedded ref fields, and mixin bases.
  - Records `required: true/false` and `cardinality: { min, max }` per field.
  - Discovers enum values from Gramps enum classes.
  - Outputs `schema.json` in the format described in design §5.
- Write `extract/test_extractor.py` with mock Gramps-like classes to verify extraction logic.

#### Step 3 — Schema artifact

- Run `extract/extract_schema.py` against a Gramps 5.2 source checkout to produce `schema.json`.
  - If Gramps source is not available, create a representative mock `schema.json` covering all 10 primary types, secondary types, and enum types as a development placeholder, annotated with a comment noting it should be replaced with a real extraction.
- Commit `schema.json` to the repository.

#### Step 4 — Codegen pipeline

- Write `crates/typed-graph/build.rs` that:
  - Reads `schema.json` (from the workspace root, via a relative path or an environment variable).
  - Generates Rust source code to `OUT_DIR`:
    - `Node` enum (one variant per primary type).
    - `Edge` enum (one variant per edge kind, typed with source/target handles and metadata).
    - Per-type data structs (PersonData, FamilyData, …).
    - Ref structs (EventRef, ChildRef, …) with all metadata fields.
    - Enum types (EventType, EventRoleType, …).
    - `Schema` runtime metadata struct with `required_fields` and `cardinality_constraints`.
  - Handles error cases gracefully (missing/malformed schema.json, unexpected types).
- Write `crates/typed-graph/src/schema.rs` that uses `include!()` to bring in generated code and re-exports public types.
- Wire `schema.rs` into `crates/typed-graph/src/lib.rs`.
- Verify `cargo build` compiles successfully.

#### Step 5 — Unit tests for schema pipeline

- Write tests for:
  - **schema.json parsing**: Parse valid schema.json, malformed JSON, missing fields, empty schema.
  - **Edge classification**: Verify that handle_ref fields, embedded ref fields, and mixin bases are correctly identified in the generated output.
  - **Type generation**: Verify that the generated `Node` enum has correct variants, `Edge` enum has correct variants with proper metadata, and `Schema` metadata contains the right required/cardinality entries.
  - **Codegen error handling**: Verify build.rs returns clear errors for missing schema.json, invalid type references, etc.
- Run `cargo test` and confirm all tests pass.
