# Implementation Plan: Gramps Python Delete Backend

Source: [`docs/research/gramps-python-delete-backend.md`](docs/research/gramps-python-delete-backend.md)

**Problem**: The `delete` command serializes its filtered output with our own
Rust `GraphXmlWriter`. Every divergence between our XML output and Gramps'
expected format is a potential crash, and that attack surface spans the entire
Gramps XML schema (namespace mismatches, date truncation, missing attributes,
element ordering). Prior round-trip fixes are defense-in-depth patches, not a
structural remedy.

**Strategy**: Keep the well-tested Rust cascade engine (it computes the
deletion set), but delegate all XML I/O for `delete` to Gramps' own
import/delete/export libraries via a Python subprocess. Input → Rust cascade →
JSON manifest → Python (Gramps DB import, dependency-ordered delete, Gramps
export). Removes the filter path from `GraphXmlWriter`; the full serializer
stays for `generate`.

> The previous delete-output-round-trip plan (committed `0ffa5a1..0ac495d`) is
> complete and stays on `main`; `gramps_xml_version`/`namespace_to_version`
> are retained because `generate` also uses them. This is a fresh feature plan.

## Verified facts (audited against the tree)

- `build_manifest(&source, selections, &seed, &to_delete, &graph)` already
  groups a flat `&[Handle]` per type via `NodeKindLabel::plural()` →
  snake_case keys (`people`, `families`, …). No new grouping function needed.
- `delete.rs` already passes `final_to_delete` (the reviewed set) to
  `build_manifest`, not the raw `DeletePlan`. The manifest is correctly
  built from the reviewed set.
- `GraphXmlWriter` filter surface (`with_filter`, `is_deleted`,
  `is_edge_deleted`, `validate_deletion_set`, `to_delete` field) is referenced
  **only** by `crates/cli/src/commands/delete.rs`; no `xml.rs` unit test
  exercises it, and `integration.rs`/other tests use `GraphXmlWriter::new`
  without a filter. So filter removal touches no other unit tests.
- Gramps **5.1.6** is installed locally (`/usr/bin/gramps`, `gramps.gen`
  importable) and `import gramps.gen.db.utils` succeeds → the Step-1 API spike
  and the Gramps round-trip E2E are runnable on this machine.
- The project's existing Python test uses stdlib `unittest`
  (`extract/test_extractor.py`), but `pytest` is **not** currently installed.
  Per user decision, the backend's Step-8 tests will use **pytest**; add a
  `pytest` dev dependency for `scripts/`. This introduces a new Python dev
toolchain the existing extract tests do not rely on.
- No `.github/` directory exists → the CI step (Step 7 of the source doc) is
  **deferred**, exactly as the source doc already states.

## Impact summary

