# Plan: Add Descriptive Metadata to Deletion Manifest

## Problem

When the delete tool's interactive review asks the user whether to delete
people, families, events, etc., it displays rich descriptive information:

```
  • p0001 [I0001] — John Smith (1800-1875)
  • e0001 [E0001] — Birth event (1800) — John Smith
  • pl0001 [P0001] — New York
```

But this information is thrown away when the manifest is built. The manifest
stores only bare handles:

```json
{
  "to_delete": [
    {"handle": "p0001", "status": "pending"},
    {"handle": "e0001", "status": "pending"}
  ]
}
```

This makes manifests opaque — a user inspecting a saved manifest has no idea
*who* or *what* is being deleted without loading the original `.gramps` file
back into a reader.

## Goal

Add `gramps_id` and `description` to every `HandleEntry` in the manifest so
that saved manifests are self-documenting.

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Version strategy | Bump to **v3** | Follows existing v1→v2 migration pattern. Clear separation. |
| Fields to add | `gramps_id` + `description` | The two fields returned by `describe_node()`. Compact, sufficient. |
| Kept entries | Same fields | Consistent data model; future-proofs kept-entry tracking. |
| Generation site | `build_manifest()` directly | Already has `&Graph`; no coupling to review loop. |
| Python backend impact | None | `_extract_handle()` only reads `entry.get("handle")`. |

## Proposed v3 Manifest Schema

```jsonc
{
  "version": 3,
  "source_file": "my-family.gramps",
  "selections_file": "selections.json",
  "created_at": "2025-01-15T10:30:00Z",
  "seed_people": ["p0001"],
  "deletion_mode": "people_only",
  "plan": {
    "people": {
      "to_delete": [
        {
          "handle": "p0001",
          "status": "deleted",
          "gramps_id": "I0001",
          "description": "John Smith (1800-1875)"
        }
      ],
      "kept": [
        {
          "handle": "p0002",
          "status": "kept",
          "gramps_id": "I0002",
          "description": "Jane Doe (b. 1810)"
        }
      ]
    },
    "events": {
      "to_delete": [
        {
          "handle": "e0001",
          "status": "pending",
          "gramps_id": "E0001",
          "description": "Birth event (1800) — John Smith"
        }
      ],
      "kept": []
    }
  }
}
```

## Backward Compatibility / Migration

| Version | Input format | Migration to v3 |
|---|---|---|
| v1 | `"to_delete": ["h1"]` | `HandleEntry { handle: "h1", status: Pending, gramps_id: None, description: "" }` |
| v2 | `"to_delete": [{"handle": "h1", "status": "pending"}]` | Add `gramps_id: None, description: ""` via `#[serde(default)]` |
| v3 | Full fields | No migration needed |

The v1 auto-migration path must be preserved: the `HandleOrEntry` untagged
enum in `TypePlan`'s custom deserializer must be extended to handle v1→v3
(flat strings become entries with default gramps_id/description), then v2
entries get optional fields defaulted by serde.

## Changes Required

