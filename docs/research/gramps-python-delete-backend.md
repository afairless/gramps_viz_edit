# Gramps Python Delete Backend

## Summary

Replace the Rust XML serialization path in the `delete` command with a Python
subprocess that uses Gramps' own import/delete/export libraries.  Our Rust
cascade engine remains intact (it is well-tested and correctly computes the
deletion set), but all XML I/O is delegated to Gramps' battle-tested code.

This eliminates the entire class of XML round-trip bugs described in
[`delete-output-roundtrip.md`](./delete-output-roundtrip.md) — namespace
mismatches, date truncation, missing attributes, element ordering — and
delegates XML I/O for the `delete` command to a ~400-line Python script that
calls Gramps' own `importxml.py` and `exportxml.py`.

> **Scope note:** This removes only the **filter** logic from `GraphXmlWriter`:
> the public methods `with_filter` and `validate_deletion_set`, the private
> methods `is_deleted` and `is_edge_deleted`, and the `to_delete` field.
> The `gramps_xml_version` field (already present, added during prior
> namespace-consistency work) is **kept** — it is used by both the `generate`
> and `delete` paths. The full Rust serializer stays in place for the
> `generate` command; it is not replaced wholesale.

## Motivation

The current delete pipeline has a serialization phase that writes Gramps XML
from our own graph representation:

```
Input .gramps → gramps-reader parse → Graph → cascade → output serialize → .gramps
```

Every difference between our XML output and Gramps' expected format is a
potential crash.  We have fixed several bugs (note `type` attribute, date
truncation, namespace mapping), but each fix uncovers new edge cases.  The
attack surface is the entire Gramps XML schema.

By contrast, Gramps' own XML exporter has been tested by thousands of users
over decades.  If we import the file into a Gramps database, delete objects
through Gramps' API, and export with Gramps' own writer, the output is
guaranteed to be valid.

## Design

### Architecture

```
┌──────────────────────────────────────────────────────────────┐
│ Rust (gramps-gen delete command)                             │
│                                                              │
│  1. Parse input .gramps → typed_graph::Graph                 │
│  2. Load selections → set of seed handles                    │
│  3. Run cascade engine → DeletePlan (read-only on Graph)     │
│  4. Serialize DeletePlan as JSON manifest                    │
│  5. Spawn: python3 scripts/delete_backend.py                 │
│     --input in.gramps --manifest plan.json                   │
│     --output out.gramps                                      │
│  6. Report results from stdout                               │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ Python (scripts/delete_backend.py)                           │
│                                                              │
│  7.  Read JSON manifest (lists of handles per type)          │
│  8.  Create temp Gramps DB (Berkeley DB)                     │
│  9.  Import input .gramps via gramps.plugins.importxml       │
│  10. For each person handle: db.delete_person_from_database()│
│  11. For each family handle: db.remove_family()              │
│  12. For each event handle: db.remove_event()                │
│  13. For each note handle: db.remove_note()                  │
│  14. … same for place, source, citation, repository,         │
│      media, tag                                              │
│  15. Export via gramps.plugins.exportxml → output.gramps     │
│  16. Print JSON result to stdout                             │
└──────────────────────────────────────────────────────────────┘
```

### Why the Check tool is NOT used

Gramps' Check & Repair tool (`check.py`) does two things that are
counterproductive for our use case:

1. **Creates replacement objects for broken references.**  When a person
   references a deleted event, Check creates a new "Unknown" event rather
   than removing the reference.  This would leave our output with dummy
   objects we never intended to keep.

2. **Only removes truly empty objects.**  `cleanup_empty_objects` removes
   objects whose data fields are all at their defaults (e.g., an event with
   no date, no place, no description).  An orphaned event that still has a
   date and place is NOT removed — it has non-default data.

The tool is designed for repairing corruption, not for cascade deletion.
We must NOT run it as part of our delete backend.

### Manifest format (Rust → Python)

The manifest reuses the existing `DeleteManifest` layout so there is a single
source of truth for the contract. Python reads each type's `to_delete` list:

