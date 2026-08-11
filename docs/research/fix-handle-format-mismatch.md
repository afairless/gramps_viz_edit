# Fix Handle Format Mismatch Between gramps-gen and Python Delete Backend

## Summary

The `delete` command silently fails to delete anything from real Gramps
`.gramps` files because `scripts/delete_backend.py` contains a UUID v4 format
validator that rejects Gramps-native handles.  Additionally, gramps-gen's own
Rust generator uses UUID v4 handles, so generated files are inconsistent with
what Gramps Desktop produces.

This plan covers two coordinated changes:

1. **Remove the handle format validator** from `delete_backend.py` — rely on
   Gramps' own `has_*_handle()` existence checks instead.
2. **Switch the Rust generator** from `uuid::Uuid::new_v4()` to Gramps-native
   handle format — so generated `.gramps` files look authentic.

---

## Root Cause Analysis

### The bug

`scripts/delete_backend.py` line ~41 defines a UUID v4 regex:

```python
UUID_V4_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    re.IGNORECASE,
)
```

This regex requires the standard 36-character UUID format **with dashes**
(`xxxxxxxx-xxxx-4xxx-[89ab]xxx-xxxxxxxxxxxx`).  Real Gramps handles use a
completely different format — underscore-prefixed hex strings like
`_103e399ca54970ad6794623655bc` (28 chars total, no dashes).  Gramps'
`create_id()` generates `%08x%08x` (16 hex chars), and the underscore is
added during XML export.

Because every handle in a real Gramps file fails this regex, a cascade of
failures occurs:

1. **`_validate_handles()`** (line ~214): Every handle goes to the `rejected`
   list.  `valid` is empty.  The function is intended to abort-before-deletion
   when a handle is in `missing` (valid-format but absent from DB), but the
   UUID filter kills every handle before the existence check ever runs.

2. **`delete_items()`** (line ~270): The delete loop iterates over
   `valid.get("people", [])` — which is empty.  **Zero people are deleted.**

3. **`all_manifest_handles`** (line ~277): Built by filtering through
   `UUID_V4_RE.match(handle)` — all real handles are skipped.  The list is
   empty.

4. **`surviving`** (line ~303): Computed from the empty `all_manifest_handles`
   — also empty.

5. **Rust reconciliation** (`crates/delete/src/reconcile.rs`): Sees empty
   `surviving` and marks every manifest entry as `"deleted"` — even though
   nothing was actually removed.

