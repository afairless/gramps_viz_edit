# Fix Plan: Visualization Tool Cannot Load `.gramps` Files

## Overview

The Tauri-based family-group visualization tool (`gramps-gen-visualize`) cannot
load any `.gramps` file — whether launched via `gramps-gen visualize <file>` or
run directly with `--path`. The root cause is a Tauri IPC parameter mismatch
between the Rust backend and the TypeScript frontend. Two additional
defects in argument-passing and flag-forwarding compound the failure.

Uncommitted changes on the `agent/family-group-visualization` branch are a
partial improvement (adding a `--path` CLI flag, a welcome screen, and a file
dialog) but do not resolve the IPC bug and introduce an argument-format mismatch
with the CLI caller.

| # | Defect | Severity |
|---|--------|----------|
| 1 | Tauri IPC `load_graph` requires 3 params, frontend sends 1 | **Fatal** — blocks ALL file loading |
| 2 | CLI passes file as positional arg, binary expects `--path` | Blocks CLI→binary launch path |
| 3 | `--no-impute` and `--generation-gap` flags silently discarded | Blocks flag forwarding |
| 4 | `dates.rs` doc-comment formula sign wrong (code is correct) | Cosmetic — misleading docs |

---

## Problem Analysis

### Defect 1: Tauri IPC Parameter Mismatch (FATAL)

**Files:** `crates/visualize/src/main.rs` (backend), `crates/visualize/frontend/src/main.ts` (frontend)

The Rust backend defines `load_graph` with three required parameters:

```rust
// main.rs line 52
#[tauri::command]
fn load_graph(
    path: &str,
    no_impute: bool,
    generation_gap: u32,
) -> Result<visualize::GraphData, String> {
    visualize::load_graph_data(path, no_impute, generation_gap)
}
```

The frontend invokes it with only one field (in two places — lines 106 and 131
of the working-copy `main.ts`):

```typescript
// main.ts — openAndRenderFile() and openAndRenderFileFromPath()
graphData = await tauri.invoke('load_graph', {
    path: selected,   // ← only path; no_impute and generation_gap are missing
});
```

Tauri v2 deserializes the JS object into the command arguments struct.  Missing
required fields produce a deserialization error _before_ `load_graph` executes.
The error is caught by the frontend and surfaces as `"Failed to load Gramps
data file."` — but the real error string (e.g. `"missing field 'no_impute'"`)
is only logged to the console.

**This defect exists identically in both the committed HEAD and the uncommitted
working copy.**  No `.gramps` file can ever be loaded through this code path.
The `load_graph_data` pure function in `lib.rs` and all its unit tests work
correctly — the break is at the IPC boundary.

#### Secondary issue: missing default values

The frontend has no way to know what `no_impute` and `generation_gap` to send.
The committed HEAD has no CLI argument handling at all.  The uncommitted changes
parse `--path` but not `--no-impute` or `--generation-gap`.  The correct
behavior is to send sensible defaults (`no_impute: false`, `generation_gap: 25`)
when unspecified, and wire CLI flags through when provided.

---

### Defect 2: Argument Format Mismatch — CLI Sends Positional, Binary Expects `--path`

**Files:** `crates/cli/src/commands/visualize.rs` (committed), `crates/visualize/src/main.rs` (uncommitted)

The CLI `visualize` subcommand spawns the visualize binary with a plain
positional argument:

```rust
// cli/commands/visualize.rs line 62
let mut cmd = std::process::Command::new(&sibling);
cmd.arg(&canonical);                          // positional: /path/to/file.gramps
cmd.arg("--no-impute");                       // flag (if set)
cmd.arg("--generation-gap").arg("25");        // flag + value
```

But the uncommitted `main.rs` parses `--path`:

```rust
// visualize/src/main.rs line 15 (uncommitted)
if arg == "--path" {
    if let Some(val) = args.next() {
        result = Some(val);
    }
}
```

