# Include `.xml` Extension in Visualizer File Browser

**Date:** 2025-01-02  
**Status:** Planned

## Overview

Gramps family tree files are sometimes saved with a `.xml` extension rather than the more common `.gramps` extension. The visualizer tool's file browser currently only accepts `*.gramps` files, which prevents users from opening valid Gramps XML files with a `.xml` extension.

This document describes the changes needed to accept both `.gramps` and `.xml` extensions throughout the toolchain.

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| File dialog filter label | Keep "Gramps XML" | Still accurate — both extensions contain Gramps XML content |
| File dialog extensions | `['gramps', 'xml']` | Accept both formats without confusing users with a second filter entry |
| Filter config in code | Extract to a named constant `GRAMPS_FILE_FILTER` | The `open()` call is embedded in a large UI/IPC function and cannot be unit-tested directly; a named constant can be imported and asserted on without invoking the Tauri dialog plugin |
| CLI extension guard | Accept both `.gramps` and `.xml` | Consistent with the file dialog; the backend already handles any extension |
| Welcome screen text | Update to mention both extensions | Keeps UI text in sync with actual behavior |
| Backend `read_gramps_file` | No changes needed | Already reads any file extension (detects gzip by magic bytes, not extension) |

## Changes Required

### 1. Frontend file dialog filter

**File:** `crates/visualize/frontend/src/main.ts`

**Location:** `openAndRenderFile()` function, the `open()` call.

The `extensions` array currently contains only `'gramps'`. Adding `'xml'` allows the native OS file browser to show both `*.gramps` and `*.xml` files.

To make the filter testable, extract it to a named constant at module scope and reference it from the `open()` call. The `open()` call itself is embedded in a large function mixing UI construction, IPC, and error handling, so asserting on the constant directly is the only practical way to unit-test it.

```typescript
// Before:
filters: [{ name: 'Gramps XML', extensions: ['gramps'] }],

// After (module scope):
/** File dialog filter for Gramps family tree files (.gramps or .xml). */
export const GRAMPS_FILE_FILTER = { name: 'Gramps XML', extensions: ['gramps', 'xml'] };

// ...inside openAndRenderFile():
filters: [GRAMPS_FILE_FILTER],
```

The `export` makes it importable from the test suite. This is the only way to verify the dialog filter without refactoring `openAndRenderFile` further or mocking the Tauri dialog plugin (which the current vitest/jsdom setup cannot do).

### 2. CLI extension guard

**File:** `crates/cli/src/commands/visualize.rs`

**Location:** `canonicalize_gramps_path()` function.

The current check:

```rust
if canonical.extension().and_then(|e| e.to_str()) != Some("gramps") {
    return Err(CliError::ConfigError(format!(
        "not a .gramps file: {}",
        canonical.display()
    )));
}
```

This should be relaxed to accept both `.gramps` and `.xml`:

```rust
let ext = canonical.extension().and_then(|e| e.to_str());
if ext != Some("gramps") && ext != Some("xml") {
    return Err(CliError::ConfigError(format!(
        "not a .gramps or .xml file: {}",
        canonical.display()
    )));
}
```

### 3. Welcome screen text

**File:** `crates/visualize/frontend/src/main.ts`

**Location:** `showWelcomeScreen()` function, subtitle text.

```typescript
// Before:
subtitle.textContent = 'Open a Gramps XML (.gramps) file to get started.';

// After:
subtitle.textContent = 'Open a Gramps XML (.gramps or .xml) file to get started.';
```

This change lives in the same file as change 1 and is part of the same implementation step (see Implementation Order).

### 4. Doc comments and error messages

Several doc comments and error messages reference `.gramps` files specifically. While not functionally required, updating them improves consistency:

