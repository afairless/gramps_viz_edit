# Plan: `--output` Directory Semantics + DB Retention Default Flip

## Goal

1. Change `--output` from a filename to a **directory**. All output files are saved
   inside that directory with names derived from `<file-stem>` (the input file's
   stem). When `--output` is omitted, the default is the input file's own directory.
2. Flip the Gramps DB retention default: the Berkeley DB directory is **not saved**
   by default. A new `--retain-db` flag opts in to keeping it.

## Current State

```
gramps-gen delete data.gramps --selections picks.json -o cleaned.gramps
```

Produces (in cwd):

```
cleaned.gramps                     ← Gramps DB export (people deleted)
data-deleted_2.gramps              ← after event clean
data-deleted_3.gramps              ← after note clean
data-deleted_4.gramps              ← after place clean
cleaned.manifest.json              ← manifest
data-gramps-db/                    ← retained Gramps Berkeley DB (default: keep)
```

Problems:

- `-o` specifies a filename but the pipeline writes multiple files with names
  derived from the **input** file, creating a confusing mixture.
- `-o` file is only partially cleaned (no events/notes/places removed); the
  actual clean output is `-deleted_4.gramps` but nothing renames it back.
- DB retained by default leaks disk space and genealogy data.

## Desired State

```
gramps-gen delete data.gramps --selections picks.json -o /tmp/out/
```

Produces (all in `/tmp/out/`):

```
data-deleted-1.gramps              ← Gramps DB export (people deleted)
data-deleted-2.gramps              ← after event clean
data-deleted-3.gramps              ← after note clean
data-deleted-4.gramps              ← after place clean
data-deleted.manifest.json         ← manifest
```

Without `--retain-db`, the `data-gramps-db/` directory is removed.

```
gramps-gen delete data.gramps --selections picks.json
```