### 1. `crates/delete/src/types.rs` — Add fields to `HandleEntry`

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandleEntry {
    pub handle: Handle,
    pub status: HandleStatus,
    /// Gramps object ID (e.g. "I0001", "E0001"), if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gramps_id: Option<String>,
    /// Human-readable description (name, dates, event type, etc.).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}
```

Notes:

- `#[serde(default)]` on new fields ensures v2 manifests deserialize without error.
- `skip_serializing_if` keeps output compact: empty strings and `None` are omitted.
- Defaults for v1→v3 migration: `gramps_id = None`, `description = ""`.

Update `MANIFEST_VERSION` (moved to `types.rs` per step 4) to `3`.

### 2. `crates/delete/src/manifest.rs` — Populate descriptions in `build_manifest`

Currently `build_manifest` receives `&Graph` and iterates handles by type.
It must now call `describe_node()` for every handle to populate the new fields:

```rust
use crate::review::describe_node;

// Inside build_manifest, when building HandleEntry:
let (desc, gramps_id) = describe_node(graph, handle);
HandleEntry {
    handle: handle.clone(),
    status: HandleStatus::Pending,
    gramps_id,
    description: desc,
}
```

`describe_node` is already a public function in `review.rs`. `build_manifest`
already has a `&Graph` parameter, so no signature changes are needed.

### 3. `crates/delete/src/manifest.rs` — Update `load_manifest`

The v1→v3 migration path in `load_manifest`/`TypePlan` deserializer:

- v1 flat strings → `HandleEntry { handle, status: Pending, gramps_id: None, description: "" }`
  (this is already handled by the `HandleOrEntry::V1` branch; defaults fill gramps_id/description)
- v2 objects → serde `#[serde(default)]` fills missing fields
- v3 objects → deserializes natively

No code changes needed for the entry conversion — the existing v1 path in
`TypePlan`'s custom deserializer already constructs `HandleEntry` structs,
and `#[serde(default)]` handles missing fields. The only change in
`load_manifest` is the version check: accept version 3 natively, auto-upgrade
versions 1 and 2 to 3, and reject everything else:

```rust
match manifest.version {
    3 => Ok(manifest),
    1 | 2 => {
        // Auto-migrate to v3; entries are already converted by TypePlan's
        // custom deserializer (v1 flat strings → HandleEntry with defaults).
        manifest.version = MANIFEST_VERSION;
        if manifest.deletion_mode.is_empty() {
            manifest.deletion_mode = "people_only".to_string();
        }
        Ok(manifest)
    }
    other => Err(ManifestError::UnsupportedVersion(other)),
}
```

### 4. `crates/delete/src/types.rs` — Add `MANIFEST_VERSION` constant

Move `MANIFEST_VERSION` from `manifest.rs` (line 83) to `types.rs`, next to
the `DeleteManifest` definition it documents. `manifest.rs` updates its
`use crate::types::{...}` import to include `MANIFEST_VERSION` and drops its
local `pub const` — existing callers (`build_manifest`, `load_manifest`)
keep working unchanged.

### 5. Tests to add/update

| File | Test | What it verifies |
|---|---|---|
| `types.rs` | `handle_entry_v3_roundtrip` | New fields serialize/deserialize correctly |
| `types.rs` | `handle_entry_v2_deserialization` | v2 JSON (no gramps_id/description) loads with defaults |
| `types.rs` | `handle_entry_serialization_skips_empty` | Empty description and None gramps_id are omitted from JSON |
| `types.rs` | `type_plan_v1_to_v3_migration` | v1 flat strings become v3 entries with defaults |
| `manifest.rs` | `build_manifest_populates_descriptions` | Descriptions and gramps_ids are non-empty for a person with data |
| `manifest.rs` | `build_manifest_empty_node` | Nodes with no name/date still produce a fallback description |
| `manifest.rs` | `v2_manifest_loads_as_v3` | A v2 manifest file loads and is upgraded to v3 |
| `manifest.rs` | `v1_manifest_loads_as_v3` | A v1 manifest file loads and is upgraded to v3 |
| `manifest.rs` | `save_load_v3_roundtrip` | Full v3 manifest roundtrip with all fields |

**Update existing tests that assume v2.** The version bump breaks several
current assertions:

- `manifest.rs` `save_load_roundtrip` — constructs a v2 manifest and asserts
  `loaded.version == 2`; after the bump, `load_manifest` upgrades it to 3.
- `manifest.rs` `build_manifest_includes_all_types` — asserts
  `manifest.version == 2`; after the bump, `build_manifest` emits 3.
- `types.rs` `delete_manifest_roundtrip` — constructs `version: 2`
  explicitly; fine as-is since it roundtrips via serde directly, but consider
  updating to 3 for consistency.

### 6. `crates/delete/tests/` — Integration tests

Add an integration test that:

1. Builds a Graph with a person (name, gramps_id, birth event)
2. Runs `build_manifest`
3. Serializes to JSON
4. Asserts that `gramps_id` and `description` fields are present in the output

## Files Changed

| File | Scope |
|---|---|
| `crates/delete/src/types.rs` | Add gramps_id, description to HandleEntry; bump version |
| `crates/delete/src/manifest.rs` | Populate fields in build_manifest; update version constant |
| `crates/cli/src/commands/delete.rs` | No changes needed (manifest is built/saved/reconciled via existing API) |
| `scripts/delete_backend.py` | No changes needed (only reads `handle` field) |

## Implementation Order

1. Add `gramps_id` and `description` fields to `HandleEntry` in `types.rs`
2. Add `#[serde(default)]` and `skip_serializing_if` annotations
3. Add tests for v1/v2→v3 deserialization in `types.rs`
4. Move `MANIFEST_VERSION` to `types.rs` and bump it to 3
5. Update `build_manifest` to call `describe_node()` for each handle
6. Add tests for populated descriptions in `manifest.rs`
7. Add integration test for v3 roundtrip
8. Run full test suite: `cargo test -p delete -- --nocapture`
9. Verify existing manifests still load (manual test with a v2 manifest)

## Out of Scope

- **Populating kept entries from review.** Currently the review loop
  (`review.rs`) returns only confirmed-to-delete handles; skipped/kept
  handles are not recorded anywhere. Adding kept entries to the manifest
  is a separate feature. The `kept` field on `HandleEntry` with descriptions
  is forward-looking — when kept-entry tracking is implemented, descriptions
  will be available.

- **Re-populating descriptions on re-load.** When a manifest is loaded and
  re-saved (e.g., after reconciliation), descriptions are preserved from
  the original load. If a handle's description changes in the source data
  between runs, the manifest won't reflect it — this is acceptable since
  manifests are immutable audit records per run.

## Risks

- **Manifest file size increase.** Descriptions add ~40-120 bytes per handle.
  For a large family tree with 10,000 handles to delete, this adds ~1MB to
  the manifest. Acceptable for an audit-trail file.

- **Breaking existing v2 consumers.** Any external tool that reads manifests
  and expects exactly `{handle, status}` will see new fields. Since the new
  fields are additive (the old fields remain), parsers that ignore unknown
  keys are unaffected.
