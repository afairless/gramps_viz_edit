# Implementation Plan: Fix Selections Export Format Inconsistency

Source: `docs/research/fix-selections-export-format-inconsistency.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: add Deserialize derive to visualizer SelectionExport` | Deserialize derive + compile test | `crates/visualize/src/graph_data.rs` | Unit (compile-time assertion) |
| 2 | `fix: wrap selections in envelope when writing via Tauri export command` | Rust export_selections fix + integration tests | `crates/visualize/src/main.rs`, `crates/visualize/Cargo.toml`, `crates/visualize/tests/integration.rs` | Unit (serialization shape), Integration (round-trip via integrate parser) |
| 3 | `fix: send full SelectionExport envelope to Tauri IPC export command` | Frontend IPC payload fix | `crates/visualize/frontend/src/selection.ts` | — (existing buildSelectionExport tests already cover envelope construction) |
| 4 | `test: add e2e test for selections export → integrate diff-viz roundtrip` | E2E integration test | `crates/cli/tests/e2e.rs` | Integration (subprocess-based E2E) |

## Step details

### Step 1 — Add `Deserialize` to `SelectionExport`

- In `crates/visualize/src/graph_data.rs`, add `Deserialize` to the derive list for `SelectionExport`.
- Add a compile-time test inside `graph_data.rs` that asserts `SelectionExport: Deserialize`.

### Step 2 — Update Rust `export_selections` to accept `SelectionExport` and write wrapped format

- In `crates/visualize/src/main.rs`, change `export_selections` to accept `path: String` and a single `export: visualize::SelectionExport` parameter (with `#[serde(flatten)]` for Tauri IPC compatibility).
- Serialize the full `SelectionExport` (the envelope) instead of the bare array.
- Add tests to `crates/visualize/tests/integration.rs`:
  - (a) Serialize a `SelectionExport` and verify the JSON has the expected envelope keys (`exported_at`, `file`, `selections`) and `selections` is an array.
  - (b) Cross-crate round-trip: serialize a `SelectionExport`, write to temp file, parse with `integrate::parse_selections_json`, verify handles match.
- Add `integrate = { path = "../integrate" }` as a dev-dependency of the `visualize` crate (needed for the round-trip test).

### Step 3 — Update frontend `exportToFile` to send full envelope

- In `crates/visualize/frontend/src/selection.ts`, change the `tauri.invoke('export_selections', ...)` call to include `exported_at` and `file` from `exportData` alongside `selections`.

### Step 4 — Add cross-crate E2E test

- Add a test to `crates/cli/tests/e2e.rs` that:
  1. Creates a temp diff CSV with 3 Person rows.
  2. Creates a matching selections JSON (wrapped envelope format) with 2 selections.
  3. Runs `gramps-gen integrate diff-viz --diff <csv> --selections <json>` as a subprocess.
  4. Verifies exit code 0 and stdout/CSV output contains merged rows with correct fields.
