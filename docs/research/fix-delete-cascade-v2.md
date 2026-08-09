# Fix Delete Cascade — Phase 2: Eliminate Remaining False Positives & False Negatives

## Summary

The previous fix (`fix-delete-cascade-isolation.md`) replaced the global-scan
cascade with a frontier-based BFS propagation and fixed several orphan rule
edge-type omissions. However, two bug classes remain:

1. **False negatives**: Items that should be deleted (because all their
   referents are deleted) are kept. Root cause: the `evaluated` set
   prevents re-evaluation when a node's connectivity state changes mid-cascade.

2. **False positives**: Items referenced only by distant relatives (not in any
   deleted family) are flagged for deletion. Root cause: TBD — may be the
   `evaluated` set interacting with multi-hop family-edge traversal, or a
   graph-construction issue; comprehensive tests will isolate this.

This plan focuses on fixing the `evaluated`-set bug (which definitely causes
false negatives and may contribute to false positives), writing the full test
matrix from the Phase 1 plan, and diagnosing any remaining issues revealed by
those tests.

---

## Root Cause: The `evaluated` Set Prevents Re-Evaluation

### The bug

The frontier loop tracks an `evaluated: HashSet<Handle>`:

```rust
// crates/delete/src/cascade.rs, lines 67–71
if to_delete.contains(&neighbor) || evaluated.contains(&neighbor) {
    continue;
}
evaluated.insert(neighbor.clone());
```

Once a node is evaluated and found **not** orphaned, it is added to
`evaluated` and is **never reconsidered** — even if the nodes that kept it
alive are later deleted. This causes **false negatives**: the node stays in
the output when it should have been deleted.

### Concrete example

```
P1 (seed) → Event E1 → Citation C1
P1 (seed) → Family F1 → P2 (seed)
F1 → Citation C1   (FamilyCitation)
```

Frontier processing order matters here. If E1 is processed before F1:

1. `to_delete = {P1, P2}`, frontier = `[P1, P2]`

2. Process P1: evaluates E1 (→ orphaned), evaluates F1 (→ orphaned — P1 and
   P2 are both in `to_delete`).
   `to_delete = {P1, P2, E1, F1}`, frontier = `[E1, F1]`

3. Process E1: evaluates C1 via `EventCitation(E1→C1)`. C1 is evaluated and
   found **orphaned** because both E1 and F1 are already in `to_delete`.
   → C1 added. ✓

But if F1 is processed **before** E1 (different seed iteration order):

1. Process P1: evaluates F1 → orphaned (both parents deleted). Evaluates
   E1 → orphaned.
   `to_delete = {P1, P2, F1, E1}`, frontier = `[F1, E1]`

2. Process F1 first: `FamilyCitation(F1→C1)`. C1 evaluated. At this point,
   E1 is still in `to_delete` (it was added in the same round). C1's
   incident edges: `EventCitation(E1→C1)` where E1 is deleted;
   `FamilyCitation(F1→C1)` where F1 is deleted. → ORPHANED.
   C1 added. ✓

Either order gives the same result *in this case*. Now consider a scenario
where one of C1's referents is NOT yet in `to_delete` at evaluation time:

```
P1 (seed) → Citation C1         (PersonCitation)
P1 (seed) → Event E1 → Citation C1   (EventCitation)
```

1. `to_delete = {P1}`, frontier = `[P1]`

2. Process P1:
   - `PersonCitation(P1→C1)`: neighbor = C1. C1 evaluated.
     - C1 has refs: `PersonCitation(P1→C1)` (P1 deleted),
       `EventCitation(E1→C1)` (E1 **not** in `to_delete` yet).
     - → LIVE. C1 **not** orphaned. C1 added to `evaluated`.
   - `PersonEventRef(P1→E1)`: neighbor = E1. E1 evaluated → ORPHANED.
     `to_delete = {P1, E1}`, frontier = `[E1]`.

