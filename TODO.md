# Implementation Plan: Connection Densifier

Source: `docs/research/connection-densifier.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: define DensifyConfig and DensifyResult structs` | Config & result types | `crates/typed-graph/src/generate/densify.rs` (new), `crates/typed-graph/src/generate/mod.rs` (re-export) | Unit |
| 2 | `feat: implement connected-component finding in densifier` | Component finding (Pass 1) | `crates/typed-graph/src/generate/densify.rs` | Unit |
| 3 | `feat: implement cross-component marriage (Pass 2)` | Cross-component marriage | `crates/typed-graph/src/generate/densify.rs` | Unit, integration (schema validation) |
| 4 | `feat: implement orphan adoption (Pass 3)` | Orphan adoption | `crates/typed-graph/src/generate/densify.rs` | Unit, integration (schema validation) |
| 5 | `feat: implement single-parent upgrade and remarriage (Pass 4)` | Single-parent upgrade + remarriage | `crates/typed-graph/src/generate/densify.rs` | Unit, integration (schema validation) |
| 6 | `feat: implement top-level densify_connections orchestrator` | Orchestration | `crates/typed-graph/src/generate/densify.rs` | Unit, integration |
| 7 | `test: add property-based tests for densifier` | Property-based tests | `crates/typed-graph/src/generate/densify.rs` | Property-based |
| 8 | `feat: integrate densifier into generate_random and GenerationResult` | Pipeline integration | `crates/typed-graph/src/generate/random.rs` | Unit, integration |
| 9 | `feat: add CLI flags for connection densifier` | CLI flags | `crates/cli/src/commands/generate.rs` | Smoke, E2E |
| 10 | `feat: add scenario YAML support for densifier` | Scenario YAML | `crates/cli/src/scenario.rs` | Unit |
| 11 | `chore: run full test suite and manual smoke tests` | Verification | — | — |

## Key design notes from the source document

### Known discrepancies corrected in the design doc

1. **`FamilyBuilder` does NOT automatically add `PersonFamily` edges or update `family_list`** — after creating a family via `GraphBuilder::add_family(handle)`, the densifier must manually add `Edge::PersonFamily` edges for both parents and push the family handle onto each parent's `family_list` via `graph.get_node_mut()`.

2. **`GraphBuilder::family()` API** — the correct method is `GraphBuilder::add_family(handle)`, which returns a `FamilyBuilder`. Call `.build()` to insert the family into the graph.

3. **Seed constant** — use `0xD3NS1F1ER` (valid hex) instead of the original `0xDEAD_DENS1FY` (invalid hex containing 'N', 'S', 'Y').

4. **`get_person_birth_year` and `collect_stats` are private** in `random.rs` — they must be changed to `pub(crate)` in Step 8 for the densifier to use them.

### Key helpers

- `gender_value()` / `into_gender_field()` — version-safe gender access (from `graph.rs`)
- `make_child_ref()` / `make_event_ref()` — version-safe ref struct creation (from `graph.rs`)
- `event_type_eq()` — version-safe event type comparison (from `graph.rs`)
- `GraphBuilder::add_family(handle)` — creates a family with required-field init (from `builder.rs`)