```json
{
  "version": 1,
  "source_file": "family.gramps",
  "created_at": "2025-01-15T10:30:00Z",
  "seed_people": ["handle1", "handle2"],
  "plan": {
    "people":      { "to_delete": ["handle1", "handle2"], "kept": [] },
    "families":    { "to_delete": ["handle3"],              "kept": [] },
    "events":      { "to_delete": ["handle4", "handle5"],  "kept": [] },
    "notes":       { "to_delete": ["handle6"],              "kept": [] },
    "places":      { "to_delete": [],                        "kept": [] },
    "sources":     { "to_delete": [],                        "kept": [] },
    "citations":   { "to_delete": [],                        "kept": [] },
    "repositories":{ "to_delete": [],                        "kept": [] },
    "media":       { "to_delete": [],                        "kept": [] },
    "tags":        { "to_delete": [],                        "kept": [] }
  }
}
```

This is the **data contract** between the Rust stage and the Python stage. Its
shape must be pinned by a shared test fixture (see Step 5) so the two sides
cannot silently drift. The manifest is written from the **reviewed** set
(`final_to_delete`), not from `DeletePlan`.

### Deletion order in Python

Gramps' `delete_person_from_database` handles family cleanup internally
(removes parent/child references; if that leaves the family empty, it
deletes the family).  However, we pass explicit handle lists for all
types from our cascade engine, so the order must respect the dependency
chain:

1. **People first** — `delete_person_from_database` may modify families
   (remove parent refs) but won't delete events, notes, etc. Pass the resolved
   `Person` object plus the shared transaction.
2. **Families** — `remove_family` removes the family node; events referenced
   by the family are NOT automatically removed
3. **Events, notes, places, sources, citations, repositories, media, tags**
   — removed via their respective `remove_*` methods in any order

All operations run inside a single `DbTxn` so the write is atomic.

### Error handling

The Python script communicates results via stdout (JSON) and errors via
stderr + exit code:

```json
{"status": "ok", "output": "out.gramps", "deleted": 42}
{"status": "error", "message": "Import failed: ..."}
```

Rust deserializes this into a `PythonResult` struct and converts to
`CliError` or success:

```rust
#[derive(Deserialize)]
struct PythonResult {
    status: String,        // "ok" or "error"
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    deleted: Option<u32>,
    #[serde(default)]
    message: Option<String>,
    /// Handles that were in the manifest but not found in the DB
    /// (dead-letter pattern — see data-contracts skill).
    #[serde(default)]
    rejected: Vec<String>,
}
```

## Step-by-Step Implementation

### Step 1 — Create `scripts/delete_backend.py`

A standalone, headless Python script (no windows created).  Uses Gramps' `gen`
layer and the plugin import/export modules. The plugin machinery does pull in
`gramps.gui`/GTK imports (non-window), so GTK must be *importable* — see the
GTK suppression note below.

**Key imports:**

- `gramps.gen.db.utils.make_database`
- `gramps.gen.db import DbTxn`
- `gramps.gen.dbstate.DbState`
- `gramps.plugins.importer.importxml.importData`
- `gramps.plugins.export.exportxml.export_data`
- `gramps.cli.user.User`

**Key functions:**

| Function | Purpose |
|----------|---------|
| `create_temp_db()` | Create empty DB in temp dir (see backend notes below) |
| `import_xml(db, path)` | Import .gramps into DB |
| `delete_items(db, manifest)` | Validate handles, then delete in dependency order |
| `export_xml(db, path)` | Export DB to .gramps file |
| `main()` | Parse args, orchestrate, print result |

**Missing from the design above — the `make_database` contract (verified against
Gramps 5.1.6):** `make_database` takes a **backend plugin id**, not a directory:
`db = make_database("bsddb")` returns an **uninitialized** DB object. You must
then assign the storage directory (`db.dir = tmpdir`) and initialize it with
`db.write_version(VERSION)` (plus nickname/options) before importing.
*Spike this in Step 1* — the init sequence is version-sensitive (5.1/5.2 use
`bsddb`; 6.x moved off it). Detect the installed Gramps version and select the
backend accordingly.