3. Process E1:
   - `EventCitation(E1→C1)`: neighbor = C1. **C1 is in `evaluated` → SKIPPED.**

**Result**: `to_delete = {P1, E1}`. C1 is **missing** — it should be deleted
because both P1 and E1 are deleted and no other entity references it. **C1
stays in the output when it should have been deleted.** This is a **false
negative**.

### How `evaluated` could contribute to false positives

The `evaluated` set creates an asymmetry: a node can be evaluated *before* one
of its referents enters the deletion set (leading to false negatives, as above).
In the reverse order — a node is evaluated *after* a referent enters `to_delete`
that would have been caught by a later, more thorough evaluation — the result
is correct deletion. But the order-dependence makes the algorithm **non-
deterministic in its outcome**: the same graph and seeds can produce different
results depending on the iteration order of `HashMap` keys or `HashSet`
elements. In some orderings, nodes may be incorrectly pulled in because a
transient "all dead" state exists before a distant relative's keep-alive edge
is checked.

Consider:

```
P1 (seed) → Event E1 → Place Pl1
P1 (seed) → Family F1 → P2 (NOT seed)
F1 → Citation C1         (FamilyCitation)
P2 → Citation C1         (PersonCitation — this is what should keep C1 alive)
```

If the cascade processes F1 before P2's edges are fully considered in the
`to_delete.contains` check, and F1 is deleted (because both parents were
seeds?) — but P2 is NOT a seed, so F1 has P2 alive. F1 stays. Then C1 from
F1→C1 is evaluated *while* E1 is also being processed. C1 has F1 alive → stays.

But if later E1's cascade causes something that makes C1 re-enter? No, C1 is
in `evaluated` now.