These do not match.  When the CLI spawns `gramps-gen-visualize /tmp/file.gramps`,
the positional path is never captured by the `--path` parser.  The binary starts,
sees no `--path` on its command line, `window.__GRAMPS_FILE__` is never set, and
the frontend shows the welcome screen instead of auto-loading the file.

**This defect exists only in the uncommitted changes** — the committed HEAD
has no CLI argument parsing at all and silently ignores all args.

**The correct approach:** the visualize binary should accept a **positional**
file argument (matching what the CLI sends), not `--path`.  This also matches
the documented usage: `cargo run -p visualize -- <file>`.

---

### Defect 3: `--no-impute` and `--generation-gap` Flags Silently Discarded

**File:** `crates/visualize/src/main.rs` (uncommitted, and absent in committed)

The CLI passes `--no-impute` and `--generation-gap <N>` as flags to the
visualize binary, which are read by Tauri's own argument parser.  Without
explicit `--` handling in the visualize binary, Tauri may consume or reject
these unknown flags.

Even after fixing Defect 2, the parsed flag values must reach the frontend so
they can be included in the `invoke('load_graph', ...)` call.  The same
`window.eval()` mechanism used for the file path can serve this purpose.

---

### Defect 4 (cosmetic): Wrong Formula Sign in `dates.rs` Doc Comment

**File:** `crates/visualize/src/dates.rs` line ~20

The doc comment says:

```rust
/// imputed_year = source_year + (source_generation - target_generation) × gap
```

But the actual code correctly uses:

```rust
let imputed = src_year as i64 + (tgt_gen - src_gen) as i64 * gap as i64;
```

The code is correct (ancestors get earlier dates than descendants; all tests
pass).  The comment is wrong: it should say `(target_generation - source_generation)`.

This is a minor documentation bug but worth fixing while touching these files.

---

## Uncommitted Changes Assessment

| File | Change | Retain? |
|------|--------|---------|
| `crates/visualize/tauri.conf.json` | `frontendDist`: `"../frontend/dist"` → `"./frontend/dist"` | **YES** — fixes path to actual frontend assets |
| `crates/visualize/src/main.rs` | Adds `--path` parsing + `setup` hook injecting `window.__GRAMPS_FILE__` | **KEEP but rework** — use positional arg, add flag parsing (see Fix 2, Fix 3) |
| `crates/visualize/src/graph_data.rs` | Adds `Deserialize` derive to `SelectedPerson` | **YES** — needed for Tauri IPC deserialization |
| `crates/visualize/frontend/src/main.ts` | Refactors into welcome screen, file dialog, path-based loading | **KEEP but fix** — IPC invocations need `no_impute` + `generation_gap` fields |
| `crates/visualize/frontend/dist/` | Vite rebuild: new `index-bQS94W3a.js`, old `index-DnH_a6KY.js` deleted | **YES** — standard dist update matching source |
| `crates/visualize/gen/` (untracked) | Tauri build-generated schemas + capabilities | **YES** — auto-generated by `tauri_build::build()` |
| `crates/visualize/icons/icon.png` (untracked) | Minimal app icon | **YES** — standard Tauri asset |

**Verdict:** Retain all uncommitted changes.  They are correct in intent and
fix real defects (the `frontendDist` path and the `Deserialize` derive).  The
`main.rs` and `main.ts` changes need modification as described in the fix plan.

---

## Fix Plan

### Fix 1: Add Missing IPC Parameters in Frontend

**Goal:** The `tauri.invoke('load_graph', ...)` calls must send all three
required parameters (`path`, `no_impute`, `generation_gap`).

**File:** `crates/visualize/frontend/src/main.ts`

**Changes:**

1. Define constants for defaults:

   ```typescript
   const DEFAULT_GENERATION_GAP = 25;
   const DEFAULT_NO_IMPUTE = false;
   ```

2. Expose `no_impute` and `generation_gap` from `window` alongside
   `__GRAMPS_FILE__` (set by the Rust backend in the `setup` hook).

