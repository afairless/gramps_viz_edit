# Place Deletion Post-Process

> Status: Plan
> Created: 2026-08-18
> Target: `gramps-gen delete` — remove newly-orphaned places from output as `-deleted_4.gramps`

## 1. Problem Statement

The `gramps-gen delete` command today:

1. Runs the Rust cascade engine to identify orphaned people, families,
   events, places, citations, sources, repositories, media, notes, and tags.
2. Delegates **people-only** deletion to the Python Gramps backend.
3. Reconciles the manifest against what Gramps actually deleted.
4. Runs an **event cleaning** post-process → `<stem>-deleted_2.gramps`.
5. Runs a **note cleaning** post-process → `<stem>-deleted_3.gramps`.

Places that the cascade identifies as newly orphaned already appear in the
manifest under `"places"` with `status: "pending"` (the Python backend never
touches places, so Gramps keeps them). But there is **no step** that removes
these places from the XML output. They survive into `-deleted_2.gramps` and
`-deleted_3.gramps`.

The goal is to remove newly-orphaned places as a final automatic
post-processing step, producing `<stem>-deleted_4.gramps`, while leaving
`-deleted_2.gramps` and `-deleted_3.gramps` unchanged.

### 1.1 Important finding — the cascade already identifies places

`crates/delete/src/cascade.rs` already contains a `Node::Place` orphan rule
(incoming-only semantics) with passing unit tests, and
`build_manifest` already emits a `"places"` section in dependency order. This
was verified empirically: a dry-run of a synthetic `.gramps` containing a
place produced this manifest fragment:

```json
"places": {
  "to_delete": [ { "handle": "_pl0001", "status": "pending" } ]
}
```

So the *identification* side is present. The work is:

- **Hardening** the already-orphaned vs. newly-orphaned distinction for
  places (see §2), and
- **Adding the missing removal step** (`clean_places_xml` → `-deleted_4.gramps`).

### 1.2 Strict orphan semantics

Only places that become **newly orphaned** as a result of the deletion
cascade may be removed. Places that were **already orphaned** before the
operation (no incoming keep-alive edges, regardless of their own outgoing
references) must remain untouched. This distinction is the core requirement
and is encoded explicitly in the cascade (see §5.1).

## 2. Current State Analysis

### 2.1 Place liveness rule (already implemented)

A place is *in use* if and only if it has at least one **incoming**
keep-alive edge from a live (non-deleted) node:

- `EventPlace` where the place is the `target` (an event is located there), or
- `PlacePlaceRef` where the place is the `target` (a child place encloses it).

A place's own **outgoing** references (`PlaceCitation`, `PlaceMediaRef`,
`PlaceNote`, `PlaceTag`, and outgoing `PlacePlaceRef` to its parent) do **not**
keep the place alive.

This is already implemented in `type_specific_orphan_rule` for
`Node::Place(_)`.

### 2.2 The pre-existing skip is imprecise for places

The cascade's fixed-point loop skips nodes whose **total** incident edge count
was zero before the operation:

```rust
let pre_count = pre_connectivity.get(&neighbor).copied().unwrap_or(0);
if pre_count == 0 {
    continue;
}
```

`pre_connectivity` counts *all* incident edges. For places this is imprecise:
a place with only outgoing references (e.g. `PlaceNote` to a note) has
`pre_count > 0` yet is semantically *already orphaned* (nothing references it).
Today this does not cause a wrong deletion only because of an incidental
circular-protection effect (the place's outgoing target is itself kept alive
by the place, so the place is never reached for evaluation). That is fragile
and non-obvious.

The plan makes the invariant explicit: the cascade records a place-specific
pre-existing **in-use** count (incoming keep-alive edges only) and skips any
place whose pre-existing in-use count is zero.

