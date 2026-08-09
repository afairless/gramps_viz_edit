# Fix Delete Cascade: Stop Unrelated Objects from Being Deleted

## Summary

The delete tool's cascade engine incorrectly marks citations, sources, media,
and other objects for deletion even when those objects have **no connection**
(direct or indirect) to the people selected for deletion. The only objects
that should be deleted are those that 1) are connected directly or indirectly
to the seed people AND 2) become completely isolated once the seed people
and their associated items are removed.

---

## Root Cause Analysis

### The bug

The cascade algorithm (`crates/delete/src/cascade.rs`) uses a **global scan**
approach: each fixed-point iteration evaluates *every* node in the graph
against the current `to_delete` set. This allows nodes that are completely
unrelated to the seed people to be pulled into the deletion set.

### Concrete example

Graph:

```
Person P1 → Event E1 → Place Pl1        (seed chain — P1 is selected for deletion)
Citation Cx → Source Sx                  (unrelated chain — no connection to P1/E1/Pl1)
```

- `Cx` has `pre_connectivity = 1` (the `CitationSource` edge `Cx→Sx`)
- `Cx` has **zero** incoming `PersonCitation`, `FamilyCitation`, `EventCitation`,
  or `PlaceCitation` edges (no one references this citation)
- The orphan rule for `Node::Citation` returns `!has_live_ref` → `true`
  because there are no incoming citation-ref edges from *any* live node
- **Result**: `Cx` is added to `to_delete` on iteration 1, then `Sx`
  is pulled in on iteration 2

The orphan rule is computing the *right answer* to the question "is this
citation orphaned?" (yes — no one references it). But it's asking the
**wrong question**: the algorithm should only evaluate nodes that are
**reachable from the seeds**. `Cx` is not reachable from `P1`, so it
should never have been considered.

### Why `pre_connectivity` doesn't prevent this

The `pre_connectivity` gate (`pre_count == 0 → skip`) only excludes nodes
that had **zero** incident edges before the operation. `Cx` has one incident
edge (the `CitationSource` edge to `Sx`), so it passes the gate. This edge
does not keep the citation "alive" in the orphan rule's semantics (only
incoming citation-ref edges do), but it still inflates the connectivity
count enough to bypass the gate.

### Algorithm comparison

| Approach | How it works | Problem |
|---|---|---|
| **Current** (global scan) | Each iteration checks *all* nodes against `to_delete`. Any node whose "alive" connections are all to deleted nodes is added. | Nodes unconnected to seeds get pulled in because they happen to be "orphaned" according to the type-specific rule. |
| **Correct** (neighbor-only) | Only check nodes that are *adjacent* to newly-deleted nodes. The cascade propagates outward from the seeds through actual graph edges. | No problem — unreachable nodes are never examined. |

---

## Proposed Fix

### Primary fix: Replace global scan with frontier-based propagation

Change the cascade algorithm from a global scan to a frontier-based BFS
propagation. Instead of iterating over all nodes each cycle, only check
**neighbors of nodes that were just added to the deletion set**.

**New algorithm:**

```
Phase A — Record pre-existing connectivity (unchanged)
Phase B — Frontier-based fixed-point loop:
  1. to_delete = seeds (filtered to existing handles)
  2. frontier = seeds
  3. While frontier is not empty:
     a. new_frontier = {}
     b. For each deleted_node in frontier:
        - Get all incident edges for deleted_node
        - For each edge, identify the OTHER node (the neighbor)
        - If neighbor is NOT in to_delete and has pre_count > 0:
          - Run type_specific_orphan_rule(neighbor, graph, to_delete)
          - If orphaned: add to to_delete AND to new_frontier
     c. frontier = new_frontier
```

**Key property**: A node is only *ever* evaluated if it is adjacent to a
node that is already marked for deletion. This guarantees that the cascade
only propagates through actual graph connections, never "reaching out" to
unrelated subgraphs.