**Delete-method signatures (verified against Gramps 5.1.6):**

- `delete_person_from_database(person, trans)` — takes a **`Person` object and a
  transaction**, not a handle. Look it up first: `db.get_person_from_handle(h)`.
- `remove_family` / `remove_event` / `remove_note` / `remove_place` /
  `remove_source` / `remove_citation` / `remove_repository` / `remove_media` /
  `remove_tag` — each take `(handle, transaction)`. Passing one shared `DbTxn`
  to all of them realizes the atomic-write goal.

**GTK suppression:**  The import/export plugin machinery pulls in `gramps.gui`
(observed PyGIWarning on import), so the script is *not* strictly GTK-free even
though it never opens windows. Import GTK early with a version lock to suppress
PyGIWarning and run headlessly.

> Correction vs. the original framing: the script does depend on
> pygobject/GTK being *importable*; it just never instantiates a window.

```python
import gi
gi.require_version('Gtk', '3.0')
```

**Input format handling:**  Detect gzip compression from file magic bytes
(`\x1f\x8b`) and decompress before passing to Gramps' importer.  Gramps'
`importxml.py` handles gzip natively when the filename ends in `.gz`, so
rename the temp file accordingly.

**Output format handling:** The Rust side passes the desired output path to
the Python script. The Python script controls whether the output is
gzip-compressed: if the output filename ends in `.gz`, the Gramps exporter
is told to gzip; otherwise it writes uncompressed XML. This avoids Rust
having to decompress after the subprocess returns.

