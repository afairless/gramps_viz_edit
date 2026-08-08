# Bulk Deletion Tool — Design Plan

## Purpose

A CLI tool (`gramps-gen delete`) that takes a `.gramps` file and a visualizer
selections JSON file, computes a safe cascade of objects to delete (people
selected in the visualizer → their now-orphaned events, families, places,
etc.), presents each type's deletion candidates for interactive user review,
and writes a new `.gramps` file with those objects removed.

**Key constraint**: Only objects that become orphaned *as a result of deleting
the selected people* are deleted. Objects that were already unreferenced
before the operation are left alone.

---

## Phase 1 — Research Gramps Internal Deletion Logic

Before designing our own cascade algorithm, research how Gramps itself handles
safe deletion to avoid reinventing edge cases.

### 1.1 Research Scope

| Area | What to investigate |
|---|---|
| Gramps source | `gramps/gen/db/upgrade.py`, `gramps/gen/db/base.py`, `gramps/gen/utils/` |
| Reference counting | How Gramps checks whether an object can be safely removed |
| Back-reference tracking | Whether Gramps maintains reverse indexes for referential integrity |
| Deletion callbacks | `DbDeletePrimary`, `delete_primary_from_referring`, update callbacks |
| Cascading rules | Which object types cascade and in what order |
| Signals / hooks | `db-delete`, `db-undelete` signals and their consumers |

### 1.2 Deliverables

- A summary markdown note (`docs/research/gramps-deletion-logic.md`) describing:
  - Gramps's deletion algorithm in pseudocode
  - Object-type deletion dependency order
  - Edge cases Gramps handles (self-referencing, circular refs, shared objects)
  - Whether Gramps has a concept of "orphan-after-delete" vs "already-orphaned"
- A decision on whether to wrap Gramps Python, reimplement in Rust, or use
  Gramps as a validation oracle for our Rust implementation

### 1.3 Risks

- Gramps's deletion logic may be spread across many files and hard to extract
- Python-level logic may use SQLite/BSDDB queries that don't translate to XML
- Gramps may not even have a dedicated "bulk safe delete" — the user's
  recollection may be of Gramps's interactive delete-in-editor behavior

---

## Phase 2 — Full-Graph XML Parser

The existing `gramps-reader` extracts only Person, Family, and Event data.
For deletion we need **all 10 primary types** plus **all edge relationships**
in the graph model.

### 2.1 New Parser: `gramps-reader/src/xml/graph.rs`

A streaming parser that reads the entire `.gramps` XML and populates a
`typed_graph::Graph`:

```
database
├── header        → metadata only, not parsed into graph
├── people        → Node::Person  + edges (eventref, parentin, childin, ...)
├── families      → Node::Family  + edges (father, mother, childref, eventref, ...)
├── events        → Node::Event   + edges (place, citationref, noteref, ...)
├── places        → Node::Place   + edges (placeref, citationref, ...)
├── sources       → Node::Source  + edges (reporef, noteref, ...)
├── citations     → Node::Citation + edges (sourceref, noteref, ...)
├── repositories  → Node::Repository + edges
├── media         → Node::Media   + edges
├── notes         → Node::Note    + edges
└── tags          → Node::Tag     + edges
```

**Design decisions**:

- Reuse existing `read_handle_attr`, `read_hlink_attr`, `strip_prefix` helpers
- Parse `<handle>` as node key; `<gramps_id>` is optional
- References via `hlink` attributes become `Edge` variants
- Handle-ref fields embedded in data structs (e.g., `place_handle` on EventData,
  `source_handle` on CitationData) are also added as edges
- Self-closing elements and mixed content handled per existing patterns
- Gzip-compressed `.gramps` files detected and transparently decompressed
  (reuse `gramps-reader/src/io.rs`)

**File**: `crates/gramps-reader/src/xml/graph.rs` (new)
**Test data**: Use existing generated `.gramps` files for round-trip testing

### 2.2 Schema Version Handling

The parser must handle both Gramps 5.1 (schema-5-1 feature) and 5.2 format:

- 5.1: flat `<type>Birth</type>` inside `<event>`; `Option<>` wrappers
- 5.2: nested `<eventtype><type>Birth</type></eventtype>`
- Detect version from the `version` attribute on the `<header>` element
  (e.g., `<header version="5.2">`). If absent, fall back to the XML namespace
  (`xmlns` on `<database>`) as a heuristic
- Use the same `#[cfg(feature = "schema-5-1")]` patterns as `typed-graph`

