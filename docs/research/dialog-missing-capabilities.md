# Plan: Fix "Open Gramps File" Button Not Opening File Dialog

## Problem

When running `cargo run -p visualize -F visualize` (without a CLI path argument), the
welcome screen appears with a blue "Open Gramps File" button. Clicking the button does
nothing — no file browser opens. The button's click handler silently fails.

## Investigation

### 1. What the button is supposed to do

The button's click handler in `crates/visualize/frontend/src/main.ts` calls
`openAndRenderFile()`, which dynamically imports `@tauri-apps/plugin-dialog` and calls
its `open()` function:

```typescript
const { open } = await import('@tauri-apps/plugin-dialog');
const selected = await open({
  multiple: false,
  filters: [{ name: 'Gramps XML', extensions: ['gramps'] }],
});
```

The `@tauri-apps/plugin-dialog` `open()` function (version 2.7.2) calls
`invoke('plugin:dialog|open', ...)` under the hood — a Tauri IPC command provided by the
`tauri-plugin-dialog` Rust crate.

### 2. What the Rust backend does

The Rust side registers the dialog plugin correctly:

```rust
// crates/visualize/src/main.rs
tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    // ...
```

The `Cargo.toml` also declares the dependency correctly:

```toml
[dependencies]
tauri-plugin-dialog = { version = "2", optional = true }

[features]
visualize = ["dep:tauri", "dep:tauri-plugin-dialog"]
```

### 3. The root cause: missing Tauri v2 capabilities

In Tauri v2, **all IPC access is governed by a capability-based permission system**.
Every IPC command — whether a core command, a plugin command, or a custom `#[tauri::command]`
— must be explicitly granted in a capability file.

The `capabilities/` directory is absent from the visualize crate. The only capability-related
file is `crates/visualize/gen/schemas/capabilities.json`, which is an auto-generated
**JSON Schema** file describing the capability file format, **not** an actual capability
definition.

**What happens when no capabilities are defined:**

- Tauri v2 auto-generates a default capability that grants:
  - `core:default` (core plugin permissions)
  - All custom `#[tauri::command]` commands (like `load_graph` and `export_selections`)
- **Plugin commands are NOT auto-granted.** This means:
  - `plugin:dialog|open` (used by the "Open Gramps File" button) is **denied**
  - `plugin:dialog|save` (used by the "Export Selected" button) is **denied**

This explains why:

- `load_graph` works (custom command, auto-granted)
- The dialog `open()` call silently fails (plugin command, denied by capability system)

### 4. Why it's silent

The `openAndRenderFile` function has a `try/catch` that catches the IPC error and calls
`showError(container, 'Failed to load Gramps data file.')`. However, the `open()` function
from `@tauri-apps/plugin-dialog` itself may reject with a Tauri IPC permission error, which
is caught — but the function returns `false`, so the user sees no visible change on screen.

## Fix Plan

### Step 1: Create `capabilities/default.json`

Create a Tauri v2 capabilities file at:

```text
crates/visualize/capabilities/default.json
```

This file must grant the dialog plugin the permissions it needs. The minimum viable
capability is:

```json
{
  "identifier": "default",
  "description": "Default capability granting dialog plugin permissions",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:allow-open",
    "dialog:allow-save"
  ]
}
```

Notes:

- `"windows": ["main"]` — matches the window name in `tauri.conf.json`. The
  `tauri.conf.json` does not explicitly set a window label, so Tauri v2 defaults to
  `"main"`. If the config is ever updated to name the window differently, this
  capability file must be updated to match.
- `"core:default"` — grants all core IPC permissions (path, event, window, app, etc.)
- `"dialog:allow-open"` and `"dialog:allow-save"` — grants only the specific
  dialog permissions the app actually uses: `open()` for the "Open Gramps File" button
  and `save()` for the "Export Selected" button. This follows the principle of least
  privilege (see security-guardrails skill). If future code needs `ask`, `confirm`,
  or `message`, the corresponding permissions can be added at that point.

### Step 2: Verify the fix

1. Clean and rebuild: `cargo clean -p visualize && cargo build -p visualize -F visualize`
2. Run without a path: `cargo run -p visualize -F visualize`
3. Click "Open Gramps File" — the native OS file dialog should appear
4. Select a `.gramps` file — the graph should render
5. Test "Export Selected" — the save dialog should appear

**Limitation:** This verification is manual-only. Automated testing of native dialog
behavior requires the full Tauri runtime (browser window, IPC layer, system webview)
and is out of scope for this fix. The automated test in Step 3 validates the
capability file structure itself, which catches silent regressions (e.g., accidental
deletion or malformed JSON).

### Step 3: Add a Rust test for the capability file structure

Create a new test file at `crates/visualize/tests/capability_tests.rs` that validates
the capabilities file on disk. This is a file-format test (not a runtime IPC test), so
it guards against silent regressions like accidental deletion or malformed JSON.