**Version handling:**  Read the `xmlns` attribute from the input XML header
to determine the Gramps XML version.  Pass the version string to the
exporter so the output matches the input format. If the namespace is
unrecognized (doesn't match any known Gramps version), fall back to
`"5.2"` as the default and emit a warning to stderr.

**Handle validation** (updated 2025): The UUID v4 format validator was removed
(see `docs/research/fix-handle-format-mismatch.md`). Handles are now validated
only by existence via Gramps' `has_*_handle()` methods. Any handle absent from
the DB causes the script to abort *before any deletion* — no partial writes.
The `rejected` field in the JSON result is always empty (kept for backward
compat with the Rust side).

**Atomic output:** Export to a temp file first, then atomically rename to
the final output path. This prevents a partial/corrupt `.gramps` file if
the script crashes during export (the deletion is atomic via `DbTxn`, but
export is a separate operation after commit).

**Temp DB cleanup:** Use `shutil.rmtree(path, ignore_errors=True)` to clean
up the Berkeley DB temp directory. Berkeley DB may hold file locks on some
platforms, causing `rmtree` to raise `OSError`; `ignore_errors=True`
prevents the cleanup failure from masking the actual operation result.

### Step 2 — Serialize the **reviewed** deletion set as a manifest

> **Audit note:** The existing `build_manifest()` in `manifest.rs` already
> groups handles per type using `node_kind_to_label(node).plural()`, which
> produces the snake_case JSON keys ("people", "families", etc.). It also
> already takes a flat `&[Handle]` list and groups them — no new grouping
> function is needed. The current `delete.rs` already passes `final_to_delete`
> (the reviewed set) to `build_manifest`, not the raw `DeletePlan`.
>
> **This step is verification, not new implementation.** Verify that:
>
> 1. `build_manifest` is called with `final_to_delete` (reviewed set), not
>    `DeletePlan.to_delete` (raw cascade output)
> 2. The manifest JSON keys are snake_case (matching `NodeKindLabel::plural()`)
> 3. The manifest includes seed people in `seed_people` and per-type
>    `to_delete`/`kept` arrays
>
> The manifest JSON uses these keys, already produced by
> `NodeKindLabel::plural()`:
>
> | Rust variant | JSON key |
> |-------------|----------|
> | `Person` | `people` |
> | `Family` | `families` |
> | `Event` | `events` |
> | `Place` | `places` |
> | `Source` | `sources` |
> | `Citation` | `citations` |
> | `Repository` | `repositories` |
> | `Media` | `media` |
> | `Note` | `notes` |
> | `Tag` | `tags` |
>
> The only code change needed: ensure `final_to_delete: HashSet<Handle>` is
> converted to `Vec<Handle>` before passing to `build_manifest` (already done
> in the current `delete.rs`).

### Step 3 — Modify `crates/cli/src/commands/delete.rs`

Replace the `GraphXmlWriter` write step (which uses `final_to_delete`) with a
subprocess call that passes a manifest built from that same **reviewed set**:

1. **Serialize manifest** to a temp JSON file
2. **Locate the Python script** — embed the path at build time:
   `concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/delete_backend.py")`
   (relative from `crates/cli/` to workspace root). Also support a
   `GRAMPS_DELETE_BACKEND` env var override for development.
3. **Spawn subprocess:**

   ```
   python3 <script_path> \
     --input <input.gramps> \
     --manifest <temp_manifest.json> \
     --output <output.gramps>
   ```

   The interpreter must resolve to the environment that has Gramps — do not
   hardcode a bare `python3`. Resolve deterministically:

   1. `GRAMPS_PYTHON` env var (if set, use as-is)
   2. Try `python3` on PATH: run `python3 -c "import gramps.gen.db.utils"` —
      if exit code is 0, use `python3`
   3. Error with: "Gramps Python libraries not found. Set GRAMPS_PYTHON env
      var to the python interpreter that can import gramps."

4. **Parse stdout** for JSON result
5. **Report** success or error

> **Commit ordering:** Step 4 (removing the filter methods) must land in the
> same commit as Step 5's *removal* of the `xml.rs` unit tests that reference
> them — otherwise the tree fails to compile/pass at the Step 4 checkpoint.

**Temp directory:**  Create a temp dir for both the manifest and the
Berkeley DB (Python script handles DB cleanup).  Clean up on exit.

**Timeout:**  Set a generous timeout (e.g., 5 minutes) since Berkeley DB
operations can be slow on large files.

**Gzip output:**  The Python script determines gzip behavior based on the
output filename extension. If the path ends in `.gz`, the Gramps exporter
writes gzip-compressed XML. Otherwise it writes uncompressed XML. Rust does
not need to decompress after the subprocess returns.

### Step 4 — Remove the Rust XML filter/serialization path

**Files to modify:**

| File | Change |
|------|--------|
| `crates/cli/src/commands/delete.rs` | Remove `GraphXmlWriter` import and write logic; add subprocess call |
| `crates/output/src/xml.rs` | Remove `with_filter` (pub), `validate_deletion_set` (pub), `is_deleted` (private), `is_edge_deleted` (private) methods and the `to_delete` field from `GraphXmlWriter`. The `to_delete` field is referenced in `with_filter`, `is_deleted`, `is_edge_deleted`, `write_section` (line ~271 filter), and `write_edge_items` (line ~776 filter). All references must be removed together. The `gramps_xml_version` field (already present) is kept — it is used by the `generate` path too. |
| `crates/output/src/lib.rs` | No change needed (public API unchanged apart from filter removal) |

**Files to keep (used by other commands):**

| File | Reason |
|------|--------|
| `crates/output/src/xml.rs` (rest) | Used by `generate` command for XML output |
| `crates/output/src/serialization_map.rs` | Used by `generate` command |
| `crates/gramps-reader/` | Still needed for parsing input into Graph |
| `crates/delete/src/cascade.rs` | Cascade engine stays — computes deletion set |
| `crates/delete/src/types.rs` | Types stay; add manifest serialization |

### Step 5 — Update tests

**Remove tests that verify XML filter behavior:**

| Test file | Tests to remove |
|-----------|----------------|
| `crates/output/src/xml.rs` | Tests for `with_filter`, `is_deleted`, `validate_deletion_set` |
| `crates/cli/tests/e2e.rs` | Any test that asserts XML structure of delete output |
| `crates/cli/tests/integration.rs` | Tests that rely on `GraphXmlWriter` with filter |

**Add tests:**

| Test file | Tests to add |
|-----------|-------------|
| `crates/cli/tests/e2e.rs` | `e2e_delete_gramps_python_roundtrip` — run delete with real Gramps, import output, verify no errors |
| `crates/delete/src/types.rs` | `manifest_json_roundtrip` — serialize/deserialize manifest |
| `scripts/delete_backend.py` | Unit tests via `pytest` (or docstring `__main__` tests) |

**Manifest contract test (from the data-contracts skill):**
`crates/delete/tests/manifest_contract.rs` must serialize a sample
`DeleteManifest` and assert the exact JSON shape the Python parser consumes
(type keys and the `to_delete` nesting). Both Rust and Python consume the same
committed fixture at
`crates/delete/tests/fixtures/manifest-contract-v1.json` so a format change
on either side fails loudly. The fixture is a minimal valid manifest with one
entry per type, generated by a test helper and committed.

**Python edge-case tests:**

- Empty manifest and all-empty per-type lists (no-op import/export succeeds)
- A manifest handle that is absent from the DB → script fails with a specific
  error *before* deleting anything (the pre-deletion handle validation promised
  in the risk table must be exercised)
- Partial presence: some handles exist, one does not → nothing is written
- Invalid handle format (not UUID v4) → handle appears in `rejected` list,
  operation continues with valid handles
- Subprocess timeout: spawn a mock Python script that sleeps indefinitely,
  verify the timeout fires and the subprocess is killed

**E2E test strategy:**

The most important test: take a real Gramps `.gramps` file, run
`gramps-gen delete --selections picks.json --yes`, then import the output
into Gramps and verify it succeeds.  This replaces the XML assertion tests
with a behavioral test.

```bash
# E2E test pseudocode
gramps-gen delete fixture.gramps --selections picks.json --yes -o clean.gramps
# Verify Gramps can import it
python3 -c "
from gramps.plugins.importer import importxml
from gramps.gen.db.utils import make_database
from gramps.gen.dbstate import DbState
...
importxml.importData(db, 'clean.gramps', User())
print('OK')
"
# Assert no errors on stderr
```

> **Limitation:** The E2E verification above uses the same
> `gramps.plugins.importer.importxml` library as the backend. A bug in that
> importer would produce a false positive (both backend and test affected).
> When available, add a secondary verification using Gramps CLI:
> `gramps -y -q -i clean.gramps` (Gramps GUI CLI import). This is optional
> and gated behind `gramps_available()` which also checks for the CLI binary.
>
> A unit test for `gramps_available()` must be added to verify it compiles
> and returns a boolean without panicking. In CI, the test should assert
> `gramps_available()` returns `true` — if Gramps is not installed, the CI
> step should fail rather than silently skipping E2E tests.

### Step 6 — Update documentation

| Document | Change |
|----------|--------|
| `AGENTS.md` | Add `scripts/delete_backend.py` to project structure |
| `docs/ARCHITECTURE.md` | Update delete pipeline diagram |
| `docs/delete-tool.md` | Update pipeline diagram (step 5 now uses Python backend); fix manifest JSON example to use snake_case keys (`"people"` not `"Person"`); note that deletion uses Gramps' native libraries |
| `docs/research/delete-output-roundtrip.md` | Add note that bugs 1-4 are obsoleted by this restructure |

### Step 7 — CI considerations

The delete command now requires a working Gramps installation (Python
libraries).  CI environments should have Gramps installed for E2E tests.

The test can be gated behind a feature flag or environment check:

```rust
#[cfg(test)]
fn gramps_available() -> bool {
    std::process::Command::new(good_gramps_python())
        .args(["-c", "import gramps.gen.db.utils; \
            import gramps.gen.const as c; \
            assert c.VERSION.startswith(('5.1','5.2'))"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

Tests that require Gramps should skip gracefully when it's not available.

**Pin/verify the supported Gramps version in CI.** The API usage in this plan
was validated against Gramps **5.1.6** (`make_database(plugin_id)`, bsddb
backend, `delete_person_from_database(person, trans)`). Ensure the CI image and
the `gramps_available()` check enforce that the tested environment matches the
backend/API assumptions — 6.x deviates (bsddb removal). `gramps_available()`
should also assert the version, not just that `gramps.gen.db.utils` imports.

## Deletion Order & Safety

The Python script MUST delete in this order to avoid breaking referential
integrity during the operation:

```
1. People        (delete_person_from_database handles family ref cleanup)
2. Families      (remove_family; events referenced by families are NOT auto-removed)
3. Events        (remove_event)
4. Notes         (remove_note)
5. Places        (remove_place)
6. Sources       (remove_source)
7. Citations     (remove_citation)
8. Repositories  (remove_repository)
9. Media         (remove_media)
10. Tags         (remove_tag)
```

People are always first because `delete_person_from_database` modifies
families (removing parent/child references).  All other types are independent.

The script uses a single `DbTxn` wrapping all operations, passed to each
`remove_*` / `delete_person_from_database` call, so the write is atomic —
either all deletions succeed or none do.

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| Gramps not installed | Medium | High — delete command fails | Detect at startup, give clear error message |
| Python/Gramps version mismatch | Low | Medium | Read XML namespace from input; use matching exporter settings |
| Berkeley DB temp dir not cleaned up | Low | Low | `shutil.rmtree(path, ignore_errors=True)` in `finally` block; Berkeley DB file locks may prevent cleanup on some platforms, so ignore errors |
| Large file performance | Medium | Medium | Berkeley DB is the bottleneck; Python overhead is negligible |
| Subprocess hangs | Low | High | Set 5-minute timeout; kill subprocess on timeout |
| Manifest handle not found in DB | Low | Medium | Python script validates all handles before deletion; reports specific errors |

## What We Keep

- **Cascade engine** (`crates/delete/src/cascade.rs`) — unchanged, well-tested
- **Delete manifest** types (`crates/delete/src/types.rs`) — kept, extended
- **Interactive review** (`crates/delete/src/review.rs`) — unchanged
- **Manifest save/load** (`crates/delete/src/manifest.rs`) — unchanged
- **CLI interface** (`crates/cli/src/commands/delete.rs`) — same args, same UX
- **Graph parser** (`crates/gramps-reader/`) — unchanged
- **Output crate** (`crates/output/`) — kept for the `generate` command

## What We Remove

- **Filter logic** from `GraphXmlWriter` — `with_filter`, `is_deleted`,
  `is_edge_deleted`, `validate_deletion_set`, and the `to_delete` field
- **XML assertion tests** — tests that verify XML structure of delete output
  (replaced by behavioral round-trip tests through Gramps)

## Success Criteria

1. Running `gramps-gen delete file.gramps --selections picks.json --yes`
   produces output that Gramps can import without errors
2. The deleted people/families/events/notes are not present in the output
3. The output is structurally equivalent to what Gramps would produce (same
   namespace, element/section ordering, DOCTYPE, encoding). Because the exporter
   emits non-deterministic header fields (creation date, DB name), assert
   *importability + semantic equality* of surviving data rather than byte/XML
   identity.
4. E2E test passes: delete → Gramps import → no errors
5. All existing cascade engine tests still pass
6. All existing non-delete-output tests still pass
7. `delete-tool.md` manifest example uses correct snake_case keys
8. `gramps_available()` guard function has a unit test; CI asserts it returns
   `true` (failing CI if Gramps is not installed)

---

> **Update (Gramps API shift):** The Python backend now creates a **persistent**
> Gramps DB (not a temp directory), deletes **only people** via
> `delete_person_from_database`, and retains the DB for user inspection. The
> `--db-dir` and `--no-retain-db` flags control the DB directory location and
> automatic cleanup. The backend also reports a `surviving` set — handles that
> still exist in the DB after deletion — for reconciliation with the Rust
> manifest. The manifest has been updated to v2 format with per-handle status
> tracking (`pending`, `deleted`, `kept`).
