# Plan: Diff Tool Robustness to Dangling References

**Source**: User report — `gramps-gen diff` hard-fails when one `.gramps` file
contains references to handles that are not defined as nodes in the same file.

## Problem

When a Gramps XML file contains a handle reference (e.g., `<noteref
hlink="_d9327717d0a61462569c28b064"/>`) but the corresponding `<note
handle="_d9327717d0a61462569c28b064">` element is absent from the file:

1. The parser's two-phase design collects the reference as a `PendingEdge` during the node pass.
2. During `build_edges()`, `Graph::add_edge()` fails with `GraphError::MissingNode(handle)`.
3. This propagates through `graph_error()` → `Error::XmlParseError` → `DiffError::ParseError` → abort.

The diff tool should tolerate this: dangling references are a fact of life in
real Gramps databases, and the fact that a reference exists but its target
doesn't is itself a diff-worthy fact (e.g., the element might be present in the
other file).

## Design Decisions

| Decision | Choice |
|---|---|
| Missing-node strategy | **Placeholder nodes** — create minimal default-constructed nodes for missing targets, inferring the type from the edge context |
| Validation handling | **Downgrade to warnings** — `validate()` logs via `log::warn!` instead of returning `Err(...)` |
| Report output | **Separate dangling counts** — `DiffSummary` gets `dangling_count_a` and `dangling_count_b` tracking placeholder counts per side |

## Rationale

### Why placeholder nodes?

- The graph becomes self-consistent (all edge targets exist), so `add_edge` never fails on missing nodes.
- Node data already stores handle references (e.g., `person.note_list`, `family.citation_list`), so the reference survives even if the edge were skipped — but placeholder nodes also ensure the reference IS a node, participating naturally in the diff.
- The diff matcher works **without any changes**: Pass 1a (exact handle match) catches placeholders vs real nodes and classifies them as `Modified` with field change details showing empty vs populated fields.
- It naturally satisfies the requirement that "the missing elements themselves still need to be diff'ed."

### Why validation downgrade?

- Placeholder nodes have default-empty data, which will trigger structural validation failures (missing required fields).
- Failing on these is counterproductive — the graph is valid *enough* for diffing.
- Warnings are still surfaced (via `log::warn!` / `RUST_LOG=warn`) so users can see integrity issues without blocking the diff.
- The `parse_graph` return type doesn't need to change.

### Why separate dangling counts?

- Users need to distinguish "this graph references 5 notes that don't exist" from "this graph has 0 notes defined."
- The per-item diffs already show the full story, but a summary count helps users decide whether to investigate further.
- Adding two `usize` fields to `DiffSummary` is lightweight.

## Implementation Steps

```
Step 1  (inferred_handles on Graph)  ──┬────────────────────────────────┐
                                       │                                │
Step 2  (target kind inference        │                                │
        + placeholder creation)  ─────┘                                │
                                       │                                │
                                       ├──► Step 3 (downgrade validation)
                                       │
                                       └──► Step 4 (dangling counts in diff)
```

Step 2 bundles target kind inference and placeholder creation in one commit.
Steps 2 and 3 are independent of each other (both depend only on Step 1) and
can be developed in parallel. Step 4 depends on Step 1 (for
`graph.is_inferred_handle()`).

