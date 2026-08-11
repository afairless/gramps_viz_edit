# Implementation Plan: Shift Delete Tool to Use Gramps Database API

Source: `docs/research/gramps-api-delete-shift.md`

## Pre-Work

Before starting implementation, ensure the workspace is clean and tests pass:

```bash
git status          # should be clean
cargo test --workspace  # all tests pass
```

Create a feature branch:

```bash
git checkout -b agent/delete-gramps-api-shift
```

---

## Steps

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add HandleStatus, HandleEntry types with v1 backward compat` | Manifest v2 types | `crates/delete/src/types.rs` — `HandleStatus`, `HandleEntry`, `HandleOrEntry` enum, updated `TypePlan` with `Vec<HandleEntry>` + backward compat deserialization | Unit: HandleEntry serde, HandleStatus serde, v1 flat-string deserialization, v2 round-trip |
| 2 | `feat: add manifest v2 save/load with backward compat and migration` | Manifest v2 save/load | `crates/delete/src/manifest.rs` — bump `MANIFEST_VERSION` to 2, add `deletion_mode` to `DeleteManifest`, `load_manifest` migration from v1→v2, `build_manifest` produces v2 entries | Unit: v1→v2 migration, v2 round-trip, migration round-trip (load v1→save v2→reload), unsupported version error, contract fixture v2 |
| 3 | `feat: update build_manifest to produce v2 entries with deletion_mode` | build_manifest v2 | `crates/delete/src/manifest.rs` — `build_manifest` takes `&[HandleEntry]` and produces v2 manifest with `deletion_mode: "people_only"` | Unit: build_manifest with v2 entries, deletion_mode field, deterministic ordering |
| 4 | `feat: persistent Gramps DB with people-only deletion in Python backend` | Python backend: persistent DB + people-only | `scripts/delete_backend.py` — `--db-dir` flag, persistent DB, `delete_items` deletes only people via `delete_person_from_database`, DB retention, `--no-retain-db` | Unit: people-only deletion (other types survive), persistent DB retention, `--no-retain-db` cleanup, `--db-dir` preexisting overwrite, `--db-dir` permission denied |
| 5 | `feat: add surviving report to Python backend` | Python backend: surviving report | `scripts/delete_backend.py` — compute `surviving` handles by checking `has_*_handle` for all manifest handles after deletion, add `surviving` to JSON stdout result | Unit: surviving report accuracy, empty manifest surviving, all types checked |
| 6 | `feat: add reconcile module for manifest reconciliation against Gramps` | Rust reconcile module | `crates/delete/src/reconcile.rs` — `reconcile()` function, `ReconciliationError` type, `surviving: Vec<Handle>` in `PythonResult` | Unit: all people deleted→Deleted, family kept by Gramps→Pending, events always survive→Pending, kept items stay Kept, empty surviving→all Deleted, surviving-handles-not-in-manifest→Err, idempotency (property-based), zero to_delete→no-op, unrecognized deletion_mode→Err |
| 7 | `feat: add --db-dir, --no-retain-db flags, auto-save manifest, reconciliation wiring` | CLI changes | `crates/cli/src/commands/delete.rs` — `--db-dir` and `--no-retain-db` flags, auto-save manifest alongside output, updated `run()` flow with reconciliation, `--load-manifest` filters pending-only handles, updated `PythonResult` with `surviving` | Unit: new args struct, updated integrated error handling, `PythonResult` deserialization with `surviving` |
| 8 | `test: add E2E tests for delete pipeline with Gramps API shift` | E2E integration tests | `crates/cli/tests/e2e.rs` — E2E tests for people-only deletion, manifest v2 reconciliation, DB retention, `--no-retain-db`, orphaned events survive, `--load-manifest` v1 and v2 | Integration: `e2e_delete_people_only_python_roundtrip`, `e2e_delete_manifest_v2_reconciliation`, `e2e_delete_db_retained`, `e2e_delete_no_retain_db`, `e2e_delete_orphaned_events_survive`, `e2e_delete_load_manifest_v1`, `e2e_delete_load_manifest_v2_reconciled` |
| 9 | `docs: update delete pipeline documentation and AGENTS.md` | Documentation update | `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/research/bulk-delete-tool.md`, `docs/research/gramps-python-delete-backend.md`, `docs/research/delete-output-roundtrip.md` | — |

---

## Step Details

### Step 1 — Manifest v2 types (`types.rs`)

**Changes:**

- Add `HandleStatus` enum: `Pending`, `Deleted`, `Kept` (serde `rename_all = "snake_case"`)
- Add `HandleEntry` struct with `handle: Handle` and `status: HandleStatus`
- Add `HandleOrEntry` untagged enum for v1 backward compat deserialization
- Change `TypePlan.to_delete` from `Vec<Handle>` to `Vec<HandleEntry>`
- Change `TypePlan.kept` from `Vec<Handle>` to `Vec<HandleEntry>`
- Implement `TryFrom<Vec<HandleOrEntry>>` for `Vec<HandleEntry>` (v1→v2 conversion)
- Custom serde on `TypePlan` to serialize as v2 format but deserialize both v1 and v2
- Update existing tests to use `HandleEntry` where needed

**Key design decisions:**

- The `HandleOrEntry` untagged enum is read-only (deserialization only). Serialization always emits v2 format.
- `HandleEntry` in `kept` gets `status: Kept` on migration; `HandleEntry` in `to_delete` gets `status: Pending` on migration.
- Existing `TypePlan` round-trip tests need updating to use the new format.

### Step 2 — Manifest v2 save/load (`manifest.rs`)

**Changes:**

- Bump `MANIFEST_VERSION` from 1 to 2
- Add `deletion_mode: String` to `DeleteManifest` (default `"people_only"`)
- Rename current `load_manifest` to `load_manifest_v1`
- New `load_manifest()` tries v2 deserialization first, catches `UnsupportedVersion(1)` → falls back to v1 loading + auto-migration
- Auto-migration: all `to_delete` entries → `HandleEntry { status: Pending }`, all `kept` entries → `HandleEntry { status: Kept }`
- Add `deletion_mode` to manifest JSON serialization
- Update `build_manifest` to set `deletion_mode: "people_only"` and `version: 2`
- Add contract fixture `manifest-contract-v2.json` with v2 format
- Update `manifest_contract.rs` tests for v2
- Update `check_source_file` to work with v2 manifests

### Step 3 — build_manifest v2 (`manifest.rs`)

**Changes:**

- `build_manifest()` signature: `to_delete: &[HandleEntry]` instead of `&[Handle]`
- Actually, to minimize API churn: keep `to_delete: &[Handle]` and internally convert `Pending` entries, OR change the signature to accept `&[HandleEntry]`. The plan says "After review, the confirmed set becomes `Vec<HandleEntry>` with status `Pending`" — so change the signature.
- Update `build_manifest` to set `deletion_mode: "people_only"` on the manifest
- Ensure deterministic ordering of entries in the manifest
- All callers updated (in `cli/src/commands/delete.rs`)

### Step 4 — Python backend: persistent DB + people-only (`delete_backend.py`)

**Changes:**

- Replace `--manifest` with `--manifest` (kept, but now with v2 format support)
- Add `--db-dir <DIR>` flag (default: not set, but required for new flow)
- Add `--no-retain-db` flag
- Change `create_temp_db()` to `create_db(db_dir: str)` — creates persistent DB at specified path
- If `--db-dir` exists, `shutil.rmtree` it first (with warning to stderr)
- `delete_items()` only deletes people via `delete_person_from_database`
- Remove all other type deletions from `delete_items()`
- Keep `_TYPE_OPS` for `has_*_handle` checks (needed for surviving)
- Update `_smoke_test()` for persistent DB
- On error/exception, retain DB directory
- With `--no-retain-db`, clean up on success

### Step 5 — Python backend: surviving report (`delete_backend.py`)

**Changes:**

- After deletion and `DbTxn` commit, compute `surviving` set:
  - Iterate all handles in all `to_delete` lists across all types in the manifest
  - For each handle, call `db.has_*_handle(handle)` based on the type
  - Collect handles that still exist in the DB
- Add `"surviving": [...]` to the JSON stdout result
- Handle the case where the manifest is v2 format (with `HandleEntry` objects)

### Step 6 — Rust reconcile module (`reconcile.rs`)

**New file:** `crates/delete/src/reconcile.rs`

**Types:**

- `ReconciliationError` enum with `SurvivingHandlesNotInManifest(Vec<Handle>)` and `UnrecognizedDeletionMode(String)` variants

**Functions:**

- `reconcile(manifest: &mut DeleteManifest, surviving: &HashSet<Handle>) -> Result<(), ReconciliationError>`:
  - Check `manifest.deletion_mode` — return `Err` for unrecognized values
  - For each type's `to_delete` entries:
    - If handle NOT in `surviving` → `status = Deleted`
    - If handle IS in `surviving` → `status = Pending` (Gramps kept it)
  - For `kept` entries: status stays `Kept`
  - Validate: if any handle in `surviving` is not in the manifest → `Err`

**Module registration:**

- Add `pub mod reconcile;` to `crates/delete/src/lib.rs`
- Re-export `reconcile` and `ReconciliationError`

### Step 7 — CLI changes (`delete.rs`)

**Changes to `DeleteArgs`:**

- Add `--db-dir <DIR>`: `Option<PathBuf>` (default: `<input>-gramps-db/`)
- Add `--no-retain-db`: `bool`

**Changes to `PythonResult`:**

- Add `#[serde(default)] surviving: Vec<String>`

