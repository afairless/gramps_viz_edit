# Fix Plan: Handle Underscore Mismatch Between Rust and Gramps DB

## Problem Statement

The `delete` command fails with:

```
Error: ConfigError("7 handle(s) in manifest not found in database (first 5):
['_103e398e0c429cd558cc4da8e35', '_103e39ab2114249d489d0884174', ...]")
```

Gramps stores handles internally **without** a leading underscore. The Rust
XML parser reads handles verbatim from XML attributes (which include `_`),
placing underscored handles in the deletion manifest. When the Python backend
queries `db.has_person_handle("_103e39...")`, it returns `False` because the
DB key is `103e39...` (no underscore).

### Root Cause Confirmed

Gramps' `importxml.importData()` strips the `_` prefix during import.
Verified experimentally with a test import:

```
XML handle:   _103e398e0c429cd558cc4da8e35
DB handle:    103e398e0c429cd558cc4da8e35
has_person_handle("_103e..."):  False
has_person_handle("103e..."):   True
```

Both short handles (14 hex chars) and long handles (26 hex chars) from
Gramps-native exports exhibit this behavior. The `_` prefix is an XML
serialization convention — Gramps adds it on export, strips it on import.

### Affected Code Path

```
.gramps file ──┬──> Rust parser (quick-xml) ──> handles with "_" ──> manifest
               │
               └──> Gramps importxml ──> DB keys without "_"
                                            │
                    Python _validate_handles ──> MISS: "_" prefix mismatch
```

## Fix Strategy

Fix entirely in the Python backend (`scripts/delete_backend.py`) by
normalizing handles before DB queries. The manifest preserves the original
XML format (with `_`) for audit purposes. Normalization is transparent and
only applied at DB interaction points.

### Why Python-side only

- The `_` prefix is a Gramps XML/DB boundary concern — the Python backend is
  the only component that interacts with Gramps' DB API.
- Changing handle format on the Rust side would ripple through the graph,
  cascade, manifest builder, selections parser, and visualizer — a much
  larger change with higher risk.
- The manifest is saved as an audit artifact — keeping the `_` prefix makes
  it directly comparable to XML handles.

## Implementation Steps

### Step 1: Add `_normalize_handle()` to `delete_backend.py`

Add a helper that strips the leading underscore for Gramps DB compatibility:

```python
def _normalize_handle(handle: str) -> str:
    """Strip leading underscore for Gramps DB compatibility.

    Gramps XML serialization prefixes handles with '_' (e.g. _a1b2c3...).
    Gramps' internal Berkeley DB stores handles without this prefix.
    This function normalizes a handle from XML format to DB format.
    """
    return handle.lstrip('_')
```

Placement: after `_extract_handle()`, before `_validate_handles()`.

### Step 2: Normalize handles in `_validate_handles()`

Change `_validate_handles()` so DB lookups use normalized handles:

```python
for entry in to_delete:
    handle = _extract_handle(entry)
    if not has_fn(_normalize_handle(handle)):  # <-- normalize here
        missing.append(handle)
        continue
    type_valid.append(handle)
```

The `missing` list still contains original (underscored) handles — these
appear in the error message, matching the manifest content.

### Step 3: Normalize handles in `delete_items()`

Two places need normalization in `delete_items()`:

**3a. Person deletion** — normalize before `get_fn()`:

```python
for handle in people_handles:
    person = get_fn(_normalize_handle(handle))  # <-- normalize here
    delete_fn(person, trans)
    deleted_count += 1
```

**3b. Surviving handles check** — normalize before `has_fn()`, but report
original handles:

```python
for handle in all_manifest_handles:
    for type_key in _DELETION_ORDER:
        ops = _TYPE_OPS.get(type_key)
        if ops is None:
            continue
        has_fn = getattr(db, ops["has"])
        if has_fn(_normalize_handle(handle)):  # <-- normalize here
            surviving.append(handle)            # original handle in report
            break
```

The `surviving` list keeps original underscored handles so the Rust
reconciliation can match them against manifest entries.

### Step 4: Add Python unit tests

Add to `scripts/test_delete_backend.py`:

**4a. `_normalize_handle` unit tests:**

```python
class TestNormalizeHandle:
    def test_strips_single_underscore(self):
        assert _normalize_handle("_abc123") == "abc123"

    def test_preserves_already_normalized(self):
        assert _normalize_handle("abc123") == "abc123"

    def test_handles_empty_string(self):
        assert _normalize_handle("") == ""

    def test_handles_multiple_underscores(self):
        assert _normalize_handle("__abc") == "abc"
```

**4b. Fix existing `test_handles_accepted_regardless_of_format` (currently
failing):**

This test constructs a manifest with `GRAMPS_HANDLE_A =
"_103f72212ad34087"` (with underscore) and expects `_validate_handles` to
accept it. It is **already failing** — confirmed via pytest:

    FAILED test_handles_accepted_regardless_of_format
    ValueError: 1 handle(s) in manifest not found in database (first 5):
    ['_103f72212ad34087']

After the fix, it will pass because `_validate_handles` will normalize the
handle before the DB query. No test logic change needed — the fix makes the
pre-existing test pass.

**4c. `_validate_handles` with Gramps-native handles:**

Add a test verifying that gramps-native handles (with `_` prefix) are
found after normalization:

```python
def test_gramps_native_handles_normalized(self, gramps_native_db):
    """Underscore-prefixed handles are found via normalization."""
    db, _, _ = gramps_native_db
    manifest = make_manifest(people=[GRAMPS_HANDLE_A])
    valid, rejected = _validate_handles(db, manifest)
    assert GRAMPS_HANDLE_A in valid.get("people", [])
    assert rejected == []
```

**4d. `delete_items` surviving with normalized handles:**

Add a test verifying that after deletion, the surviving report uses
original (underscored) handle format.

### Step 5: Integration verification

1. Run the failing user command again:

   ```
   target/release/gramps-gen delete ~/Documents/gramps01/gramps-ui-gen02.gramps \
     -s ~/Documents/gramps01/selections2.json \
     -o ~/Documents/gramps01/gen02-selections02-del_2.gramps
   ```

2. Run Python tests:

   ```bash
   pip install pytest
   pytest scripts/test_delete_backend.py -v
   ```

3. Run Rust tests:

   ```bash
   cargo test -p delete
   cargo test -p cli
   ```

### Step 6: Rust-side future consideration (deferred)

Optionally strip `_` from handles in the Rust XML parser (`gramps-reader`).
This would make all handles consistent with Gramps' internal format, but is
a larger change affecting the graph model, cascade engine, manifest builder,
selections, and visualizer. Deferred as out of scope for this fix.

## Risk Assessment

| Risk | Mitigation |
|---|---|
| `lstrip('_')` strips too aggressively | Gramps handles never contain internal underscores; `_` is only a prefix |
| Surviving report format changed | We append original handles (with `_`) to surviving, not normalized ones |
| Other commands affected | Only the `delete` command uses the Python backend; no other command path is affected |
| v1 manifest backward compat | `_extract_handle` unchanged; normalization only in DB-query functions |
| Re-introducing `_` in surviving | Surviving handles come from `all_manifest_handles` which uses `_extract_handle` (unchanged, returns with `_`). Only the DB query uses normalization. |

## Files Changed

| File | Change |
|---|---|
| `scripts/delete_backend.py` | Add `_normalize_handle()`, use in `_validate_handles` and `delete_items` |
| `scripts/test_delete_backend.py` | Add normalization tests, fix gramps-native handle tests |