**Performance**: For a graph with V nodes and E edges, the global scan is
O(V² · E) worst-case (V iterations checking V nodes, each evaluating E
incident edges). The frontier approach is O(V · E) worst-case (each node
is only evaluated once when one of its neighbors is deleted). For the
practical case of deleting a few people from a large family tree, the
frontier approach visits far fewer nodes.

### Secondary fix: Audit orphan rules for edge-direction correctness

While the primary fix prevents unrelated nodes from being evaluated, the
orphan rules should also be semantically correct. Some orphan rules currently
treat **outgoing** edges (where the node is the **source**, not the target)
as keeping the node alive, which is semantically wrong.

**Principle**: A node N is "kept alive" if there exists a non-deleted node O
that **references** N. "References" means there is an edge **from O to N**
(N is the *target*). N's own outgoing references (where N is the *source*)
don't prove N is in use — they're N's own usage of other resources.

#### Edge-direction audit table

For each edge type `A → B` (source → target), this table shows which node
type the edge "keeps alive" and whether the current orphan rule handles it
correctly.

| Edge type | Source → Target | Keeps Target alive? | Keeps Source alive? | Current rule correct? |
|---|---|---|---|---|
| **PersonEventRef** | Person → Event | Yes (event is in use) | No | ✓ |
| **FamilyEventRef** | Family → Event | Yes | No | ✓ |
| **EventPlace** | Event → Place | Yes (place referenced by event) | No | ✓ |
| **PersonCitation** | Person → Citation | Yes | No | ✓ |
| **FamilyCitation** | Family → Citation | Yes | No | ✓ |
| **EventCitation** | Event → Citation | Yes | No | ✓ |
| **PlaceCitation** | Place → Citation | Yes | No | ✓ |
| **CitationSource** | Citation → Source | Yes (source referenced by citation) | No | ✓ |
| **SourceRepoRef** | Source → Repository | Yes (repo referenced by source) | No | ✓ |
| **PersonMediaRef** | Person → Media | Yes | No | ✓ |
| **FamilyMediaRef** | Family → Media | Yes | No | ✓ |
| **EventMediaRef** | Event → Media | Yes | No | ✓ |
| **PlaceMediaRef** | Place → Media | Yes | No | ✓ |
| **SourceMediaRef** | Source → Media | Yes | No | ✓ |
| **RepositoryMediaRef** | Repository → Media | Yes | No | ✓ |
| **CitationMediaRef** | Citation → Media | Yes | No | MISSING from Media rule |
| **PersonNote** | Person → Note | Yes | No | ✓ |
| **FamilyNote** | Family → Note | Yes | No | ✓ |
| **EventNote** | Event → Note | Yes | No | ✓ |
| **PlaceNote** | Place → Note | Yes | No | ✓ |
| **SourceNote** | Source → Note | Yes | No | ✓ |
| **CitationNote** | Citation → Note | Yes | No | MISSING from Note rule |
| **RepositoryNote** | Repository → Note | Yes | No | ✓ |
| **MediaNote** | Media → Note | Yes | No | ✓ |
| **PersonTag** | Person → Tag | Yes | No | ✓ |
| **FamilyTag** | Family → Tag | Yes | No | ✓ |
| **EventTag** | Event → Tag | Yes | No | ✓ |
| **PlaceTag** | Place → Tag | Yes | No | ✓ |
| **SourceTag** | Source → Tag | Yes | No | ✓ |
| **CitationTag** | Citation → Tag | Yes | No | MISSING from Tag rule |
| **RepositoryTag** | Repository → Tag | Yes | No | ✓ |
| **MediaTag** | Media → Tag | Yes | No | ✓ |
| **NoteTag** | Note → Tag | Yes | No | ✓ |
| **TagTag** | Tag → Tag | Yes | Yes | MISSING from Tag rule |
| **FamilyFather** | Family → Person | No (person exists independently) | No (family is source) | ✓ |
| **FamilyMother** | Family → Person | No | No | ✓ |
| **FamilyChildRef** | Family → Person | No | No | ✓ |
| **PersonFamily** | Person → Family | No | No | (not checked, correct) |
| **PersonParentFamily** | Person → Family | No | No | (not checked, correct) |
| **PlacePlaceRef** | Place → Place | Yes, if N is target | No, if N is source | PARTIAL: treats source-side "other" as keeping place alive |
| **MediaCitation** | Media → Citation | Yes | No | MISSING from Citation rule |
| **NoteCitation** | Note → Citation | Yes | No | MISSING from Citation rule |