However, if the order is such that C1 is evaluated from P1's deleted path
*without* seeing the P2→C1 edge (maybe because it's an indirect path through
an event that hasn't been evaluated yet), the evaluation could see "all dead"
and incorrectly flag C1. But this requires the edge to P2 to not appear in
C1's `edges_incident_to` at evaluation time — which can't happen because the
graph is immutable during the cascade.

**Conclusion**: The `evaluated` set definitively causes false negatives. It
may also cause false positives through order-dependent behavior if combined
with other edge cases, but the primary mechanism for false positives is more
likely to be found in graph construction (spurious edges) or in orphan rules
that check the wrong edge direction for some edge types. Comprehensive tests
will distinguish.

### The fix: remove the `evaluated` set

The `to_delete.contains(&neighbor)` check already prevents infinite loops:
once a node enters `to_delete`, all its incident edges are skipped. Nodes
that are **not** in `to_delete` can be re-evaluated each time a neighbor
enters `to_delete`, which is the correct behavior — the node's "keep-alive"
status can change as more of its referents are deleted.

Performance impact: each node can be re-evaluated up to its degree (number of
incident edges). For realistic family trees, most nodes have degree < 20, so
the overhead is negligible. The worst-case is bounded by O(V·E) but
pathological cases are implausible in genealogical data.

---

## Test Plan

The full test matrix from `fix-delete-cascade-isolation.md` (31 scenario tests
- 6 unrelated-object tests + 5 property tests = 42 tests) was never fully
implemented. The current `cascade.rs` tests cover only ~16 basic scenarios.

### Test structure

All tests go in `crates/delete/src/cascade.rs` in the existing
`#[cfg(test)] mod tests` block. Each test constructs a minimal graph, runs
the cascade, and asserts the exact set of deleted handles.

### Scenario catalog

#### Category A: Directly associated, isolated → DELETED

| # | Test name | Graph | Seeds | Assert deleted | Assert kept |
|---|---|---|---|---|---|
| A1 | `seed_person_deleted` | P1 alone | {P1} | P1 | — |
| A2 | `family_all_parents_deleted` | P1,P2 in F1 | {P1,P2} | P1,P2,F1 | — |
| A3 | `family_all_parents_and_children_deleted` | P1,P2,P3 in F1 | {P1,P2,P3} | P1,P2,P3,F1 | — |
| A4 | `event_birth_deleted` | P1→E1 | {P1} | P1,E1 | — |
| A5 | `event_shared_two_people_both_deleted` | P1→E1, P2→E1 | {P1,P2} | P1,P2,E1 | — |
| A6 | `event_marriage_both_parents_deleted` | P1,P2 in F1, F1→E1 | {P1,P2} | P1,P2,F1,E1 | — |
| A7 | `citation_person_deleted` | P1→C1 | {P1} | P1,C1 | — |
| A8 | `source_cascade_from_citation` | P1→C1→S1 | {P1} | P1,C1,S1 | — |
| A9 | `repository_cascade_from_source` | P1→C1→S1→R1 | {P1} | P1,C1,S1,R1 | — |
| A10 | `media_cascade_from_person` | P1→M1 | {P1} | P1,M1 | — |
| A11 | `media_cascade_from_source` | P1→C1→S1→M1 | {P1} | P1,C1,S1,M1 | — |
| A12 | `media_cascade_from_citation` | P1→C1→M1 | {P1} | P1,C1,M1 | — |
| A13 | `note_cascade_from_person` | P1→N1 | {P1} | P1,N1 | — |
| A14 | `note_cascade_from_citation` | P1→C1→N1 | {P1} | P1,C1,N1 | — |
| A15 | `tag_cascade_from_person` | P1→T1 | {P1} | P1,T1 | — |
| A16 | `tag_cascade_from_event` | P1→E1→T1 | {P1} | P1,E1,T1 | — |
| A17 | `tag_tag_cascade` | P1→T1→T2 | {P1} | P1,T1,T2 | — |
| A18 | `place_transitive_cascade` | P1→E1→Pl1 | {P1} | P1,E1,Pl1 | — |

#### Category B: Directly associated, NOT isolated → KEPT

| # | Test name | Graph | Seeds | Assert deleted | Assert kept |
|---|---|---|---|---|---|
| B1 | `family_one_parent_remains` | P1,P2 in F1 | {P1} | P1 | F1,P2 |
| B2 | `family_child_keeps_family_alive` | P1,P3 in F1 (child) | {P1} | P1 | F1,P3 |
| B3 | `event_shared_two_people_one_deleted` | P1→E1, P2→E1 | {P1} | P1 | E1,P2 |
| B4 | `event_marriage_one_parent_remains` | P1,P2 in F1, F1→E1 | {P1} | P1 | F1,E1,P2 |
| B5 | `citation_shared_person_keeps_alive` | P1→C1, P2→C1 | {P1} | P1 | C1,P2 |
| B6 | `source_shared_citation_keeps_alive` | P1→C1→S1, P2→C2→S1 | {P1} | P1,C1 | S1,P2,C2 |
| B7 | `repository_shared_source_keeps_alive` | P1→C1→S1→R1, P2→C2→S2→R1 | {P1} | P1,C1,S1 | R1,P2,C2,S2 |
| B8 | `media_shared_person_keeps_alive` | P1→M1, P2→M1 | {P1} | P1 | M1,P2 |
| B9 | `note_shared_person_keeps_alive` | P1→N1, P2→N1 | {P1} | P1 | N1,P2 |
| B10 | `tag_shared_person_keeps_alive` | P1→T1, P2→T1 | {P1} | P1 | T1,P2 |
| B11 | `place_shared_event_keeps_alive` | P1→E1→Pl1, P2→E2→Pl1 | {P1} | P1,E1 | Pl1,P2,E2 |
| B12 | `place_place_ref_target_keeps_alive` | P1→E1→Pl1, Pl2→Pl1, P2→E2→Pl2 | {P1} | P1,E1 | Pl1,Pl2,P2,E2 |

#### Category C: Indirectly associated, isolated → DELETED (transitive cascade)

| # | Test name | Graph | Seeds | Assert deleted | Assert kept |
|---|---|---|---|---|---|
| C1 | `citation_event_cascade` | P1→E1→C1 | {P1} | P1,E1,C1 | — |
| C2 | `citation_place_cascade` | P1→E1→Pl1→C1 (PlaceCitation) | {P1} | P1,E1,Pl1,C1 | — |

Note: Place→Citation is an outgoing edge from Place. In the current direction-
corrected orphan rules, a Place's outgoing references (PlaceCitation,
PlaceMediaRef, PlaceNote, PlaceTag) do NOT keep the place alive. Only incoming
edges (EventPlace, PlacePlaceRef target) do. So when the place becomes
orphaned, its outgoing cascades propagate as expected.

