# Implementation Plan: Fix Delete Cascade v2

Source: [`docs/research/fix-delete-cascade-v2.md`](docs/research/fix-delete-cascade-v2.md)

Two bug classes remain in the deletion cascade engine:

1. **False negatives** (items that should be deleted are kept) — root cause: the `evaluated` set prevents re-evaluation when a node's connectivity state changes mid-cascade.
2. **False positives** (items incorrectly flagged for deletion) — to be diagnosed via comprehensive tests.

## Strategy

Write the full test matrix (49 new tests + helpers) first against the current code to establish a baseline, then apply algorithmic fixes and verify previously-failing tests turn green.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `test: add test helper functions for cascade tests` | Test helpers | `crates/delete/src/cascade.rs` (`#[cfg(test)] mod tests`) | — (test-only scaffolding) |
| 2 | `test: add Category A cascade tests — isolated direct cascades` | Direct cascade A1–A18 | `crates/delete/src/cascade.rs` | Unit (18 tests) |
| 3 | `test: add Category B–D cascade tests — shared refs kept alive` | Direct/indirect kept-alive B1–B12, C1–C2, D1 | `crates/delete/src/cascade.rs` | Unit (15 tests) |
| 4 | `test: add Category E–G cascade tests — unrelated and regression` | Unrelated E1–E6, distant-relative F1–F3, evaluated-set G1–G3 | `crates/delete/src/cascade.rs` | Unit (12 tests) |
| 5 | `test: add Category H property invariants for cascade` | Property invariants H6–H9 (+ preserve H1–H5) | `crates/delete/src/cascade.rs` | Unit, property (4 new + 5 existing) |
| 6 | `fix: remove evaluated set and sort frontier for deterministic cascade` | Core bug fix | `crates/delete/src/cascade.rs` | Unit (all 54 pass) |
| 7 | `fix: add non-seed person guard to type_specific_orphan_rule` | Person guard | `crates/delete/src/cascade.rs` | Unit (all 54 pass) |
| 8 | `fix: diagnose and resolve remaining cascade failures` | Iterative fix loop | `crates/delete/src/cascade.rs` | Unit (all 54 pass) |
| 9 | `chore: run full workspace tests and lint after cascade fixes` | Final verification | — (no source changes) | Integration, lint |

## Step details

### Step 1 — Test helper functions

Write all graph-construction helper functions in `crates/delete/src/cascade.rs` `#[cfg(test)] mod tests`. Each helper builds a minimal graph fragment and returns handles for assertions:

```
make_person, make_family_with_parents, make_family_with_parents_and_child,
make_event, make_event_with_place, make_place, make_place_with_place_ref,
citation_from_person, citation_from_event, citation_from_family, citation_from_place,
source_from_citation, repository_from_source,
media_from_person, media_from_citation, media_from_source,
note_from_person, note_from_citation,
tag_from_person, tag_from_event, tag_tag
```

No tests yet — helpers are used by Steps 2–5.

### Step 2 — Category A: isolated direct cascades → DELETED

18 tests (A1–A18): seed person → family, event, citation, source, repository, media, note, tag, place — all single-path cascades where every referent is deleted. All should pass against current code.

### Step 3 — Categories B–D: shared references kept alive → KEPT

15 tests: B1–B12 (directly associated but kept by a second referent), C1–C2 (indirect cascade through events/places to citations, isolated → DELETED), D1 (indirect cascade kept alive). All should pass against current code.

### Step 4 — Categories E–G: unrelated, distant-relative, regression

12 tests: E1–E6 (unrelated subgraphs never touched → KEPT), F1–F3 (distant-relative shared items kept alive), G1–G3 (evaluated-set false-negative scenarios → currently fail due to `evaluated` set bug, expected to fail).

### Step 5 — Category H: property invariants

4 new property tests + preserve 5 existing (H1–H5):

| Test | Description |
|---|---|
| H6 `non_seed_people_never_deleted` | People not in seeds are never in `to_delete` |
| H7 `unrelated_subgraph_untouched` | Nodes with no path to seeds never deleted |
| H8 `deterministic_output` | Same graph + seeds = same `to_delete` set |
| H9 `monotonic_growth` | `to_delete` only grows; nothing removed once added |

After writing H6–H9, run `cargo test -p delete` to record the baseline of passing/failing tests before the algorithmic fix. Expected: H6 may fail (person guard not yet in place), H8 may fail (nondeterministic ordering), G1–G3 fail (evaluated-set bug).

### Step 6 — Remove `evaluated` set + sort frontier

Remove the `evaluated: HashSet<Handle>` variable and both references to it in the frontier-processing loop. The `to_delete.contains(&neighbor)` check alone prevents infinite loops.

Add `frontier.sort_unstable()` at the top of each while-loop iteration for deterministic output (needed for H8).

**Expected effect**: G1–G3 turn green. H8 turns green. Other tests unaffected.

### Step 7 — Add non-seed person guard

Add an explicit guard at the top of `type_specific_orphan_rule`:

```rust
if matches!(node, Node::Person(_)) {
    return false;
}
```

Remove the `Node::Person(_) => false` match arm (now dead code) to keep the pattern exhaustive.

**Expected effect**: H6 turns green.

### Step 8 — Diagnose and fix remaining failures

Run `cargo test -p delete`. If any test still fails, categorize the failure and fix iteratively:

- **E1–E6 failures**: Cascade reaching unrelated nodes → graph construction or orphan-rule direction bug in `type_specific_orphan_rule`
- **F1–F3 failures**: Distant-relative items being deleted → family cascade propagation too aggressive
- **H8/H9 failures**: Residual ordering or monotonicity issues

Fix → run tests → repeat until all 54 pass.

### Step 9 — Final verification

```bash
cargo test -p delete
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

All tests pass, zero warnings/errors.
