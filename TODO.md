# Implementation Plan: Fix Save Button in Visualizer

Source: `docs/research/fix-selections-export-save-button.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix: correct IPC camelCase key and delay blob URL revocation in exportToFile` | Selection export IPC + blob fallback fix | `crates/visualize/frontend/src/selection.ts` | — (cannot unit-test Tauri IPC or blob download timing; manual verification) |
| 2 | `test: add Rust integration test for export_selections JSON output` | Rust export_selections integration test | `crates/visualize/tests/integration.rs` | Integration |

## Step details

### Step 1 — Fix IPC key casing and blob URL revocation

**Primary bug** (camelCase/snake_case mismatch): The frontend `exportToFile` sends `exported_at` (snake_case) as a Tauri IPC payload key, but Tauri v2's `#[tauri::command]` macro expects **camelCase** JavaScript parameter names (`exportedAt`), which it then maps to the Rust snake_case parameter name `exported_at`. Sending `exported_at` directly causes Tauri to reject the `invoke()` call with an error such as `"unknown field exported_at, expected exportedAt"`, so no file is saved.

**Fix**: Change the key from `exported_at` to `exportedAt` in the `tauri.invoke()` call.

**Secondary bug** (premature blob URL revocation): When the Tauri IPC path fails (the catch block runs), `URL.revokeObjectURL(url)` is called **synchronously** right after `a.click()`. Browsers start the download asynchronously, so the blob URL is revoked before the download begins, and no file is saved.

**Fix**: Wrap `URL.revokeObjectURL(url)` in a `setTimeout(..., 100)` to give the browser time to start the download.

Both changes in `crates/visualize/frontend/src/selection.ts`:

```typescript
// Primary fix: exported_at → exportedAt (camelCase for Tauri IPC)
await tauri.invoke('export_selections', {
    path,
    exportedAt: exportData.exported_at,  // ← was: exported_at
    file: exportData.file,
    selections: exportData.selections,
});
```

```typescript
// Secondary fix: delay URL revocation
a.click();
setTimeout(() => URL.revokeObjectURL(url), 100);  // ← was: URL.revokeObjectURL(url);
```

### Step 2 — Add Rust integration test (optional)

The `export_selections` Tauri command is pure file I/O — it writes a `SelectionExport` JSON to a given path. Add a test to `crates/visualize/tests/integration.rs` that:

1. Constructs a `SelectionExport` with known values
2. Writes it to a temp file (directly, without going through Tauri IPC)
3. Reads the file back and parses the JSON
4. Asserts:
   - Top-level is an object (not an array)
   - Keys `exported_at`, `file`, `selections` exist
   - `selections` is an array
   - Round-trip: `serde_json::from_str::<SelectionExport>(&json)` succeeds

This provides regression coverage for the Rust side independent of Tauri IPC.