#### Category D: Indirectly associated, NOT isolated → KEPT

| # | Test name | Graph | Seeds | Assert deleted | Assert kept |
|---|---|---|---|---|---|
| D1 | `event_shared_indirect_keeps_citation_alive` | P1→E1→C1, P2→E2→C1 | {P1} | P1,E1 | C1,P2,E2 |

#### Category E: Unrelated — no connection to seeds → NOT deleted

These are the tests that catch the user-reported false-positive bug.

| # | Test name | Graph | Seeds | Assert kept |
|---|---|---|---|---|
| E1 | `unrelated_citation_not_deleted` | P1→E1→Pl1 (seed subgraph); Cx→Sx (unrelated) | {P1} | Cx, Sx |
| E2 | `unrelated_source_not_deleted` | P1→E1 (seed); Sx→Rx→Mx (unrelated chain) | {P1} | Sx, Rx, Mx |
| E3 | `unrelated_media_not_deleted` | P1 alone (seed); Mx connected to P2 (not seed) | {P1} | Mx, P2 |
| E4 | `unrelated_note_not_deleted` | P1 alone (seed); Nx connected to P2 (not seed) | {P1} | Nx, P2 |
| E5 | `unrelated_tag_not_deleted` | P1 alone (seed); Tx connected to P2 (not seed) | {P1} | Tx, P2 |
| E6 | `unrelated_full_chain_not_deleted` | P1→E1→Pl1 (seed); Cx→Sx→Rx→Mx→Nx→Tx (unrelated) | {P1} | all unrelated |

#### Category F: Distant-relative sharing (regression for user's scenario)

| # | Test name | Graph | Seeds | Assert deleted | Assert kept |
|---|---|---|---|---|---|
| F1 | `distant_relative_shared_citation_kept` | P1→E1→C1, P2→C1 (P2 distant, not in same family) | {P1} | P1,E1 | C1,P2 |
| F2 | `distant_relative_shared_source_kept` | P1→C1→S1, P2→C1 (P2 distant) | {P1} | P1,C1 | S1,P2 |
| F3 | `distant_relative_via_family_chain` | P1 in F1←P2; P2 in F2←P3; P3→C1. No shared items with P1. | {P1} | P1 | F1,P2,F2,P3,C1 |

#### Category G: `evaluated`-set regression tests (false negatives from Phase 1)

These test the specific scenario where the `evaluated` set causes a missed
deletion.

| # | Test name | Graph | Seeds | Assert deleted | Assert kept |
|---|---|---|---|---|---|
| G1 | `re_evaluate_when_referent_deleted` | P1→C1, P1→E1→C1 | {P1} | P1,E1,C1 | — |
| G2 | `re_evaluate_shared_citation_all_deleted` | P1→C1, P2→C1, P1→E1→C1, P2→E2→C1 | {P1,P2} | P1,P2,E1,E2,C1 | — |
| G3 | `re_evaluate_multi_hop` | P1→E1→C1→S1, P1→C2→S1 (two citation paths to same source) | {P1} | P1,E1,C1,C2,S1 | — |