---

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat(typed-graph): add inferred-handle tracking to Graph` | Add `inferred_handles: HashSet<Handle>` to `Graph`, plus accessor methods | `crates/typed-graph/src/graph.rs`: field, `record_inferred_handle()`, `is_inferred_handle()`, `inferred_handle_count()`, initialize in `Graph::new()`. | Unit: new graph has count 0, record one → count 1, `is_inferred_handle` returns true for recorded and false for unrecorded. |
| 2 | `feat(gramps-reader): create placeholder nodes for dangling references` | In `build_edges()`, when a target handle doesn't exist: infer the `NodeKind` from the edge type, create a minimal default-constructed node, record it as inferred, then proceed with the edge | `crates/gramps-reader/src/xml/parse.rs`: <br>— `target_kind_for_edge(&PendingEdge) -> NodeKind` maps each edge variant to its target `NodeKind` (returns non-optional — exhaustive match on both `SimpleEdgeKind` and `PendingEdge` ensures every variant is covered)<br>— `placeholder_node(kind: NodeKind, handle: &str) -> Node` creates default `Node` variant with `handle` field set to the given handle<br>— `build_edges()`: before `add_edge`, check if target exists; create placeholder if missing. If a placeholder already exists for this handle but with a different `NodeKind`, log a `warn!` about the kind conflict (e.g., edge says Citation but a Note placeholder already exists).<br>— all 10 primary types cover all edge variants in the `target_kind_for_edge` mapping (the compiler enforces exhaustiveness) | Unit tests: <br>— Parse XML with a dangling `noteref` → graph has a placeholder Note node, edge exists, `is_inferred_handle()` is true for the placeholder, placeholder's NoteData.handle matches the target handle.<br>— Parse XML with dangling `citationref` → placeholder Citation.<br>— Parse XML with all references intact → no placeholders created (existing behavior).<br>— Test one dangling case per **target node kind** (person, family, event, place, source, citation, repository, media, note, tag — all 10 primary types). This is more systematic than "per edge variant family" and ensures every `NodeKind` branch in `placeholder_node` is exercised.<br>— Parse XML where two edges reference the same missing handle but infer different kinds (e.g., a `CitationRef` says Citation, a `NoteRef` says Note) → first edge wins, second edge logs a `warn!` kind-conflict message but the edge is still added.<br>— Parse XML with intact graph → no placeholders, no kind-conflict warnings. |
| 3 | `feat(gramps-reader): downgrade validation errors to warnings` | In `parse_graph()`, call `parser.validate()` but convert errors to `log::warn!` instead of returning `Err(...)` | `crates/gramps-reader/src/xml/parse.rs`: change `validate()` call site in `parse_graph` to log warnings and continue. Remove the `?` operator. | Integration: parse a graph that has structural validation errors (like placeholder nodes with missing required fields) → succeeds but logs warnings. Use `testing_logger` (or `log` test harness) to capture log output and assert that warnings contain the expected validation error messages — this catches regressions where errors are silently swallowed instead of logged. |
| 4 | `feat(diff): add dangling reference counts to diff report` | Add `dangling_count_a` / `dangling_count_b` to `DiffSummary`, populate in `run_diff()` | `crates/diff/src/report.rs`: two new `usize` fields on `DiffSummary` (default: 0).<br>`crates/diff/src/lib.rs`: in `run_diff`, count `graph_a.inferred_handle_count()` and `graph_b.inferred_handle_count()`, assign to summary.<br>`crates/diff/src/output.rs`: include dangling counts in text and JSON output. | Unit: `DiffSummary` defaults have 0 counts, serde round-trip includes the fields. <br>Integration: diff two files where one has a dangling reference → summary shows `dangling_count_a = 1, dangling_count_b = 0`.<br>Integration: diff two files where each has a different dangling handle → counts 1 and 1, items show both as placeholders with different handles.<br>Integration: diff two files where the *same* handle is dangling in both → counts 1 and 1, Pass 1a matches them as `Same` (both sides have a placeholder with the same handle). This is correct behavior — the element is absent from both files — but the dangling counts give users the signal to investigate. |

## Detailed Design

### Step 1 — `inferred_handles` on Graph

Location: `crates/typed-graph/src/graph.rs`

```rust
pub struct Graph {
    nodes: HashMap<Handle, Node>,
    edges: Vec<Edge>,
    forward_edges: HashMap<Handle, Vec<usize>>,
    reverse_edges: HashMap<Handle, Vec<usize>>,
    validation_state: ValidationState,
    /// Handles that were created as placeholders because they were
    /// referenced (by an edge or a handle-ref field) but never defined
    /// as a full node in the source data.
    inferred_handles: HashSet<Handle>,
}

impl Graph {
    /// Record that a handle was inferred/placeholder rather than
    /// sourced from the original data.
    pub fn record_inferred_handle(&mut self, handle: Handle) {
        self.inferred_handles.insert(handle);
    }

    /// Returns true if the handle was created as a placeholder.
    pub fn is_inferred_handle(&self, handle: &Handle) -> bool {
        self.inferred_handles.contains(handle)
    }