Produces (in input file's directory, e.g. `/home/user/`):

```
data-deleted-1.gramps
data-deleted-2.gramps
data-deleted-3.gramps
data-deleted-4.gramps
data-deleted.manifest.json
```

> **Final artifact:** `-deleted-4.gramps` is the finished, fully-cleaned output
> (people + orphaned events/notes/places removed). Stages `-deleted-1` through
> `-deleted-3` are intermediate checkpoints kept for debugging/audit. This is
> documented prominently in `docs/delete-tool.md` so users don't mistake
> `-deleted-1.gramps` (people-only deletion) for the final result.

## Design Decisions

| Question | Decision |
|---|---|
| Default dir when `--output` omitted | Input file's directory |
| Intermediate files | **Keep** all stages (deleted-1 through deleted-4) |
| Output naming | `<file-stem>-deleted-N.gramps` (hyphen-separated) |
| Manifest location | Same directory as output files |
| DB retention default | **Do not retain** by default; `--retain-db` opts in |

## Detailed Changes

### 1. `crates/cli/src/commands/delete.rs` — DeleteArgs struct

```diff
-    /// Output .gramps file (default: <input>-cleaned.gramps)
-    #[arg(short = 'o', long = "output")]
-    pub output: Option<PathBuf>,
+    /// Output directory (default: input file's directory).
+    /// All output files are named <file-stem>-deleted-N.gramps.
+    #[arg(short = 'o', long = "output")]
+    pub output_dir: Option<PathBuf>,
```

```diff
-    /// Clean up Gramps DB after successful export
-    #[arg(long = "no-retain-db")]
-    pub no_retain_db: bool,
+    /// Retain the Gramps Berkeley DB directory after export
+    #[arg(long = "retain-db")]
+    pub retain_db: bool,
```

### 2. `crates/cli/src/commands/delete.rs` — Path derivations

Replace three helper functions with unified naming:

```rust
/// Derive the `-deleted-1.gramps` path from the input stem in a directory.
fn deleted_1_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{}-deleted-1.gramps", stem))
}

fn deleted_2_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{}-deleted-2.gramps", stem))
}

fn deleted_3_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{}-deleted-3.gramps", stem))
}

fn deleted_4_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{}-deleted-4.gramps", stem))
}
```

Remove old functions: `derive_no_events_path`, `derive_deleted_3_path`, `derive_deleted_4_path`.

### 3. `crates/cli/src/commands/delete.rs` — `run()` function

Key changes in order:

1. **Resolve output directory**:

   ```rust
   let output_dir = args.output_dir.unwrap_or_else(|| {
       input_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
   });
   let stem = strip_gramps_extensions(input_path);
   // Do NOT use unwrap() here: input paths like "/" have no file_name and
   // would panic. Fall back to a default like the current code does.
   let stem = Path::new(&stem)
       .file_name()
       .map(|s| s.to_string_lossy().to_string())
       .unwrap_or_else(|| "output".to_string());
   fs::create_dir_all(&output_dir)?;
   ```

   Note: `create_dir_all` on a path that is an existing *file* (the old
   `--output <filename>` usage) fails with "File exists". Map that error to a
   clear `CliError::ConfigError` message telling the user `--output` now takes a
   directory.

   Note on `main.rs` (plan step 10): confirmed by inspection that `main.rs`
   contains no help text mentioning `--no-retain-db` or output-file semantics —
   this step is a **no-op**, no changes needed.

2. **Resolve output paths** (before Python backend invocation):

   ```rust
   let output_path = deleted_1_path(&output_dir, &stem);
   let d2_path = deleted_2_path(&output_dir, &stem);
   let d3_path = deleted_3_path(&output_dir, &stem);
   let d4_path = deleted_4_path(&output_dir, &stem);
   ```

3. **Manifest path**:

   ```rust
   let manifest_path = match args.save_manifest {
       Some(ref p) => p.clone(),
       None => output_dir.join(format!("{}-deleted.manifest.json", stem)),
   };
   ```

4. **DB directory default: temp dir, removed unless `--retain-db`**:

   ```rust
   // Unique per-run suffix: NEVER use a literal "XXXXXX" mkdtemp-looking
   // template — it is not substituted. PIDs are recycled, so two runs (or a
   // leftover dir from a crashed run) would collide and create_db() would
   // silently rmtree the stale directory. Use a timestamp or UUID instead.
   let db_dir = args.db_dir.clone().unwrap_or_else(|| {
       std::env::temp_dir().join(format!(
           "gramps-delete-{}-{}",
           std::process::id(),
           std::time::SystemTime::now()
               .duration_since(std::time::UNIX_EPOCH)
               .map(|d| d.as_nanos())
               .unwrap_or_default(),
       ))
   });
   // ... after Python backend succeeds ...
   if args.retain_db {
       log::info!("Gramps DB retained at: {}", db_dir.display());
   } else {
       if db_dir.exists() {
           fs::remove_dir_all(&db_dir).ok(); // best-effort
       }
   }
   ```

   **CRITICAL — the Python backend flag pass-through must be INVERTED.**
   `delete_backend.py` only understands `--no-retain-db` (default: keep). The
   current code passes `--no-retain-db` only when `args.no_retain_db` is set.
   After the default flip, the Rust side must pass `--no-retain-db` **unless**
   `--retain-db` was given:

   ```rust
   // Before:  if no_retain_db { cmd.arg("--no-retain-db"); }
   // After:   pass cleanup flag by default; skip it only when retaining
   if !args.retain_db {
       cmd.arg("--no-retain-db");
   }
   ```

   If this inversion is forgotten, the Python backend keeps its old
   retain-by-default behavior and the headline goal silently fails. The Python
   backend itself does **not** change (its `--no-retain-db` flag stays as-is);
   only the Rust call site inverts.

5. **Pipeline** — replace all old path references:
   - Python backend writes to `output_path` (deleted-1.gramps)
   - Event clean reads `output_path` → writes `d2_path`
   - Note clean reads `d2_path` or `output_path` → writes `d3_path`
   - Place clean reads `d3_path` or `d2_path` or `output_path` → writes `d4_path`

6. **Remove the DB directory size warning** — no longer needed since DB is
   removed by default. The `dir_size()` helper becomes dead code and must be
   **deleted too** (with its unit test), otherwise `cargo clippy -D warnings`
   (project convention) fails on the unused function.

7. **Module docstring + final log lines** — the module docstring at the top of
   `delete.rs` says *"The Gramps DB is retained for user inspection (unless
   `--no-retain-db` is specified)"*; flip it to reflect the new default. The
   final summary log in `run()` also flips:

   ```rust
   // Before:
   if args.no_retain_db {
       log::info!("Gramps DB removed (--no-retain-db).");
   } else {
       log::info!("Gramps DB retained at: {}", db_dir.display());
   }
   // After:
   if args.retain_db {
       log::info!("Gramps DB retained at: {}", db_dir.display());
   } else {
       log::info!("Gramps DB removed (temp dir cleaned up).");
   }
   ```