#### Category H: Property invariants

| # | Test name | Description |
|---|---|---|
| H1 | `seeds_always_in_result` | All seed handles appear in `to_delete` (already exists) |
| H2 | `idempotency` | Running cascade twice with same input produces same result (already exists) |
| H3 | `empty_seeds_returns_empty` | No seeds → no deletions (already exists) |
| H4 | `pre_connectivity_zero_never_deleted` | Nodes with zero incident edges before operation are never deleted (already exists) |
| H5 | `already_orphaned_nodes_not_deleted` | Nodes that were orphaned before the operation are NOT pulled in (already exists) |
| H6 | `non_seed_people_never_deleted` | People not in the seed set are never added to `to_delete` |
| H7 | `unrelated_subgraph_untouched` | Nodes with no path to any seed are never in `to_delete` |
| H8 | `deterministic_output` | Same graph + same seeds = same `to_delete` set regardless of iteration order |
| H9 | `monotonic_growth` | `to_delete` only grows; nothing is removed once added |

---

## Implementation Steps

### Step 1 — Write test helpers

**File**: `crates/delete/src/cascade.rs`

Write all test helper functions needed by the test matrix. These helpers
construct small graphs with specific node/edge configurations, returning
handles for use in assertions.

```rust
// People
fn make_person(graph: &mut Graph, handle: &str) -> Handle { ... }

// Families — `father` and `mother` are both parent handles; edges are
// FamilyFather(source=fam, target=father) and FamilyMother(source=fam, target=mother).
fn make_family_with_parents(graph: &mut Graph, handle: &str, father: &Handle, mother: &Handle) -> Handle { ... }

// Family with a child and both parents. `father` and `mother` are parent
// handles; `child` gets a FamilyChildRef edge from the family. The parents
// also get PersonParentFamily edges back to the family.
fn make_family_with_parents_and_child(
    graph: &mut Graph, handle: &str,
    father: &Handle, mother: &Handle, child: &Handle
) -> Handle { ... }

// Events — creates PersonEventRef(source=person, target=event)
fn make_event(graph: &mut Graph, handle: &str, person: &Handle) -> Handle { ... }
fn make_event_with_place(graph: &mut Graph, event_h: &str, person: &Handle, place_h: &Handle) -> Handle { ... }

// Places — creates EventPlace(source=event, target=place)
fn make_place(graph: &mut Graph, handle: &str, event: &Handle) -> Handle { ... }

// PlacePlaceRef: source_place → target_place. The orphan rule only considers
// incoming PlacePlaceRef edges (where this place is the target) as keep-alive.
fn make_place_with_place_ref(graph: &mut Graph, source_h: &str, target_h: &str) { ... }

// Citations
fn citation_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle { ... }
fn citation_from_event(graph: &mut Graph, handle: &str, event: &Handle) -> Handle { ... }
fn citation_from_family(graph: &mut Graph, handle: &str, family: &Handle) -> Handle { ... }
fn citation_from_place(graph: &mut Graph, handle: &str, place: &Handle) -> Handle { ... }

// Sources
fn source_from_citation(graph: &mut Graph, handle: &str, citation: &Handle) -> Handle { ... }

// Repositories
fn repository_from_source(graph: &mut Graph, handle: &str, source: &Handle) -> Handle { ... }

// Media
fn media_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle { ... }
fn media_from_citation(graph: &mut Graph, handle: &str, citation: &Handle) -> Handle { ... }
fn media_from_source(graph: &mut Graph, handle: &str, source: &Handle) -> Handle { ... }

// Notes
fn note_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle { ... }
fn note_from_citation(graph: &mut Graph, handle: &str, citation: &Handle) -> Handle { ... }

// Tags
fn tag_from_person(graph: &mut Graph, handle: &str, person: &Handle) -> Handle { ... }
fn tag_from_event(graph: &mut Graph, handle: &str, event: &Handle) -> Handle { ... }
fn tag_tag(graph: &mut Graph, source_h: &str, target_h: &str) { ... }
```

