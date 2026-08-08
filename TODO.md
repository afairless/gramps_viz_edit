# Implementation Plan: Bulk Delete Tool (`gramps-gen delete`)

Source: `docs/research/bulk-delete-tool.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | skip Step 1 | skip Step 1 | skip Step 1 | — |
| 2 | `feat(typed-graph): add edges_incident_to query method` | Graph query API addition | `crates/typed-graph/src/graph.rs` | Unit: empty node, edges_from only, edges_to only, both directions, missing handle |
| 3 | `feat(gramps-reader): build full-graph XML parser` | Full-graph streaming parser | `crates/gramps-reader/src/xml/graph.rs` | Round-trip: parse → write → re-parse, semantic equivalence (same node/edge counts) |
| 4 | `feat(delete): create deletion cascade engine and types` | Deletion cascade engine | `crates/delete/Cargo.toml`, `crates/delete/src/lib.rs`, `crates/delete/src/types.rs`, `crates/delete/src/cascade.rs` | Unit: all per-type orphan rules, edge cases (§3.4), property-based idempotency/monotonicity/already-orphaned-exclusion invariants |
| 5 | `feat(delete): add manifest types and serialization` | Deletion manifest | `crates/delete/src/manifest.rs` | Unit: manifest round-trip, validation (bad handles, mismatched source_file) |
| 6 | `feat(delete): add interactive review CLI` | Interactive review loop | `crates/delete/src/review.rs` | Unit: state machine (y/n/r/l/s/q), handle removal, abort, all-skip, `--yes` flag |
| 7 | `feat(output): add filter-during-serialization, namespace preservation, and gzip output` | Output writer enhancements | `crates/output/src/xml.rs` | Unit: filter removes specified handles, namespace override round-trip, gzip input → gzip output |
| 8 | `feat(cli): wire delete command with all options` | CLI command wiring | `crates/cli/src/commands/delete.rs`, `crates/delete/src/lib.rs` (update) | Integration: generate tree → select → delete → verify output |
| 9 | `test: add end-to-end integration tests for bulk delete` | End-to-end validation | `crates/cli/tests/e2e_delete.rs` or extend `crates/cli/tests/e2e.rs` | Integration: subprocess-based, verify output loads in round-trip, already-orphaned objects preserved |

## Step Details

### Step 1 — Research Gramps deletion logic

- skip Step 1

### Step 2 — Add `edges_incident_to` to `typed-graph::Graph`

```rust
impl Graph {
    /// Return all edges incident to a node (both as source and target).
    pub fn edges_incident_to(&self, handle: &Handle) -> Vec<&Edge>;
}
```

- Combines `edges_from` + `edges_to` results
- O(1) via existing indexes
- Add unit tests: empty node, edges_from only, edges_to only, both directions, missing handle

### Step 3 — Full-graph XML parser (`crates/gramps-reader/src/xml/graph.rs`)

- Streaming parser that reads entire `.gramps` XML and populates `typed_graph::Graph`
- Parse all 10 primary types: Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag
- Handle-ref fields become Edge variants; hlink attributes become Edge variants
- Graph population order: (1) all nodes first, (2) handle-ref edges from data structs, (3) hlink element edges
- Reuse existing helpers: `read_handle_attr`, `read_hlink_attr`, `strip_prefix`
- Detect Gramps version from `<header>` version attribute (fall back to xmlns heuristic)
- Handle both 5.1 (flat `<type>`) and 5.2 (nested `<eventtype>`) using existing `#[cfg(feature = "schema-5-1")]` patterns
- Capture `xmlns` attribute from `<database>` root for output namespace preservation
- Gzip-compressed input transparently decompressed (reuse `io.rs`)
- Self-closing elements and mixed content handled per existing `extract.rs` patterns

### Step 4 — Deletion cascade engine

**New crate**: `crates/delete/`

- `types.rs`: `DeleteCandidate`, `DeletePlan`, `ReviewState` enums/types
- `cascade.rs`: Core fixed-point algorithm with pre-existing connectivity recording
  - Phase A: Record pre_connectivity[n] = count of incident edges for every node
  - Phase B: Fixed-point loop — seeds → orphan detection → repeat until stable
  - Phase C: Post-condition invariant (no dangling refs to live nodes)
- `type_specific_orphan_rule` per §3.2
- All edge cases from §3.4: multi-family people, shared events, place hierarchy, self-referencing edges, already-orphaned, PersonRef, dangling refs, inferred nodes
- Property-based tests: idempotency, monotonicity, no dangling refs, already-orphaned exclusion

### Step 5 — Manifest types and serialization

- `manifest.rs`: Serialize/deserialize `DeleteManifest` to/from JSON
- Format per §4.4: version, source_file, selections_file, created_at, seed_people, plan (per-type to_delete/kept)
- Validation: cross-reference handles against graph; reject invalid handles; warn on source_file mismatch
- Deterministic serialization (sorted handles)

### Step 6 — Interactive review CLI

- `review.rs`: Interactive terminal loop
- Per-type prompt in dependency order (people → families → events → places → citations → sources → repositories → media → notes → tags)
- Commands: y (confirm), n (skip), r (remove handles), l (list all), s (summary), q (abort)
- `--yes` flag: skip all prompts
- Sample candidates display with handle + description

### Step 7 — Output writer enhancements

- **Filter-during-serialization**: `GraphXmlWriter::new` or new constructor accepts optional `&to_delete: HashSet<Handle>` — skips nodes/edges in the set during serialization
- **Namespace preservation**: New constructor/setter to accept explicit namespace override (captured by parser in Step 3)
- **Gzip output**: When input path ends with `.gz`, write through `GzEncoder`
- **Header updates**: New `created` timestamp, `<researcher>` note indicating cleanup
- **Pre-write validation**: Verify every handle in `to_delete` exists; sanity-check that no live node's sole reference is a `to_delete` handle

### Step 8 — Wire CLI delete command

- `crates/cli/src/commands/delete.rs`: Clap subcommand for `gramps-gen delete`
- Arguments: `<INPUT.gramps>`, `--selections`, `--output`, `--yes`, `--dry-run`, `--save-manifest`, `--load-manifest`
- Pipeline: parse input file → load selections → run cascade engine → (optional review) → validate → write output
- Wire into CLI dispatch in `crates/cli/src/commands/mod.rs` and `crates/cli/src/main.rs`
- Update `crates/delete/Cargo.toml` dependencies as needed (cli, output, gramps-reader)

### Step 9 — End-to-end integration tests

- Generate a random family tree via `gramps-gen generate`
- Run `gramps-gen delete` on the generated file
- Verify output loads in round-trip (parse → re-parse)
- Verify selected people are gone
- Verify orphaned events/families are gone
- Verify pre-existing unreferenced objects remain
- Test `--yes`, `--dry-run`, `--save-manifest`/`--load-manifest` flags
- Test selections file validation (0% match → error, 50%+ match → proceed)