**Namespace capture**: The parser must capture the `xmlns` attribute from the
`<database>` root element and store it for output. This is needed to preserve
 the input file's namespace in the output (Resolved Decision #3).

### 2.3 Existing Dependency: gramps-reader → typed-graph

**Note**: The `gramps-reader` crate already depends on `typed-graph`
(`Cargo.toml`: `typed-graph = { path = "../typed-graph" }`). The new
full-graph parser in `gramps-reader/src/xml/graph.rs` uses this existing
dependency to populate a `typed_graph::Graph`. The streaming extractors
(`extract.rs`, `count.rs`) do not use the `typed-graph` types and remain
self-contained — consumers that only need person/family streaming are
unaffected by the full-graph path.

### 2.4 Graph Population Order

To avoid `MissingNode` errors when adding edges, populate in this order:

1. Parse all nodes first (all 10 types) → add to graph
2. For handle-ref fields within data structs (not hlink elements), populate
   as edges in a second pass
3. Parse all hlink elements → add as edges

This ensures all nodes exist before any edge references them.

### 2.5 Verification

- Parse a generated `.gramps` file → write back via `GraphXmlWriter` →
  parse again → verify semantic equivalence (same node/edge counts)
- This validates that the new parser + existing writer form a round-trip

---

## Phase 3 — Deletion Cascade Engine

### 3.1 Core Algorithm

The cascade engine lives in a new crate `crates/delete/`.

The engine operates **read-only** on the graph — it does not mutate nodes or
edges. It walks the graph's edge indexes to determine which handles become
orphaned when `to_delete` nodes are removed, and returns a final `HashSet<Handle>`.

```
Input:  &Graph G, Set<Handle> seed_handles (selected people)
Output: HashSet<Handle> handles_to_delete (all types)

Algorithm: Fixed-point cascade with pre-existing connectivity filter

Phase A — Record pre-existing connectivity

For every node n in G:
    pre_connectivity[n] ← count of G.edges_incident_to(n)

Phase B — Fixed-point loop

1. to_delete ← seed_handles
2. Loop:
   a. candidates ← {}
   b. For each node n NOT in to_delete:
      - Let incident ← G.edges_incident_to(n)
      - Let live_edges ← { e in incident | neither endpoint in to_delete }
      - If live_edges is empty
        AND pre_connectivity[n] > 0          -- was in use before op
        AND type_specific_orphan_rule(n, G, to_delete):
            add n to candidates
   c. If candidates is empty → break (fixed point)
   d. to_delete ← to_delete ∪ candidates
3. Return to_delete

Phase C — Post-condition invariant

For every node n in to_delete, for every edge e incident to n:
    the other endpoint of e is also in to_delete, OR
    the other endpoint is now orphaned (which will be caught by a subsequent
    iteration of the fixed-point loop in Phase B — the invariant holds at
    the fixed point)
```

The `type_specific_orphan_rule(node, graph, to_delete)` predicate applies the
per-type rules from §3.2. Each rule inspects incident edge variants and returns
`true` only when all edges to live (non-deleted) nodes are severed.

### 3.2 Type-Specific Orphan Rules

Not all nodes with zero remaining edges are automatically deleted. The rules
differ by type:

| Type | Orphan rule | Rationale |
|---|---|---|
| **Person** | Always deleted if in seed set | User explicitly selected |
| **Family** | Deleted if has NO remaining connections to non-deleted people (father, mother, OR children) | A family with no members is meaningless |
| **Event** | Deleted if has NO remaining incoming edges from non-deleted Person/Family nodes | Events are "owned" by the people/families that reference them |
| **Place** | Deleted if has NO remaining edges to non-deleted Events AND no PlacePlaceRef edges to/from non-deleted Places | Places are shared — only delete if nothing uses them anymore |
| **Source** | Deleted if has NO remaining CitationSource edges from non-deleted Citations | Sources exist to be cited |
| **Citation** | Deleted if has NO remaining incoming citationref edges from any non-deleted object | Citations only exist to be referenced |
| **Repository** | Deleted if has NO remaining SourceRepoRef edges from non-deleted Sources | Repositories hold sources |
| **Media** | Deleted if has NO remaining mediaref edges from any non-deleted object | Media objects are shared resources |
| **Note** | Deleted if has NO remaining noteref edges from any non-deleted object | Notes are shared across many object types |
| **Tag** | Deleted if has NO remaining tagref edges from any non-deleted object | Tags are shared across many object types |