### Step 2 — Write Category A tests (directly associated, isolated → DELETED)

**File**: `crates/delete/src/cascade.rs`

Implement tests A1–A18. These cover cascades from seed people through
families, events, places, citations, sources, repositories, media, notes,
and tags when all referents are deleted.

### Step 3 — Write Category B, C, D tests (direct/indirect, NOT isolated → KEPT)

**File**: `crates/delete/src/cascade.rs`

Implement tests B1–B12 (directly associated, kept via shared referent),
C1–C2 (indirect cascade through events/places to citations), and D1
(indirect association kept by a live referent).

### Step 4 — Write Category E, F, G tests (unrelated, distant-relative, evaluated-set regressions)

**File**: `crates/delete/src/cascade.rs`

Implement tests E1–E6 (unrelated subgraphs never touched), F1–F3 (distant
relatives don't cascade onto shared items), and G1–G3 (evaluated-set
false-negative regression scenarios).

### Step 5 — Write Category H tests (property invariants)

**File**: `crates/delete/src/cascade.rs`

Implement new property tests H6–H9:

- H6: `non_seed_people_never_deleted` — People not in seeds are never in `to_delete`
- H7: `unrelated_subgraph_untouched` — Nodes with no path to seeds are never deleted
- H8: `deterministic_output` — Same graph + same seeds = same `to_delete` set
- H9: `monotonic_growth` — `to_delete` only grows; nothing is removed once added

Tests H1–H5 already exist and should be preserved.

### Step 6 — Run all tests against current code (baseline)

```bash
cargo test -p delete
```

Record which tests pass and fail against the current cascade implementation.
Expected: several G tests will fail (false negatives from `evaluated` set).
E tests may fail if the global-scan bug still persists. This establishes
the baseline before algorithmic changes.

### Step 7 — Remove the `evaluated` set and sort frontier for determinism

**File**: `crates/delete/src/cascade.rs`

Remove the `evaluated` variable and both references to it in the frontier loop.
Also add `frontier.sort_unstable()` at the top of each while-loop iteration to
guarantee deterministic output (needed for H8).

```rust
// REMOVE this variable:
let mut evaluated: HashSet<Handle> = HashSet::new();

// In the frontier loop, change:
if to_delete.contains(&neighbor) || evaluated.contains(&neighbor) {
    continue;
}
evaluated.insert(neighbor.clone());
// TO:
if to_delete.contains(&neighbor) {
    continue;
}

// At the top of each frontier-processing iteration, add:
frontier.sort_unstable();
```

The `to_delete.contains(&neighbor)` check alone prevents infinite loops.
Nodes not in `to_delete` can now be re-evaluated each time a neighbor
enters `to_delete`, which is the correct behavior. Sorting the frontier
guarantees that re-evaluation happens in a deterministic order regardless
of HashMap/HashSet iteration.

### Step 8 — Add `non_seed_people_never_deleted` guard to cascade

**File**: `crates/delete/src/cascade.rs`

Add an explicit check at the top of `type_specific_orphan_rule`: if a node
is a Person, immediately return `false`. This formalizes the invariant that
non-seed people are never deleted by cascade. The existing `Node::Person(_) => false`
match arm must be removed to avoid dead code.

```rust
fn type_specific_orphan_rule(handle: &Handle, graph: &Graph, to_delete: &HashSet<Handle>) -> bool {
    let node = match graph.get_node(handle) {
        Some(n) => n,
        None => return false,
    };

    // Hard invariant: non-seed people are never deleted by cascade.
    // The Person arm is removed from the match below to avoid dead code.
    if matches!(node, Node::Person(_)) {
        return false;
    }

    match node {
        // NOTE: Node::Person arm removed — handled by the guard above
        Node::Family(_) => { ... }
        // ... rest of rules unchanged
    }
}
```

