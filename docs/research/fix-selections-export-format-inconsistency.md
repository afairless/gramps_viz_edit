# Plan: Fix Selections Export Format Inconsistency Between Tauri and Browser Paths

Source: `crates/visualize/src/main.rs` (Rust Tauri command), `crates/visualize/frontend/src/selection.ts`
(frontend export logic), `crates/integrate/src/json_reader.rs` (JSON consumer)

## Motivation

The visualizer (`gramps-gen visualize`) provides two export paths for selected persons:

1. **Tauri IPC path** — Writes the file via the Rust `export_selections` Tauri command
2. **Browser fallback path** — Downloads the file as a blob via a browser `<a>` element

These two paths produce **different JSON formats**, causing the `integrate diff-viz`
subcommand to fail with a parse error when reading selections exported via the
Tauri path.

The `integrate` crate's `json_reader.rs` expects the **wrapped envelope** format
(`{"exported_at": ..., "file": ..., "selections": [...]}`), which is what the
browser path produces. The Tauri path writes a **bare array** (`[...]`).

The `integrate diff-viz` subcommand merges a Gramps diff CSV with visualizer
selection JSON, matching Person rows by handle, and produces a combined CSV or
JSON report.

## Root Cause

### Frontend (`selection.ts` — `exportToFile`)

Two code paths:

```typescript
// Tauri path — sends only the selections array to Rust
await tauri.invoke('export_selections', {
  path,
  selections: exportData.selections,   // <── bare array, no envelope
});

// Browser fallback path — writes full SelectionExport object
const blob = new Blob([JSON.stringify(exportData, null, 2)], ...);
//                                 ^^^^^^^^^
//                                 full {exported_at, file, selections}
```

### Rust backend (`main.rs` — `export_selections`)

Writes the raw `Vec<SelectedPerson>` — a bare JSON array:

```rust
fn export_selections(
    path: String,
    selections: Vec<visualize::SelectedPerson>,
) -> Result<String, String> {
    let export = serde_json::to_string_pretty(&selections)  // bare array
    std::fs::write(&path, &export)
}
```

### Integrate consumer (`json_reader.rs` — `parse_selections_json`)

Expects the wrapped envelope:

```rust
struct SelectionExport {
    selections: Vec<Selection>,
}
```

| Component | Format |
|---|---|
| Frontend browser export | `{exported_at, file, selections: [...]}` |
| Frontend Tauri export | Sends `selections: [...]` (array only) to Rust IPC |
| Rust `export_selections` | Writes `[...]` (bare array) |
| `integrate` JSON reader | Expects `{selections: [...]}` |

## Design Decisions (from user consultation)

| Decision | Choice | Rationale |
|---|---|---|
| Root cause fix location | Rust Tauri command | The root cause is in Rust: `export_selections` writes a bare array instead of the envelope. |
| Frontend change scope | Send full `SelectionExport` from frontend | Add `exported_at` and `file` fields to the Tauri IPC payload so Rust receives the complete object. More faithful to the frontend's `buildSelectionExport()` than reconstructing fields on the Rust side. Note: the frontend change is *necessary* (Rust must receive the envelope fields), not optional — the fix spans both Rust and frontend. |
| Rust IPC signature | Single `export` parameter | Accept a single `SelectionExport` struct (via Deserialize) instead of three individual params. Cleaner API, and `SelectionExport` already carries all envelope fields. |
| Type sharing | Keep independent definitions | The `integrate` crate defines its own `Selection`/`SelectionExport` types rather than depending on the `visualize` crate (which has heavy Tauri/webview deps). The `visualize::SelectedPerson` and `integrate::Selection` structs are structurally similar but independent — no refactoring needed. |

## Design

### Data flow after fix

```
Frontend (selection.ts)
│
├── User clicks Export
│
├── buildSelectionExport(file, people) → SelectionExport {exported_at, file, selections}
│
├── Try Tauri path:
│   tauri.invoke('export_selections', {
│       path,                    ← from save dialog
│       exported_at,             ← NOW SENT (was missing)
│       file,                    ← NOW SENT (was missing)
│       selections               ← already sent
│   })
│   │
│   ▼
│   Rust export_selections()
│   ├── Receives full SelectionExport (with exported_at + file)
│   ├── serde_json::to_string_pretty(&full_export)   ← NOW wraps envelope
│   ├── std::fs::write(&path, &export)
│   └── Returns path to frontend
│
└── Fallback (if Tauri unavailable):
    JSON.stringify(exportData, null, 2)  ← unchanged, already correct
```