### 3.3 Cascade Order (Iterative Rounds)

The cascade naturally proceeds through rounds because deletion of one type
triggers orphan detection in the next:

```
Round 1: Delete selected People
Round 2: Orphaned Families (no remaining members)
Round 3: Orphaned Events (no remaining person/family references)
Round 4: Orphaned Places (no remaining event references)
Round 5: Orphaned Citations (no remaining references from any object)
Round 6: Orphaned Sources (no remaining citations)
Round 7: Orphaned Repositories (no remaining sources)
Round 8: Orphaned Media, Notes, Tags (no remaining references)
```

The fixed-point loop handles this automatically; the explicit ordering is for
user-facing presentation only.

### 3.4 Edge Cases

| Case | Handling |
|---|---|
| **Person in multiple families** | Family is only deleted if ALL its parent+child connections are to deleted people (all-or-nothing threshold) |
| **Event shared by multiple people** | Event is only deleted if ALL PersonEventRef edges come from deleted people |
| **Place used by multiple events** | Place is only deleted if ALL EventPlace edges come from deleted events |
| **Place hierarchy (PlacePlaceRef)** | A place is only deleted if it has no edges to any non-deleted place AND no edges from events |
| **Self-referencing edges** | Treated same as any edge — if one endpoint is deleted, the edge is severed |
| **Citation with no source** | Already-orphaned citations are NOT deleted (they were orphaned before our operation) |
| **PersonRef (association)** | PersonPersonRef edges do NOT trigger cascade — they are associations, not ownership. Deleting the source removes the edge; the target is unaffected |
| **Dangling hlink refs** | Already-dangling refs (pointing to nonexistent handles) are treated as not counting for connectivity — they don't prevent orphan detection |
| **Inferred/placeholder nodes** | Nodes marked as inferred in the graph are treated normally — they may become orphans if their only connections are to deleted nodes |
| **Family with only children deleted** | If only children are deleted but parents remain, the family stays (parents could have more children added later) |

### 3.5 Distinguishing "Newly Orphaned" from "Already Orphaned"

This is the core requirement — we must NOT delete objects that were already
unreferenced before the operation.

**Approach**:

1. Before simulating deletion, record each node's **pre-existing connectivity
   count** (number of incident edges to non-deleted nodes)
2. After simulating deletion, a node is a candidate ONLY if:
   - Its pre-existing connectivity count was > 0 (it was in use)
   - Its post-deletion connectivity count is 0 (all its connections were to
     now-deleted nodes)
3. Nodes that had 0 connections BEFORE the operation are **always skipped**

This cleanly separates "newly orphaned" from "already orphaned."

### 3.5.1 Complexity

The fixed-point loop in Phase B checks every node not yet in `to_delete` on
each iteration. In the worst case (all nodes eventually deleted), the total
work is O(N × R) where N is the total node count and R is the number of
cascade rounds (at most 8, bounded by the type dependency chain in §3.3).

For typical Gramps files (≤ 10,000 nodes) this is acceptable. For very large
files, the algorithm can be optimized to an **incremental propagation**
approach: maintain a queue of nodes whose connectivity changed in the
previous round, and only re-evaluate those nodes and their neighbors. This
reduces the cost to O(E) total edge traversals, since each edge is examined
at most once per endpoint deletion. The incremental optimization is deferred
as a future enhancement until profiling demonstrates a need for it.

### 3.6 Files

| File | Purpose |
|---|---|
| `crates/delete/src/lib.rs` | Crate root, re-exports |
| `crates/delete/src/cascade.rs` | Deletion cascade engine |
| `crates/delete/src/types.rs` | `DeleteCandidate`, `DeletePlan`, `ReviewState` types |
| `crates/delete/Cargo.toml` | Dependencies (typed-graph, gramps-reader, serde, serde_json) |

---

## Phase 4 — Interactive Review CLI

### 4.1 Command Interface

```
gramps-gen delete <INPUT.gramps> --selections <selections.json> [OPTIONS]

OPTIONS:
  --output, -o <FILE>       Output .gramps file (default: <input>-cleaned.gramps)
  --yes, -y                 Skip all review prompts (delete everything)
  --dry-run                 Compute deletions but don't write output
  --save-manifest <FILE>    Save deletion manifest to JSON file
  --load-manifest <FILE>    Load pre-reviewed deletion manifest (skip review)
```

### 4.2 Review Workflow

The tool presents types in dependency order (people first, then families, then
events, etc.). For each type:

```
═══ Step 3/8: Events ═══
42 events would be deleted (35 Birth, 5 Death, 2 Marriage).

Sample candidates:
  • e-abc123 — Birth of "John Smith" (1850-07-13)
  • e-def456 — Death of "Mary Jones" (1920-03-01)
  • e-ghi789 — Marriage of "John Smith" & "Mary Jones" (1875-06-15)
  ... and 39 more

Actions:
  [y] Delete all 42 events and continue
  [n] Skip this type (keep all 42 events)
  [l] List all 42 candidates with details
  [r] Remove specific handles (type handles separated by spaces)
  [s] Show summary of remaining types
  [?] Help

Choice:
```

### 4.3 Interactive Commands Per Type

| Key | Action |
|---|---|
| `y` | Confirm deletion of all candidates for this type; advance to next type |
| `n` | Skip this type entirely; keep all candidates; advance to next type |
| `r` | Enter handle-removal mode: user types space-separated handles to remove from the deletion set |
| `l` | List all candidates with full details (handle, name/description, type-specific info) |
| `s` | Print a summary of all remaining types and candidate counts |
| `q` | Abort the entire operation (no file written) |

### 4.4 Deletion Manifest Format

```json
{
  "version": 1,
  "source_file": "family_tree.gramps",
  "selections_file": "selections.json",
  "created_at": "2025-01-15T10:30:00Z",
  "seed_people": ["p-abc123", "p-def456"],
  "plan": {
    "people": {
      "to_delete": ["p-abc123", "p-def456"],
      "kept": []
    },
    "families": {
      "to_delete": ["f-ghi789"],
      "kept": []
    },
    "events": {
      "to_delete": ["e-001", "e-002", "e-003"],
      "kept": ["e-004"]
    }
  }
}
```

The manifest is both human-editable and machine-readable. When loaded with
`--load-manifest`, the review phase is skipped entirely and the manifest
drives deletion directly.