    /// Returns the count of inferred (placeholder) nodes.
    pub fn inferred_handle_count(&self) -> usize {
        self.inferred_handles.len()
    }
}
```

### Step 2 — Target kind inference and placeholder creation

Location: `crates/gramps-reader/src/xml/parse.rs`

**`target_kind_for_edge` function** — maps each `PendingEdge` variant to the
`NodeKind` of the *target* node. Returns `NodeKind` (not `Option`) — the
compiler-enforced exhaustive match on both `SimpleEdgeKind` and `PendingEdge`
ensures every variant is covered:

| Edge variant / `SimpleEdgeKind` | Target kind |
|---|---|
| `SimpleEdgeKind::PersonFamily` | `NodeKind::Family` |
| `SimpleEdgeKind::PersonParentFamily` | `NodeKind::Family` |
| `SimpleEdgeKind::FamilyFather` | `NodeKind::Person` |
| `SimpleEdgeKind::FamilyMother` | `NodeKind::Person` |
| `SimpleEdgeKind::FamilyCitation` | `NodeKind::Citation` |
| `SimpleEdgeKind::FamilyNote` | `NodeKind::Note` |
| `SimpleEdgeKind::FamilyTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::EventPlace` | `NodeKind::Place` |
| `SimpleEdgeKind::EventCitation` | `NodeKind::Citation` |
| `SimpleEdgeKind::EventNote` | `NodeKind::Note` |
| `SimpleEdgeKind::EventTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::PersonCitation` | `NodeKind::Citation` |
| `SimpleEdgeKind::PersonNote` | `NodeKind::Note` |
| `SimpleEdgeKind::PersonTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::PlaceCitation` | `NodeKind::Citation` |
| `SimpleEdgeKind::PlaceNote` | `NodeKind::Note` |
| `SimpleEdgeKind::PlaceTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::PlacePlaceRef` | `NodeKind::Place` |
| `SimpleEdgeKind::SourceNote` | `NodeKind::Note` |
| `SimpleEdgeKind::SourceTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::CitationNote` | `NodeKind::Note` |
| `SimpleEdgeKind::CitationTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::CitationRef` | `NodeKind::Citation` |
| `SimpleEdgeKind::CitationSource` | `NodeKind::Source` |
| `SimpleEdgeKind::MediaCitation` | `NodeKind::Citation` |
| `SimpleEdgeKind::MediaNote` | `NodeKind::Note` |
| `SimpleEdgeKind::MediaTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::NoteCitation` | `NodeKind::Citation` |
| `SimpleEdgeKind::NoteTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::RepositoryNote` | `NodeKind::Note` |
| `SimpleEdgeKind::RepositoryTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::TagTag` | `NodeKind::Tag` |
| `SimpleEdgeKind::NoteRef` | `NodeKind::Note` |
| `SimpleEdgeKind::MediaRef` | `NodeKind::Media` |
| `SimpleEdgeKind::TagRef` | `NodeKind::Tag` |
| `SimpleEdgeKind::PersonMediaRef` | `NodeKind::Media` |
| `SimpleEdgeKind::EventMediaRef` | `NodeKind::Media` |
| `SimpleEdgeKind::FamilyMediaRef` | `NodeKind::Media` |
| `SimpleEdgeKind::CitationMediaRef` | `NodeKind::Media` |
| `SimpleEdgeKind::SourceMediaRef` | `NodeKind::Media` |
| `SimpleEdgeKind::PlaceMediaRef` | `NodeKind::Media` |
| `SimpleEdgeKind::RepositoryMediaRef` | `NodeKind::Media` |
| `PendingEdge::PersonEventRef` | `NodeKind::Event` |
| `PendingEdge::FamilyChildRef` | `NodeKind::Person` |
| `PendingEdge::FamilyEventRef` | `NodeKind::Event` |
| `PendingEdge::PersonPersonRef` | `NodeKind::Person` |
| `PendingEdge::SourceRepoRef` | `NodeKind::Repository` |

**`placeholder_node` function** — creates a minimal default-constructed `Node`
for the given kind, with the handle field set to the supplied handle:

```rust
fn placeholder_node(kind: NodeKind, handle: &str) -> Node {
    let h = handle.to_string();
    match kind {
        NodeKind::Person => Node::Person(PersonData { handle: h, ..PersonData::default() }),
        NodeKind::Family => Node::Family(FamilyData { handle: h, ..FamilyData::default() }),
        NodeKind::Event => Node::Event(EventData { handle: h, ..EventData::default() }),
        NodeKind::Place => Node::Place(PlaceData { handle: h, ..PlaceData::default() }),
        NodeKind::Source => Node::Source(SourceData { handle: h, ..SourceData::default() }),
        NodeKind::Citation => Node::Citation(CitationData { handle: h, ..CitationData::default() }),
        NodeKind::Repository => Node::Repository(RepositoryData { handle: h, ..RepositoryData::default() }),
        NodeKind::Media => Node::Media(MediaData { handle: h, ..MediaData::default() }),
        NodeKind::Note => Node::Note(NoteData { handle: h, ..NoteData::default() }),
        NodeKind::Tag => Node::Tag(TagData { handle: h, ..TagData::default() }),
    }
}
```

Setting the handle field ensures downstream code that reads `node.handle` from
the payload sees the same value as the map key. This avoids misleading field
changes in the diff where a placeholder appears to have an empty handle.

**`build_edges` modification** — before calling `add_edge`, check if the target
exists. Use `get_node()` which returns `Option<&Node>`. `add_node` already
returns `Err(GraphError::DuplicateHandle)` if the handle exists, so the
guard-then-add pattern is safe:

```rust
// In build_edges(), for each PendingEdge:
if self.graph.get_node(&target).is_none() {
    let kind = target_kind_for_edge(&edge);
    let node = placeholder_node(kind, &target);
    self.graph.add_node(target.clone(), node)
        .map_err(graph_error)?;
    self.graph.record_inferred_handle(target.clone());
} else if self.graph.is_inferred_handle(&target) {
    // A placeholder already exists for this handle. Verify the existing
    // node kind matches what this edge expects.
    let expected_kind = target_kind_for_edge(&edge);
    let actual_kind = node_kind(self.graph.get_node(&target).unwrap());
    if expected_kind != actual_kind {
        log::warn!(
            "kind conflict for inferred handle '{}': edge expects {:?}, but {:?} placeholder already exists",
            target, expected_kind, actual_kind
        );
    }
    // Edge is still added — the first kind inference wins.
}
self.graph.add_edge(edge).map_err(graph_error)?;
```

Note: source handles are not guarded for dangling — every source handle must
exist because the parser always creates the referring node before collecting
pending edges. If a source is somehow missing, it's a parser bug and
`add_edge` will return `MissingNode`.

### Step 3 — Validation downgrade

Location: `crates/gramps-reader/src/xml/parse.rs`

Current `parse_graph`:

```rust
pub fn parse_graph(content: &str) -> Result<Graph, Error> {
    let version = detect_schema_version(content)?;
    let schema = Schema::for_version(&version).ok_or_else(|| Error::UnsupportedSchema {
        version: version.clone(),
        schema_version: version.clone(),
    })?;

    let mut parser = Parser::new(schema);
    parser.parse_all(content)?;
    parser.build_edges()?;
    parser.validate()?;       // ← FAILS here on validation errors
    Ok(parser.into_graph())
}
```

Changed to:

```rust
    parser.parse_all(content)?;
    parser.build_edges()?;
    // Validation errors are non-fatal: some are expected for placeholder
    // nodes (missing required fields). Log warnings so the user can see
    // integrity issues without blocking the diff.
    let validation_errors = parser.graph.validate(schema);
    for err in &validation_errors {
        log::warn!("validation warning: {}", err);
    }
    Ok(parser.into_graph())
