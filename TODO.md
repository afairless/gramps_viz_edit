# Implementation Plan: Place Deletion Post-Process

Source: `docs/research/place-deletion-postprocess.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: harden cascade to distinguish already-orphaned places` | Cascade hardening — `pre_place_in_use_count` helper, Phase-A map, already-orphaned skip | `crates/delete/src/cascade.rs` | Unit: `pre_place_in_use_counts_incoming_only`, `already_orphaned_place_with_outgoing_refs_not_deleted`, `newly_orphaned_place_still_deleted` |
| 2 | `feat: add clean_places_xml wrapper for streaming place removal` | Place cleaning wrapper in `clean.rs` | `crates/cli/src/commands/clean.rs` | Unit: `remove_single_place`, `remove_self_closing_place`, `keep_unrelated_place`, `place_with_body_and_children`, `place_hlink_in_event_not_removed`, `multiple_places_mixed`, `namespace_prefixed_place`, `places_section_preserved` |
| 3 | `feat: add derive_deleted_4_path helper and place-cleaning pipeline step` | Path helper + place-cleaning step 17 in `delete.rs` | `crates/cli/src/commands/delete.rs` | Unit: `derive_deleted_4_path_basic`, `derive_deleted_4_path_gz`, `derive_deleted_4_path_no_ext`, `derive_deleted_4_path_already_cleaned` |
| 4 | `test: add e2e tests for place cleaning in delete pipeline` | E2E integration tests for place cleaning | `crates/cli/tests/e2e_delete.rs` | Integration: `e2e_delete_with_place_clean`, `e2e_delete_no_pending_places_noop`, `e2e_delete_deleted_2_and_3_unchanged`, `e2e_delete_manifest_places_marked_deleted` |
| 5 | `docs: update delete-tool.md and delete.rs module docstring for place cleaning` | Documentation updates | `docs/delete-tool.md`, `crates/cli/src/commands/delete.rs` | — |
| 6 | `chore: run full workspace test and verify all outputs` | Full workspace test run | — | Workspace: `cargo test --workspace` |

## Details

### Step 1 — Cascade hardening (`crates/delete/src/cascade.rs`)

- Add `pre_place_in_use_count(graph, handle)` helper that counts only incoming keep-alive edges: `EventPlace` (target) and `PlacePlaceRef` (target). Outgoing `PlaceCitation`, `PlaceMediaRef`, `PlaceNote`, `PlaceTag` are excluded.
- In `cascade()` Phase A, populate `pre_place_in_use: HashMap<Handle, usize>` alongside `pre_connectivity`.
- In the fixed-point loop, after the existing `pre_count == 0` skip, add a place-specific skip: if the node is a `Place` with `pre_place_in_use == 0`, skip it (already orphaned before the operation).
- Ensure the existing `type_specific_orphan_rule` for `Node::Place` is unchanged.

### Step 2 — Place cleaning wrapper (`crates/cli/src/commands/clean.rs`)

- Add `clean_places_xml(input, output, place_handles)` calling `clean_elements_xml(input, output, "place", place_handles)`.
- Mirrors the existing `clean_events_xml` / `clean_notes_xml` pattern.
- Add unit tests covering: single place removal, self-closing place, unrelated place preserved, place with body/children, `<place hlink>` in event not removed, mixed multiple places, namespace-prefixed place, `<places>` section preserved when empty.

### Step 3 — Pipeline integration (`crates/cli/src/commands/delete.rs`)

- Add `derive_deleted_4_path(input_path)` helper alongside `derive_deleted_3_path`.
- After the note-cleaning step (current step 16), add a new place-cleaning step (step 17):
  - Collect pending place handles from `manifest.plan["places"]`.
  - If non-empty, pick input `-deleted_3` → `-deleted_2` → `-cleaned` (fallback chain).
  - Call `clean_places_xml`, log results, mark places `Deleted` in manifest, re-save.
- Renumber the existing temp directory cleanup from step 17 to step 18.
- Update the module-level docstring to reflect the full pipeline: events → notes → places.

### Step 4 — E2E tests (`crates/cli/tests/e2e_delete.rs`)

- Add a fixture with people, events, notes, and a place referenced by an event.
- `e2e_delete_with_place_clean`: Full pipeline; `-deleted_4.gramps` has the orphaned place removed.
- `e2e_delete_no_pending_places_noop`: With no pending places, `-deleted_4.gramps` is not written.
- `e2e_delete_deleted_2_and_3_unchanged`: Intermediate files still contain the place.
- `e2e_delete_manifest_places_marked_deleted`: Manifest marks places `Deleted` after cleaning.

### Step 5 — Documentation updates

- `docs/delete-tool.md`: Fix the stale place orphan-rule table (line ~159) — remove `PlaceCitation`, `PlaceMediaRef`, `PlaceNote`, `PlaceTag` from keep-alive list; keep only incoming `EventPlace` and `PlacePlaceRef`. Document the `-deleted_4.gramps` output file.
- `crates/cli/src/commands/delete.rs`: Update module docstring to show events → notes → places pipeline.

### Step 6 — Full workspace verification

- Run `cargo test --workspace` and verify all tests pass.
- Verify `cargo clippy --all-targets --all-features -- -D warnings` passes.
