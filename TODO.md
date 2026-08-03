# Implementation Plan: Fix "Open Gramps File" Button Not Opening File Dialog

Source: `docs/research/dialog-missing-capabilities.md`

## Context

The "Open Gramps File" button on the welcome screen silently fails because Tauri v2's
capability-based permission system does not grant the `tauri-plugin-dialog` plugin the
required IPC permissions. Custom `#[tauri::command]` commands are auto-granted, but
plugin commands (like `plugin:dialog|open`) are denied unless explicitly listed in a
capability file under `crates/visualize/capabilities/`.

## Steps

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix: Create Tauri v2 capabilities file for dialog plugin permissions` | Capability file | `crates/visualize/capabilities/default.json` | — (manual verification only — native dialog behavior requires the full Tauri runtime) |
| 2 | `test: Add capability file structure validation test` | Capability file tests | `crates/visualize/tests/capability_tests.rs`, `crates/visualize/Cargo.toml` (dev-deps) | Unit |
| 3 | `feat: Improve frontend error messages for IPC failures` | Error message differentiation | `crates/visualize/frontend/src/main.ts` | — |
| 4 | `test: Add frontend test for IPC error handling` | Frontend error-handling tests | `crates/visualize/frontend/tests/dialog.test.ts` | Unit |

## Notes

- **Step 1** is the core fix. After it, manually verify by running
  `cargo run -p visualize -F visualize`, clicking "Open Gramps File", and confirming
  the native OS file dialog appears.
- **Step 2** validates the capability file structure on disk. It guards against silent
  regressions (accidental deletion, malformed JSON) and runs in CI.
- **Step 3** improves the error message shown when `load_graph` IPC fails, distinguishing
  permission errors from data errors.
- **Step 4** adds a frontend unit test for the error-handling logic. The test is a
  placeholder — full implementation requires refactoring `openAndRenderFile` to make the
  error-message mapping independently testable.
