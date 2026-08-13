# Implementation Plan: Add Descriptive Metadata to Deletion Manifest

Source: `docs/research/manifest-descriptive-metadata.md`

## Summary

Add `gramps_id` and `description` fields to `HandleEntry` in the deletion manifest so that saved manifests are self-documenting. Bump manifest format from v2 to v3.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add gramps_id and description fields to HandleEntry` | HandleEntry schema | `crates/delete/src/types.rs` (add fields, serde attrs, update V1 deserializer branches); update all struct literals in `manifest.rs`, `reconcile.rs`, `manifest_contract.rs` | unit: `handle_entry_v3_roundtrip`, `handle_entry_v2_deserialization`, `handle_entry_serialization_skips_empty`, `type_plan_v1_to_v3_migration` |
| 2 | `feat: bump manifest version to 3` | Version migration | `crates/delete/src/types.rs` (move `MANIFEST_VERSION` here, bump to 3); `crates/delete/src/manifest.rs` (update imports, `load_manifest` match 1\|2→3, drop local const); `crates/delete/tests/manifest_contract.rs` (update `contract_v1_fixture_load_migrates_to_v2` → v3) | unit: `v1_manifest_loads_as_v3`, `v2_manifest_loads_as_v3`; update `save_load_roundtrip`, `build_manifest_includes_all_types`, `contract_v1_fixture_load_migrates_to_v2` assertions |
| 3 | `feat: populate manifest entries with descriptions from graph` | Build description population | `crates/delete/src/manifest.rs` (call `describe_node()` in `build_manifest`) | unit: `build_manifest_populates_descriptions`, `build_manifest_empty_node`, `save_load_v3_roundtrip`; integration: new v3 manifest roundtrip test |
| 4 | `docs: update manifest format documentation to v3` | Documentation | `docs/ARCHITECTURE.md` (update v2 → v3, add gramps_id/description to example) | — |

## Per-Step Detail

### Step 1 — HandleEntry fields

- Add `gramps_id: Option<String>` and `description: String` to `HandleEntry` in `crates/delete/src/types.rs`
- Annotate with `#[serde(default, skip_serializing_if = "Option::is_none")]` on `gramps_id` and `#[serde(default, skip_serializing_if = "String::is_empty")]` on `description`
- Update the two `HandleOrEntry::V1(s)` struct literals in `TypePlan`'s custom deserializer (`types.rs` lines 137, 149) to include `gramps_id: None, description: String::new()`
- Update all test struct literals across the crate:
  - `types.rs` tests: `handle_entry_serde_v2`, `type_plan_v2_roundtrip`, `type_plan_v1_deserialization`, `type_plan_mixed_v1_v2_still_works`, `delete_manifest_roundtrip` (3 entries)
  - `manifest.rs` tests: `make_entry` helper, `build_manifest_includes_all_types` (via `make_entry`)
  - `reconcile.rs` tests: `make_manifest` and `make_manifest_multi` helpers (4 literals total)
  - `manifest_contract.rs` test: `contract_v2_serialize_from_rust_matches_fixture_shape` entry
- Add tests:
  - `handle_entry_v3_roundtrip`: serialize/deserialize with gramps_id + description
  - `handle_entry_v2_deserialization`: v2 JSON (no new fields) loads with defaults
  - `handle_entry_serialization_skips_empty`: empty description + None gramps_id omitted from JSON
  - `type_plan_v1_to_v3_migration`: v1 flat strings produce HandleEntry with defaults for new fields

### Step 2 — Version bump

- Move `pub const MANIFEST_VERSION: u32` from `manifest.rs` to `types.rs` (next to `DeleteManifest`), bump from 2 to 3
- Update `manifest.rs` imports: add `MANIFEST_VERSION` to `use crate::types::{...}`, remove local `pub const`
- Update `load_manifest` version match: `3 => Ok`, `1 | 2 => migrate (version = MANIFEST_VERSION, fix deletion_mode)`, `other => Err`
- Undo the `load_manifest_v1` change (still rejects non-1 — unchanged)
- Unit tests to update:
  - `manifest.rs save_load_roundtrip`: expects `loaded.version == 2` → `3`
  - `manifest.rs build_manifest_includes_all_types`: expects `manifest.version == 2` → `3`
- Integration test to update:
  - `manifest_contract.rs contract_v1_fixture_load_migrates_to_v2`: rename to `contract_v1_fixture_load_migrates_to_v3`, assert `version == 3`
- Add tests:
  - `v1_manifest_loads_as_v3`: write v1 fixture, load via `load_manifest`, assert version 3 and auto-migrated fields
  - `v2_manifest_loads_as_v3`: write v2 fixture, load via `load_manifest`, assert version 3 and entries preserved

### Step 3 — Populate descriptions

- In `build_manifest` (`manifest.rs`), replace the `|h| HandleEntry { handle: h.clone(), status: HandleStatus::Pending }` closure with one that calls `describe_node(graph, h)`:

  ```rust
  .map(|h| {
      let (desc, gramps_id) = describe_node(graph, h);
      HandleEntry {
          handle: h.clone(),
          status: HandleStatus::Pending,
          gramps_id,
          description: desc,
      }
  })
  ```

- Add `use crate::review::describe_node;` to `manifest.rs`
- Add tests:
  - `build_manifest_populates_descriptions`: build graph with a named person (gramps_id, birth event), run `build_manifest`, assert gramps_id + description are non-empty
  - `build_manifest_empty_node`: add node with no name/date, assert fallback description + no gramps_id
  - `save_load_v3_roundtrip`: construct v3 manifest with all fields, save/load, assert equality
- Add integration test: new file in `crates/delete/tests/` (e.g. `manifest_descriptions.rs`) that builds a graph with a person (name, gramps_id, birth event), runs `build_manifest`, serializes to JSON, and asserts `gramps_id` and `description` fields are present in the output

### Step 4 — Documentation

- Update `docs/ARCHITECTURE.md`:
  - "JSON manifest v2" → "JSON manifest v3" in the ASCII diagram (line ~131)
  - "Deletion manifests use **v2 format**" → "**v3 format**" (line ~849)
  - Update the example JSON to include `gramps_id` and `description` fields
  - Update the version note: "v2 manifests are auto-migrated to v3 on load"
  - Update the Status table as needed (no new statuses, same three)