| Commit | Component | Change |
|---|---|---|
| 1 | `scripts/delete_backend.py` | temp DB init spike (make_database contract, backend/version selection), gzip input, import |
| 2 | `scripts/delete_backend.py` | dependency-ordered deletion, handle validation, `rejected` dead-letter |
| 3 | `scripts/delete_backend.py` | version-from-xmlns, export, atomic rename, `main`, JSON result |
| 4 | `crates/delete/tests/`, `types.rs` | pin shared manifest-contract fixture; roundtrip test |
| 5 | `crates/cli/src/commands/delete.rs` | build reviewed manifest + delegate to Python backend; drop gzip/write_gzip_output |
| 6 | `crates/output/src/xml.rs` | remove `with_filter`/`is_deleted`/`is_edge_deleted`/`to_delete`/`validate_deletion_set` |
| 7 | `crates/cli/tests/e2e.rs`, `integration.rs` | rework/remove obsolete delete-output XML assertions |
| 8 | `scripts/`, pytest | Python edge-case unit tests (pytest, fixtures) |
| 9 | `crates/cli/tests/e2e.rs` | Gramps round-trip E2E + `gramps_available()` guard test |
| 10 | `AGENTS.md`, docs | document Python-backed delete pipeline |
| 11 | — | CI gating (deferred — no `.github/` present) |

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat(delete): add Python backend temp DB init and XML import` | Backend DB/import spike | `scripts/delete_backend.py` (`create_temp_db`, `import_xml`, version/backend detection, gzip input) | Smoke, python unit |
| 2 | `feat(delete): add Python backend dependency-ordered deletion` | Deletion engine | `scripts/delete_backend.py` (`delete_items`, per-type `remove_*`, handle validation, `rejected`) | Python unit |
| 3 | `feat(delete): add Python backend export, atomic output, main` | Export + orchestration | `scripts/delete_backend.py` (`export_xml`, version-from-xmlns, atomic rename, `main`, JSON result) | Smoke, python unit |
| 4 | `test(delete): pin manifest contract with shared JSON fixture` | Manifest data contract | `crates/delete/tests/manifest_contract.rs`, `crates/delete/tests/fixtures/manifest-contract-v1.json`, `manifest_json_roundtrip` | Unit, integration, doc |
| 5 | `feat(delete): delegate delete XML I/O to Python backend` | CLI wiring | `crates/cli/src/commands/delete.rs` (interpreter resolution, temp manifest, subprocess, timeout, `PythonResult`, report; remove `write_gzip_output`/flate2) | Integration |
| 6 | `refactor(output): remove GraphXmlWriter filter/deletion methods` | Filter removal | `crates/output/src/xml.rs` | Unit |
| 7 | `test(delete): remove obsolete delete-output XML assertions` | Test rework | `crates/cli/tests/e2e.rs`, `crates/cli/tests/integration.rs` | Integration |
| 8 | `test(delete): Python backend edge-case unit tests` | Python edge cases | `scripts/test_delete_backend.py`, `scripts/pytest.ini` or config (pytest) | Python unit |
| 9 | `test(delete): Gramps round-trip E2E with import verification` | Behavioral E2E | `crates/cli/tests/e2e.rs` (`e2e_delete_gramps_python_roundtrip`, `gramps_available()`) | Integration (E2E), unit |
| 10 | `docs(delete): document Python-backed delete pipeline` | Docs | `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/delete-tool.md`, `docs/research/delete-output-roundtrip.md` | — |
| 11 | `chore(delete): gate E2E behind gramps_available and pin version` | CI guardrail | `.github/workflows/` (deferred — no dir present) | — |

## Step details

### Step 1 — Python backend: temp DB init + import (the API spike)

**`scripts/delete_backend.py`** — create the headless script shell proving the
Gramps init contract (validated against the locally installed **Gramps 5.1.6**):

- `gi.require_version('Gtk', '3.0')` up-front to suppress PyGIWarning.
- `make_database(backend_plugin_id)` returns an **uninitialized** DB (not a
  dir path). Set `db.dir = tmpdir`, then `db.write_version(VERSION)` before
  importing. Version/backend selection: 5.1/5.2 → `"bsddb"`; read
  `gramps.gen.const.VERSION` and pick accordingly. **Verify the exact init
  sequence here against the installed Gramps** — this is version-sensitive.
- `create_temp_db()` → temp dir + initialized DB (Berkeley DB).
- `import_xml(db, path)` → gzip-detect via `\x1f\x8b` magic bytes, rename to
  `.gz` so `importxml.importData` handles it; call
  `gramps.plugins.importer.importxml.importData(db, path, User())`.
- Temp-DB cleanup via `shutil.rmtree(path, ignore_errors=True)` in a
  `finally` (Berkeley DB holds locks on some platforms).

**Tests**: smoke (script imports without a window; a no-op import then export
succeeds). Minimal — this commit is primarily the spike.

### Step 2 — Python backend: dependency-ordered deletion + validation

**`scripts/delete_backend.py`** — add `delete_items(db, manifest)`:

- **Pre-deletion handle validation**: every manifest handle must match UUID
  v4 (`/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i`).
  Non-matching handles → `rejected` list (dead-letter), skipped. Matching
  handles absent from the DB → abort **before any deletion** (no partial
  writes).
- Delete strictly in this order inside one shared `DbTxn`
  (`gramps.gen.db.DbTxn`):
  1. People — `db.get_person_from_handle(h)` then
     `db.delete_person_from_database(person, trans)` (takes a **`Person`
     object**, not a handle; handles family ref cleanup)
  2. Families — `remove_family(h, trans)`
  3. Events — `remove_event(h, trans)`
  4. Notes — `remove_note(h, trans)`
  5. Places — `remove_place(h, trans)`
  6. Sources — `remove_source(h, trans)`
  7. Citations — `remove_citation(h, trans)`
  8. Repositories — `remove_repository(h, trans)`
  9. Media — `remove_media(h, trans)`
  10. Tags — `remove_tag(h, trans)`
- Passing one shared `DbTxn` to every call realizes the atomic-write goal.

**Tests**: Python unit — ordering is fixed; malformed handles land in
`rejected`; a well-formed-but-absent handle aborts before any write.

### Step 3 — Python backend: export, atomic output, main

**`scripts/delete_backend.py`**:

- Read `xmlns` from the input XML header to determine the Gramps XML version;
  pass it to the exporter so output matches input. Unrecognized namespace →
  fall back to `"5.2"` + warning on stderr.
- `export_xml(db, path)` → gzip if the output filename ends in `.gz` else
  uncompressed; write via `gramps.plugins.export.exportxml.export_data`.
- **Atomic output**: export to a temp file, then `os.replace` onto the final
  path (deletion is atomic via `DbTxn`; export happens after commit).
- `main()` → parse args (`--input`, `--manifest`, `--output`), orchestrate,
  print JSON to stdout (see Step 5's `PythonResult`), errors via stderr +
  non-zero exit.

**Tests**: smoke — empty manifest → no-op import/export succeeds; result JSON
parses into the `PythonResult` shape.

### Step 4 — Pin the manifest data contract with a shared fixture

The JSON shape Rust emits and Python consumes must be pinned so the two sides
cannot silently drift (data-contracts skill).

**`crates/delete/tests/manifest_contract.rs`** — serialize a sample
`DeleteManifest` built from a test helper and assert the exact JSON shape: top
keys (`version`, `source_file`, `created_at`, `seed_people`, `plan`) and the
per-type `to_delete`/`kept` nesting under snake_case keys (`people`,
`families`, `events`, `notes`, `places`, `sources`, `citations`,
`repositories`, `media`, `tags`).

**`crates/delete/tests/fixtures/manifest-contract-v1.json`** — a minimal valid
manifest with one entry per type, committed so both Rust and Python consume
the same fixture. A format change on either side fails loudly.

**`crates/delete/src/types.rs`** — add `manifest_json_roundtrip` unit test
(serialize → deserialize → equal `DeleteManifest`).

### Step 5 — Delegate delete XML I/O to the Python backend

**`crates/cli/src/commands/delete.rs`** — replace the `GraphXmlWriter` write
step with a subprocess call passing a manifest built from the **reviewed set**
`final_to_delete`:

1. **Always serialize the manifest** to a temp JSON file (not just when
   `--save-manifest`) — Python needs it. Reuse the existing
   `build_manifest(..., &seed_vec, &delete_vec, &graph)`.
2. **Locate the Python script**: embedded at build time via
   `concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/delete_backend.py")`;
   honour a `GRAMPS_DELETE_BACKEND` env override for development.
