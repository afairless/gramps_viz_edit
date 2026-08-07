# Bug Report: Save Button in Visualizer Produces No File

## Symptoms

When using the visualizer's "Export Selected" button:

1. User selects nodes in the graph
2. Click "Export Selected"
3. The native save dialog appears (user picks a location and clicks Save)
4. **No file is saved** — the file does not appear at the chosen path

This bug was introduced in the most recent commit series that changed the selection
export format (commits `d346d5a`, `b66a0cd`, `31b1001`).

## Root Cause

The bug is a **camelCase / snake_case mismatch** between the frontend JavaScript and
the Tauri v2 IPC layer. Two compounding issues cause both the Tauri path and the
browser-fallback path to fail.

---

### Primary Bug: Parameter name casing mismatch in Tauri IPC

Tauri v2's `#[tauri::command]` macro expects JavaScript-style **camelCase** parameter
names from the frontend, then deserializes them to Rust-style **snake_case** on the
backend. This is confirmed by the working `load_graph` IPC call:

| Frontend sends | Rust receives | Works? |
|---|---|---|
| `noImpute` | `no_impute` | ✅ |
| `generationGap` | `generation_gap` | ✅ |

The `export_selections` call sends:

```typescript
// selection.ts — exportToFile() function — BROKEN
await tauri.invoke('export_selections', {
    path,
    exported_at: exportData.exported_at,  // ← snake_case, but Tauri expects camelCase!
    file: exportData.file,
    selections: exportData.selections,
});
```

Tauri v2 expects `exportedAt` (camelCase) from the frontend, which it then maps to
the Rust parameter `exported_at` (snake_case). Because the frontend sends
`exported_at` (snake_case) directly, Tauri's deserialization cannot match it to
the expected camelCase parameter. The invoke call fails with an error such as
`"unknown field \`exported_at\`, expected \`exportedAt\`"`.

Since the `exported_at` parameter is required (not `Option<String>`), the entire
IPC call fails, throwing an exception from `tauri.invoke()`.

**The fix**: Change the key from `exported_at` to `exportedAt` in the frontend
invoke call.

```typescript
// Fixed
await tauri.invoke('export_selections', {
    path,
    exportedAt: exportData.exported_at,  // ← camelCase, matches Tauri convention
    file: exportData.file,
    selections: exportData.selections,
});
```

---

### Secondary Bug: Premature blob URL revocation in browser fallback

When the Tauri IPC call fails (due to the primary bug), the `catch` block in
`exportToFile` runs the browser fallback:

```typescript
// selection.ts — exportToFile() catch block — BROKEN
} catch {
    const blob = new Blob([JSON.stringify(exportData, null, 2)], {
        type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = exportData.file;
    a.click();
    URL.revokeObjectURL(url);  // ← revokes BEFORE download starts
    return exportData.file;
}
```

`URL.revokeObjectURL(url)` is called **synchronously** after `a.click()`. In many
browser/webview implementations, the download triggered by `a.click()` is
**asynchronous** — the browser hasn't started fetching the blob yet when the URL
is revoked. This results in a failed/silent download: no save dialog appears, and
no file is created.

This is a well-known anti-pattern. The standard fix is to delay the revocation:

```typescript
// Fixed
a.click();
// Delay revocation to give the browser time to start the download
setTimeout(() => URL.revokeObjectURL(url), 100);
```

**Trade-off**: `setTimeout(100)` is a heuristic — 100ms is sufficient for all
normal Tauri webview scenarios, but on extremely slow systems the revocation
could still race the download start. For full robustness, the URL could be
revoked in a `click` event handler or via `requestAnimationFrame`, but the
timeout approach is the industry-standard workaround and adequate here.

Note: This bug is *latent* when the Tauri path works correctly (the catch block
isn't reached). But since the primary bug causes the Tauri path to always fail,
the browser fallback is currently the only code path, and the premature revocation
prevents even the fallback from working.

---

## Data Flow After Fix

```
User clicks "Export Selected"
  │
  ├── buildSelectionExport()  →  SelectionExport { exported_at, file, selections }
  │
  └── exportToFile(exportData)
        │
        ├── Tauri save dialog opens ✓
        │     └── User picks path, clicks Save
        │
        ├── tauri.invoke('export_selections', {
        │       path,
        │       exportedAt: ...,     ← FIXED: camelCase
        │       file: ...,
        │       selections: ...
        │   })
        │     │
        │     └── Rust backend writes file ✓
        │
        └── (fallback only if Tauri unavailable)
              └── Blob download with delayed URL revocation ✓
```

## Files Changed

| File | Change | Risk |
|---|---|---|
| `crates/visualize/frontend/src/selection.ts` | Change `exported_at` to `exportedAt` in Tauri invoke call + delay `URL.revokeObjectURL` | Very Low — one-line key rename + one-line delay addition |

### Important: Only the IPC key changes

The TypeScript `SelectionExport` interface **retains** `exported_at` (snake_case)
— that is the internal data shape, not the IPC wire format. The interface does
not need to change. Only the `invoke()` parameter name is corrected to match
Tauri's camelCase convention.

## Testing Plan

### Frontend tests (`crates/visualize/frontend/tests/selection.test.ts`)

No existing unit tests cover `exportToFile` (it's an async integration function that
depends on Tauri IPC). The `buildSelectionExport` and `buildSelectedPeople` pure
functions are already tested. No test changes needed for the TS logic — the fix is
a string-key rename (primary bug) and a timing change (secondary bug) that cannot
be verified in isolated unit tests.

### Rust tests

The `export_selections` command signature and behavior are correct — only the
frontend IPC invocation was wrong, so no Rust-side changes are needed.

However, a **Rust integration test** could verify that `export_selections`
correctly writes a `SelectionExport` JSON payload to disk when called with
properly deserialized arguments. This would provide regression coverage for
the Rust side independent of the Tauri IPC layer. This is optional — the fix
is low-risk and the command is pure file I/O.

### Manual verification

1. Build the visualizer: `cargo build -p visualize --features visualize`
2. Load a `.gramps` file
3. Select 1+ nodes
4. Click "Export Selected"
5. Choose a save path
6. Verify the file appears with correct JSON envelope format

### E2E tests

The existing E2E test `e2e_integrate_diff_viz_wrapped_envelope` in
`crates/cli/tests/e2e.rs` tests the `integrate diff-viz` pipeline with the
envelope format, but does NOT test the visualizer's save-button flow (it creates
the selection JSON file manually). The visualizer button flow can only be tested
manually or via Tauri integration test harness.

## Edge Cases

| Case | Expected behavior |
|---|---|
| User cancels save dialog | `save()` returns null, `exportToFile` returns null, no fallback — handled correctly (the `if (!path) return null` guard fires before the invoke call) |
| Tauri IPC unavailable (dev mode) | Catch block runs, blob download triggers with delayed revocation — should work in dev webviews |
| Very large selection set | Serialization and blob creation scale with data size — no change from before |
| `exportedAt` value is null/undefined (defensive) | The Rust `export_selections` parameter is `exported_at: String` (not `Option<String>`), so null would fail deserialization. Currently unreachable — `buildSelectionExport` always produces a valid ISO string |
| No selections (Export button disabled) | Button is disabled when `manager.size === 0`, so this code path is unreachable |