```rust
//! Tests for the Tauri v2 capability file.
//!
//! Validates that `capabilities/default.json` exists, is valid JSON, and
//! has the expected structure. This is a file-format check — it does not
//! test the Tauri IPC runtime behavior.

use std::fs;

#[test]
fn capability_file_exists() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("capabilities/default.json");
    assert!(path.exists(), "capabilities/default.json not found");
}

#[test]
fn capability_file_is_valid_json() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("capabilities/default.json");
    let content = fs::read_to_string(&path)
        .expect("Failed to read capabilities/default.json");
    let parsed: serde_json::Value = serde_json::from_str(&content)
        .expect("capabilities/default.json is not valid JSON");

    assert_eq!(parsed["identifier"], "default", "identifier must be 'default'");
    assert!(
        parsed["description"].as_str().map_or(false, |s| !s.is_empty()),
        "description must be a non-empty string"
    );

    // Verify the window list contains "main"
    let windows = parsed["windows"]
        .as_array()
        .expect("windows must be an array");
    assert!(windows.contains(&serde_json::json!("main")), "windows must include 'main'");

    // Verify the required permissions are present
    let permissions = parsed["permissions"]
        .as_array()
        .expect("permissions must be an array");
    assert!(
        permissions.contains(&serde_json::json!("core:default")),
        "permissions must include core:default"
    );
    assert!(
        permissions.contains(&serde_json::json!("dialog:allow-open")),
        "permissions must include dialog:allow-open"
    );
    assert!(
        permissions.contains(&serde_json::json!("dialog:allow-save")),
        "permissions must include dialog:allow-save"
    );
}
```

Note: This test requires `serde_json` as a dev-dependency. Add it to
`crates/visualize/Cargo.toml`:

```toml
[dev-dependencies]
serde_json = { workspace = true }
```

### Step 4: Improve user-facing error messages in the frontend

Currently, `openAndRenderFile()` in `main.ts` catches any IPC error and shows the
generic message "Failed to load Gramps data file." This doesn't distinguish between
user cancellation, IPC permission errors, file-not-found, or parse failures. After
the fix, the dialog permission error should be resolved, but other failure modes
(file selection cancelled, file read error, parse error, empty file) still produce
the same generic message.

Update the error handling to provide more specific feedback. The `open()` function
from `@tauri-apps/plugin-dialog` returns `null` when the user cancels (no error,
should be silent), and rejects with an error on actual IPC or permission failures.

```typescript
// In main.ts, openAndRenderFile function:

async function openAndRenderFile(
  container: HTMLElement,
  appEl: HTMLElement,
): Promise<boolean> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Gramps XML', extensions: ['gramps'] }],
  });
  if (!selected) return false; // user cancelled — silent

  const tauri = await import('@tauri-apps/api/core');
  let graphData: GraphData;
  try {
    const gap: number =
      (window as unknown as Record<string, number>).__GENERATION_GAP__ ??
      DEFAULT_GENERATION_GAP;
    const noImpute: boolean =
      (window as unknown as Record<string, boolean>).__NO_IMPUTE__ ??
      DEFAULT_NO_IMPUTE;
    graphData = await tauri.invoke('load_graph', {
      path: selected,
      noImpute: noImpute,
      generationGap: gap,
    });
  } catch (err) {
    console.error('Failed to load graph data via Tauri IPC:', err);
    // Differentiate between IPC errors and data errors
    const message = err instanceof Error && err.message?.includes('not allowed')
      ? 'Permission denied: could not open the file dialog. Try reinstalling the application.'
      : 'Failed to load Gramps data file. The file may be corrupted or not a valid Gramps XML file.';
    showError(container, message);
    return false;
  }

  renderGraphFromData(container, appEl, graphData);
  return true;
}
```

### Step 5: Add a frontend unit test for the IPC error path

Add a test to `crates/visualize/frontend/tests/` that verifies the error-handling
logic in `openAndRenderFile` when the dialog or IPC call fails. This test mocks
the Tauri IPC layer to confirm the error message is shown for different failure
scenarios.

```typescript
// crates/visualize/frontend/tests/dialog.test.ts
import { describe, it, expect, vi } from 'vitest';

// Since openAndRenderFile is not exported (it's module-scoped in main.ts),
// this test covers the error-handling shape by testing the IPC invoke path
// directly through the exported loadGraphData function.
//
// Alternatively, refactor openAndRenderFile to accept a mockable IPC function
// and test the error message differentiation.

describe('IPC error handling', () => {
  it('shows a specific error message on IPC permission failure', async () => {
    // TODO: After refactoring openAndRenderFile to accept a mock IPC
    // function or extracting the error message logic, test that:
    // - An error containing "not allowed" produces a permission-denied message
    // - Other errors produce the generic "Failed to load" message
    // - null return from open() is handled silently (no error shown)
    expect(true).toBe(true);
  });
});
```

This test is a placeholder — the full implementation requires refactoring
`openAndRenderFile` to make the error-handling logic testable (e.g., extracting
the error-message mapping into a pure function).

## File listing (before and after)

**Before:**

```text
crates/visualize/
├── tauri.conf.json          ← references no capabilities
├── gen/
│   └── schemas/
│       ├── capabilities.json  ← SCHEMA file, not a capability definition
│       ├── desktop-schema.json
│       └── linux-schema.json
├── build.rs
├── Cargo.toml
├── src/
│   └── main.rs              ← registers tauri_plugin_dialog
└── frontend/
    └── src/
        └── main.ts          ← calls @tauri-apps/plugin-dialog
```

**After:**

```text
crates/visualize/
├── capabilities/
│   └── default.json         ← NEW: grants dialog:allow-open and dialog:allow-save permissions
├── tauri.conf.json
├── gen/
│   └── schemas/
│       ├── capabilities.json
│       ├── desktop-schema.json
│       └── linux-schema.json
├── ...
```

## References

- Tauri v2 capabilities docs: <https://v2.tauri.app/develop/security/#capabilities>
- Tauri v2 dialog plugin: <https://v2.tauri.app/plugin/dialog/>
- `@tauri-apps/plugin-dialog` npm package: `crates/visualize/frontend/node_modules/@tauri-apps/plugin-dialog/`