3. Update `openAndRenderFile()` (line 106) to send all three fields:

   ```typescript
   const gap: number =
     (window as unknown as Record<string, number>).__GENERATION_GAP__ ??
     DEFAULT_GENERATION_GAP;
   const noImpute: boolean =
     (window as unknown as Record<string, boolean>).__NO_IMPUTE__ ??
     DEFAULT_NO_IMPUTE;
   graphData = await tauri.invoke('load_graph', {
     path: selected,
     no_impute: noImpute,
     generation_gap: gap,
   });
   ```

   **Important:** Use `??` (nullish coalescing), not `||`.  The `||` operator
   treats `0` as falsy, so if `__GENERATION_GAP__` were `0` it would silently
   fall back to the default.  `??` only falls through on `null`/`undefined`,
   which is the correct behavior when the Rust backend hasn't injected the
   value (file-dialog launch mode).

   **Two launch modes — both intentional:**
   - **CLI launch:** `window.__*__` values are set by the Rust `setup` hook
     → CLI flags are forwarded to the IPC call.
   - **File-dialog launch:** `window.__*__` values are `undefined` (never
     injected) → `??` falls back to defaults (`no_impute: false`,
     `generation_gap: 25`). This is the desired UX for interactive use.

4. Update `openAndRenderFileFromPath()` (line 131) identically.

**Atomicity:** This is a one-file change and can be verified by rebuilding
the frontend (`npm run build`) and confirming the bundled JS contains
`no_impute` and `generation_gap` keys in the `invoke` call.

---

### Fix 2: Accept Positional File Argument + Parse Flags (Single Pass)

**Goal:** The visualize binary must (a) accept the file path as a positional
argument matching what the CLI sends, and (b) parse `--no-impute` and
`--generation-gap` flags, forwarding all three values to the frontend via
`window.__*__`.  Parse everything in a **single pass** to avoid iterating
`std::env::args()` multiple times and to prevent boolean-flag consumption bugs.

**File:** `crates/visualize/src/main.rs`

**Changes:**

Replace the `--path` parser with a single-pass extractor that handles
positional args, boolean flags, and key-value flags correctly:

```rust
// Single-pass argument extraction: positional path + flags.
let (cli_path, cli_no_impute, cli_generation_gap) = {
    let mut args = std::env::args().skip(1); // skip binary name
    let mut path: Option<String> = None;
    let mut no_impute = false;
    let mut generation_gap = 25u32;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-impute" => {
                // Boolean flag — no value to consume.
                no_impute = true;
            }
            "--generation-gap" => {
                if let Some(val) = args.next() {
                    generation_gap = val.parse().unwrap_or(25);
                }
            }
            other if !other.starts_with("--") && path.is_none() => {
                // First non-flag token is the file path.
                path = Some(other.to_string());
            }
            _ => {
                // Unknown flag or extra positional — ignore.
            }
        }
    }
    (path, no_impute, generation_gap)
};
```

**Why single-pass:** Parsing `std::env::args()` three separate times (once
for the path, once for `--no-impute`, once for `--generation-gap`) is fragile.
A single pass guarantees correct handling of all argument orderings.

**Why explicit flag names instead of `arg.starts_with("--")`:** A generic
`if arg.starts_with("--") { args.next(); }` handler would consume the file
path as the "value" of `--no-impute`, causing `gramps-gen-visualize --no-impute
/path/to/file.gramps` to lose the path.  Explicit `match` arms only consume
values for flags that actually take them.

This accepts all reasonable orderings:

- `gramps-gen-visualize /path/to/file.gramps`
- `gramps-gen-visualize /path/to/file.gramps --no-impute --generation-gap 30`
- `gramps-gen-visualize --no-impute /path/to/file.gramps --generation-gap 30`

**Rationale:** Positional arguments are the standard Unix convention for
required file arguments.  The CLI already sends the file positionally.
This also matches the `cargo run -p visualize -- <file>` documented usage.

---

### Fix 3: Inject All Three Values into the Window

**Goal:** Forward `cli_path`, `cli_no_impute`, and `cli_generation_gap` to
the frontend via `window.__*__` globals in the Tauri `setup` hook.

