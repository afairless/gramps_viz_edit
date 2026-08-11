# Implementation Plan: Event Cleaning Post-Process

Source: `docs/research/event-cleaning-postprocess.md`

## Overview

Add an automatic post-processing step to the `gramps-gen delete` command that
removes orphaned events (marked `Pending` in the manifest after reconciliation)
from the `.gramps` XML output using a streaming `quick-xml` filter. Produces
a `<input-stem>-no-events.gramps` file alongside the existing `-cleaned.gramps`.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add streaming XML event filter for cleaning orphaned events` | Clean module | `crates/cli/src/commands/clean.rs` (new), `crates/cli/src/commands/mod.rs` (+1 line), `crates/cli/src/error.rs` (+`From<CleanError> for CliError`) | Unit |
| 2 | `feat: integrate event cleaning into delete command pipeline` | Delete integration | `crates/cli/src/commands/delete.rs` (~30 lines; make `strip_gramps_extensions` `pub(crate)`, add clean step, derive helper) | — |
| 3 | `test: add integration tests for event cleaning in delete pipeline` | Integration tests | `crates/cli/tests/e2e_delete.rs` (new tests) | Integration |

## Step Details

### Step 1 — Clean module

**Key deliverables:**

- `crates/cli/src/commands/clean.rs` — new module with:
  - `CleanStats` struct (events_removed, events_not_found)
  - `CleanError` enum (Io, XmlParse variants)
  - `clean_events_xml(input, output, event_handles) -> Result<CleanStats, CleanError>` — streaming XML filter using `quick-xml` Reader + Writer with transparent gzip decompression via `flate2`
  - Namespace-aware element detection (using `strip_prefix` from `gramps_reader::xml`)
  - Handle attribute detection (using `read_handle_attr` from `gramps_reader::xml`)
  - Temp-file write-then-rename for atomic output
  - Unit tests: remove_single_event, remove_self_closing_event, keep_unrelated_event, no_events_to_remove, handle_not_found, namespace_prefixed_event, nested_eventtype, flat_type, multiple_events_mixed, non_event_xml_preserved, gzip_input
- `crates/cli/src/commands/mod.rs` — add `pub mod clean;`
- `crates/cli/src/error.rs` — add `From<CleanError> for CliError` impl mapping Io→Io, XmlParse→XmlParseError

**Test scope:** Unit tests for all 11 cases described in the plan.

### Step 2 — Delete integration

**Key deliverables:**

- `crates/cli/src/commands/delete.rs`:
  - Make `strip_gramps_extensions` `pub(crate)` (used by clean.rs's derive helper)
  - Add `derive_no_events_path(input_path: &Path) -> PathBuf` helper (uses `strip_gramps_extensions`)
  - After reconciliation (step 14 in current flow), add clean step:
    1. Collect pending event handles from manifest's `events` plan
    2. If non-empty, call `clean::clean_events_xml(&output_path, &no_events_path, &pending_events)`
    3. Log stats
    4. Mark pending events as `Deleted` in manifest
    5. Re-save manifest (overwrite checkpoint copy)
  - If no pending events, log info and skip

**Test scope:** — (no new standalone tests; the module is wired but only exercised by integration tests)

### Step 3 — Integration tests

**Key deliverables:**

- `crates/cli/tests/e2e_delete.rs` — add tests:
  - `e2e_delete_with_event_clean`: Generate a family with events, delete people, verify `-no-events.gramps` has events removed
  - `e2e_delete_no_pending_events_noop`: When all events are deleted by Gramps, `-no-events` = `-cleaned`
  - `e2e_delete_manifest_re_saved_after_clean`: After successful clean, manifest on disk has pending events marked `Deleted`

**Test scope:** Integration (subprocess-based, full pipeline)