| File | Location | Current text | Suggested text |
|---|---|---|---|
| `crates/visualize/src/main.rs` | Doc comment on `load_graph` | Load a `.gramps` file | Load a Gramps XML file |
| `crates/visualize/src/main.rs` | Doc comment on `get_stats` | Return summary statistics for a `.gramps` file | Return summary statistics for a Gramps XML file |
| `crates/visualize/src/lib.rs` | Doc comments on `load_graph_data`, `load_graph_data_with_stats`, `get_stats` | Load a `.gramps` file | Load a Gramps XML file |
| `crates/visualize/src/lib.rs` | Doc comment on `LoadedGraph` | Combined result of loading a `.gramps` file | Combined result of loading a Gramps XML file |
| `crates/visualize/src/args.rs` | Doc comment on `CliArgs::path` | Path to the `.gramps` file | Path to the Gramps XML file |
| `crates/cli/src/commands/visualize.rs` | Doc comment / error text | Not a `.gramps` file | Not a `.gramps` or `.xml` file |
| `crates/gramps-reader/src/lib.rs` | Top-level doc comment | Shared `.gramps` XML parsing | Shared Gramps XML parsing |
| `crates/gramps-reader/src/io.rs` | Module doc comment | File I/O with transparent gzip decompression for `.gramps` files | File I/O with transparent gzip decompression for Gramps XML files |
| `crates/gramps-reader/src/io.rs` | Doc comment on `read_gramps_file` | Read a `.gramps` file | Read a Gramps XML file |
| `crates/cli/src/commands/stats/mod.rs` | Module doc comment | Summarize the contents of a `.gramps` file | Summarize the contents of a Gramps XML file |
| `crates/cli/src/commands/validate.rs` | Module doc comment | Validate a .gramps file's structure | Validate a Gramps XML file's structure |

### 5. Test files

Test files that use `.gramps` extensions in test data paths do not need to be changed — they test the `.gramps` path, which continues to work. However, adding tests for the `.xml` path is recommended (see below).

## Testing Strategy

### New tests

1. **Frontend dialog filter** (`crates/visualize/frontend/tests/`): Import `GRAMPS_FILE_FILTER` and assert its `extensions` array contains both `'gramps'` and `'xml'`, and that the label remains `'Gramps XML'`. This works without invoking the Tauri dialog plugin because the filter is a plain exported constant.

2. **Frontend welcome screen text** (`crates/visualize/frontend/tests/`): Render `showWelcomeScreen()` into a container and assert the subtitle text contains both `.gramps` and `.xml`. The function builds DOM directly, so a vitest/jsdom test can call it and inspect `subtitle.textContent`.

3. **CLI extension guard** (`crates/cli/tests/` or inline unit tests in `visualize.rs`):
   - `.xml` file is accepted by `canonicalize_gramps_path` (new test; rename a temp file with `with_extension("xml")`)
   - `.gramps` file still accepted (existing test)
   - `.txt` file still rejected (existing test, error message updated to mention both extensions)

4. **Backend path** (`crates/visualize/src/lib.rs`):
   - `load_graph_data` with a `.xml`-extension file returns valid results (new test; reuse an existing fixture XML string but rename the temp file with `with_extension("xml")`)
   - `get_stats` with a `.xml`-extension file returns valid results (new test; same rename approach)
   - Existing backend tests that use `with_extension("gramps")` continue to cover the `.gramps` path unchanged

### Existing tests that still pass

- All existing tests using `.gramps` extension files continue to work unchanged.
- Backend tests that test the `read_gramps_file` function are unaffected.

## Implementation Order

Each step is a single logical unit that compiles and passes its tests before the next begins (per the incremental-development workflow). Tests come before cosmetic doc updates so behavior is verified first.

1. **CLI extension guard** — update the extension check in `crates/cli/src/commands/visualize.rs`
2. **Frontend: dialog filter constant + welcome screen text** — both changes in `crates/visualize/frontend/src/main.ts`; extract `GRAMPS_FILE_FILTER`, reference it from `open()`, update the welcome subtitle
3. **Tests** — frontend `GRAMPS_FILE_FILTER` and welcome-screen tests; CLI `.xml` acceptance test; backend `load_graph_data` / `get_stats` `.xml`-extension tests
4. **Doc comments and error messages** — cosmetic updates across files (including the `stats` and `validate` module doc comments)

## Files Not Requiring Changes

- **`crates/gramps-reader/src/io.rs`**: `read_gramps_file` reads by content (detects gzip via magic bytes), not by extension. Doc comments are updated cosmetically in step 4 only.
- **`crates/visualize/src/args.rs`**: `parse_cli_args` handles any string as a path — it doesn't validate extensions. Doc comment on `CliArgs::path` is updated cosmetically in step 4 only.
- **`crates/visualize/src/main.rs`**: The `load_graph` and `get_stats` IPC commands accept any string path. Doc comments updated cosmetically in step 4 only.
- **`crates/visualize/src/lib.rs`**: `load_graph_data`, `load_graph_data_with_stats`, and `get_stats` delegate to `read_gramps_file` which is extension-agnostic. No functional changes needed; doc comments updated cosmetically in step 4.
- **`crates/cli/src/commands/stats/mod.rs` and `crates/cli/src/commands/validate.rs`**: These commands already accept any extension — they pass the path straight to `read_gramps_file` with no guard. No functional changes needed; module doc comments updated cosmetically in step 4 for consistency.
