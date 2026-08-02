# Implementation Plan: Fix Visualization Loading

Source: `docs/research/fix-visualization-loading.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `test(visualize): add arg-parsing unit tests` | Arg-parsing module | `crates/visualize/src/args.rs` (new), `crates/visualize/src/lib.rs` (re-export) | Unit |
| 2 | `fix(visualize): accept positional file arg, parse --no-impute and --generation-gap` | Argument parser + window injection | `crates/visualize/src/main.rs` | Unit |
| 3 | `fix(visualize): send no_impute and generation_gap in IPC invoke` | Frontend IPC fix | `crates/visualize/frontend/src/main.ts` | — |
| 4 | `build(visualize): rebuild frontend dist` | Frontend build artifacts | `crates/visualize/frontend/dist/` | — |
| 5 | `docs(visualize): fix date imputation formula sign in doc comment` | Doc comment fix | `crates/visualize/src/dates.rs` | — |
| 6 | `chore: add visualize gen/ to .gitignore, commit app icon` | Gitignore + icon asset | `.gitignore`, `crates/visualize/icons/icon.png` | — |
| 7 | `test(visualize): add integration test with exp01.gramps fixture` | Integration test | `crates/visualize/tests/fixtures/exp01.gramps`, `crates/visualize/tests/integration.rs` | Integration |

**Verification steps (not committed):**

- After each step: `cargo build -p visualize -F visualize` — must compile cleanly
- Before step 4: `cd crates/visualize/frontend && npm run build` — rebuild frontend assets
- After step 4: `cargo test --workspace` — all tests pass
- After step 7: `cargo clippy --all-targets --all-features -- -D warnings` — zero warnings
- Final smoke test: `cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps` — graph renders with 64 persons, 49 families

## Step Details

### Step 1 — Arg-parsing module

Extract command-line argument parsing into a pure function `parse_cli_args(args: &[String]) -> CliArgs` in a new `crates/visualize/src/args.rs` module. Write unit tests covering:

- `["/path/to/file.gramps"]` → `path = Some(...)`, `no_impute = false`, `gap = 25`
- `["/path/to/file.gramps", "--no-impute"]` → `no_impute = true`
- `["/path/to/file.gramps", "--generation-gap", "50"]` → `gap = 50`
- `[]` → `path = None`
- `["--no-impute", "/path/to/file.gramps"]` → flags-before-path works
- `["--generation-gap", "abc", "/p.gramps"]` → invalid parse → default 25

### Step 2 — Argument parser + window injection

Replace the `--path` parser in `main.rs` with a single-pass extractor using explicit `match` arms for `--no-impute` and `--generation-gap`, accepting a positional file argument. Inject all three values (`__GRAMPS_FILE__`, `__NO_IMPUTE__`, `__GENERATION_GAP__`) into the Tauri `setup` hook using `serde_json::to_string`.

### Step 3 — Frontend IPC fix

In `main.ts`, update both `openAndRenderFile()` and `openAndRenderFileFromPath()` to send all three IPC parameters (`path`, `no_impute`, `generation_gap`). Use `??` (nullish coalescing) with defaults (`DEFAULT_GENERATION_GAP = 25`, `DEFAULT_NO_IMPUTE = false`) so file-dialog launches still work.

### Step 4 — Frontend build

Run `cd crates/visualize/frontend && npm run build` to produce updated `dist/` assets. Verify the bundled JS contains `no_impute` and `generation_gap` keys in the `invoke` call.

### Step 5 — Doc comment fix

Fix the sign in the date imputation formula doc comment: `(source_generation - target_generation)` → `(target_generation - source_generation)`.

### Step 6 — Gitignore + icon

Add `crates/visualize/gen/` to `.gitignore`. Stage and commit `crates/visualize/icons/icon.png`.

### Step 7 — Integration test

Copy `~/Documents/gramps01/exp01.gramps` into `crates/visualize/tests/fixtures/`. Add an integration test that calls `load_graph_data` and verifies the returned node/link counts match expected values (64 persons, 49 families).
