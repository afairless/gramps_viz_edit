# Shift Delete Tool to Use Gramps Database API

## Status: Plan — Not Yet Implemented

---

## 1. Summary

Shift the `gramps-gen delete` command from a two-phase architecture (Rust
cascade engine computes deletion set → Python backend executes ALL deletions)
to a model where **the Gramps database API is the sole authority for
deletion**, the Rust cascade engine is **advisory only**, and the manifest
tracks what Gramps actually did versus what is still orphaned.

**Key finding from Gramps source investigation:** `delete_person_from_database`
in `gramps/gen/db/base.py` line 1855 has a very narrow cascade:

| What Gramps deletes | What Gramps does NOT delete |
|---|---|
| The person row | Events |
| Families that become completely empty (no father, no mother, no children) | Notes |
| | Citations / Sources / Repositories |
| | Places / Media / Tags |
| | Other people |

There is **no garbage collection** anywhere in Gramps. Orphaned secondary
objects are legal and survive XML export/import without errors. The current
Rust cascade engine is dramatically more aggressive than Gramps, deleting
events, notes, citations, places, and more — a full transitive closure of
orphan detection that Gramps itself never performs.

---

## 2. Current Architecture (for reference)

```
Input .gramps → gramps-reader parse → Graph → cascade engine → DeletePlan
                                                      ↓
                                          Interactive review
                                                      ↓
                                          final_to_delete (all types)
                                                      ↓
                              ┌───────────────────────────────────┐
                              │ Python backend (temp DB)          │
                              │   import → delete ALL types       │
                              │   (people, families, events,      │
                              │    notes, places, citations…)     │
                              │   → export .gramps                │
                              │   → cleanup temp DB               │
                              └───────────────────────────────────┘
                                                      ↓
                                              output.gramps
```

**Problems:**

1. The Rust cascade deletes secondary objects (events, notes, etc.) that Gramps
   itself would never delete — the output diverges from what Gramps expects.
2. Any bug in the Rust parser (missed reference, version-specific edge type)
   causes incorrect orphan detection and data loss.
3. The Gramps DB is temporary — the user cannot inspect the post-deletion
   state directly in Gramps.
4. The manifest doesn't track what actually happened — it's just a plan.

---

## 3. Proposed Architecture

```
Input .gramps → gramps-reader parse → Graph → cascade engine → DeletePlan
                                                      ↓
                                          Interactive review
                                                      ↓
                                          final_to_delete (all types)
                                                      ↓
                                          Save manifest v2 (all types, status=pending)
                                                      ↓
                              ┌───────────────────────────────────────────────┐
                              │ Python backend (PERSISTENT Gramps DB)         │
                              │   import → delete PEOPLE ONLY                 │
                              │   (Gramps auto-cascades families internally)  │
                              │   → export .gramps                            │
                              │   → report: which handles still exist in DB   │
                              │     ("surviving" — all manifest handles         │
                              │      checked via has_*_handle after deletion)   │
                              │   → DB RETAINED for user inspection           │
                              └───────────────────────────────────────────────┘
                                                      ↓
                              ┌───────────────────────────────────────────────┐
                              │ Rust reconciliation (transformation only)     │
                              │   Compare: manifest plan vs. actual DB state  │
                              │   Mark each handle:                           │
                              │     "deleted"  — Gramps removed it           │
                              │     "pending"  — cascade said delete, but    │
                              │                  Gramps kept it              │
                              │     "kept"     — user chose not to delete    │
                              │   Save enriched manifest alongside output     │
                              └───────────────────────────────────────────────┘
                                                      ↓
                              output.gramps + output.manifest.json
                              input-gramps-db/ (retained Gramps DB)

                              ┌───────────────────────────────────────────────┐
                              │ CLI output stage                             │
                              │   Format reconciliation summary for display  │
                              │   Log summary, save enriched manifest        │
                              └───────────────────────────────────────────────┘
```

**Key design principles:**

1. **Gramps is the authority for deletion.** Only people are deleted
   through the Gramps API via `delete_person_from_database`. Gramps
   handles family ref cleanup internally and auto-deletes families that
   become completely empty (no father, no mother, no children) as a side
   effect. No secondary objects (events, notes, etc.) are deleted.

2. **The cascade engine is advisory.** It still computes what *would* become
   orphaned and presents it to the user for review. But it no longer drives
   the actual deletion of non-people objects. The manifest includes
   `"deletion_mode": "people_only"` to make the advisory nature explicit.