3. **Resolve the interpreter deterministically**:
   1. `GRAMPS_PYTHON` env var (use as-is);
   2. probe `python3 -c "import gramps.gen.db.utils"` → use `python3` if exit 0;
   3. else error: "Gramps Python libraries not found. Set GRAMPS_PYTHON env var
      to the python interpreter that can import gramps."
4. **Spawn**: `python3 <script> --input <in.gramps> --manifest <plan.json> --output <out.gramps>`.
   Generous timeout (e.g. 5 min) — Berkeley DB is slow on large files; kill on
   timeout.
5. **Parse stdout** JSON into `PythonResult { status, output?, deleted?,
   message?, rejected }`; map to success or `CliError`.
6. **Remove dead gzip paths**: `write_gzip_output`, `is_gzip`, the `flate2`
   usage in `delete.rs`, and the `output::GraphXmlWriter`/`SerializationMap`
   import — Python controls gzip via the output filename. `--dry-run` must not
   invoke the subprocess; `--save-manifest`/`--load-manifest` UX unchanged.
   Temp dir cleaned up on exit.

**Tests**: integration — dry-run makes no subprocess call; success path parses
`PythonResult`; error/output paths map to `CliError`.

### Step 6 — Remove the GraphXmlWriter filter/serialization path

**`crates/output/src/xml.rs`** — remove together, as one atomic change (they
reference each other):

- `with_filter` (pub), `is_deleted` (private), `is_edge_deleted` (private),
  `validate_deletion_set` (pub), and the `to_delete` field from
  `GraphXmlWriter`. Its uses are at `write_section` (~line 271 filter) and
  `write_edge_items` (~line 776 filter); remove those references too.
- **Keep** `gramps_xml_version` and `with_namespace` — used by `generate`.
- `crates/output/src/lib.rs` — no change (public API changes are only the
  removed methods).

**Tests**: existing `xml.rs` writer unit tests (incl. `gramps_xml_version`
derivation at lines ~2483/2487) still pass.

### Step 7 — Rework/remove obsolete delete-output XML assertions

The four E2E tests asserting our writer's XML for delete output no longer
apply now that Gramps produces the output (Gramps re-normalizes; our
`type="General"` default and flat 5.1 event types are emitted by Gramps itself
only where it preserves the input). A format change on either side of the
contract is caught by Step 4's fixture + Step 9's import test, not by string
matching.

**`crates/cli/tests/e2e.rs`** — rework or replace:

- `e2e_delete_notes_import_roundtrip` (line 1266)
- `e2e_delete_51_namespace_emits_flat_event_type` (line 1396)
- `e2e_delete_51_fixture_import_roundtrip` (line 1473)
- `e2e_delete_note_default_type_and_format_roundtrip` (line 1578)