6. **Output `.gramps`** is byte-for-byte identical to the input (modulo
   formatting differences from Gramps' XML writer).

### Why the validator exists

The validator was added as a safety guard during the initial Python backend
implementation ([`gramps-python-delete-backend.md`](./gramps-python-delete-backend.md)),
under the assumption that gramps-gen always generates UUID v4 handles.  The
design document states:

> **Handle validation:** Before any database operation, validate that every
> handle in the manifest matches the expected UUID v4 format.

This assumption was wrong for two reasons:

1. Real Gramps files use Gramps-native handle format, not UUID v4.
2. Gramps' own `has_*_handle()` methods already validate handle existence —
   calling `db.has_person_handle("garbage")` returns `False` without side
   effects.  The regex adds no safety.

---

## Design

### Part A — Remove handle format validation from delete_backend.py

**Principle:** Gramps' `has_*_handle()` methods are the authoritative existence
check.  A separate format regex only creates a mismatch risk with zero safety
benefit.

Changes:

| Location | Current | New |
|----------|---------|-----|
| Module-level `UUID_V4_RE` | Regex that requires dashed UUID v4 | **Removed** |
| `_validate_handles()` | Rejects non-UUID handles; populates `rejected` list | Only checks `has_*_handle(handle)` existence; populates `valid`; aborts on `missing` |
| `delete_items()` → `all_manifest_handles` | Filters through `UUID_V4_RE.match(handle)` | Includes ALL handles unconditionally |
| `PythonResult.rejected` field | Populated with UUID-rejected handles | Always empty (kept for backward compat) |

The `rejected` field in the JSON result and the Rust `PythonResult` struct
are **kept** but will always be empty after this change.  They can be removed
in a follow-up cleanup if desired.

### Part B — Switch Rust generator to Gramps-native handles

**Principle:** Generated `.gramps` files should use the same handle format
Gramps Desktop uses, so they are indistinguishable from real Gramps output.

Gramps' `create_id()` in `gramps/gen/utils/id.py`:

```python
def create_id():
    global _rand
    if _det_id:
        _rand = _rand + 1
        return "%08x%08x" % (_rand, _rand)
    else:
        return "%08x%08x" % (int(time.time()*10000),
                             _rand.randint(0, sys.maxsize))
```

This produces a 16-character hex string (no underscore — the underscore is
added during XML serialization by Gramps' exporter).

**Target format:** `_` + 16 hex chars = 17 chars total.  Example:
`_103f72212ad34087`.  The underscore is part of the stored handle so our XML
writer emits handles that match Gramps' output.

```rust
/// Generate a Gramps-compatible handle: `_` + 16 hex chars.
///
/// The 16 hex chars are two `%08x`-formatted u32 values:
/// - First 8: timestamp-derived (lower 32 bits of unix centiseconds)
/// - Second 8: random u32
pub fn generate_handle(rng: &mut impl Rng) -> Handle {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let random: u32 = rng.gen();
    format!("_{:08x}{:08x}", (ts / 10) as u32, random)
}
```

**Add to:** `crates/typed-graph/src/generate/mod.rs` as a public free function
so all generate submodules can use it.

**Replace all `uuid::Uuid::new_v4().to_string()` calls** in:

| File | Approx. locations |
|------|-------------------|
| `crates/typed-graph/src/generate/random.rs` | ~20 call sites |
| `crates/typed-graph/src/generate/densify.rs` | 2 call sites |
| `crates/typed-graph/src/generate/adversarial.rs` | ~7 call sites |
| `crates/typed-graph/src/generate/builder.rs` | 5 call sites (auto-handle methods) |

The builder's `add_person_auto`, `add_family_auto`, etc. currently document
"UUID v4 handle" — update docs to say "Gramps-compatible handle."

**Remove `uuid` dependency.**  `uuid` is used only in `typed-graph/src/generate/`
and only for `uuid::Uuid::new_v4().to_string()` — handle generation. No other
crate or module uses it.  After all ~33 call sites are migrated, remove the
dependency from both locations:

1. `crates/typed-graph/Cargo.toml`: remove `uuid = { workspace = true }`
2. Root `Cargo.toml`: remove `uuid = { version = "1", features = ["v4"] }` from
   `[workspace.dependencies]`

Run `cargo update` afterward to prune the lock file.

---

## Step-by-Step Implementation

### Step 1 — Add `generate_handle()` to typed-graph

**File:** `crates/typed-graph/src/generate/mod.rs`

Add a public function:

```rust
use rand::Rng;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Generate a Gramps-compatible handle: underscore + 16 hex chars.
///
/// Matches the format produced by Gramps' `create_id()`:
/// `_` + `%08x%08x` (timestamp_part, random_part).
///
/// The `u128 → u64 → u32` truncation chain on the timestamp is intentional:
/// it mirrors Gramps' `create_id()` which formats `time.time()*10000` (a
/// float whose integer part fits in ~45 bits) with `%08x`, taking the lower
/// 32 bits. This matches the same modulo-32-bit wrapping behavior.
pub fn generate_handle(rng: &mut impl Rng) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;  // u128 → u64 truncation
    let random: u32 = rng.gen();
    // u64 → u32 truncation via `as u32` matches Gramps' %08x wrapping
    format!("_{:08x}{:08x}", (ts / 10) as u32, random)
}
```

Add unit tests:

- `test_generate_handle_format`: assert output matches `^_[0-9a-f]{16}$`
- `test_generate_handle_unique`: generate 10,000 handles, assert all unique
- `test_generate_handle_has_underscore_prefix`: assert `handle.starts_with('_')`

### Step 2 — Migrate generator code to `generate_handle()`

Replace every `uuid::Uuid::new_v4().to_string()` call in the generation
code with `generate_handle(rng)`.

**Files to change:**

1. `crates/typed-graph/src/generate/random.rs` — ~20 occurrences
2. `crates/typed-graph/src/generate/densify.rs` — 2 occurrences
3. `crates/typed-graph/src/generate/adversarial.rs` — ~7 occurrences
4. `crates/typed-graph/src/generate/builder.rs` — 4 occurrences (auto-handle methods)

**Pattern for random.rs / densify.rs / adversarial.rs** (these already have
`rng: &mut impl Rng` in scope):

```rust
// Before
let handle = uuid::Uuid::new_v4().to_string();
// After
let handle = generate_handle(rng);
```

**Pattern for builder.rs** (builder doesn't have an RNG — it currently uses
`uuid::Uuid::new_v4()` which is self-contained):

The builder's auto-handle methods need either:

- An RNG parameter added, OR
- Use `rand::thread_rng()` internally

Prefer `rand::thread_rng()` for the builder since it avoids changing the
public API (5 call sites, one per auto-handle method: `add_person_auto`,
`add_family_auto`, `add_event_auto`, `add_place_auto`, `add_source_auto`):

```rust
// Before
let handle = uuid::Uuid::new_v4().to_string();
// After
let handle = generate_handle(&mut rand::thread_rng());
```

Update doc comments from "auto-generated UUID v4 handle" to "auto-generated
Gramps-compatible handle."

### Step 3 — Pre-implementation investigation: find all test assertions on handle format

Before editing any test code, enumerate every test function that references
handle format, with file:line, to ensure nothing is missed:

```bash
# All test references to uuid or handle format
grep -rn 'uuid\|UUID\|new_v4\|handle.*len\|handle.*format' \
  crates/typed-graph/src/generate/ --include='*rs' | grep -i test

# Integration tests
grep -rn 'uuid\|UUID\|[0-9a-f]\{8\}' crates/typed-graph/tests/
```

Likely affected:

- Tests in `random.rs` that construct handles with `uuid::Uuid::new_v4()` or
  assert `handle.len()` or UUID format — these need to use `generate_handle(rng)`
- Tests in `builder.rs` that check auto-generated handle properties — update
  format assertions from UUID pattern to `^_[0-9a-f]{16}$`
- Integration tests in `crates/typed-graph/tests/merged_schema.rs` and
  `crates/typed-graph/tests/schema_convert_tests.rs` if they reference handles

### Step 4 — Remove UUID v4 validator from delete_backend.py

**File:** `scripts/delete_backend.py`

**4a. Remove `UUID_V4_RE`:**

Delete lines ~39-44:

```python
# Remove this entire block
UUID_V4_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    re.IGNORECASE,
)
```

Also remove the `import re` if it's only used for `UUID_V4_RE` (check for
other uses — `read_xmlns_from_input` also uses `re`, so keep the import).

**4b. Simplify `_validate_handles()`:**

Remove the UUID check; only keep the existence check:

```python
def _validate_handles(
    db: Any,
    manifest: Dict[str, Any],
) -> tuple[Dict[str, List[str]], List[str]]:
    """Validate all handles in the manifest.

    Returns (valid_handles, rejected):
    - valid_handles: {type_key: [handle, ...]} for handles that exist in DB.
    - rejected: always empty (kept for backward compat with Rust side).

    Raises ValueError if any handle is absent from the DB.
    """
    plan: Dict[str, Any] = manifest.get("plan", {})
    rejected: List[str] = []  # Always empty — no format filter
    valid: Dict[str, List[str]] = {}
    missing: List[str] = []

    for type_key in _DELETION_ORDER:
        type_plan = plan.get(type_key)
        if type_plan is None:
            continue
        to_delete: List[Any] = type_plan.get("to_delete", [])
        if not to_delete:
            continue

        type_valid: List[str] = []
        ops = _TYPE_OPS.get(type_key)
        if ops is None:
            continue
        has_fn = getattr(db, ops["has"])

        for entry in to_delete:
            handle = _extract_handle(entry)
            if not has_fn(handle):
                missing.append(handle)
                continue
            type_valid.append(handle)

        if type_valid:
            valid[type_key] = type_valid

    if missing:
        raise ValueError(
            f"{len(missing)} handle(s) in manifest not found in database "
            f"(first 5): {missing[:5]}"
        )

    return valid, rejected
```

Key changes:

- Drop the `UUID_V4_RE.match(handle)` block
- Handles that don't exist go to `missing` (raises before any deletion)
- `rejected` is always empty

**4c. Fix `all_manifest_handles` in `delete_items()`:**

The current code:

```python
for entry in to_delete:
    handle = _extract_handle(entry)
    if UUID_V4_RE.match(handle):
        all_manifest_handles.append(handle)
```

Replace with:

```python
for entry in to_delete:
    handle = _extract_handle(entry)
    all_manifest_handles.append(handle)
```

All manifest handles are now included in the surviving check — no format
filter.

**4d. Update documentation in the delete_items docstring:**

Remove the mention of UUID v4 from the Returns description:

```python
# Before
# - rejected: handles with invalid UUID v4 format (skipped).
# After
# - rejected: always empty (kept for backward compat).
```

### Step 5 — Update Python tests

**File:** `scripts/test_delete_backend.py`

Check for tests that reference the UUID v4 validator:

```bash
grep -n 'UUID\|uuid_v4\|rejected\|UUID_V4' scripts/test_delete_backend.py
```

**Remove the `TestUuidV4Regex` class entirely** — it directly tests
`UUID_V4_RE.match()`, which no longer exists. The three methods
(`test_valid_uuid_v4`, `test_invalid_uuid_not_v4`, `test_invalid_uuid_wrong_format`)
are dead code after `UUID_V4_RE` is deleted.

Update any remaining tests that:

- Assert that non-UUID handles are rejected → change to assert they are
  accepted (if they exist in the test DB) or raise ValueError (if absent)
- Reference `UUID_V4_RE` → remove

**Add a second Gramps-native XML fixture** for testing. The test DB cannot
have handles inserted programmatically — they come from XML import. Create a
`GRAMPS_NATIVE_XML` fixture with Gramps-native handles (e.g.,
`_103f72212ad34087`) alongside the existing `MINIMAL_GRAMPS_XML` UUID fixture.
Import it into a separate test DB for the format-acceptance test.

Add new tests:

- `test_handles_accepted_regardless_of_format`: verify that handles in
  Gramps-native format (`_103f72212ad34087`) are accepted. Uses the
  Gramps-native XML fixture with a separate DB import.
- `test_missing_handle_raises_before_deletion`: verify ValueError is raised
  when a handle from the manifest doesn't exist in the DB

### Step 6 — Rust `PythonResult` struct

**File:** `crates/cli/src/commands/delete.rs`

The `rejected` field stays in the struct (Python side still emits it, always
empty).  No Rust-side changes needed.  Verify the `serde(default)` attribute
is present so the field gracefully handles omission if we remove it from
Python output later.

### Step 7 — Verify the full pipeline

1. Generate a `.gramps` file with the new handle format:

   ```bash
   cargo run -- generate -n 20 -o /tmp/test-new.gramps
   # Verify handles use _XXXXXXXXXXXX format
   grep -oP 'handle="[^"]*"' /tmp/test-new.gramps | head -5
   ```

2. Run delete on the generated file:

   ```bash
   # Create selections from a few handles
   cargo run -- delete /tmp/test-new.gramps \
     -s /tmp/selections.json -o /tmp/test-cleaned.gramps
   ```

3. Run delete on a real Gramps file (the user's original scenario):

   ```bash
   cargo run -- delete ~/Documents/gramps01/gramps-ui-gen02.gramps \
     -s ~/Documents/gramps01/selections2.json \
     -o ~/Documents/gramps01/test-fixed.gramps
   # Verify: output should have FEWER people than input
   ```

4. Check that the manifest correctly reflects what was actually deleted
   (people should be `deleted`, non-people types should be `pending` since
   the Python backend only deletes people).

### Step 8 — Update documentation

| Document | Change |
|----------|--------|
| `AGENTS.md` | Note the handle format change in the generate pipeline description |
| `docs/research/gramps-python-delete-backend.md` | Add a note that the UUID v4 validator was removed (this plan) |
| `docs/ARCHITECTURE.md` | Update handle format description if mentioned |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| Handle collision with `generate_handle()` | Very low | Medium — duplicate handle crashes graph insertion | 64 bits of entropy (16 hex chars); collision probability is < 10⁻¹⁰ for any realistic generation run. Existing duplicate-handle checks in Graph catch collisions. |
| Existing `.gramps` files with UUID v4 handles become un-deletable | None | None | The Python backend no longer validates handle format at all — UUID v4 handles work fine because `has_*_handle()` just checks existence. |
| `uuid` crate no longer needed | None | None | `uuid` is only used for handle generation. After migration it is fully removed from both `typed-graph/Cargo.toml` and the workspace `Cargo.toml`. |
| Builder API change | None | None | Using `rand::thread_rng()` internally preserves the existing `add_*_auto()` signatures. |
| `re` import removal | Low | Low | `read_xmlns_from_input` still uses `re` — do not remove the import. |

---

## Success Criteria

1. `delete_backend.py` accepts Gramps-native handles (`_103f72212ad34087`)
2. `delete_backend.py` accepts UUID v4 handles (`a1b2c3d4-e5f6-4789-ab01-cdef01234567`)
3. Running `delete` on a real Gramps file actually removes people from the output
4. The manifest's reconciliation correctly marks people as `deleted` and
   non-people types as `pending` (since only people are deleted by Gramps)
5. Generated `.gramps` files use `_XXXXXXXXXXXX` format handles
6. All existing tests pass (updated where needed for new handle format)
7. `cargo clippy --all-targets --all-features -- -D warnings` passes