**Findings from the audit:**

1. **Media orphan rule** — missing `CitationMediaRef`. A citation can reference
   media, and if the citation is alive, it should keep the media alive.
2. **Note orphan rule** — missing `CitationNote`. (Though `MediaNote` is present.)
3. **Tag orphan rule** — missing `CitationTag` and `TagTag`.
4. **Citation orphan rule** — missing `MediaCitation` and `NoteCitation`.
   Media objects and notes can reference citations.
5. **Place orphan rule** — `PlacePlaceRef`, `PlaceCitation`, `PlaceMediaRef`,
   `PlaceNote`, `PlaceTag` all have the direction issue: they check if the
   *target* is alive to keep the *source* (place) alive, which is inverted.
   These should be restructured to check INCOMING references to the place,
   not OUTGOING references from the place.

**Decision on item 5**: With the frontier-based propagation fix, the direction
issue in the Place orphan rule becomes less critical because the place is
only evaluated when a neighbor is being deleted. If the neighbor is the
*target* of a Place→X edge (e.g., the place's citation is being deleted), the
place's own outgoing reference disappearing shouldn't cause the place to
become orphaned. However, fixing the direction makes the semantics clearer
and prevents edge cases. Recommended: fix as part of this plan.

### Revised orphan rules (after fix)

Each rule should only check edges where the node is the **target** (i.e.,
the node is referenced/used by another non-deleted node):

#### Family

- Check `FamilyFather`, `FamilyMother`, `FamilyChildRef` edges (Family→Person):
  is there at least one non-deleted Person as a target?
- Also check `PersonFamily`, `PersonParentFamily` edges (Person→Family):
  is there at least one non-deleted Person as a source?
- Note: A family is defined by its members. If all members are deleted,
  the family is empty and should be deleted.
- *(revised — add Person→Family edge checks)*

#### Event

- Check `PersonEventRef`, `FamilyEventRef`: is there at least one non-deleted
  Person or Family referencing this event? (Event is the *target* of these edges.)
- *(unchanged from current)*

#### Place

- Check `EventPlace` (Event→Place): is there a non-deleted Event referencing
  this place? (Place is the *target*.)
- Check `PlacePlaceRef` where this place is the **target**: is there a
  non-deleted Place referencing this place?
- Remove checks for `PlaceCitation`, `PlaceMediaRef`, `PlaceNote`, `PlaceTag`
  (Place is the *source* of these — the place's own references don't keep it alive).
- *(revised — remove outgoing-edge checks, add direction-aware PlacePlaceRef)*

#### Source

- Check `CitationSource` (Citation→Source): is there a non-deleted Citation
  referencing this source? (Source is the *target*.)
- *(unchanged from current)*

#### Citation

- Check `PersonCitation`, `FamilyCitation`, `EventCitation`, `PlaceCitation`,
  `MediaCitation`, `NoteCitation`: is there a non-deleted node of any type
  referencing this citation? (Citation is the *target*.)
- *(revised — add `MediaCitation`, `NoteCitation`)*

#### Repository

- Check `SourceRepoRef` (Source→Repository): is there a non-deleted Source
  referencing this repository? (Repository is the *target*.)
- *(unchanged from current)*

#### Media

- Check `PersonMediaRef`, `FamilyMediaRef`, `EventMediaRef`, `PlaceMediaRef`,
  `SourceMediaRef`, `RepositoryMediaRef`, `CitationMediaRef`: is there a
  non-deleted node of any type referencing this media? (Media is the *target*.)
- *(revised — add `CitationMediaRef`)*

#### Note

- Check `PersonNote`, `FamilyNote`, `EventNote`, `PlaceNote`, `SourceNote`,
  `CitationNote`, `RepositoryNote`, `MediaNote`: is there a non-deleted node
  of any type referencing this note? (Note is the *target*.)
- *(revised — add `CitationNote`)*

#### Tag

- Check `PersonTag`, `FamilyTag`, `EventTag`, `PlaceTag`, `SourceTag`,
  `CitationTag`, `RepositoryTag`, `MediaTag`, `NoteTag`, `TagTag`: is there a
  non-deleted node of any type referencing this tag? (Tag is the *target*.)
- *(revised — add `CitationTag`, `TagTag`)*

---

## Test Plan

### Test structure

Tests will live in `crates/delete/src/cascade.rs` in the existing
`#[cfg(test)] mod tests` block. Each test constructs a minimal graph,
runs the cascade, and asserts the exact set of handles to delete.

### Scenario catalog

The user requested tests covering 6 categories across all object types.
Below is the full matrix.

#### Legend

- **(S) Seed**: Person selected for deletion
- **(D-DL) Direct, DeLeted**: Directly associated with seed, becomes isolated → should be deleted
- **(D-KL) Direct, Kept aLive**: Directly associated with seed, NOT isolated → should NOT be deleted
- **(I-DL) Indirect, DeLeted**: Indirectly associated with seed, becomes isolated → should be deleted
- **(I-KL) Indirect, Kept aLive**: Indirectly associated with seed, NOT isolated → should NOT be deleted
- **(U) Unrelated**: No connection to seed → should NOT be deleted

#### Test matrix

| # | Test name | Seed | Person | Family | Event | Place | Citation | Source | Repository | Media | Note | Tag |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `seed_person_deleted` | P1(S) | P1(D-DL) | — | — | — | — | — | — | — | — | — |
| 2 | `family_all_parents_deleted` | P1(S),P2(S) | P1(D-DL),P2(D-DL) | F1(D-DL) | — | — | — | — | — | — | — | — |
| 3 | `family_one_parent_remains` | P1(S) | P1(D-DL) | F1(D-KL) | — | — | — | — | — | — | — | — |
| 4 | `family_child_keeps_family_alive` | P1(S) | P1(D-DL) | F1(D-KL) | — | — | — | — | — | — | — | — |
| 5 | `event_birth_deleted` | P1(S) | P1(D-DL) | — | E1(D-DL) | — | — | — | — | — | — | — |
| 6 | `event_shared_by_two_people_one_deleted` | P1(S) | P1(D-DL) | — | E1(D-KL) | — | — | — | — | — | — | — |
| 7 | `event_shared_by_two_people_both_deleted` | P1(S),P2(S) | P1(D-DL),P2(D-DL) | — | E1(D-DL) | — | — | — | — | — | — | — |
| 8 | `event_marriage_both_parents_deleted` | P1(S),P2(S) | P1(D-DL),P2(D-DL) | F1(D-DL) | E1(D-DL) | — | — | — | — | — | — | — |
| 9 | `event_marriage_one_parent_remains` | P1(S) | P1(D-DL) | F1(D-KL) | E1(D-KL) | — | — | — | — | — | — | — |
| 10 | `place_transitive_cascade` | P1(S) | P1(D-DL) | — | E1(D-DL) | Pl1(D-DL) | — | — | — | — | — | — |
| 11 | `place_shared_event_keeps_alive` | P1(S) | P1(D-DL) | — | E1(D-DL) | Pl1(D-KL) | — | — | — | — | — | — |
| 12 | `place_place_ref_target_keeps_alive` | P1(S) | P1(D-DL) | — | E1(D-DL) | Pl1(D-KL) | — | — | — | — | — | — |
| 13 | `citation_person_deleted` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | — | — | — | — | — |
| 14 | `citation_shared_person_keeps_alive` | P1(S) | P1(D-DL) | — | — | — | C1(D-KL) | — | — | — | — | — |
| 15 | `citation_event_cascade` | P1(S) | P1(D-DL) | — | E1(D-DL) | — | C1(D-DL) | — | — | — | — | — |
| 16 | `citation_place_cascade` | P1(S) | P1(D-DL) | — | E1(D-DL) | Pl1(D-DL) | C1(D-DL) | — | — | — | — | — |
| 17 | `source_cascade_from_citation` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | S1(D-DL) | — | — | — | — |
| 18 | `source_shared_citation_keeps_alive` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | S1(D-KL) | — | — | — | — |
| 19 | `repository_cascade_from_source` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | S1(D-DL) | R1(D-DL) | — | — | — |
| 20 | `repository_shared_source_keeps_alive` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | S1(D-DL) | R1(D-KL) | — | — | — |
| 21 | `media_cascade_from_person` | P1(S) | P1(D-DL) | — | — | — | — | — | — | M1(D-DL) | — | — |
| 22 | `media_shared_person_keeps_alive` | P1(S) | P1(D-DL) | — | — | — | — | — | — | M1(D-KL) | — | — |
| 23 | `media_cascade_from_source` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | S1(D-DL) | — | M1(D-DL) | — | — |
| 24 | `media_cascade_from_citation` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | — | — | M1(D-DL) | — | — |
| 25 | `note_cascade_from_person` | P1(S) | P1(D-DL) | — | — | — | — | — | — | — | N1(D-DL) | — |
| 26 | `note_shared_person_keeps_alive` | P1(S) | P1(D-DL) | — | — | — | — | — | — | — | N1(D-KL) | — |
| 27 | `note_cascade_from_citation` | P1(S) | P1(D-DL) | — | — | — | C1(D-DL) | — | — | — | N1(D-DL) | — |
| 28 | `tag_cascade_from_person` | P1(S) | P1(D-DL) | — | — | — | — | — | — | — | — | T1(D-DL) |
| 29 | `tag_shared_person_keeps_alive` | P1(S) | P1(D-DL) | — | — | — | — | — | — | — | — | T1(D-KL) |
| 30 | `tag_cascade_from_event` | P1(S) | P1(D-DL) | — | E1(D-DL) | — | — | — | — | — | — | T1(D-DL) |
| 31 | `tag_tag_cascade` | P1(S) | P1(D-DL) | — | — | — | — | — | — | — | — | T1(D-DL), T2(D-DL) |

#### Unrelated-object tests (the core bug)

| # | Test name | Seed | Unrelated objects | Assertion |
|---|---|---|---|---|
| 32 | `unrelated_citation_not_deleted` | P1(S)→E1→Pl1 | Cx→Sx (separate chain) | Cx, Sx NOT deleted |
| 33 | `unrelated_source_not_deleted` | P1(S)→E1 | Sx→Rx→Mx (separate chain) | Sx, Rx, Mx NOT deleted |
| 34 | `unrelated_media_not_deleted` | P1(S) | Mx connected to P2 (not seed) | Mx NOT deleted |
| 35 | `unrelated_note_not_deleted` | P1(S) | Nx connected to P2 (not seed) | Nx NOT deleted |
| 36 | `unrelated_tag_not_deleted` | P1(S) | Tx connected to P2 (not seed) | Tx NOT deleted |
| 37 | `unrelated_full_chain_not_deleted` | P1(S)→E1→Pl1 | Cx→Sx→Rx→Mx→Nx→Tx | entire unrelated chain NOT deleted |

#### Property-based tests

| # | Test name | Description |
|---|---|---|
| 38 | `seeds_always_in_result` | All seed handles appear in the deletion set (existing, keep) |
| 39 | `idempotency` | Running cascade twice with same input produces same result (existing, keep) |
| 40 | `empty_seeds_returns_empty` | No seeds → no deletions |
| 41 | `pre_connectivity_respected` | Nodes with pre_count=0 are never deleted |
| 42 | `unrelated_subgraph_preserved` | After deletion, all nodes not in to_delete are unchanged |

### Test helpers needed

New graph-builder helpers for tests:

- `person_with_citation()` — Person → Citation (PersonCitation edge)
- `person_with_media()` — Person → Media (PersonMediaRef edge)
- `person_with_note()` — Person → Note (PersonNote edge)
- `person_with_tag()` — Person → Tag (PersonTag edge)
- `citation_with_source()` — Citation → Source (CitationSource edge)
- `source_with_repository()` — Source → Repository (SourceRepoRef edge)
- `source_with_media()` — Source → Media (SourceMediaRef edge)
- `citation_with_media()` — Citation → Media (CitationMediaRef edge)
- `citation_with_note()` — Citation → Note (CitationNote edge)
- `citation_with_tag()` — Citation → Tag (CitationTag edge)
- `event_with_place_and_citation()` — Event → Place, Event → Citation
- `place_with_place_ref()` — Place1 → Place2 (PlacePlaceRef edge)
- `tag_with_tag()` — Tag1 → Tag2 (TagTag edge)

---

## Implementation Steps

### Step 1 — Rewrite cascade algorithm to frontier-based propagation

**File**: `crates/delete/src/cascade.rs`

Replace the Phase B loop with a frontier-based approach. Key changes:

1. Build an adjacency lookup or use `edges_incident_to()` to find neighbors
2. Track a `frontier` set of handles just added
3. Only evaluate neighbors of frontier nodes
4. New orphans become the next frontier
5. Terminate when frontier is empty

Pseudocode:

```rust
// Phase B: Frontier-based fixed-point loop
let mut to_delete: HashSet<Handle> = seeds.clone();
to_delete.retain(|h| graph.contains_node(h));

let mut frontier: Vec<Handle> = to_delete.iter().cloned().collect();

while !frontier.is_empty() {
    let mut new_frontier: Vec<Handle> = Vec::new();

    for deleted_handle in &frontier {
        // Get all neighbors via incident edges
        let incident = graph.edges_incident_to(deleted_handle);
        for edge in &incident {
            // Determine the neighbor handle
            let neighbor = /* extract the other endpoint of the edge */;
            if to_delete.contains(&neighbor) {
                continue;
            }
            let pre_count = pre_connectivity.get(&neighbor).copied().unwrap_or(0);
            if pre_count == 0 {
                continue;
            }
            if type_specific_orphan_rule(&neighbor, graph, &to_delete) {
                to_delete.insert(neighbor.clone());
                new_frontier.push(neighbor);
            }
        }
    }

    if new_frontier.is_empty() {
        break;
    }
    frontier = new_frontier;
}
```

**Edge case**: When extracting the neighbor from an edge, we need to handle both
directions (the edge's source and target). The `edge_source_target` helper from
`graph.rs` can be reused. Since `deleted_handle` was passed to `edges_incident_to`,
it appears as either source or target. The neighbor is the other handle.

Actually, `edges_incident_to` already returns edges where the handle is either
source or target. We can use a helper:

```rust
fn edge_other_endpoint(edge: &Edge, handle: &Handle) -> Handle {
    let (source, target) = edge_source_target(edge);
    if source == *handle { target } else { source }
}
```

### Step 2 — Fix Place orphan rule (direction issue)

**File**: `crates/delete/src/cascade.rs`

Revise `Node::Place` orphan rule:

- Keep: `EventPlace` (Event→Place, place is target) ✓
- Keep: `PlacePlaceRef` but only when place is **target** (another place references this place)
- Remove: `PlaceCitation` (outgoing from place)
- Remove: `PlaceMediaRef` (outgoing from place)
- Remove: `PlaceNote` (outgoing from place)
- Remove: `PlaceTag` (outgoing from place)

```rust
Node::Place(_) => {
    let incident = graph.edges_incident_to(handle);
    let has_live_connection = incident.iter().any(|e| match e {
        // EventPlace: Event → Place. Place is target.
        Edge::EventPlace { source, target } => {
            // target is the place, source is the event
            target == handle && !to_delete.contains(source)
        }
        // PlacePlaceRef: only when this place is the target (referenced by another place)
        Edge::PlacePlaceRef { source, target, .. } => {
            target == handle && !to_delete.contains(source)
        }
        _ => false,
    });
    !has_live_connection
}
```

### Step 3 — Add missing edge types to orphan rules

**File**: `crates/delete/src/cascade.rs`

- **Media rule**: Add `CitationMediaRef` to the check
- **Note rule**: Add `CitationNote` to the check
- **Tag rule**: Add `CitationTag` and `TagTag` to the check
- **Citation rule**: Add `MediaCitation` and `NoteCitation` to the check

### Step 4 — Write comprehensive unit tests

**File**: `crates/delete/src/cascade.rs`

Implement all tests from the test matrix (tests #1–#42). Group them as:

- `mod person_tests` — Tests 1 (seed deletion)
- `mod family_tests` — Tests 2–4 (family cascade scenarios)
- `mod event_tests` — Tests 5–9 (event cascade scenarios)
- `mod place_tests` — Tests 10–12 (place transitive cascade)
- `mod citation_tests` — Tests 13–16 (citation cascade)
- `mod source_tests` — Tests 17–18 (source cascade)
- `mod repository_tests` — Tests 19–20 (repository cascade)
- `mod media_tests` — Tests 21–24 (media cascade)
- `mod note_tests` — Tests 25–27 (note cascade)
- `mod tag_tests` — Tests 28–31 (tag cascade)
- `mod unrelated_tests` — Tests 32–37 (unrelated objects NOT deleted — the core bug)
- `mod property_tests` — Tests 38–42 (invariants)

### Step 5 — Build, test, and lint

```bash
cargo test -p delete           # All unit tests
cargo test --workspace          # Full workspace
cargo clippy --all-targets --all-features -- -D warnings
```

### Step 6 — Update documentation

**File**: `docs/delete-tool.md` (if it exists)
**File**: `AGENTS.md` (if cascade behavior is documented)

---

## Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Frontier algorithm changes the semantics for existing correct cases | Comprehensive test suite (42 tests) validates both expected deletions and expected non-deletions |
| Place orphan rule change (removing outgoing-edge checks) could cause places to NOT be deleted when they should be | The primary deletion trigger for a place is `EventPlace` (incoming). Outgoing edges like `PlaceCitation` are the place's own references, not evidence it's in use. Review with user before implementing. |
| Missing edge types in orphan rules were previously serving as implicit "don't cascade" behavior | Adding `CitationMediaRef`, `CitationNote`, `CitationTag`, `MediaCitation`, `NoteCitation`, `TagTag` makes cascade MORE aggressive. Verify with tests that this is correct. |
| Large graph performance | Frontier approach is strictly more efficient than global scan. No regression expected. |

---

## Decisions

1. **Place orphan rule direction**: **Incoming-only.** A place survives only
   if something else references it (via `EventPlace` or `PlacePlaceRef` where
   the place is the target). Outgoing edges (`PlaceCitation`, `PlaceMediaRef`,
   `PlaceNote`, `PlaceTag`) do NOT keep the place alive.

2. **Family orphan rule**: **Both directions.** Check both Family→Person edges
   (`FamilyFather`, `FamilyMother`, `FamilyChildRef`) AND Person→Family edges
   (`PersonFamily`, `PersonParentFamily`) to verify the family has remaining
   members.

3. **Already-orphaned nodes**: **Keep** the `pre_connectivity` gate as a
   safety net, even though the frontier approach makes it redundant.
