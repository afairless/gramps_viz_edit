# Implementation Plan: Phase 2 — Graph Core

Source: `docs/research/design.md`

## Phase 2 Scope

Phase 2 builds the core graph model on top of Phase 1's codegen types. It implements the `Graph` struct (concrete typed directed multigraph), graph construction/query API, and the structural + referential validation pass using the `Schema` metadata.

**Phase 1 prerequisite issue**: The last Phase 1 commit (`f56f2cb`) moved `schema.json` from the workspace root to `schemas/` but did **not** update `build.rs` which still references `../../schema.json`. This must be fixed before Phase 2 code can compile. Step 1 addresses this.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix: update build.rs schema.json path to schemas/ directory` | Build fix | `crates/typed-graph/build.rs` | — |
| 2 | `feat: implement Graph struct with ValidationState and node/edge storage` | Graph core struct | `crates/typed-graph/src/graph.rs` | Unit |
| 3 | `feat: implement graph query methods with reverse edge index` | Graph query API | `crates/typed-graph/src/graph.rs` | Unit |
| 4 | `feat: implement validation module with structural and referential passes` | Validation engine | `crates/typed-graph/src/validate.rs` | Unit |
| 5 | `test: add comprehensive unit tests for graph construction, queries, and validation` | Graph core tests | `crates/typed-graph/src/graph.rs`, `crates/typed-graph/src/validate.rs`, `crates/typed-graph/src/lib.rs` | Unit |

### Step Details

#### Step 1 — Fix build.rs schema path (prerequisite)

- In `crates/typed-graph/build.rs`, change the `rerun-if-changed` path from `../../schema.json` to `../../schemas/schema.json`.
- Change the workspace root lookup from `workspace_root.join("schema.json")` to `workspace_root.join("schemas/schema.json")`.
- Verify `cargo build` compiles successfully with the codegen types.
- Run `cargo test` to confirm all existing Phase 1 tests still pass.

#### Step 2 — Graph struct, ValidationState, and node/edge storage

- Create `crates/typed-graph/src/graph.rs`:
  - `Pub use` the `Handle` type alias from the generated schema module.
  - Define `ValidationState` enum:

    ```rust
    pub enum ValidationState {
        Unvalidated,
        Valid,
        Invalid(Vec<ValidationError>),
    }
    ```

  - Define `Graph` struct:

    ```rust
    pub struct Graph {
        nodes: HashMap<Handle, Node>,
        edges: Vec<Edge>,
        reverse_edges: HashMap<Handle, Vec<usize>>,
        validation_state: ValidationState,
    }
    ```

  - Implement `Graph::new()`.
  - Implement `Graph::add_node(handle: Handle, node: Node) -> Result<(), GraphError>` — returns error if handle already exists.
  - Implement `Graph::add_edge(edge: Edge) -> Result<(), GraphError>` — the edge's source/target handles must already exist in `nodes`.
  - Implement `Graph::get_node(handle: &Handle) -> Option<&Node>`.
  - Implement `Graph::get_node_mut(handle: &Handle) -> Option<&mut Node>`.
  - Implement `Graph::get_edge(index: usize) -> Option<&Edge>`.
  - Implement `Graph::node_count() -> usize` and `Graph::edge_count() -> usize`.
  - Define `GraphError` enum with `DuplicateHandle`, `MissingNode`, and `InvalidEdge` variants.
  - Implement `Graph::validation_state() -> &ValidationState`.
  - Implement `Graph::set_validation_state(state: ValidationState)`.
  - Update `crates/typed-graph/src/lib.rs` to `pub mod graph;` and re-export key types.
  - Write unit tests:
    - `graph_new_is_empty`: `Graph::new()` has 0 nodes, 0 edges, `Unvalidated` state.
    - `add_node_ok`: Adding a node succeeds and increments node_count.
    - `add_node_duplicate_handle`: Adding a node with an existing handle returns `DuplicateHandle`.
    - `add_edge_ok`: Adding an edge between two existing nodes succeeds.
    - `add_edge_missing_node`: Adding an edge with a nonexistent handle returns `MissingNode`.
    - `get_node_returns_node`: Getting a node by handle returns the correct node.
    - `get_node_returns_none`: Getting a nonexistent handle returns `None`.
    - `get_edge_returns_edge`: Getting an edge by index returns the correct edge.
    - `get_edge_out_of_bounds`: Getting an edge past the edge count returns `None`.
    - `validation_state_initially_unvalidated`.
    - `set_validation_state_updates`.

#### Step 3 — Graph query methods with reverse edge index

- In `crates/typed-graph/src/graph.rs`:
  - Implement `Graph::iter_nodes() -> impl Iterator<Item = (&Handle, &Node)>`.
  - Implement `Graph::iter_nodes_by_type() -> impl Iterator<Item = (&Handle, &Node)>` — filter by Node variant. Return a struct/enum that allows filtering, or provide discrete methods:
    - `iter_persons()`, `iter_families()`, `iter_events()`, etc. — generated pattern, or a single `nodes_by_type(NodeType)` method.
    - *Recommendation*: use a `NodeType` enum or `fn nodes_by_type(&self, variant: NodeVariant) -> Vec<&Handle>` for simplicity (no lifetime gymnastics with generics). Alternatively, implement `iter_nodes_of_kind(&self, kind: &str) -> Vec<&Handle>`.
  - Implement `Graph::iter_edges() -> impl Iterator<Item = &Edge>`.
  - Implement `Graph::edges_from(handle: &Handle) -> Vec<&Edge>` — queries the reverse edge index.
  - Implement `Graph::edges_to(handle: &Handle) -> Vec<&Edge>` — queries the reverse edge index.
  - Implement `Graph::contains_node(handle: &Handle) -> bool`.
  - Update `add_node` and `add_edge` to maintain the reverse edge index.
  - Write unit tests:
    - `iter_nodes_empty`: Empty graph yields no nodes.
    - `iter_nodes_with_nodes`: Graph with 3 nodes iterates 3 items.
    - `iter_nodes_by_type`: After adding a Person and a Family, `iter_persons` returns 1, `iter_families` returns 1.
    - `iter_edges_empty`: Empty graph yields no edges.
    - `iter_edges_with_edges`: Graph with 2 edges iterates 2 items.
    - `edges_from`: After adding Person→Family edge, `edges_from(person_handle)` returns it.
    - `edges_to`: After adding Person→Family edge, `edges_to(family_handle)` returns it.
    - `contains_node`: Returns true for existing handle, false for missing.
    - `reverse_edge_index_maintained`: After multiple edges, from/to queries return correct counts.

#### Step 4 — Validation module

- Create `crates/typed-graph/src/validate.rs`:
  - Define `ValidationError` enum:

    ```rust
    pub enum ValidationError {
        DanglingReference { source: Handle, link: &'static str, target: Handle },
        MissingRequired { node: Handle, field: &'static str },
        CardinalityViolation { node: Handle, field: &'static str, expected: String, actual: usize },
        PlausibilityWarning { node: Handle, message: String },
    }
    ```

  - Implement `ValidationError` display/formatting: each error includes the affected handle, the violated constraint, and a suggested action.
  - Implement `fn structural_validation(graph: &Graph, schema: &Schema) -> Vec<ValidationError>`:
    - For each node, check that all fields listed in `schema.required_fields[type_name]` are present (non-empty for strings, `Some` for Option fields, etc.).
    - For each node, check cardinality constraints: field values in `schema.cardinality_constraints` are within `[min, max]`.
  - Implement `fn referential_validation(graph: &Graph) -> Vec<ValidationError>`:
    - Walk every edge in `graph.edges` and verify that `source` and `target` handles exist in `graph.nodes`.
    - Report `DanglingReference` for any missing handle.
  - Implement `fn validate(graph: &Graph, schema: &Schema) -> Vec<ValidationError>`:
    - Runs structural validation first, then referential validation.
    - Collects all errors into a single `Vec`.
    - Plausibility warnings are non-blocking, collected separately.
  - Implement `fn validate_strict(graph: &Graph, schema: &Schema) -> Result<(), Vec<ValidationError>>`:
    - Same as `validate` but promotes plausibility warnings to errors.
    - Returns `Ok(())` if no errors, `Err(errors)` otherwise.
  - Update `crates/typed-graph/src/lib.rs` to `pub mod validate;` and re-export key types.
  - Wire `validate` into `Graph`:
    - `Graph::validate(&mut self, schema: &Schema) -> Vec<ValidationError>` — runs validation, sets `validation_state` to `Valid` (if no errors) or `Invalid(errors)`.
    - `Graph::assert_valid(&self) -> Result<(), &[ValidationError]>` — returns error if `validation_state` is not `Valid`.
  - Write unit tests:
    - `structural_missing_required`: Person without primary_name returns `MissingRequired`.
    - `structural_required_present`: Person with all required fields passes structural check.
    - `cardinality_violation`: Family with 3 father_handle entries (if cardinality max is 1) returns `CardinalityViolation`.
    - `referential_dangling_edge`: Edge pointing to nonexistent handle returns `DanglingReference`.
    - `referential_valid_edges`: All edges pointing to existing nodes passes.
    - `validate_collects_all_errors`: Graph with multiple errors returns all of them.
    - `validate_ok`: Valid graph returns empty vec.
    - `validate_strict_promotes_warnings`: Graph with plausibility warning errors with `--strict` semantics.
    - `graph_validate_sets_state`: After `graph.validate()`, state is `Valid` or `Invalid` accordingly.
    - `graph_assert_valid_ok`: After validate on a valid graph, `assert_valid()` returns Ok.

#### Step 5 — Comprehensive tests for graph core

- Write additional edge-case tests:
  - `add_node_empty_handle`: Adding a node with an empty string handle.
  - `add_node_after_validation`: Adding a node should reset `validation_state` to `Unvalidated`.
  - `add_edge_after_validation`: Adding an edge should reset `validation_state` to `Unvalidated`.
  - `reverse_edge_index_after_remove`: (deferred — no node removal in Phase 2, just document that it's not yet supported).
  - `validate_empty_graph`: Empty graph passes validation with zero errors.
  - `validate_graph_with_all_required_fields`: Full-featured Person node passes validation.
  - `dangling_reference_in_mixin_edge`: CitationRef to nonexistent Citation handle.
  - `multiple_errors_reported`: Graph with 3 missing required fields returns all 3 errors.
  - `validation_state_transitions`: `Unvalidated` → `Valid` (on success) → `Invalid` (on failure) → `Unvalidated` (after mutation).
- Ensure `cargo test` passes with zero warnings.
- Ensure `cargo clippy` passes (if configured) or at minimum no compiler warnings.

### Key design references

- **Graph struct**: design §6 — `Graph` with `nodes: HashMap<Handle, Node>`, `edges: Vec<Edge>`, `reverse_edges: HashMap<Handle, Vec<usize>>`, `validation_state: ValidationState`.
- **ValidationState**: design §6 — `Unvalidated`, `Valid`, `Invalid(Vec<ValidationError>)`.
- **ValidationError**: design §6 — `DanglingReference`, `MissingRequired`, `CardinalityViolation`, `PlausibilityWarning`.
- **Validation layers**: design §6 — structural (required fields, cardinality) then referential (dangling handles).
- **Schema metadata**: design §5 — `required_fields: HashMap<&str, Vec<&str>>` and `cardinality_constraints: HashMap<&str, (Option<u32>, Option<u32>)>` — already generated by Phase 1 build.rs.
- **Edge classification**: design §6 — three categories: direct handle ref, embedded ref, mixins. All are already generated as `Edge` enum variants.
- **Phase 1-2 iteration note**: design §10 — the codegen's `Schema` metadata shape may need adjustment if the validation code's requirements don't match. The existing generated `Schema` struct (see `target/debug/build/typed-graph-*/out/generated_schema.rs`) uses `HashMap<&'static str, Vec<&'static str>>` for required fields and `HashMap<&'static str, (Option<u32>, Option<u32>)>` for cardinality — this matches the design spec and should be compatible with the validation code in this phase.