Any surviving behavior (input imports cleanly, deleted handles absent) moves
into Step 9's Gramps-backed round-trip test. Remove the standalone
`GraphXmlWriter`/filter usage if present.

**Tests**: integration.

### Step 8 — Python backend edge-case unit tests

**`scripts/test_delete_backend.py`** (pytest, per user decision; add `pytest`
as a Python dev dependency — e.g. a `scripts/` `requirements-dev.txt` with
`pytest`, since it is not in the stdlib and not yet installed):

- Empty manifest and all-empty per-type lists → no-op import/export succeeds.
- Manifest handle absent from the DB → fails with a specific error **before**
  deleting anything.
- Partial presence (some handles exist, one doesn't) → nothing written.
- Invalid handle format (not UUID v4) → handle in `rejected`, valid handles
  still deleted.
- Subprocess timeout: spawn a mock Python script that sleeps indefinitely,
  assert the timeout fires and the subprocess is killed (run from the Rust
  side or a `unittest.mock` subprocess double).

**Tests**: python unit.

### Step 9 — Gramps round-trip E2E + guard

**`crates/cli/tests/e2e.rs`**:

- `e2e_delete_gramps_python_roundtrip`: real `.gramps` fixture →
  `gramps-gen delete --selections picks.json --yes -o clean.gramps` → import
  output via `gramps.plugins.importer.importxml` → assert deleted
  people/families/events/notes absent and no errors on stderr. (Limitation:
  test uses the same importer as the backend; optional secondary check via
  `gramps -y -q -i clean.gramps` when the CLI binary is present.)
- `gramps_available()` guard unit test: compiles, returns a `bool`, doesn't
  panic; asserts the Gramps version starts with `("5.1","5.2")` (rejects 6.x's
  bsddb removal). E2E tests skip gracefully when Gramps is unavailable.

**Tests**: integration (E2E), unit.

### Step 10 — Documentation

| Document | Change |
|---|---|
| `AGENTS.md` | Add `scripts/delete_backend.py` to project structure |
| `docs/ARCHITECTURE.md` | Update the delete pipeline diagram (output delegate = Python backend) |
| `docs/delete-tool.md` | Update pipeline diagram; fix manifest JSON example to snake_case keys (`"people"` not `"Person"` — verified the doc currently uses `"Person"` at line 231); note deletion uses Gramps' native libraries |
| `docs/research/delete-output-roundtrip.md` | Note that bugs 1–4 are obsoleted by this restructure |

### Step 11 — CI (deferred)

The delete command now requires a working Gramps install (Python libraries).
CI E2E must run with Gramps present and pinned to 5.1/5.2 (the API usage is
validated against 5.1.6; 6.x deviates). Deferred: no `.github/` directory
exists in this repo. Revisit when CI is introduced; the `gramps_available()`
version assert in Step 9 already enforces the environment contract.

## Dependencies / ordering notes

- Steps 1–4 are independent of `crates/cli` and `crates/output`; they build
  the backend and its contract.
- Step 5 (delegation) depends on Steps 1–3 (script exists) and Step 4
  (contract fixtures make drift visible).
- **Step 6 and Step 7 must land together or keep the tree green**: Step 6
  removes the filter surface, and Step 7 removes tests/`delete.rs` references
  to it. Because verification showed no `xml.rs` unit tests exercise the
  filter (only `delete.rs`), the compile/pass boundary is safe if Step 5
  already dropped `delete.rs`'s filter usage. Confirm the tree compiles and
  tests pass at each checkpoint.
- `docs/delete-tool.md` currently shows uppercase keys (`"Person"` at line
  231) — Step 10 must fix this to the actual snake_case output.

## Repository health / pre-work

Before implementing, satisfy the incremental-development pre-work gate:
`git status` clean, `cargo test --workspace` green, `git branch
--show-current` on `main`. Create a feature branch (e.g.
`agent/delete-python-backend`) per git-workflow before the first step commit.
`docs/research/gramps-python-delete-backend.md` is currently untracked and
must be committed with this plan in Step 10's/dedicated docs commit.

## Test commands

```bash
pytest scripts/                                  # Steps 1-3, 8 (pytest, after installing dev reqs)
cargo test -p delete                              # Step 4 (contract fixture + roundtrip)
cargo test -p cli                                 # Steps 5, 7, 9
cargo test -p output                              # Step 6
cargo test --workspace                            # full suite before each commit
cargo clippy --all-targets --all-features -- -D warnings   # final gate
python3 -c "import gramps.gen.db.utils"           # sanity check for local Gramps
```