3. **The manifest is a living document.** It records the user's intent
   (what the cascade said + what the user confirmed), then gets enriched
   with ground truth from Gramps (what actually happened).

4. **The Gramps DB is retained.** The user can open it directly in the
   Gramps UI to inspect the post-deletion state, see what's orphaned,
   and manually clean up if desired. Users should be aware that the
   retained DB contains a full copy of their genealogy data and should
   secure or delete it appropriately. A `--no-retain-db` flag allows
   automatic cleanup for users who don't need inspection.

5. **Extra deletions by Gramps are not tracked in the manifest.** Gramps
   may auto-delete families beyond those the cascade engine flagged
   (e.g., a family that becomes completely empty as a side effect of
   person deletion but that the Rust cascade would have kept because it
   detected a live connection). These extra deletions are not reflected
   in the manifest since Gramps' internal cascade cannot be queried
   programmatically. The reconciliation summary warns the user to
   inspect the output file for unexpected removals.

---

## 4. Manifest v2 Format

The manifest needs to track per-handle status. This requires a format change
from the current `to_delete: [handle, ...]` (flat list) to a list of objects
with status:

```json
{
  "version": 2,
  "source_file": "family.gramps",
  "selections_file": "picks.json",
  "created_at": "2025-01-15T10:30:00Z",
  "seed_people": ["a1b2c3d4-...", "e5f6g7h8-..."],
  "deletion_mode": "people_only",
  "plan": {
    "people": {
      "to_delete": [
        {"handle": "a1b2c3d4-...", "status": "deleted"},
        {"handle": "e5f6g7h8-...", "status": "deleted"}
      ],
      "kept": []
    },
    "families": {
      "to_delete": [
        {"handle": "f1f1f1f1-...", "status": "deleted"},
        {"handle": "f2f2f2f2-...", "status": "pending"}
      ],
      "kept": []
    },
    "events": {
      "to_delete": [
        {"handle": "e1e1e1e1-...", "status": "pending"},
        {"handle": "e2e2e2e2-...", "status": "pending"}
      ],
      "kept": [
        {"handle": "e3e3e3e3-...", "status": "kept"}
      ]
    },
    "notes": {
      "to_delete": [
        {"handle": "n1n1n1n1-...", "status": "pending"}
      ],
      "kept": []
    }
  }
}
```

### Status values

| Status | Meaning |
|---|---|
| `"pending"` | Initial state when the manifest is saved before deletion. The cascade says this should be deleted. |
| `"deleted"` | Confirmed: Gramps actually removed this object. Set during reconciliation. |
| `"kept"` | User chose not to delete this during review, or it was in `kept` from the start. |

**Expected reconciliation patterns:**