```

The `Parser::validate()` method is kept for standalone use but not called by
`parse_graph`. The `parse_graph` doc comment is updated to reflect the new
behavior:

```rust
/// Parse a complete Gramps XML document into a [`Graph`].
///
/// ... existing docs ...
///
/// # Dangling references
///
/// If the XML contains handle references to elements that are not defined
/// in the file, placeholder nodes are created for the missing targets.
/// These are tracked via [`Graph::is_inferred_handle`].
///
/// # Validation
///
/// Structural and referential validation errors are logged as warnings
/// (via `log::warn!`) rather than returned as errors. This allows files
/// with dangling references or missing required fields — common in real
/// Gramps databases — to be parsed successfully for diff analysis.
///
/// Parse errors (malformed XML, unsupported schema, I/O errors) remain
/// fatal and are returned as [`Error`].
```

### Step 4 — Dangling counts in diff

Location: `crates/diff/src/report.rs`

Add to `DiffSummary`:

```rust
pub struct DiffSummary {
    // ... existing fields ...
    /// Number of placeholder (inferred) nodes in graph A — nodes created
    /// because a reference targeted a handle that was not defined in the file.
    pub dangling_count_a: usize,
    /// Same for graph B.
    pub dangling_count_b: usize,
}
```

In `run_diff` (`crates/diff/src/lib.rs`):

```rust
summary.dangling_count_a = graph_a.inferred_handle_count();
summary.dangling_count_b = graph_b.inferred_handle_count();
```

Output formatters (`crates/diff/src/output.rs`) include the counts in text and
JSON output.

## What does NOT change

- **Graph model** — no new `Node` variants, no changes to `add_edge` semantics.
- **Diff matcher** — placeholder nodes are just nodes; Pass 1a/1b/comparison
  logic is unchanged.
- **Serialization** — placeholder nodes serialize like any other node (fields
  default to empty/None, so the XML output would show `<note handle="..."/>` as
  self-closing, which is valid).
- **CLI interface** — no new flags or options needed.
- **`parse_graph` return type** — still `Result<Graph, Error>`; parse errors
  (malformed XML, unsupported schema, I/O errors) remain fatal.

### Cross-cutting impact

- **`gramps-reader` is shared** by `diff`, `cli stats`, and `visualize`. After
  this change, `parse_graph` may return graphs with placeholder Person nodes
  (from dangling `FamilyChildRef` or `PersonPersonRef` targets). These
  placeholders have default-empty names, genders, and dates. The visualizer
  and `compute_generation_table` should be reviewed for graceful handling of
  these nodes — for example, filtering out `is_inferred_handle()` persons from
  the visualization or DSU component computation. The `diff` crate is the
  primary consumer and is unaffected.

## Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Placeholder node with empty data matches a real empty node in the other file via Pass 1a, producing a false `Same` classification | Low — handles are UUIDs; exact handle match requires the same handle, which happens only when the same element was dangling in both files (correct) or one file is a derived copy of the other | The dangling counts in the summary give users a signal to investigate. A future enhancement could add a specific `Dangling` classification. |
| `target_kind_for_edge` mapping is incomplete — some edge variants are missed | Medium — there are ~45 entries | Exhaustive match using `match` on every variant of both enums, which the compiler will verify is exhaustive. A `// non_exhaustive` attribute would need a wildcard arm. |
| Placeholder creation hides real bugs (e.g., a typo in `target_kind_for_edge` creating wrong-kind nodes) | Low — the mapping is mechanical; tests cover one case per edge type family | Unit tests verify correct `NodeKind` for each edge variant. |
| Validation warnings are noisy for large files | Medium — a file with 200 dangling notes will produce 200 warnings | `log::warn!` goes to stderr; users can suppress with `RUST_LOG=error`. The summary report includes the dangling count, which is the user-friendly signal. Future enhancement: aggregate warnings by target kind (e.g., "32 dangling note references, 5 dangling citation references") to emit a single summary log line instead of per-node warnings. |
| Conflicting kind inference from multiple edges targeting the same missing handle | Low — unlikely in real Gramps data because handles are typed (handle prefixes usually indicate the type) | When an edge infers one kind but a placeholder of a different kind already exists, log a `warn!` with the conflict details. The first inference wins. If this occurs in practice, the warning message helps diagnose the root cause. |

## Future Enhancements

- **Dangling-specific classification**: instead of `Same`/`Modified` for
  placeholder-vs-placeholder or placeholder-vs-real, add
  `DanglingBoth`/`DanglingResolved` to `Classification`.
- **Graceful handling per dangling pattern**: distinguish "note was completely
  absent from A, referenced in both, defined only in B" from "note was absent
  from both" — currently both cases produce `Same` in Pass 1a (both sides have
  the placeholder), but the second is more concerning.
- **Validation warning aggregation in report**: include a summary of validation
  warnings in the `DiffReport` itself, not just in stderr logs.