**Manifest validation** (security): On load, cross-reference every handle in
the manifest against the graph. Reject any handle that doesn't exist in the
graph with a clear error. Also cross-reference `source_file` against the
input file path — warn (but don't block) on mismatch, as the file may have
been renamed. This prevents a malicious or misdirected manifest from deleting
arbitrary objects.

### 4.5 Files

| File | Purpose |
|---|---|
| `crates/delete/src/review.rs` | Interactive review loop, terminal I/O |
| `crates/delete/src/manifest.rs` | Manifest serialization/deserialization |
| `crates/cli/src/commands/delete.rs` | CLI argument parsing, wiring |

---

## Phase 5 — XML Output

### 5.1 Approach

**Filter-during-serialization**: The cascade engine produces a
`HashSet<Handle>` of all handles to delete. The output stage passes this
set to `GraphXmlWriter`, which skips deleted nodes and edges during
serialization. The original graph is **never mutated** — preserving the
ARCHITECTURE.md design property of no node deletion, and avoiding an
unnecessary clone of a potentially large graph.

1. Pass `&to_delete: HashSet<Handle>` to `GraphXmlWriter`
2. Writer skips any node whose handle is in `to_delete`
3. Writer skips any edge where either endpoint is in `to_delete`
4. Writer produces valid XML from the filtered view of the graph

### 5.2 Graph Query Operations

To support the cascade engine without mutating the graph, `Graph` needs
one new query method (no mutation needed):

```rust
impl Graph {
    /// Return all edges incident to a node (both as source and target).
    pub fn edges_incident_to(&self, handle: &Handle) -> Vec<&Edge>;
}
```

This combines `edges_from` + `edges_to` results, consistent with the existing
query API (both return `Vec<&Edge>`). Used by the cascade engine to test
connectivity without removing edges.

### 5.3 Pre-Write Validation

Before writing, validate that the `to_delete` set is internally consistent:

- Every handle in `to_delete` exists in the graph
- No handle in `to_delete` is the sole remaining reference for a
  non-deleted node (sanity check against cascade bugs)

If validation fails, the tool reports errors and does NOT write the output
file (safety first).

### 5.4 Output Writer Changes

**Namespace preservation**: `GraphXmlWriter` currently derives the namespace
from the Gramps version string. It needs a new constructor or setter to accept
an explicit namespace override (captured during parsing).

**Gzip output**: When the input was gzip-compressed, the output should also be
gzip-compressed. This requires extending the output path to write through a
`GzEncoder` when the input path ends with `.gz`.

**Header**: The output file's `<header>` section gets:

- A new `created` timestamp (current time)
- The same Gramps version as the input
- A `<researcher>` note indicating "Cleaned by gramps-gen delete"

---

## Phase 6 — Testing & Validation

### 6.1 Unit Tests

#### Cascade Engine — General

| Test area | Coverage |
|---|---|
| Cascade engine | Seed with known people → verify correct cascade, verify already-orphaned objects are excluded |
| Fixed-point convergence | A→B→C chain where deleting A orphanes B which orphanes C — verify transitive cascade |
| Empty seed set | No people selected → zero candidates, `to_delete` is empty |
| Non-existent handles in seed | Handles not in graph → ignored (don't contribute to cascade), logged as warning |

#### Cascade Engine — Per-Type Orphan Rules

Each type-specific rule from §3.2 needs dedicated unit tests:

| Type | Test cases |
|---|---|
| **Person** | Seed person deleted; unselected person not deleted; person with edge to deleted person stays |
| **Family** | All parents+children deleted → family deleted; one parent remains → family stays; one child remains → family stays |
| **Event** | All PersonEventRef from deleted people → event deleted; one live referrer → event stays; event shared by N people, all deleted → event deleted |
| **Place** | All EventPlace from deleted events → place deleted; one live event → place stays; place hierarchy: child place orphaned but parent still used → only child deleted |
| **Source** | All CitationSource from deleted citations → source deleted; one live citation → source stays |
| **Citation** | All incoming citationref from deleted objects → citation deleted; one live referrer → citation stays |
| **Repository** | All SourceRepoRef from deleted sources → repository deleted; one live source → repository stays |
| **Media** | All mediaref from deleted objects → media deleted; one live referrer → media stays; media referenced by multiple types → needs all referrers deleted |
| **Note** | All noteref from deleted objects → note deleted; one live referrer → note stays |
| **Tag** | All tagref from deleted objects → tag deleted; one live referrer → tag stays |

#### Edge Case Tests

Each edge case from §3.4 needs a dedicated test:

| Edge case | Test |
|---|---|
| Person in multiple families | Person deleted → check both families independently (one may stay, one may go) |
| Event shared by multiple people | 3 people share an event, delete 2 → event stays (one live referrer) |
| Place hierarchy | Delete only events in child place → child place deleted, parent place stays |
| Self-referencing edges | Node with self-loop → when deleted, edge is severed like any other |
| Citation with no source | Pre-existing orphan citation → NOT deleted (pre_connectivity was 0) |
| PersonRef (association) | Delete source of PersonPersonRef → target unaffected |
| Dangling hlink refs | Edge to nonexistent handle → doesn't count for connectivity, doesn't block orphan |
| Family with only children deleted | Delete all children, parents remain → family stays |

#### Property-Based Tests

The cascade engine has invariants suitable for proptest:

| Invariant | Test |
|---|---|
| Idempotency | `cascade(G, cascade(G, S)) == cascade(G, S)` for any graph G and seed set S |
| Monotonicity | `S ⊆ cascade(G, S)` — seeds are always in the result |
| No dangling refs | After cascade, for every remaining node n and every edge e of n, the other endpoint of e is NOT in `cascade(G, S)` (excluding dangling hlink refs to nonexistent handles) |
| Already-orphaned exclusion | For any node n with `pre_connectivity[n] == 0`, n ∉ `cascade(G, S)` regardless of S |

#### Interactive Review

| Test area | Coverage |
|---|---|
| State machine | Each command key (y/n/r/l/s/q) tested in isolation; verify correct state transition and prompt advancement |
| Handle removal | Enter 'r' mode, remove specific handles, verify they move from `to_delete` to `kept` |
| Abort | Press 'q' mid-review → no file written, exit with non-zero code |
| All-skip | Press 'n' for every type → output file equals input file (semantically) |
| `--yes` flag | Skips all prompts, deletes everything the cascade computed |

#### Other

| Test area | Coverage |
|---|---|
| Manifest round-trip | Serialize → deserialize → verify equivalence; verify all handle arrays are sorted for deterministic comparison |
| Manifest validation | Load manifest with handles not in graph → rejected with clear error; load manifest with mismatched `source_file` → warning but proceed |
| Selection file validation | Handle overlap ≥50% → proceed silently; exactly one non-zero match → warning; 0% match → error; boundary cases at 49% and 51% tested explicitly |
| XML full-parse → write → re-parse | Semantic equivalence verification (same node/edge counts, same data fields) |

### 6.2 Integration Tests

- Generate a random family tree via `gramps-gen generate`
- Export selections via the visualizer
- Run `gramps-gen delete` on the generated file
- Verify the output loads in Gramps
- Verify deleted people are gone, their orphaned events are gone, but
  pre-existing unreferenced objects remain

### 6.3 Gramps-as-Oracle Validation

If Phase 1 research yields a usable Gramps Python deletion function:

- Run our Rust tool and Gramps's Python deletion on the same input
- Compare results programmatically via the `diff` crate
- Flag any discrepancies for investigation

---

## Implementation Order

Each step includes its own tests (per the incremental-development workflow).
The implementation follows the per-step loop: code + tests → verify → commit
before proceeding to the next step.

| Step | Description | Dependencies | Key tests |
|---|---|---|---|
| 1 | Research Gramps deletion logic → write findings | None | Verify sources cited, pseudocode matches Gramps behavior |
| 2 | Add `Graph::edges_incident_to` query method to typed-graph | None | Unit: empty node, node with edges_from only, edges_to only, both |
| 3 | Build full-graph XML parser in gramps-reader (`graph.rs`) | None | Round-trip: parse → write → re-parse, semantic equivalence |
| 4 | Build deletion cascade engine in new `delete` crate | Step 2 (unit tests), Step 3 (integration tests) | All per-type orphan rule tests, edge case tests, property-based tests |
| 5 | Build manifest types and serialization | Step 4 | Manifest round-trip, validation (bad handles, mismatched file) |
| 6 | Build interactive review CLI | Steps 4, 5 | State machine, each command key, `--yes`, abort, all-skip |
| 7 | Wire CLI command + output (filter writer, namespace, gzip) | Steps 2, 4, 6 | Integration: generate tree → select → delete → verify output |
| 8 | End-to-end validation + Gramps-as-oracle | All | Load output in Gramps, compare with Python oracle if available |

---

## Resolved Design Decisions

1. **Family deletion threshold**: All-or-nothing. A family is only deleted
   when EVERY connected person (both parents AND all children) is deleted.
   If even one person remains, the family stays. This is the safest default
   and handles step-families correctly.

2. **Person ↔ Person refs**: PersonPersonRef is an association, not an
   ownership relation. It does NOT count toward orphan detection for the
   target person. Deleting the source removes the edge, but the target is
   unaffected.

3. **Gramps XML namespace in output**: Preserve the input file's namespace.
   The full-graph parser must capture the namespace from the input file's
   `<database xmlns="...">` attribute. The output writer must accept an
   explicit namespace override rather than always deriving it from the
   Gramps version.

4. **Gzip handling**: Match the input. If the input is `.gramps.gz`, output
   `.gramps.gz`. If plain `.gramps`, output plain `.gramps`. The output
   path default (`<input>-cleaned.gramps`) should also preserve the `.gz`
   extension when present.

5. **Selection file validation**: Heuristic check. Cross-reference person
   handles in the selections file against handles in the `.gramps` file.
   If fewer than 50% match, print a warning but allow continuing. If 0%
   match, error out with a clear message suggesting the files may be
   mismatched.

6. **Filter-during-serialization (no graph mutation)**: The cascade engine
   operates read-only on the graph, computing a `HashSet<Handle>` of handles
   to delete. The output stage passes this set to `GraphXmlWriter`, which
   skips deleted nodes and edges during serialization. The original graph is
   **never mutated** and **never cloned**. This preserves the `ARCHITECTURE.md`
   design property that nodes are never removed from the graph, and avoids the
   memory cost of cloning a large graph.

7. **gramps-reader → typed-graph dependency (already exists)**: The
   `gramps-reader` crate already depends on `typed-graph` in its
   `Cargo.toml`. The new full-graph parser in `xml/graph.rs` uses this
   existing dependency to populate a `typed_graph::Graph`. The streaming
   extractors (`extract.rs`, `count.rs`) do not use the `typed-graph`
   types and remain self-contained — consumers that only need
   person/family streaming are unaffected by the full-graph path.

---

## Future Extensions

- **Undelete / restore**: Reverse a deletion manifest to restore deleted objects
- **Split tree**: Given a set of people, split the tree into two separate
  `.gramps` files (deleted branch → one file, remaining → another)
- **Selection by criteria**: Instead of using visualizer selections, select
  people by query (e.g., "all people born before 1800")
- **Merge cleanup**: After merging two `.gramps` files, automatically clean up
  duplicate or orphaned objects
