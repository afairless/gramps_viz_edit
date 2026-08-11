# Implementation Plan: Fix Handle Format Mismatch

Source: `docs/research/fix-handle-format-mismatch.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add generate_handle() to typed-graph` | `generate_handle()` function | `crates/typed-graph/src/generate/mod.rs` | Unit |
| 2 | `refactor: migrate random densify adversarial to generate_handle()` | Generator code migration | `crates/typed-graph/src/generate/random.rs`, `densify.rs`, `adversarial.rs` | Unit |
| 3 | `refactor: migrate builder auto-handle methods to generate_handle()` | Builder handle migration | `crates/typed-graph/src/generate/builder.rs` | Unit |
| 4 | `chore: remove uuid dependency from typed-graph and workspace` | Dependency cleanup | `Cargo.toml`, `crates/typed-graph/Cargo.toml` | — |
| 5 | `fix: remove UUID v4 format validator from delete_backend.py` | Python backend validator removal | `scripts/delete_backend.py` | Unit |
| 6 | `test: update Python tests for handle format changes` | Python test updates | `scripts/test_delete_backend.py` | Unit |
| 7 | `test: verify full pipeline end-to-end` | E2E verification | Manual verification commands | — |
| 8 | `docs: update handle format documentation` | Documentation update | `AGENTS.md`, `docs/research/gramps-python-delete-backend.md`, `docs/ARCHITECTURE.md` | — |

---

## Step 1 — Add `generate_handle()` to `generate/mod.rs`

**Files:** `crates/typed-graph/src/generate/mod.rs`

Add a public `generate_handle(rng: &mut impl Rng) -> String` function that produces Gramps-compatible handles (`_` + 16 hex chars). The format mirrors Gramps' `create_id()`: `_{:08x}{:08x}` (timestamp_part, random_part).

Add unit tests at the bottom of `mod.rs`:

- `test_generate_handle_format`: assert output matches `^_[0-9a-f]{16}$`
- `test_generate_handle_unique`: generate 10,000 handles, assert all unique
- `test_generate_handle_has_underscore_prefix`: assert `handle.starts_with('_')`

**Why separate:** `generate_handle()` is a pure, testable utility function. Adding it first means every subsequent migration step can immediately use it without needing to modify the same file.

---

## Step 2 — Migrate `random.rs`, `densify.rs`, `adversarial.rs` to `generate_handle(rng)`

**Files:**

- `crates/typed-graph/src/generate/random.rs` (~20 occurrences)
- `crates/typed-graph/src/generate/densify.rs` (2 occurrences)
- `crates/typed-graph/src/generate/adversarial.rs` (6 occurrences)

These files already have `rng: &mut impl Rng` in scope. Replace every `uuid::Uuid::new_v4().to_string()` with `generate_handle(rng)`.

**Why separate from builder.rs:** These three files share the same replacement pattern (use existing `rng` parameter). Builder.rs needs a different approach (`rand::thread_rng()`). Splitting them keeps each step's changes uniform and easy to review.

---

## Step 3 — Migrate `builder.rs` auto-handle methods to `generate_handle()`

**File:** `crates/typed-graph/src/generate/builder.rs` (5 occurrences)

The builder's auto-handle methods (`add_person_auto`, `add_family_auto`, `add_event_auto`, `add_place_auto`, `add_source_auto`) don't have an RNG parameter. Use `rand::thread_rng()` internally:

```rust
let handle = generate_handle(&mut rand::thread_rng());
```

Update doc comments from "auto-generated UUID v4 handle" to "auto-generated Gramps-compatible handle."

---

## Step 4 — Remove `uuid` dependency

**Files:** `Cargo.toml` (workspace root), `crates/typed-graph/Cargo.toml`

After all `uuid::Uuid::new_v4()` calls are gone:

1. Remove `uuid = { workspace = true }` from `crates/typed-graph/Cargo.toml`
2. Remove `uuid = { version = "1", features = ["v4"] }` from workspace `[workspace.dependencies]` in root `Cargo.toml`
3. Run `cargo update` to prune the lock file
4. Run `cargo build` to confirm no missing dependencies

---

## Step 5 — Remove UUID v4 validator from `delete_backend.py`

**File:** `scripts/delete_backend.py`

Three changes:

1. **Remove `UUID_V4_RE`** constant (lines 39-44). Keep `import re` (still used by `read_xmlns_from_input`).
2. **Simplify `_validate_handles()`**: Remove the `UUID_V4_RE.match(handle)` block. Only check `has_*_handle(handle)` existence. Handles absent from the DB raise `ValueError`. `rejected` is always empty.
3. **Fix `all_manifest_handles`** in `delete_items()`: Remove the `if UUID_V4_RE.match(handle)` guard — include all handles unconditionally.

---

## Step 6 — Update Python tests

**File:** `scripts/test_delete_backend.py`

1. **Remove `TestUuidValidation` class entirely** — it directly tests `UUID_V4_RE.match()`, which no longer exists.
2. **Update `test_invalid_handle_rejected`** — non-existent handles now raise `ValueError` (they go to `missing`, not `rejected`). Change to assert `ValueError` is raised.
3. **Update `test_all_invalid_handles_rejected`** — same as above; change to assert `ValueError` is raised.
4. **Add Gramps-native XML fixture** — a `GRAMPS_NATIVE_XML` constant with Gramps-native handles (e.g., `_103f72212ad34087`).
5. **Add `gramps_native_db` fixture** — imports the Gramps-native XML into a test DB.
6. **Add `test_handles_accepted_regardless_of_format`** — verify that Gramps-native handles `_103f72212ad34087` are accepted by the validator.
7. **Add `test_missing_handle_raises_before_deletion`** — verify `ValueError` when a handle from the manifest doesn't exist in the DB.

---

## Step 7 — Verify the full pipeline

Manual verification commands:

```bash
# 1. Generate a .gramps file with new handle format
cargo run -- generate -n 20 -o /tmp/test-new.gramps
# Verify handles use _XXXXXXXXXXXX format
grep -oP 'handle="[^"]*"' /tmp/test-new.gramps | head -5

# 2. Run delete on the generated file
cargo run -- delete /tmp/test-new.gramps \
  -s /tmp/selections.json -o /tmp/test-cleaned.gramps

# 3. Run delete on a real Gramps file
cargo run -- delete ~/Documents/gramps01/gramps-ui-gen02.gramps \
  -s ~/Documents/gramps01/selections2.json \
  -o ~/Documents/gramps01/test-fixed.gramps
# Verify: output should have fewer people than input

# 4. Check manifest reflects correct deletion status
```

---

## Step 8 — Update documentation

| Document | Change |
|---|---|
| `AGENTS.md` | Note the handle format change in the generate pipeline description |
| `docs/research/gramps-python-delete-backend.md` | Add a note that the UUID v4 validator was removed (this plan) |
| `docs/ARCHITECTURE.md` | Update handle format description if mentioned |