**File:** `crates/visualize/src/main.rs`

**Changes:**

Inject all three values using `serde_json::to_string` for consistent,
safe JavaScript literal generation:

```rust
.setup(move |app| {
    if let Some(window) = app.get_webview_window("main") {
        let path_json = serde_json::to_string(&cli_path).unwrap_or_else(|_| "null".into());
        let no_impute_json = serde_json::to_string(&cli_no_impute).unwrap_or_else(|_| "false".into());
        let gap_json = serde_json::to_string(&cli_generation_gap).unwrap_or_else(|_| "25".into());
        window.eval(&format!(
            "window.__GRAMPS_FILE__ = {}; window.__NO_IMPUTE__ = {}; window.__GENERATION_GAP__ = {};",
            path_json, no_impute_json, gap_json,
        ))?;
    }
    Ok(())
})
```

**Why `serde_json::to_string` for all three:** While Rust's `Display` for
`bool` and `u32` happens to produce valid JS literals (`true`, `false`,
integer), using `serde_json` for every value is consistent and robust
against future type changes.

**Validation note:** The `--generation-gap` value is already validated by the
CLI to be in range 1–100 before the binary is spawned.  The parse fallback `25`
is a defense-in-depth measure for direct binary usage.

---

### Fix 4 (cosmetic): Fix `dates.rs` Doc Comment

**File:** `crates/visualize/src/dates.rs` line ~20

**Change:**

```diff
-/// imputed_year = source_year + (source_generation - target_generation) × gap
+/// imputed_year = source_year + (target_generation - source_generation) × gap
```

---

### Fix 5 (recommended): Add `gen/` to `.gitignore`, Commit `icons/icon.png`

**File:** `.gitignore`

**Change:** Add `crates/visualize/gen/` to prevent auto-generated Tauri build
artifacts from being accidentally committed.  These are produced by
`tauri_build::build()` in `build.rs` and regenerate on every build.

**Also:** The untracked `crates/visualize/icons/icon.png` is a standard Tauri
source asset (not generated) and should be **committed** (`git add`), not
gitignored.  It is the app icon displayed in the window title bar and system
task switcher.

---

## Verification

### Before merging

1. **Build the frontend:**

   ```bash
   cd crates/visualize/frontend && npm run build
   ```

2. **Build the visualize binary:**

   ```bash
   cargo build -p visualize -F visualize
   ```

   (The `visualize` feature is defined in `crates/visualize/Cargo.toml`;
   `-F visualize` passes it directly to that crate rather than through the
   workspace root.)

3. **Launch directly with the test file:**

   ```bash
   cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps
   ```

   **Expected:** Window opens, graph renders with 64 persons and 49 families.
   Force simulation settles.  Hover tooltips show.  Family group filter works.

4. **Launch without arguments:**

   ```bash
   cargo run -p visualize -F visualize
   ```

   **Expected:** Welcome screen appears with "Open Gramps File" button.

5. **Launch via CLI (requires `cargo install` or build of both binaries):**

   ```bash
   cargo run --release --features visualize -- visualize ~/Documents/gramps01/exp01.gramps
   ```

   **Expected:** Same as test 3 — auto-loads the file.

6. **Test with flags:**

   ```bash
   cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps --no-impute --generation-gap 30
   ```

   **Also test flag-before-path ordering** (must work with the single-pass parser):

   ```bash
   cargo run -p visualize -F visualize -- --no-impute ~/Documents/gramps01/exp01.gramps --generation-gap 30
   ```

**Expected:** Graph renders with imputation disabled (all imputed nodes show
`birth_year: null` with neutral gray) and generation gap 30 used for any
remaining imputation.

1. **Run existing tests:**

   ```bash
   cargo test --workspace
   cargo clippy --all-targets --all-features -- -D warnings
   ```

### Optional UX improvement (not required for the fix)

When `invoke('load_graph', ...)` fails, the frontend currently shows a
generic `"Failed to load Gramps data file."` message and logs the real
error only to `console.error`.  Consider surfacing the backend error
message in the UI so users can diagnose issues (e.g., corrupt file,
missing permissions) without opening dev tools:

```typescript
} catch (err) {
  console.error('Failed to load graph data via Tauri IPC:', err);
  showError(container, `Failed to load Gramps data file: ${err}`);
  return false;
}
```

### Automated tests (required additions — written BEFORE or alongside fixes)

Tests must be written **before** the corresponding implementation code so
every fix is verified by a failing-then-passing test cycle.

**A. Rust unit test — argument parsing** (`crates/visualize/src/args.rs`, new)

Extract argument parsing into a small pure function `parse_cli_args(args:
&[String]) -> CliArgs` to make it testable without involving Tauri.  Test
cases:

- `["/path/to/file.gramps"]` → `path = Some(...)`, `no_impute = false`, `gap = 25`
- `["/path/to/file.gramps", "--no-impute"]` → `no_impute = true`
- `["/path/to/file.gramps", "--generation-gap", "50"]` → `gap = 50`
- `[]` → `path = None`
- `["--no-impute", "/path/to/file.gramps"]` → `path = Some(...)`, `no_impute = true` (flags-before-path)
- `["--generation-gap", "abc", "/p.gramps"]` → `gap = 25` (invalid parse → default)

**B. Integration test — `load_graph_data` with exp01.gramps fixture**
(`crates/visualize/tests/fixtures/exp01.gramps`)

Copy `exp01.gramps` into the fixtures directory and add an integration test
that calls `load_graph_data` and verifies the node/link counts match expected
values (64 persons, 49 families).

---

## Implementation Order

Each step is a single logical, testable unit.  **Commit after every step**
with a conventional commit message.  Do not batch multiple steps into one
commit — each step's commit must pass tests independently.

Tests are written **before or alongside** their corresponding implementation
so that every fix is verified by a failing-then-passing test cycle.

| Step | Commit | File(s) | Description |
|------|--------|---------|-------------|
| 1 | `test(visualize): add arg-parsing unit tests` | `src/args.rs` (new) | Extract argument parsing into a pure function `parse_cli_args()` and add unit tests covering: positional-only, positional + flags, flags-before-path, empty args |
| 2 | `fix(visualize): accept positional file arg, parse --no-impute and --generation-gap` | `src/main.rs` | Implement the single-pass argument parser (Fix 2), inject all three `window.__*__` values (Fix 3). Verify Step 1 tests pass |
| 3 | `fix(visualize): send no_impute and generation_gap in IPC invoke` | `frontend/src/main.ts` | Add `??`-based defaults and send all three params to `invoke('load_graph')` in both `openAndRenderFile()` and `openAndRenderFileFromPath()` (Fix 1) |
| 4 | `build(visualize): rebuild frontend dist` | `frontend/dist/` | `npm run build` to produce updated `dist/` with the IPC fix bundled. Verify the built JS contains `no_impute` and `generation_gap` keys |
| 5 | `build(visualize): verify compilation` | — | `cargo build -p visualize -F visualize` — must compile cleanly. `cargo test --workspace` — all tests pass |
| 6 | — | — | **Manual smoke test:** `cargo run -p visualize -F visualize -- ~/Documents/gramps01/exp01.gramps` — graph renders |
| 7 | `docs(visualize): fix date imputation formula sign in doc comment` | `src/dates.rs` | Fix doc-comment formula sign (Fix 4) |
| 8 | `chore: add visualize gen/ to .gitignore, commit app icon` | `.gitignore`, `icons/icon.png` | Add `crates/visualize/gen/` to `.gitignore`; `git add crates/visualize/icons/icon.png` (Fix 5) |
| 9 | `test(visualize): add integration test with exp01.gramps fixture` | `tests/fixtures/`, `tests/integration.rs` | Copy `exp01.gramps` into `crates/visualize/tests/fixtures/`, add integration test verifying node/link counts (64 persons, 49 families) |

**Step 6 (manual smoke test) is not committed** — it's a human verification
step.  All other steps produce a commit.