### Step 9 — Run tests and diagnose failures

```bash
cargo test -p delete
```

If any test fails, the failure pinpoints a specific bug:

- **E1–E6 failures**: The cascade is reaching unrelated nodes → graph
  construction or orphan rule direction bug.
- **F1–F3 failures**: Distant-relative items are being deleted → family
  cascade is propagating too far.
- **G1–G3 failures**: The `evaluated` set removal wasn't sufficient; there
  may be additional ordering issues.
- **H8 failure**: The algorithm is non-deterministic → residual ordering
  dependency.

### Step 10 — Fix any issues revealed by tests

Iterate: fix → run tests → repeat until all pass.

### Step 11 — Build, lint, and full workspace test

```bash
cargo test -p delete
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Graph Construction Audit (if tests E1–E6 pass but user still sees bug)

If the cascade unit tests pass (confirming the algorithm is correct for
synthetic graphs) but the user's real `.gramps` file still produces false
positives, the bug is in graph construction from the XML parser.

### Audit checklist for `crates/gramps-reader/src/xml/graph.rs`

1. **Verify edge direction for all edge types**: Each `self.edges.push(Edge::...)`
   should have the correct source/target assignment according to the Gramps data
   model.

2. **Check for duplicate edges**: If the same hlink reference is parsed twice
   (e.g., once from inline data and once from an element), duplicate edges
   could create phantom connectivity paths.

3. **Check inferred handles**: Handles created as placeholders for dangling
   references should NOT create edges in the graph (the `build` method skips
   edges with missing targets but existing sources — verify this is consistent
   for all edge types).

4. **Cross-reference with Gramps XML spec**: Some Gramps XML versions encode
   references differently (e.g., `<sourceref hlink="..."/>` vs inline source
   data). Ensure all reference patterns are parsed correctly.

---

## Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Removing `evaluated` degrades performance | Worst-case nodes evaluated O(degree) times; family trees have low degree. Measure with `cargo bench` if needed. |
| `to_delete.contains` alone doesn't prevent infinite cycles | Cycles in the graph (A↔B) are handled: first one evaluated → orphaned → added to `to_delete`; second one sees the first in `to_delete` → skipped. Non-orphaned cycles just get re-evaluated. Bounded by node count. |
| Determinism (H8) may be hard to guarantee with HashSet | Switch frontier to `Vec<Handle>` (already done) + sort frontier before iteration for deterministic ordering. |
| Graph construction bugs not caught by cascade tests | Tests E1–E6 specifically test the cascade against fully-disconnected subgraphs. If these pass, the cascade respects graph topology. If user still sees bug with real data, construct a minimal repro from their data. |

---

## Decisions

1. **Remove `evaluated` set**: Yes. The `to_delete.contains` check already
   prevents infinite loops. Correctness > minor performance optimization.

2. **Non-seed people are never deleted**: Formalized as a hard guard at the
   top of `type_specific_orphan_rule`, replacing (not supplementing) the
   existing `Node::Person(_) => false` match arm to avoid dead code.

3. **Test ordering**: Write all tests first against current code (Steps 1–6),
   establish a baseline, then apply algorithmic fixes (Steps 7–8) and verify
   that previously-failing tests turn green. This gives a clear before/after
   picture of each fix's effect.

4. **Place orphan rule direction**: Keep the existing (already fixed)
   direction — only incoming edges (EventPlace, PlacePlaceRef target) keep
   a place alive. No further changes needed.

5. **Frontier determinism**: Sort frontier at the start of each iteration to
   guarantee deterministic output regardless of HashMap/HashSet order.
   Required for property test H8.

6. **Step granularity**: The test matrix is split into 5 implementation steps
   (helpers, cat A, cat B-D, cat E-G, cat H) so each commit is a small,
   testable unit per the incremental-development workflow.