### Rust-side: `main.rs` changes

**Before:**

```rust
#[tauri::command]
fn export_selections(
    path: String,
    selections: Vec<visualize::SelectedPerson>,
) -> Result<String, String> {
    let export = serde_json::to_string_pretty(&selections)  // bare array
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, &export)
        .map_err(|e| format!("Cannot write to '{}': {}", path, e))?;
    Ok(path)
}
```

**After:**

```rust
#[tauri::command]
fn export_selections(
    path: String,
    #[serde(flatten)]
    export: visualize::SelectionExport,
) -> Result<String, String> {
    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, &json)
        .map_err(|e| format!("Cannot write to '{}': {}", path, e))?;
    Ok(path)
}
```

Note: Tauri deserializes each command parameter independently from the IPC
payload. `path` is a top-level key from the save dialog; `export` is
deserialized from the remaining top-level keys (`exported_at`, `file`,
`selections`). This requires `SelectionExport` to derive `Deserialize`.

### Frontend-side: `selection.ts` changes

**Before:**

```typescript
await tauri.invoke('export_selections', {
  path,
  selections: exportData.selections,
});
```

**After:**

```typescript
await tauri.invoke('export_selections', {
  path,
  exported_at: exportData.exported_at,
  file: exportData.file,
  selections: exportData.selections,
});
```

The frontend sends the three envelope fields as top-level keys alongside
`path`. Tauri deserializes `exported_at`, `file`, and `selections` together
into the `SelectionExport` parameter.

## Files Changed

| File | Change | Risk |
|---|---|---|
| `crates/visualize/src/graph_data.rs` | Add `Deserialize` to `SelectionExport` derive list | Low — adds capability without changing behavior |
| `crates/visualize/src/main.rs` | Accept single `export` param (with `#[serde(flatten)]`); serialize full envelope instead of bare array | Low — IPC payload shape changes (new top-level keys), but Rust binary and frontend ship together |
| `crates/visualize/frontend/src/selection.ts` | Send `exported_at` + `file` to Tauri IPC | Low — same reasoning |
| `crates/integrate/src/json_reader.rs` | **No change needed** — already expects wrapped format | None |

## Testing Plan

### Rust tests (`crates/visualize/tests/integration.rs`)

No existing Rust unit tests for `export_selections` (it's a Tauri command, hard
to unit-test directly via IPC). Instead, test the serialization logic directly:

1. **SelectionExport serialization test** (new test in
   `crates/visualize/tests/integration.rs`): Construct a `SelectionExport`
   with known values, serialize to `serde_json::to_string_pretty`, parse the
   result as a `serde_json::Value`, and assert:
   - Top-level is an object (not an array)
   - Keys `exported_at`, `file`, `selections` exist
   - `selections` is an array
   - Round-trip: `serde_json::from_str::<SelectionExport>(&json).unwrap()` succeeds

2. **Cross-crate roundtrip test** (new test in
   `crates/visualize/tests/integration.rs`): Construct a `SelectionExport`,
   serialize to JSON string, then call
   `integrate::parse_selections_json_str(&json)` and verify it returns the
   expected `Vec<Selection>` with matching handles.

3. **Manual end-to-end**: Run the visualizer, export selections, verify the
   file starts with `{` not `[`.

### Rust compile-time check (`graph_data.rs` tests)

Add a one-line compile test inside `graph_data.rs` that verifies
`SelectionExport: Deserialize` compiles, protecting against accidental
removal of the derive:

```rust
#[test]
fn selection_export_is_deserializable() {
    fn assert_deserialize<T: serde::Deserialize>() {}
    assert_deserialize::<SelectionExport>();
}
```

### Frontend tests (`crates/visualize/frontend/tests/selection.test.ts`)

The existing `exportToFile` function isn't unit-tested (it calls Tauri IPC or
browser APIs). The `buildSelectionExport` function is already tested. No
frontend test changes needed.

### Integrate tests (`crates/integrate`)

The existing `json_reader.rs` tests already test the wrapper format. The
`parse_bare_array` test confirms it correctly rejects bare arrays. **No
changes needed** — the fix makes the input match what these tests expect.

### Cross-crate E2E test (`crates/cli/tests/e2e.rs`)

Add an E2E test that exercises the full pipeline:

1. Create a temporary diff CSV with 3 Person rows:

   ```csv
   handle,name,status
   abc-1,John Smith,modified
   abc-2,Jane Doe,added
   abc-3,Bob Brown,deleted
   ```

2. Create a matching selections JSON (wrapped envelope format):

   ```json
   {
     "exported_at": "2025-01-15T10:30:00.000Z",
     "file": "selections.json",
     "selections": [
       {"handle":"abc-1","name":"John Smith","birth_date":"1840-07-13","death_date":"1910-03-22","gender":"male","family_group":3},
       {"handle":"abc-2","name":"Jane Doe","birth_date":null,"death_date":null,"gender":"female","family_group":3}
     ]
   }
   ```

3. Run `gramps-gen integrate diff-viz --diff <csv_path> --selections <json_path>`
4. Assert exit code 0 and stdout contains the merged rows (abc-1 and abc-2,
   with diff status + selection fields; abc-3 excluded because it has no
   matching selection)
5. Optionally, re-run with `--format json` and verify structured output

## Edge Cases

| Case | Expected behavior |
|---|---|
| `selections` is empty array | Writes `{"exported_at": ..., "file": ..., "selections": []}` — valid |
| `exported_at` has unusual timestamp format | Passed through as-is from frontend; stored in envelope |
| `file` contains path separators | Extracted from the frontend's `buildSelectionExport` call; stored as-is |
| Very large selections array | Serialization scales with `Vec` size — same as before |
| Old selections files (bare arrays) | `integrate` reader rejects them (designed behavior; re-export to fix) |

## Implementation Plan

### Dependencies

No new crate dependencies. Changes are confined to `crates/visualize/src/main.rs`,
`crates/visualize/src/graph_data.rs`, and `crates/visualize/frontend/src/selection.ts`.

### Step 1 — Add `Deserialize` to `SelectionExport` in `graph_data.rs`

| Commit | `chore: add Deserialize derive to visualizer SelectionExport` |
|---|---|
| Crate | `visualize` |
| Change | Add `Deserialize` to the derive list for `SelectionExport` in `crates/visualize/src/graph_data.rs`. |
| Tests | Add a compile test that asserts `SelectionExport: Deserialize`. |

This is needed so the Tauri command can deserialize the incoming IPC payload
directly into a `SelectionExport` struct. Currently it only derives `Serialize`.
The compile test prevents accidental removal of the derive.

### Step 2 — Update Rust `export_selections` to accept `SelectionExport` and write wrapped format

| Commit | `fix: wrap selections in envelope when writing via Tauri export command` |
|---|---|
| Crate | `visualize` |
| Changes | In `crates/visualize/src/main.rs`, change `export_selections` to accept `path: String` and a single `export: visualize::SelectionExport` parameter (with `#[serde(flatten)]` for Tauri IPC compatibility). Serialize the full `SelectionExport` instead of the bare array. |
| Tests | Add tests to `crates/visualize/tests/integration.rs`: (a) serialize a `SelectionExport` and verify the JSON has the expected envelope keys; (b) round-trip through `integrate::parse_selections_json_str` to confirm cross-crate compatibility. |

### Step 3 — Update frontend `exportToFile` to send full envelope

| Commit | `fix: send full SelectionExport envelope to Tauri IPC export command` |
|---|---|
| Crate | `visualize` (frontend) |
| Changes | In `crates/visualize/frontend/src/selection.ts`, change the `tauri.invoke('export_selections', ...)` call to include `exported_at` and `file` from `exportData`. |
| Tests | Existing `selection.test.ts` tests for `buildSelectionExport` remain valid. `exportToFile` is an async integration function not directly unit-tested. |

### Step 4 — Add cross-crate E2E test for the full pipeline

| Commit | `test: add e2e test for selections export → integrate diff-viz roundtrip` |
|---|---|
| Crate | `cli` |
| Change | Add a test to `crates/cli/tests/e2e.rs` that creates a temp diff CSV, a temp selections JSON (wrapped envelope), runs `gramps-gen integrate diff-viz` as a subprocess, and verifies the merged output. |
| Tests | The test itself is the E2E test (see [Cross-crate E2E test](#cross-crate-e2e-test-cratesclitestse2ers) above for exact inputs and assertions). |

## Future Work

- [ ] Consider adding a `SelectionsReadError` variant for bare arrays with a helpful message suggesting re-export