## 3. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Ownership of place logic | Keep it in the cascade engine (`cascade.rs`) | Modularity/separation of concerns: the cascade owns *what* to delete; the post-process owns *how* to remove it from XML |
| Already-orphaned vs. newly-orphaned | Explicit per-place pre-existing in-use count; skip places with zero incoming keep-alive edges | Strictly adheres to the requirement; removes reliance on incidental circular protection |
| Integration point | New step in `delete.rs` after note cleaning | Follows the established event/note pattern; produces a 4th output |
| Output file | `<input-stem>-deleted_4.gramps` | Consistent with current `-deleted_2` / `-deleted_3` suffixes |
| Input to place cleaning | Latest available: `-deleted_3` → `-deleted_2` → `-cleaned` | Mirrors the note-cleaning fallback logic |
| Implementation | `clean_places_xml` wrapper over the existing `clean_elements_xml` | Reuses the streaming filter; `clean.rs` remains the single source of truth for element removal |
| Scope of removal | Manifest `"places"` entries with `status: "pending"` only | Only newly-orphaned places (the cascade's output) are removed; no re-cascade |

## 4. Architecture

### 4.1 Pipeline (updated)

```
Parse → Cascade → Review → Manifest → Python backend → Reconcile
    → [Checkpoint: save -cleaned.gramps + manifest to disk]
    → Clean Events  (remove pending events) → <stem>-deleted_2.gramps
    → Clean Notes   (remove pending notes)  → <stem>-deleted_3.gramps
    → Clean Places  (remove pending places) → <stem>-deleted_4.gramps
```

Each cleaning step reads the previous step's output and writes a new file.
The manifest is re-saved after each successful step with the corresponding
type's pending entries marked `Deleted`.

### 4.2 Component Diagram

```
┌──────────────────────────────────────────────────────────────┐
│  cascade.rs (unchanged shape, hardened)                       │
│                                                               │
│  Phase A:                                                     │
│   pre_connectivity  : HashMap<Handle, usize>  // all edges    │
│   pre_place_in_use  : HashMap<Handle, usize>  // NEW          │
│                       incoming EventPlace + PlacePlaceRef     │
│                       edges where the place is the target     │
│                                                               │
│  Fixed-point loop:                                            │
│   skip pre_count == 0              (already isolated)         │
│   skip Place with pre_place_in_use == 0  (NEW: already        │
│        orphaned — has only outgoing refs)                     │
└──────────────────────────┬───────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  delete.rs (orchestrator)                                     │
│                                                               │
│  After note cleaning succeeds:                                │
│  1. Collect pending place handles from manifest               │
│  2. Pick input: -deleted_3 → -deleted_2 → -cleaned           │
│  3. Call clean_places_xml(input, deleted_4_path, handles)     │
│  4. Log result, mark places Deleted in manifest               │
│  5. Re-save manifest                                          │
└──────────────────────────┬───────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  clean.rs (generalized, unchanged core)                       │
│                                                               │
│  pub fn clean_elements_xml(input, output, element_name,       │
│                            handles) -> CleanStats             │
│                                                               │
│  pub fn clean_events_xml(...) → "event"                       │
│  pub fn clean_notes_xml(...)  → "note"                        │
│  pub fn clean_places_xml(...) → "place"   // NEW              │
└──────────────────────────────────────────────────────────────┘
```

## 5. Implementation Plan

### 5.1 Cascade hardening — `crates/delete/src/cascade.rs`

#### 5.1.1 Record pre-existing place in-use count (Phase A)

Extract the counting logic into a small, directly testable helper and have
`cascade()` populate the map from it:

```rust
/// Count a place's pre-existing incoming keep-alive edges.
///
/// Incoming keep-alive edges are `EventPlace` and `PlacePlaceRef` where the
/// place is the `target`. Outgoing edges (`PlaceCitation`, `PlaceMediaRef`,
/// `PlaceNote`, `PlaceTag`, and outgoing `PlacePlaceRef`) do not count.
fn pre_place_in_use_count(graph: &Graph, handle: &Handle) -> usize {
    graph
        .edges_incident_to(handle)
        .iter()
        .filter(|e| match e {
            Edge::EventPlace { target, .. } => target == handle,
            Edge::PlacePlaceRef { target, .. } => target == handle,
            _ => false,
        })
        .count()
}
```

In `cascade()`, alongside `pre_connectivity`, compute:

```rust
let mut pre_place_in_use: HashMap<Handle, usize> = HashMap::new();
for (handle, node) in graph.iter_nodes() {
    if matches!(node, Node::Place(_)) {
        pre_place_in_use.insert(handle.clone(), pre_place_in_use_count(graph, handle));
    }
}
```

The helper is directly unit-testable (see §5.1.3), pinning the counting
semantics without exposing `pre_place_in_use` beyond `cascade()`.

#### 5.1.2 Skip already-orphaned places in the fixed-point loop

After the existing `pre_count == 0` skip, add a place-specific skip:

```rust
// NEW: a place with zero incoming keep-alive edges before the operation
// was already orphaned — never flag it, even if it has outgoing refs.
if let Some(node) = graph.get_node(&neighbor) {
    if matches!(node, Node::Place(_))
        && pre_place_in_use.get(&neighbor).copied().unwrap_or(0) == 0
    {
        continue;
    }
}
```

The existing `Node::Place` orphan rule in `type_specific_orphan_rule` is
unchanged. This change makes the already-orphaned distinction explicit and
correct-by-construction rather than incidental.

#### 5.1.3 Cascade tests

| Test | What it verifies |
|---|---|
| `pre_place_in_use_counts_incoming_only` | The `pre_place_in_use_count` helper counts `EventPlace` (target) and `PlacePlaceRef` (target); ignores outgoing `PlaceNote`/`PlaceCitation`/`PlaceMediaRef`/`PlaceTag` |
| `already_orphaned_place_with_outgoing_refs_not_deleted` | A place with only an outgoing `PlaceNote` (no incoming keep-alive edge) is not in `to_delete`, while a neighboring newly-orphaned place in the same graph is |
| `newly_orphaned_place_still_deleted` | Regression: a place referenced only by a deleted event is still cascaded (existing `transitive_cascade` already covers this) |

> Note: because of the current circular-protection behavior these hardening
> tests document an invariant rather than guard against a presently
> reachable wrong result. They ensure the invariant stays true if the
> surrounding rules are later refactored.

### 5.2 Add `clean_places_xml` — `crates/cli/src/commands/clean.rs`

Add a thin wrapper mirroring `clean_notes_xml`:

```rust
/// Remove places matching the given handles from a Gramps XML file.
///
/// Convenience wrapper around [`clean_elements_xml`] with `element_name = "place"`.
pub fn clean_places_xml(
    input: &Path,
    output: &Path,
    place_handles: &HashSet<String>,
) -> Result<CleanStats, CleanError> {
    clean_elements_xml(input, output, "place", place_handles)
}
```

No core changes to `clean_elements_xml` are required. It already:

- Matches the element by stripped local name (`place`),
- Matches the `handle` attribute via `read_handle_attr`,
- Skips the whole element subtree (body + children),
- Handles gzip input transparently and writes plain `.gramps`.

### 5.3 Add place-cleaning step — `crates/cli/src/commands/delete.rs`

#### 5.3.1 Path helper

Add `derive_deleted_4_path` alongside `derive_deleted_3_path`:

```rust
/// Derive the `-deleted_4.gramps` path from the input file path.
pub(crate) fn derive_deleted_4_path(input_path: &std::path::Path) -> std::path::PathBuf {
    let stem = strip_gramps_extensions(input_path);
    std::path::PathBuf::from(format!("{}-deleted_4.gramps", stem))
}
```

#### 5.3.2 New step (after note cleaning, before the final summary)

```rust
// 17. Clean pending places from the output XML
let pending_places: HashSet<String> = manifest
    .plan
    .get("places")
    .map(|p: &TypePlan| {
        p.to_delete
            .iter()
            .filter(|e| e.status == HandleStatus::Pending)
            .map(|e| e.handle.clone())
            .collect()
    })
    .unwrap_or_default();

if !pending_places.is_empty() {
    let deleted_2_path = derive_no_events_path(input_path);
    let deleted_3_path = derive_deleted_3_path(input_path);
    let deleted_4_path = derive_deleted_4_path(input_path);

    // Use the latest available cleaned output as input.
    let place_input = if deleted_3_path.exists() {
        deleted_3_path
    } else if deleted_2_path.exists() {
        deleted_2_path
    } else {
        output_path.clone()
    };

    log::info!(
        "Cleaning {} pending places from {} -> {}",
        pending_places.len(),
        place_input.display(),
        deleted_4_path.display()
    );
    let stats = crate::commands::clean::clean_places_xml(
        &place_input,
        &deleted_4_path,
        &pending_places,
    )?;
    log::info!(
        "Place cleaning complete: {} removed, {} not found in XML",
        stats.elements_removed,
        stats.elements_not_found,
    );
    // Mark these places as Deleted in the manifest
    if let Some(places_plan) = manifest.plan.get_mut("places") {
        for entry in &mut places_plan.to_delete {
            if entry.status == HandleStatus::Pending {
                entry.status = HandleStatus::Deleted;
            }
        }
    }
    // Persist the updated manifest to disk
    save_manifest(&manifest, &manifest_path)?;
} else {
    log::info!("No pending places to clean.");
}
```

> Note: the existing final `// 17. Clean up temp directory` step in
> `delete.rs` must be renumbered to `// 18.` to avoid a numbering collision
> with this new step.

#### 5.3.3 Output file naming

| File | Content |
|---|---|
| `<stem>-cleaned.gramps` | Gramps DB export (people deleted, other types survive) |
| `<stem>-deleted_2.gramps` | Events removed (unchanged) |
| `<stem>-deleted_3.gramps` | Events AND notes removed (unchanged) |
| `<stem>-deleted_4.gramps` | Events AND notes AND places removed (new) |

All cleaned outputs are plain `.gramps` regardless of `.gramps.gz` input.
When the input is `.gramps.gz`, the `-cleaned.gramps` fallback input
(`output_path`) is itself gzip-compressed; `clean_elements_xml` decodes it
transparently, so the §5.3.2 fallback chain needs no special handling.

### 5.4 Documentation updates

| File | Change |
|---|---|
| `docs/delete-tool.md` | Fix the stale place orphan-rule description (line ~159) — it currently lists outgoing edges (`PlaceCitation`, `PlaceMediaRef`, `PlaceNote`, `PlaceTag`) as keep-alive; replace with the incoming-only rule and mention the `-deleted_4.gramps` output |
| `crates/cli/src/commands/delete.rs` | Update the module-level docstring (lines ~1–11), whose pipeline summary stops at "clean orphaned events from output XML"; extend it to events → notes → places |
| `docs/research/place-deletion-postprocess.md` | This document |

## 6. Error Handling

| Scenario | Behavior |
|---|---|
| No pending places in manifest | Skip place cleaning entirely, log info |
| `-deleted_3`/`-deleted_2` missing | Fall back to the latest available input (`-cleaned.gramps`); if that is also missing, `CleanError::Io` propagates |
| Place handle in deletion set but not found in XML | Increment `elements_not_found`, log, continue |
| Malformed XML | `CleanError::XmlParse` — propagate to user |
| Place cleaning fails | `-deleted_4.gramps` not written; `-deleted_2`, `-deleted_3`, and the manifest remain on disk; user can re-run with `--load-manifest` |
| Place cleaning succeeds but manifest re-save fails | `-deleted_4.gramps` exists with places removed, but manifest still shows them `Pending`; re-running with `--load-manifest` re-cleans (no-op) and reconciles |

## 7. Testing Strategy

### 7.1 Unit tests — `crates/delete/src/cascade.rs`

See §5.1.3.

### 7.2 Unit tests — `crates/cli/src/commands/clean.rs`

| Test | What it verifies |
|---|---|
| `remove_single_place` | A `<place handle="...">` with matching handle is removed |
| `remove_self_closing_place` | `<place handle="..."/>` self-closing form |
| `keep_unrelated_place` | Places not in the deletion set pass through |
| `place_with_body_and_children` | `<place>` with `<ptitle>`, `<coord>`, nested content is skipped as a whole |
| `place_hlink_in_event_not_removed` | An event's `<place hlink="..."/>` reference is **not** treated as a place definition (it has no `handle` attribute) and passes through |
| `multiple_places_mixed` | Some removed, some kept, stats correct |
| `namespace_prefixed_place` | `<ns:place ns:handle="...">` detected |
| `places_section_preserved` | `<places>` section tag remains even when all its places are removed |

### 7.3 Integration tests — `crates/cli/tests/e2e_delete.rs`

| Test | What it verifies |
|---|---|
| `e2e_delete_with_place_clean` | Full pipeline deletes people → events → notes → places; `-deleted_4.gramps` has the orphaned place removed |
| `e2e_delete_no_pending_places_noop` | With no pending places, `-deleted_4.gramps` is not written (and `-deleted_3` still is, if notes existed) |
| `e2e_delete_deleted_2_and_3_unchanged` | `-deleted_2.gramps` and `-deleted_3.gramps` still contain the place; only `-deleted_4.gramps` drops it |
| `e2e_delete_manifest_places_marked_deleted` | After place cleaning, manifest on disk marks pending places `Deleted` |

> Fixture requirement: the e2e fixture must produce pending **events** and
> **notes** (not just a place) so that `-deleted_2.gramps` and
> `-deleted_3.gramps` are actually written. Those intermediate files are only
> produced when their respective cleaning steps have pending handles; a
> people+place-only fixture would write neither, making the
> `e2e_delete_deleted_2_and_3_unchanged` assertion vacuous.

## 8. Edge Cases & Considerations

### 8.1 `<place>` name ambiguity

The tag name `place` appears in two contexts:

- **Definition**: `<places><place handle="...">…</place></places>`
- **Event reference**: `<event>…<place hlink="..."/>…</event>`

`clean_elements_xml` only removes an element when its `handle` attribute
matches (`read_handle_attr`). The event reference has an `hlink`, not a
`handle`, so it is correctly passed through untouched. The place-cleaning step
must not and will not strip event place references.

### 8.2 Place hierarchy (`PlacePlaceRef`)

A place may enclose another via `<placeref hlink="..."/>`. The cascade handles
the hierarchy transitively:

- A **parent** place is kept alive by any live child referencing it
  (incoming `PlacePlaceRef`).
- A **child** place's liveness comes only from events or its own children.

Consequence: when a place is finally removed, no surviving place references it
and no surviving event references it, so removing the `<place>` element creates
no dangling `hlink` in surviving objects. No reference clean-up is needed.

### 8.3 Already-orphaned places

Places with zero incoming keep-alive edges before the operation are excluded
by the new `pre_place_in_use` skip (§5.1) and therefore never enter the
manifest. The post-process only ever removes manifest-listed pending places,
so already-orphaned places are doubly protected from removal.

### 8.4 Schema version compatibility

The streaming filter is schema-agnostic (matches only the `place` element name
and `handle` attribute). Both Gramps 5.1 and 5.2 use the same structure.

### 8.5 Deterministic output

Elements are removed in place; everything else passes through in original
order. Output is deterministic.

### 8.6 Large files

Same streaming approach as event/note cleaning — O(depth of XML nesting)
memory, single pass.

## 9. Files Changed

| File | Change |
|---|---|
| `crates/delete/src/cascade.rs` | Add `pre_place_in_use_count` helper + Phase-A map + place-specific already-orphaned skip; add hardening tests |
| `crates/cli/src/commands/clean.rs` | Add `clean_places_xml` wrapper + place-specific unit tests |
| `crates/cli/src/commands/delete.rs` | Add `derive_deleted_4_path`; add place-cleaning step 17; add path-derivation tests; update module docstring |
| `crates/cli/tests/e2e_delete.rs` | Add place-cleaning integration tests |
| `docs/delete-tool.md` | Correct stale place orphan-rule description; document `-deleted_4.gramps` |

## 10. Non-Goals

- **Re-cascading after event/note/place removal**: Only places the cascade
  already identified are removed. No new orphan detection after the
  post-process steps.
- **Removing `<place hlink>` / `<placeref>` references**: Dangling hlinks in
  surviving objects cannot occur for places (see §8.1–§8.2) and, per the
  event/note precedent, Gramps skips dangling hlinks on import anyway.
- **Deleting places via the Python Gramps backend**: The backend remains
  people-only. Places are removed by the streaming XML filter.
- **New CLI subcommand**: Not exposed as a separate `gramps-gen clean-places`
  command.
- **Single-pass combined cleaning**: Events, notes, and places remain separate
  passes producing separate outputs, preserving the step-by-step contract and
  intermediate inspection artifacts.
- **Renaming existing outputs**: `-deleted_2.gramps` and `-deleted_3.gramps`
  keep their current names; only `-deleted_4.gramps` is added.

## 11. Implementation Order

| Step | Description | Tests |
|---|---|---|
| 1 | Add `pre_place_in_use_count` helper + Phase-A map + place-specific skip in `cascade.rs` | New cascade unit tests; `cargo test -p delete` passes |
| 2 | Add `clean_places_xml` wrapper in `clean.rs` | New place unit tests in `clean.rs`; `cargo test -p cli` passes |
| 3 | Add `derive_deleted_4_path` helper in `delete.rs` | Unit test for path derivation |
| 4 | Add place-cleaning step 17 in `delete.rs` pipeline | Compiles; existing event/note e2e tests still pass |
| 5 | Mark places `Deleted` in manifest + re-save | Integration test asserts manifest update |
| 6 | Add e2e place-cleaning integration tests | Verify `-deleted_4.gramps` exists with place removed; `-deleted_2`/`-deleted_3` unchanged |
| 7 | Update `docs/delete-tool.md` and the `delete.rs` module docstring | Documentation in sync |
| 8 | Run full `cargo test --workspace` and a manual end-to-end run with real data | All outputs valid |