**Changes to `run()` flow:**

1. Parse input → Graph (unchanged)
2. If `--load-manifest`: filter manifest to pending-only handles, use as seeds. Deleted/kept handles excluded before `validate_manifest`.
3. Run cascade engine → DeletePlan (unchanged)
4. Interactive review → final_to_delete (skipped if `--yes` or `--load-manifest`)
5. Build manifest v2 with `deletion_mode: "people_only"`
6. If `--dry-run`: save manifest, exit
7. Resolve Python interpreter + script path (unchanged)
8. Write manifest JSON to temp file
9. Spawn Python backend with `--db-dir` and `--no-retain-db` flags
10. Parse `PythonResult` (including `surviving`)
11. Reconcile manifest against `surviving`
12. If DB directory >100MB, emit warning to stderr
13. Save enriched manifest alongside output (always, not just with `--save-manifest`)
14. Report summary
15. Clean up temp manifest file

**Manifest auto-save path:**

- Default: `<output-path-stem>.manifest.json`
- `--save-manifest <FILE>` overrides the path
- On `--dry-run`: save manifest (all `Pending`)
- On Python backend failure: save pre-reconciliation manifest (all `Pending`), warn user

### Step 8 — E2E tests

Add to `crates/cli/tests/e2e.rs`:

- `e2e_delete_people_only_python_roundtrip`: delete people, verify Gramps import succeeds on output
- `e2e_delete_manifest_v2_reconciliation`: verify manifest correctly marks deleted/pending
- `e2e_delete_db_retained`: verify Gramps DB directory exists after delete
- `e2e_delete_no_retain_db`: verify `--no-retain-db` cleans up DB directory
- `e2e_delete_orphaned_events_survive`: verify events referenced only by deleted people still exist in output
- `e2e_delete_load_manifest_v1`: verify `--load-manifest` with v1 manifest works (auto-migration)
- `e2e_delete_load_manifest_v2_reconciled`: verify `--load-manifest` with reconciled v2 manifest skips already-deleted handles

### Step 9 — Documentation

Update these files to reflect the new architecture:

- `AGENTS.md` — update delete pipeline description; mention DB retention, advisory-only cascade for non-people types
- `docs/ARCHITECTURE.md` — update delete pipeline diagram
- `docs/research/bulk-delete-tool.md` — add note: cascade engine now advisory-only for non-people types
- `docs/research/gramps-python-delete-backend.md` — add note: people-only deletion, DB retention
- `docs/research/delete-output-roundtrip.md` — add note: orphaned objects now intentionally preserved