8. **`strip_gramps_extensions`**: Keep this helper but use it only for deriving
   the stem and the DB directory path (if the user provides `--db-dir`). The file
   extension (`gramps`) in the stem output isn't needed since we use `stem` to
   build the output file names.

### 4. `crates/cli/src/commands/delete.rs` — Unit tests

- Update `DeleteArgs` construction in tests to use `output_dir` and `retain_db`.
- Replace tests for `derive_no_events_path` / `derive_deleted_3_path` /
  `derive_deleted_4_path` with tests for `deleted_1_path` / `deleted_2_path` /
  `deleted_3_path` / `deleted_4_path`.
- Update `strip_gramps_extensions` test for `test-cleaned` → now `test-deleted-1`
  etc.
- Verify the path functions join the directory and stem correctly, including
  edge inputs: empty stem, stem with dots (e.g. `data.v2`), and directory paths
  (e.g. `dir/data.gramps` → `dir/data-deleted-1.gramps`).
- Add a test that the stem derivation falls back to `"output"` (no panic) for
  root-like input paths (`/`).

### 5. `crates/cli/tests/e2e_delete.rs` — Integration tests

- Replace all `--output <filename>` with `--output <dir>`.
- **New helper needed**: the existing `temp_path()` creates *files* in `/tmp`.
  Add a `temp_dir()` helper that actually creates a directory (`fs::create_dir_all`)
  — passing a file path to `-o` now fails with "File exists" because the CLI
  calls `create_dir_all` on it.
- **Retarget all file reads**: every existing test does
  `std::fs::read_to_string(&output)` on the `-o` path. After the change the
  `-o` path is a directory, so reads must target
  `<dir>/<stem>-deleted-1.gramps` (the stage under test).
- Update helpers `no_events_path`, `deleted_3_path`, `deleted_4_path` to take
  the **output directory** (not the input path) and use new naming
  (`deleted-2.gramps` etc.) — intermediate files now land inside the `-o`
  directory, not next to the input file.
- Update all assertions that reference `-cleaned`, `-deleted_2`, `-deleted_3`,
  `-deleted_4` to use `-deleted-1`, `-deleted-2`, `-deleted-3`, `-deleted-4`.
- Update the `e2e_delete_db_retained` test: now passes `--retain-db` to verify
  the DB is kept.
- Update `e2e_delete_no_retain_db`: flip the assertion — without `--retain-db`,
  DB should NOT exist.
- **New test: default DB location** — run without `--db-dir` and without
  `--retain-db`, assert the temp DB directory under the system temp dir is gone
  afterwards (no leak on the happy path).
- **New test: old-style usage error** — pass `-o` pointing at an existing *file*,
  assert a clean non-zero exit with the "output is a directory" error message
  (not a raw "File exists" io error).
- Update all `gramps_gen` invocations to pass `-o` with a directory.
- Note: input files and `-o` directories may share `/tmp`; use distinct
  `temp_dir()` subdirectories per test to avoid cross-test collisions.

### 6. `scripts/delete_backend.py` — No changes

The Python backend receives `--output` as a specific file path. The Rust side
continues to pass the full path (`deleted-1.gramps` inside the output
directory). No Python changes needed.

The only Rust↔Python interface change is the **inverted `--no-retain-db`
pass-through** described in §3 step 4: the Rust call site now passes
`--no-retain-db` by default and omits it when `--retain-db` is set. The Python
flag itself, its argparse definition, and its cleanup behavior are unchanged.

**Verify during implementation**: run one e2e invocation without `--retain-db`
and confirm the DB directory is actually removed — this is the regression that
silently passes if the inversion is forgotten (the Python backend's
retain-by-default would otherwise mask it).

### 7. `scripts/test_delete_backend.py` — No changes

The Python backend's interface is unchanged (still receives `--output` as a file
path). Tests continue to use temp directories. No changes needed.

### 8. `docs/delete-tool.md` — User guide rewrite

Sections to update:

| Section | Change |
|---|---|
| Quick Start | Update all examples to use `--output <dir>` and show output file listing |
| How the Delete Pipeline Works | Update diagram output paths |
| Post-processing table | Rename from `-cleaned` / `-deleted_2` / etc. to `-deleted-1` through `-deleted-4` |
| Command Reference (Arguments) | Change `--output <PATH>` description to "Output directory" |
| Command Reference (Options) | Add `--retain-db`; remove `--no-retain-db` |
| Tips and Common Workflows | Update all shell examples |
| Save/Load Manifests | Update path examples |

Add a prominent **"Which file is the final output?"** callout right after the
Post-processing table: `-deleted-4.gramps` is the finished, fully-cleaned file;
`-deleted-1` through `-deleted-3` are intermediate checkpoints. (The old docs
had no guidance on this, which the plan's Goal section criticizes.)

### 9. `docs/ARCHITECTURE.md` — Architecture diagram update

Sections to update:

| Section | Change |
|---|---|
| Architecture diagram (delete pipeline) | Update output file names; update DB retention default |
| Architecture diagram — final **Output** box | Currently shows `<input>-gramps-db/ (retained Gramps DB)` — flip to "DB removed by default; retained only with `--retain-db`" |
| Delete Tool — Integration flags | Replace `--output <FILE>` with `--output <DIR>`; replace `--no-retain-db` with `--retain-db` |
| Delete Tool — Manifest paragraph | Says "always saved alongside the output file (`<output-stem>.manifest.json`)" — change to "saved in the output directory as `<file-stem>-deleted.manifest.json`" |

### 10. `crates/cli/src/main.rs` — Help text

**No-op, confirmed by inspection**: `main.rs` contains no text mentioning
`--no-retain-db` or output-file semantics. No changes needed. (This step exists
only as a verification check — run `rg -n "retain|cleaned|deleted_"
crates/cli/src/main.rs` during implementation to re-confirm.)

### 11. Optional: `docs/research/` files

The existing research notes (`gramps-api-delete-shift.md`, `note-deletion-postprocess.md`,
`place-deletion-postprocess.md`, `event-cleaning-postprocess.md`) are historical
records and should **not** be modified. The new plan document supersedes them
for implementation guidance.

## Implementation Order

| Step | What | Estimated complexity |
|---|---|---|
| 1 | Rename/rewrite path helper functions in `delete.rs` | Small |
| 2 | Rewrite `run()` output directory + path logic | Medium |
| 3 | Flip DB retention default + add `--retain-db` | Small |
| 4 | Update unit tests in `delete.rs` | Small |
| 5 | Update e2e tests in `e2e_delete.rs` | Medium |
| 6 | Update `docs/delete-tool.md` | Medium |
| 7 | Update `docs/ARCHITECTURE.md` | Small |
| 8 | `cargo test --workspace` + `cargo clippy` | Verification |
| 9 | Run e2e tests (requires Gramps) | Verification |

## Risks

- **E2E test fragility**: The e2e tests depend on Gramps being installed and
  parse the output files. Renaming the output files requires updating every test
  assertion path. Run the e2e suite early and often.
- **Silent flag-inversion regression**: If the Rust→Python `--no-retain-db`
  pass-through is not inverted (§3 step 4), the DB is retained despite the new
  default and the goal silently fails. The e2e DB-cleanup assertions are the
  safety net; they must be updated and run before this change is considered
  done.
- **Backward compatibility**: Users who have scripts depending on the old
  `--output <filename>` semantics will break. This is acceptable — the old
  behavior was buggy (output file wasn't fully cleaned). The failure mode is a
  clear error from `create_dir_all` ("File exists") rather than silent
  misbehavior.
- **Temp DB directory cleanup**: If the process crashes between DB creation and
  cleanup, a temp directory may leak. Mitigated by using a unique per-run name
  (timestamp suffix) so a stale directory from a previous PID never collides
  with a live run and gets silently deleted by `create_db()`. Temp directories
  are in `/tmp` and get cleaned on reboot.

## Files Affected (complete list)

| File | Type of change |
|---|---|
| `crates/cli/src/commands/delete.rs` | Major: args, path helpers, `run()`, tests |
| `crates/cli/tests/e2e_delete.rs` | Major: all paths, all assertions, all test names |
| `docs/delete-tool.md` | Major: user-facing documentation rewrite |
| `docs/ARCHITECTURE.md` | Minor: flag descriptions, diagram |
| `scripts/delete_backend.py` | **No changes** |
| `scripts/test_delete_backend.py` | **No changes** |
