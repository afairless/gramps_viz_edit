# Implementation Plan: Handle Underscore Mismatch Fix

Source: `docs/research/handle-underscore-fix.md`

## Problem

Gramps XML serialization prefixes handles with `_` (e.g. `_103e398e0c42...`), but Gramps' internal Berkeley DB stores handles without the prefix. The Python delete backend (`delete_backend.py`) queries the DB using the underscored XML handles, which always returns `False` — causing the delete command to fail with "handle(s) in manifest not found in database".

## Fix Strategy

Add a `_normalize_handle()` helper that strips the leading `_` for DB queries, and use it in both `_validate_handles()` and `delete_items()`. The manifest preserves original XML handles for audit purposes; normalization is only applied at DB interaction points.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix: normalize handles before DB queries in delete backend` | Handle normalization in all DB query paths | `scripts/delete_backend.py` | Unit (existing tests pass, including `test_handles_accepted_regardless_of_format`) |
| 2 | `test: add unit tests for handle normalization` | Normalization unit tests | `scripts/test_delete_backend.py` | Unit |
| 3 | `chore: verify integration end-to-end` | Integration verification | Manual run of delete command + Python + Rust test suites | — |

## Step Details

### Step 1 — Add `_normalize_handle()` and normalize all DB query paths

**Changes to `scripts/delete_backend.py`:**

1. Add `_normalize_handle(handle: str) -> str` helper after `_extract_handle()`:
   - Strips leading `_` via `handle.lstrip('_')`
   - Preserves handles without `_` unchanged

2. In `_validate_handles()`: normalize handle before `has_fn()` call
   - `has_fn` is called with `_normalize_handle(handle)` instead of raw `handle`
   - The `missing` list retains original (underscored) handles for error messages

3. In `delete_items()`:
   - **3a**: Normalize handle before `get_fn()` in person deletion loop
   - **3b**: Normalize handle before `has_fn()` in surviving handles check loop
   - The `surviving` list retains original (underscored) handles for reconciliation

**Test evidence:** The existing test `test_handles_accepted_regardless_of_format` (which is currently failing) will pass after this step.

### Step 2 — Add unit tests for handle normalization

**Changes to `scripts/test_delete_backend.py`:**

1. Add `TestNormalizeHandle` class with:
   - `test_strips_single_underscore` — `_abc123` → `abc123`
   - `test_preserves_already_normalized` — `abc123` → `abc123`
   - `test_handles_empty_string` — `""` → `""`
   - `test_handles_multiple_underscores` — `__abc` → `abc`

2. Add `test_gramps_native_handles_normalized` to `TestValidateHandles`:
   - Gramps-native handles (with `_` prefix) are found after normalization

3. Add `test_gramps_native_delete_items` to `TestDeleteItems`:
   - Verify person deletion with Gramps-native handles works
   - Verify surviving report uses original (underscored) handles

### Step 3 — Integration verification

1. Run the Python test suite: `pytest scripts/test_delete_backend.py -v`
2. Run the Rust test suite: `cargo test -p delete && cargo test -p cli`
3. Run the original failing command to verify the fix