| Type | Typical status after deletion |
|---|---|
| People | `"deleted"` (Gramps always deletes people when asked) |
| Families | `"deleted"` (if Gramps auto-cascaded) or `"pending"` (if Gramps kept it — e.g., family still has live connections the Rust parser didn't see) |
| Events, Notes, Places, Citations, Sources, Repositories, Media, Tags | Always `"pending"` — Gramps never deletes these. They remain as orphans in the DB. |

### Backward compatibility

The current manifest format uses `"to_delete": ["handle1", ...]` (flat string
array). The v2 format uses `"to_delete": [{"handle": "...", "status": "..."},
...]`. On load, detect the format by inspecting the first element of
`to_delete`:

- If it's a string → v1 format → treat all entries as `"pending"`
- If it's an object with `"handle"` key → v2 format

This ensures existing saved manifests continue to work.

---

## Phase 1 — Manifest v2 Types and Serialization

### 1.1 New types in `crates/delete/src/types.rs`

```rust
/// Status of a handle in the deletion manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleStatus {
    /// Awaiting deletion (cascade says delete, but not yet executed).
    Pending,
    /// Successfully deleted by Gramps.
    Deleted,
    /// User chose to keep this object.
    Kept,
}

/// An entry in the to_delete or kept list with status tracking.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandleEntry {
    pub handle: Handle,
    pub status: HandleStatus,
}
```

### 1.2 Updated TypePlan

Change both `TypePlan.to_delete` and `TypePlan.kept` from `Vec<Handle>` to
`Vec<HandleEntry>` (both fields undergo the same migration — v1 string
entries in `kept` are treated identically to `to_delete`, with migrated
entries getting `status: Kept`). Use a `#[serde(untagged)]` enum so that
v1 flat-string format is still readable on deserialization:

```rust
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
enum HandleOrEntry {
    V1(String),
    V2(HandleEntry),
}
```

**Serialization is always v2 format** (object form). The v1 backward compat
is read-only via `TryFrom<Vec<HandleOrEntry>>`. A separate serialization impl
on `TypePlan` (or `#[serde(serialize_with)]`) ensures `to_delete` always
serializes as `Vec<HandleEntry>`, never as raw strings.

Then implement `TryFrom<Vec<HandleOrEntry>>` for `Vec<HandleEntry>` that
converts v1 entries to `HandleEntry { status: Pending }`.

### 1.3 Bump MANIFEST_VERSION and add migration

Change `MANIFEST_VERSION` from `1` to `2` in `manifest.rs`. The existing
`load_manifest()` rejects mismatched versions, so it must be updated to
support migration:

1. Rename current `load_manifest` to `load_manifest_v1` (handles v1 only).
2. New `load_manifest()` tries v2 deserialization first. If that fails
   with `UnsupportedVersion(999)` or a JSON structure mismatch, fall back
   to v1 loading + auto-migration (all handles → `Pending`).
3. If both fail, return the original v2 error.

This ensures backward compatibility without a flag day.

### 1.4 Update build_manifest and DeleteManifest

`build_manifest()` currently takes `to_delete: &[Handle]`. After review,
the confirmed set becomes `Vec<HandleEntry>` with status `Pending`.

Add `deletion_mode: String` to `DeleteManifest`, set to `"people_only"` in
`build_manifest()`. The `reconcile()` function checks this field and returns
`Err` for unrecognized values. Serialize it in the JSON alongside `version`
and other top-level fields (see the JSON example in §4).

### 1.5 Tests

- v1 manifest load → auto-migration to v2 (all statuses `Pending`)
- v2 manifest round-trip (serialize → deserialize)
- HandleEntry JSON format: `{"handle": "...", "status": "pending"}`
- HandleStatus serde: `"pending"`, `"deleted"`, `"kept"`
- v1 → v2 migration round-trip: load v1, save as v2, reload, verify

---

## Phase 2 — Python Backend: People-Only Deletion + DB Retention

### 2.1 Changes to `scripts/delete_backend.py`

**Current behavior:**

- Creates a **temporary** Berkeley DB directory
- Imports the `.gramps` file
- Deletes ALL types from the manifest (people, families, events, notes, etc.)
- Exports to `.gramps`
- Cleans up the temp DB

**New behavior:**

- Creates a **persistent** Gramps DB at a specified directory
- Imports the `.gramps` file
- Deletes **ONLY people** using `delete_person_from_database(person, trans)`
  (Gramps handles family cascading internally)
- Does NOT touch families, events, notes, or any other type
- Exports to `.gramps`
- DOES NOT clean up the DB — it remains for user inspection
- **Reports which handles still exist** in the DB after deletion (for
  reconciliation)

### 2.2 New CLI interface for the Python script

```
python3 delete_backend.py \
    --input in.gramps \
    --manifest plan.json \
    --output out.gramps \
    --db-dir /path/to/persistent/db/ \
    [--no-retain-db]
```

- `--db-dir` is new. The script no longer creates a temp directory.
- `--no-retain-db` cleans up the DB directory on successful exit.
  Default is to retain (for inspection). Use this in automated/CI
  pipelines where DB inspection is not needed.

The JSON stdout result adds a `"surviving"` field containing all
handles from the manifest that still exist in the DB after deletion:

```json
{"status": "ok", "output": "out.gramps", "deleted": 5, "rejected": [],
 "surviving": ["e0101...", "n0202...", "f0303..."]}
```

### 2.3 Modified `delete_items` function

```python
def delete_items(db, manifest):
    """Delete ONLY people from the database.
    
    Gramps' delete_person_from_database handles family ref cleanup
    and family deletion internally — we don't touch families, events,
    or any other type directly.
    
    Returns (deleted_count, rejected, surviving):
    - deleted_count: number of people deleted
    - rejected: handles with invalid format
    - surviving: all handles from the manifest that still exist
      in the DB AFTER deletion (for reconciliation)
    """
```

The `surviving` set is the key new output: it tells Rust which handles
are still in the DB. All people should be gone; families may or may not be
gone (depending on Gramps' internal cascade); events, notes, etc. will
all survive.

The `surviving` list is computed by calling `db.has_*_handle(h)` for
every handle in every `to_delete` list in the manifest (all types),
after the `DbTxn` commits. Performance impact is acceptable: a typical
file with <10K nodes has <1s overhead for these checks.

### 2.4 Handle validation (unchanged)

The pre-deletion handle validation stays: every handle must be UUID v4
format. Invalid handles go to `rejected`. Valid-but-missing handles cause
an abort before any deletion.

### 2.5 DB retention and overwrite behavior

- `--db-dir` is created if it doesn't exist and a new Berkeley DB is
  initialized there.
- If the directory already exists, `shutil.rmtree` it first and create
  fresh (with a warning to stderr). This avoids stale data from a
  previous run contaminating results.
- On normal exit, the DB files are retained (for user inspection).
- With `--no-retain-db`, the directory is removed on successful exit.
- On error/exception, the DB is always retained (so the user can
  inspect partial state).

### 2.6 Smoke test update

Update the `_smoke_test()` to verify the new behavior: create persistent
DB, import, delete only people, verify other types survive.

### 2.7 Tests

- Delete only people: verify families/events/notes survive
- Persistent DB: verify files exist after script exits
- `surviving` report: verify it correctly identifies what's still in the DB
- Empty manifest: no-op, all objects survive
- `--db-dir` preexisting: verify overwrite warning and fresh start
- `--db-dir` permission denied: verify clear error, not a crash
- `--no-retain-db`: verify DB directory is cleaned up on success
- `--no-retain-db` on error: verify DB directory is retained

---

## Phase 3 — Rust Reconciliation Engine

### 3.1 New module: `crates/delete/src/reconcile.rs`

A new module that takes the manifest (pre-deletion, all `Pending`) and the
Python backend's `surviving` report, and produces an enriched manifest with
per-handle status:

```rust
/// Reconcile the manifest against what Gramps actually deleted.
///
/// For each handle in the manifest's to_delete lists:
/// - If handle is NOT in surviving → status = Deleted
/// - If handle IS in surviving → status = Pending (Gramps kept it)
///
/// Handles in kept lists always stay Kept.
///
/// # Errors
///
/// Returns `Err` if the manifest's `deletion_mode` is unrecognized
/// or if the surviving set contains handles not present in the manifest
/// (which indicates a data contract mismatch).
pub fn reconcile(
    manifest: &mut DeleteManifest,
    surviving: &HashSet<Handle>,
) -> Result<(), ReconciliationError>;
```

### 3.2 PythonResult update

Add `surviving: Vec<Handle>` to the `PythonResult` struct:

```rust
#[derive(Deserialize, Debug)]
struct PythonResult {
    status: String,
    output: Option<String>,
    deleted: Option<u32>,
    message: Option<String>,
    rejected: Vec<String>,
    /// Handles that still exist in the DB after deletion.
    #[serde(default)]
    surviving: Vec<String>,
}
```

### 3.3 Reconciliation output

`reconcile()` is pure transformation — it modifies the manifest in-place
and returns `Result<(), ReconciliationError>`. The display formatting is
handled separately by the CLI output stage:

After reconciliation, log a summary:

```
Reconciliation complete:
  People:    2 deleted, 0 pending
  Families:  1 deleted, 1 pending (Gramps kept it)
  Events:    0 deleted, 3 pending (orphaned, not deleted by Gramps)
  Notes:     0 deleted, 1 pending (orphaned, not deleted by Gramps)

The Gramps database has been retained at: family-gramps-db/
Open it in Gramps to inspect orphaned objects.

Manifest saved to: family-cleaned.manifest.json
```

### 3.4 Tests

- All people deleted → status = Deleted
- Family auto-deleted by Gramps → status = Deleted  
- Family kept by Gramps (not empty) → status = Pending
- Event/note always survives → status = Pending
- Kept items stay Kept regardless of surviving set
- Empty surviving set → everything is Deleted
- surviving includes handles not in manifest → `Err(ReconciliationError)` (data contract mismatch)
- Reconcile twice (idempotency): `reconcile(reconcile(m, s), s) == reconcile(m, s)`
- Manifest with zero to_delete entries across all types → no-op, no error
- Empty surviving set with non-people cascade items → all become Deleted (this is expected — Gramps only deletes people, so all non-people handles appear "not in surviving")

---

## Phase 4 — CLI Changes

### 4.1 New flags: `--db-dir` and `--no-retain-db`

```
--db-dir <DIR>        Where to store the Gramps database
                      (default: <input-filename>-gramps-db/)
--no-retain-db         Clean up Gramps DB after successful export
```

If the directory already exists, warn but proceed (it will be overwritten).
`--no-retain-db` is useful for automated/CI pipelines where DB inspection is
not needed.

### 4.2 Manifest always saved

Currently `--save-manifest <FILE>` is optional. New behavior:

- The manifest is **always** saved alongside the output file.
- `--save-manifest <FILE>` still works as an explicit override.
- The manifest path is `<output-path-stem>.manifest.json`:
  - If `--output out.gramps` → manifest at `out.manifest.json`
  - If `--output` is not specified, default output is
    `<input>-cleaned.gramps` → manifest at `<input>-cleaned.manifest.json`
  - If `--output` is specified as `-` (stdout) or the output path ends
    in `.gz`, the manifest still uses the stem path.
- On `--dry-run`, the manifest is saved (all statuses `pending`).
- On Python backend failure, the pre-reconciliation manifest is saved
  (all statuses `pending`) with a warning that reconciliation was skipped.
  If the failure occurred AFTER the `.gramps` file was written (e.g., during
  `surviving` computation), the output file is deleted alongside the manifest
  save so that no stale partial-result pair is left on disk.

### 4.3 Updated help text

```
gramps-gen delete <INPUT.gramps> --selections <selections.json> [OPTIONS]

OPTIONS:
  --output, -o <FILE>       Output .gramps file
  --db-dir <DIR>            Gramps DB directory (default: <input>-gramps-db/)
  --no-retain-db            Clean up Gramps DB after successful export
  --yes, -y                 Skip all review prompts
  --dry-run, -n             Compute cascade, save manifest, but don't delete
  --save-manifest <FILE>    Override manifest output path
  --load-manifest <FILE>    Load pre-reviewed manifest (skip review)
```

When `--load-manifest` is used with a post-reconciliation manifest, the
manifest is filtered **before** graph validation: only handles with status
`pending` from `to_delete` lists are loaded. Handles with status `deleted`
or `kept` are excluded. This avoids `validate_manifest` rejecting handles
that were already removed in a previous run. The cascade engine then runs
on the current graph with only the still-pending seed people, and the
interactive review is skipped (same as `--yes`).

### 4.4 Updated `run()` flow

```
1. Parse input → Graph
2. If --load-manifest: filter manifest to pending-only handles, use as seeds
   If --selections: load selections JSON, cross-reference with graph
3. Run cascade engine → DeletePlan
4. Interactive review → final_to_delete (HashSet<Handle>)
   (Skipped if --yes or --load-manifest)
5. Build manifest v2 (all Pending, deletion_mode: "people_only")
6. If --dry-run: save manifest, exit
7. Resolve Python interpreter + script path
8. Write manifest JSON to temp file
9. Spawn Python backend:
   --input in.gramps
   --manifest temp_manifest.json
   --output out.gramps
   --db-dir <input>-gramps-db/
10. Parse PythonResult (including surviving)
11. Reconcile manifest against surviving
12. If DB directory >100MB, emit a warning to stderr
13. Save enriched manifest alongside output
14. Report summary
15. Clean up temp manifest file
```

---

## Phase 5 — Remove Dead Code

### 5.1 Python backend: remove non-people explicit deletion

In `scripts/delete_backend.py`:

- The `delete_items()` function explicitly deletes **only people**.
  Families are removed as a side effect of Gramps'
  `delete_person_from_database` (which auto-removes empty families).
  All other types in the manifest's `to_delete` lists are advisory —
  they are checked for the `surviving` report but never explicitly
  deleted by the script.

### 5.2 Rust: cascade engine unchanged; review UX updated

The cascade engine (`crates/delete/src/cascade.rs`) is unchanged. It still
computes the full orphan closure. The change is only in how the results are
used: the full cascade is presented to the user and saved in the manifest,
but only the people subset is sent to Gramps for deletion.

The interactive review (`review.rs`) is updated to clearly distinguish
advisory-only types from actually-deletable types:

- People: labeled as **"will be deleted"** — these are sent to Gramps.
- All other types (families, events, notes, etc.): labeled as
  **"advisory only — Gramps will not delete these"**. The user still
  reviews and confirms these for the manifest record, but the prompt
  makes the advisory nature explicit so the user isn't surprised when
  reconciliation shows them as `pending`.

### 5.3 Rust: remove `GraphXmlWriter` filter code (already done)

The filter methods (`with_filter`, `is_deleted`, `is_edge_deleted`,
`validate_deletion_set`, `to_delete` field) were already removed in the
Python backend restructure. No further changes needed.

---

## Phase 6 — Tests

### 6.1 Unit tests

| Test | Location |
|---|---|
| Manifest v1 → v2 auto-migration | `crates/delete/src/manifest.rs` |
| Manifest v2 round-trip | `crates/delete/src/manifest.rs` |
| v1 → v2 migration round-trip (load, save, reload) | `crates/delete/src/manifest.rs` |
| HandleEntry/HandleStatus serde | `crates/delete/src/types.rs` |
| Reconcile: all people deleted → status = Deleted | `crates/delete/src/reconcile.rs` |
| Reconcile: family survives (Gramps kept it) → status = Pending | `crates/delete/src/reconcile.rs` |
| Reconcile: events always survive → status = Pending | `crates/delete/src/reconcile.rs` |
| Reconcile: kept items stay kept | `crates/delete/src/reconcile.rs` |
| Reconcile: empty surviving set → everything is Deleted | `crates/delete/src/reconcile.rs` |
| Reconcile: surviving includes handles not in manifest → Err (data contract mismatch) | `crates/delete/src/reconcile.rs` |
| Reconcile: manifest with zero to_delete entries → no-op | `crates/delete/src/reconcile.rs` |
| Reconcile: idempotency (property-based): for any manifest m with all-Pending statuses and any surviving set S, `reconcile(reconcile(m, S), S) == reconcile(m, S)` | `crates/delete/src/reconcile.rs` |
| Reconcile: deletion_mode unrecognized → Err | `crates/delete/src/reconcile.rs` |
| reconcile() return type: Error on malformed surviving data | `crates/delete/src/reconcile.rs` |
| Python: delete only people, other types survive | `scripts/test_delete_backend.py` |
| Python: surviving report accuracy | `scripts/test_delete_backend.py` |
| Python: persistent DB not cleaned up (default) | `scripts/test_delete_backend.py` |
| Python: `--no-retain-db` cleans up DB on success | `scripts/test_delete_backend.py` |
| Python: `--no-retain-db` retains DB on error | `scripts/test_delete_backend.py` |
| Python: `--db-dir` preexisting → overwrite with warning | `scripts/test_delete_backend.py` |
| Python: `--db-dir` permission denied → clear error | `scripts/test_delete_backend.py` |
| Manifest contract fixture: committed JSON both sides consume | `crates/delete/tests/fixtures/` |
| PythonResult surviving field: committed JSON fixture for format | `crates/delete/tests/fixtures/` |

### 6.2 Integration tests

| Test | Description |
|---|---|
| `e2e_delete_people_only_python_roundtrip` | Delete people, verify Gramps import succeeds |
| `e2e_delete_manifest_v2_reconciliation` | Verify manifest correctly marks deleted/pending |
| `e2e_delete_db_retained` | Verify Gramps DB directory exists after delete |
| `e2e_delete_no_retain_db` | Verify `--no-retain-db` cleans up DB directory |
| `e2e_delete_orphaned_events_survive` | Verify events referenced only by deleted people still exist in output |
| `e2e_delete_load_manifest_v1` | Verify `--load-manifest` with v1 manifest works (auto-migration) |
| `e2e_delete_load_manifest_v2_reconciled` | Verify `--load-manifest` with reconciled v2 manifest skips already-deleted handles and re-deletes only `pending` items. The manifest filtering happens before `validate_manifest`, so previously-deleted handles (no longer in graph) don't cause rejection |

### 6.3 Manual verification

1. Generate a sample tree: `gramps-gen generate -n 20 -o test.gramps`
2. Open in visualizer, select 3 people, export selections
3. Run delete: `gramps-gen delete test.gramps -s picks.json -y`
4. Open `test-cleaned.gramps` in Gramps UI — should import without errors
5. Open `test-gramps-db/` in Gramps UI as a family tree — should see the
   post-deletion state with orphaned events/notes still present
6. Inspect `test-cleaned.manifest.json` — verify statuses are correct
7. Run delete again with the manifest: `gramps-gen delete test.gramps
   --load-manifest test-cleaned.manifest.json -y` — verify it still works
   (only deletes items still marked `pending`)

---

## Phase 7 — Documentation

| Document | Changes |
|---|---|
| `AGENTS.md` | Update delete pipeline description; mention DB retention |
| `docs/ARCHITECTURE.md` | Update delete pipeline diagram |
| `docs/research/bulk-delete-tool.md` | Add note: cascade engine now advisory-only for non-people types |
| `docs/research/gramps-python-delete-backend.md` | Add note: people-only deletion, DB retention |
| `docs/research/delete-output-roundtrip.md` | Add note: orphaned objects now intentionally preserved |

---

## 7. Implementation Order

Each step includes its own tests. Follow the incremental-development workflow:
code + tests → verify → commit before proceeding.

**Note on parallelism:** Steps 1-3 (Manifest v2) and Steps 4-5 (Python backend)
have no mutual dependencies and can be implemented in parallel by separate
contributors. Steps 6-8 build on both tracks and must be sequential.

| Step | Description | Dependencies |
|---|---|---|
| 1 | Manifest v2 types (`HandleEntry`, `HandleStatus`, serde, migration) | None |
| 2 | Manifest v2 save/load with backward compat | Step 1 |
| 3 | Update `build_manifest` to produce v2 entries | Step 2 |
| 4 | Python backend: `--db-dir` flag, persistent DB, people-only deletion | None |
| 5 | Python backend: `surviving` report | Step 4 |
| 6 | Rust: reconcile module (`reconcile.rs`) | Steps 2, 5 |
| 7 | Rust CLI: `--db-dir` + `--no-retain-db` flags, auto-save manifest, updated `run()` flow, reconciliation wiring | Steps 3, 6 |
| 8 | E2E tests: round-trip through Gramps import | All |
| 9 | Documentation update | All |

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Gramps `delete_person_from_database` behavior differs from investigation | Low | Verified against Gramps 5.1.6 source; add E2E test with real Gramps |
| Gramps auto-deletes a family the user expected to keep | Low | Manifest surfaces this clearly (status=deleted vs pending); family deletion only happens when completely empty |
| Berkeley DB file locks prevent DB reuse | Low | `shutil.rmtree` with `ignore_errors=True`; user can always re-import |
| Large files: Berkeley DB is slow | Medium | Acceptable; this is a one-time operation, not a hot path |
| User confused by orphaned objects remaining in output | Medium | Clear reconciliation summary; manifest shows what's pending; DB retained for manual cleanup |
| v1 manifest loaded as v2 → wrong statuses | Low | Auto-migration treats all v1 entries as Pending; reconciliation then sets correct status |
| Python backend fails after writing `.gramps` but before surviving report | Low | Delete the output file alongside saving the pre-reconciliation manifest; warn user. This avoids a stale .gramps/manifest pair where the file reflects deletions but the manifest claims everything is still pending. |
| Retained DB leaks sensitive genealogy data | Low | Document privacy implication in Phase 7; provide `--no-retain-db` for automated pipelines; users control the DB directory path |
| `HandleOrEntry` serde untagged enum ambiguity | Low | A JSON string always deserializes to V1, an object to V2 — no ambiguity; serialization always emits v2 format |
| Disk space consumed by persistent Berkeley DB | Low | Warn if DB directory >100MB; `--no-retain-db` avoids the issue entirely |

---

## 9. Success Criteria

1. `gramps-gen delete file.gramps --selections picks.json --yes` produces
   output that Gramps can import without errors.
2. The output `.gramps` file contains orphaned events/notes (they survive).
3. The Gramps DB at `<input>-gramps-db/` is retained and can be opened in
   the Gramps UI.
4. The manifest at `<output-stem>.manifest.json` correctly marks each handle as
   `deleted`, `pending`, or `kept`.
5. People are deleted; families are deleted only if Gramps auto-cascades;
   all other types always survive.
6. `--dry-run` saves the manifest with all statuses `pending` and does not
   call the Python backend.
7. All existing cascade engine tests still pass.
8. `--load-manifest` works with both v1 and v2 manifests.
9. `--no-retain-db` cleans up the Gramps DB directory on successful export.
10. On Python backend failure, the pre-reconciliation manifest is saved
    (statuses `pending`) and the user gets a clear warning.
11. The reconciliation summary is displayed to the user and the enriched
    manifest is saved to disk.
