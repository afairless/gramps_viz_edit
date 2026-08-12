# Note Deletion Post-Process

> Status: Plan  
> Created: 2026-08-18  
> Target: `gramps-gen delete` command — add note cleaning step after event cleaning

## 1. Problem Statement

The `gramps-gen delete` command currently:

1. Identifies orphaned objects (people, families, events, notes, etc.) via the Rust cascade engine
2. Delegates people-only deletion to the Python Gramps backend
3. Reconciles the manifest against what Gramps actually deleted
4. Runs an **event cleaning** post-process that removes pending (orphaned) events from the XML, producing `<stem>-deleted_2.gramps`

Notes that the cascade identified as orphaned appear in the manifest under
`"notes"` with `status: "pending"` (because the Python backend only deletes
people — it never touches notes). These notes survive into the final output,
leaving orphaned notes in the `.gramps` file.

**Example** from a real run (3 people deleted, events cleaned, but orphaned
notes remain):

```json
{
  "plan": {
    "people":   { "to_delete": [3 items, status: "deleted"] },
    "families": { "to_delete": [1 item,  status: "deleted"] },
    "events":   { "to_delete": [3 items, status: "deleted"] },
    "notes":    { "to_delete": [2 items, status: "pending"] }
  }
}
```

The goal is to remove these pending notes from the `.gramps` XML output as an
automatic post-processing step, producing `<stem>-deleted_3.gramps`.

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Integration point | After event cleaning, as a new step in the `delete` pipeline | Follows the existing pattern; produces a third output file |
| Output file | `<input-stem>-deleted_3.gramps` | Preserves `-deleted_2.gramps` (events cleaned); `-deleted_3.gramps` adds note cleaning on top |
| Scope | Manifest-only notes with `status: "pending"` | Same approach as event cleaning — only remove notes the cascade already identified. No re-cascade after event removal |
| Implementation | Generalize `clean.rs` to support any element type (`clean_elements_xml`), add `clean_notes_xml` wrapper | Avoids duplicating the streaming XML filter logic; keeps clean.rs as the single source of truth for element removal |
| Cascade depth | Notes only (no re-cascade) | The cascade already handles transitive closure on the graph before any deletions occur. Notes referenced only by events-to-be-deleted are already in the manifest. After event cleaning removes those events, no *newly* orphaned notes can appear because those notes were already identified |

## 3. Architecture

### 3.1 Pipeline (updated)

```
Parse → Cascade → Review → Manifest → Python backend → Reconcile
    → [Checkpoint: save -cleaned.gramps + manifest to disk]
    → Clean Events (remove pending events from XML) → <stem>-deleted_2.gramps
    → Clean Notes  (remove pending notes from <stem>-deleted_2.gramps) → <stem>-deleted_3.gramps
```

Each cleaning step reads the previous step's output and writes a new file.
The manifest is re-saved after each successful step with the corresponding
type's pending entries marked `Deleted`.

### 3.2 Component Diagram

```
┌──────────────────────────────────────────────────────────────┐
│  delete.rs (orchestrator)                                     │
│                                                               │
│  After event cleaning succeeds:                               │
│  1. Collect pending note handles from manifest                │
│  2. Call clean_notes_xml(deleted_2_path, deleted_3_path,     │
│                          pending_notes)                       │
│  3. Log result, mark notes Deleted in manifest                │
│  4. Re-save manifest                                          │
└──────────────────────────┬───────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  clean.rs (generalized)                                       │
│                                                               │
│  pub fn clean_elements_xml(                                   │
│      input: &Path,                                            │
│      output: &Path,                                           │
│      element_name: &str,   // "event" or "note"              │
│      handles: &HashSet<String>,                               │
│  ) -> Result<CleanStats, CleanError>                          │
│                                                               │
│  pub fn clean_events_xml(input, output, handles)              │
│      → clean_elements_xml(input, output, "event", handles)   │
│                                                               │
│  pub fn clean_notes_xml(input, output, handles)               │
│      → clean_elements_xml(input, output, "note", handles)    │
│                                                               │
│  Uses quick-xml Reader + Writer to:                           │
│  1. Stream through the input .gramps file                     │
│  2. Detect <event> or <note> elements by handle attribute     │
│  3. Skip elements whose handle is in the deletion set         │
│  4. Pass everything else through verbatim                     │
└──────────────────────────────────────────────────────────────┘
```

## 4. Implementation Plan

### 4.1 Refactor `crates/cli/src/commands/clean.rs`

#### Rename `CleanStats` fields

```rust
/// Statistics from an element cleaning run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanStats {
    /// Number of elements removed from the XML.
    pub elements_removed: usize,
    /// Number of handles requested for removal but not found in the XML.
    pub elements_not_found: usize,
}
```

The field names `events_removed` and `events_not_found` are renamed to
`elements_removed` and `elements_not_found` since the struct now represents
results for any element type, not just events.

#### Extract core function: `clean_elements_xml`

```rust
pub fn clean_elements_xml(
    input: &Path,
    output: &Path,
    element_name: &str,    // "event" or "note"
    handles: &HashSet<String>,
) -> Result<CleanStats, CleanError>
```

The existing `clean_events_xml` function body is moved into
`clean_elements_xml` with one change: the string `"event"` is replaced with
the `element_name` parameter in the check:

```rust
// Before (hardcoded):
if local_name == b"event" { ... }

// After (parameterized):
if local_name == element_name.as_bytes() { ... }
```

#### Add thin wrapper functions

```rust
/// Remove events matching the given handles from a Gramps XML file.
///
/// Convenience wrapper around [`clean_elements_xml`] with `element_name = "event"`.
pub fn clean_events_xml(
    input: &Path,
    output: &Path,
    event_handles: &HashSet<String>,
) -> Result<CleanStats, CleanError> {
    clean_elements_xml(input, output, "event", event_handles)
}

/// Remove notes matching the given handles from a Gramps XML file.
///
/// Convenience wrapper around [`clean_elements_xml`] with `element_name = "note"`.
pub fn clean_notes_xml(
    input: &Path,
    output: &Path,
    note_handles: &HashSet<String>,
) -> Result<CleanStats, CleanError> {
    clean_elements_xml(input, output, "note", note_handles)
}
```

#### Update existing tests

All existing tests for `clean_events_xml` must be updated:

- Field references: `stats.events_removed` → `stats.elements_removed`
- Field references: `stats.events_not_found` → `stats.elements_not_found`

#### Add note-specific tests

| Test | What it verifies |
|---|---|
| `remove_single_note` | A `<note>` with matching handle is removed |
| `remove_self_closing_note` | `<note handle="x"/>` self-closing form |
| `keep_unrelated_note` | Notes not in deletion set pass through |
| `no_notes_to_remove` | Empty deletion set → output = input |
| `note_with_text_body` | `<note handle="n0001"><text>Hello</text></note>` — body removed correctly |
| `note_with_style` | Note with `<style name="..."><range .../></style>` — all children skipped |
| `note_with_type_attr` | `<note handle="n0001" type="Research">` — detected correctly |
| `notes_not_in_xml` | Handle in set but not in XML → increments `elements_not_found` |
| `namespace_prefixed_note` | `<ns:note ns:handle="x">` detected correctly |
| `multiple_notes_mixed` | Some removed, some kept, stats correct |
| `notes_section_preserved` | Empty `<notes/>` or `<notes></notes>` section tag preserved even when all notes removed |

### 4.2 Integration into `delete.rs`

After the event cleaning step (current step 15 in `delete.rs`), add a new
note cleaning step:

```rust
// After event cleaning (step ~15 in delete.rs), clean pending notes

// Derive output path for note-cleaned file
let deleted_3_path = derive_deleted_3_path(input_path);

let pending_notes: HashSet<String> = manifest
    .plan
    .get("notes")
    .map(|p: &TypePlan| {
        p.to_delete
            .iter()
            .filter(|e| e.status == HandleStatus::Pending)
            .map(|e| e.handle.clone())
            .collect()
    })
    .unwrap_or_default();

if !pending_notes.is_empty() {
    let deleted_2_path = derive_no_events_path(input_path);
    log::info!(
        "Cleaning {} pending notes from output -> {}",
        pending_notes.len(),
        deleted_3_path.display()
    );
    let stats = crate::commands::clean::clean_notes_xml(
        &deleted_2_path,
        &deleted_3_path,
        &pending_notes,
    )?;
    log::info!(
        "Note cleaning complete: {} removed, {} not found in XML",
        stats.elements_removed,
        stats.elements_not_found,
    );
    // Mark these notes as Deleted in the manifest
    if let Some(notes_plan) = manifest.plan.get_mut("notes") {
        for entry in &mut notes_plan.to_delete {
            if entry.status == HandleStatus::Pending {
                entry.status = HandleStatus::Deleted;
            }
        }
    }
    // Persist the updated manifest to disk
    save_manifest(&manifest, &manifest_path)?;
} else {
    log::info!("No pending notes to clean.");
}
```

#### Output file naming

| File | Content |
|---|---|
| `<stem>-cleaned.gramps` | Gramps DB export (people deleted, other types survive) |
| `<stem>-deleted_2.gramps` | Events removed (current behavior, preserved) |
| `<stem>-deleted_3.gramps` | Events AND notes removed (new) |

All cleaned outputs are written as plain `.gramps` regardless of whether the
original input was `.gramps` or `.gramps.gz`. The `strip_gramps_extensions`
helper already strips both extensions correctly.

Implementation:

```rust
/// Derive the `-deleted_3.gramps` path from the input file path.
fn derive_deleted_3_path(input_path: &std::path::Path) -> std::path::PathBuf {
    let stem = strip_gramps_extensions(input_path);
    std::path::PathBuf::from(format!("{}-deleted_3.gramps", stem))
}
```

This mirrors the existing `derive_no_events_path` function (which produces
`-deleted_2.gramps`). Both derive from the original input path, not from
intermediate outputs.

### 4.3 Note XML structure

Gramps `<note>` elements have the following structure:

```xml
<notes>
  <note handle="_n0001" type="Research" format="0">
    <text>This is the note text. Can be multi-paragraph.</text>
    <style name="bold">
      <range start="0" end="4"/>
    </style>
  </note>
</notes>
```

The streaming filter must handle:

- Standard `<note>...</note>` elements with child content
- Self-closing `<note handle="..." />` elements
- Optional `type` and `format` attributes (ignored by the filter)
- Namespace-prefixed variants (`<ns:note ns:handle="...">`)
- Underscore-prefixed handles (Gramps XML convention: `_n0001`)

All of these are already handled by the generalized `clean_elements_xml`
because it uses the same `read_handle_attr` + `remove_matching_handle`
helpers that already work for events.

## 5. Error Handling

| Scenario | Behavior |
|---|---|
| No pending notes in manifest | Skip note cleaning entirely, log info |
| `-deleted_2.gramps` doesn't exist | `CleanError::Io` — propagate (shouldn't happen since event cleaning succeeded) |
| Note handle in deletion set but not found in XML | Increment `elements_not_found`, log warning, continue |
| Malformed XML | `CleanError::XmlParse` — propagate to user |
| Note cleaning fails | `-deleted_3.gramps` not written; `-deleted_2.gramps` and manifest still on disk; user can re-run with `--load-manifest` |
| Note cleaning succeeds but manifest re-save fails | `-deleted_3.gramps` exists with notes removed, but manifest still shows them as `Pending`; user can re-run with `--load-manifest` to reconcile (notes already removed, re-cleaning is a no-op) |

## 6. Testing Strategy

### 6.1 Unit tests (clean.rs)

All existing event-focused tests renamed to be element-generic where
applicable, plus new note-specific tests (see §4.1).

Additionally, a **property-based test** for idempotency should be added:
given arbitrary XML with arbitrary handle sets, running `clean_elements_xml`
twice with the same handles on the output of the first run must produce
identical output (second run is always a no-op). This satisfies the
property-based testing guidelines for pure algorithmic functions with
idempotency invariants.

### 6.2 Integration tests (cli/tests/)

| Test | What it verifies |
|---|---|
| `delete_with_note_clean` | Full pipeline: delete people → events cleaned → notes cleaned; verify `-deleted_3.gramps` has no pending notes |
| `no_pending_notes_noop` | When no notes are pending, `-deleted_3.gramps` is still written (identical to `-deleted_2.gramps`) |
| `manifest_re_saved_after_note_clean` | After successful note cleaning, manifest on disk has pending notes marked `Deleted` |
| `deleted_2_still_produced` | Verify `-deleted_2.gramps` still exists alongside `-deleted_3.gramps` |

## 7. Edge Cases & Considerations

### 7.1 Note references in other elements

When a note is removed from the XML, any `<noteref hlink="..."/>` elements
in `<person>`, `<family>`, `<event>`, `<place>`, `<source>`, `<citation>`,
`<repository>`, or `<media>` that point to the deleted note become **dangling
references**. Gramps handles dangling hlinks gracefully on import (it skips
them). We do NOT need to clean up noteref elements.

### 7.2 Notes that share references

A note may be referenced by multiple objects (e.g., two different people).
The cascade engine correctly identifies that the note is only orphaned when
ALL referrers are deleted. Notes with at least one surviving referrer are not
in the manifest and are not removed.

### 7.3 Note → Citation edges (NoteCitation)

Notes can reference citations via `NoteCitation` edges. The cascade engine
handles these correctly: removing a note severs its NoteCitation edge, which
may orphan the citation. This is handled by the original cascade (citations
are already in the manifest if they become orphaned). No additional handling
needed.

### 7.4 Note → Tag edges (NoteTag)

Same as NoteCitation — handled by the original cascade.

### 7.5 Deterministic output

The output is deterministic: notes are removed, everything else passes
through in original order. No reordering or reformatting occurs.

### 7.6 Large files

Same streaming approach as event cleaning — O(depth of XML nesting) memory,
single pass, efficient for large files.

## 8. Files Changed

| File | Change |
|---|---|
| `crates/cli/src/commands/clean.rs` | Generalize `clean_events_xml` → extract `clean_elements_xml` core; rename `CleanStats` fields; add `clean_notes_xml` wrapper; add note-specific tests |
| `crates/cli/src/commands/delete.rs` | Add note cleaning step after event cleaning (~40 lines); add `derive_deleted_3_path` helper; update log messages to mention `-deleted_3.gramps` |

## 9. Non-Goals

- **Removing non-note orphans after event cleaning**: Only notes that the cascade
  already identified are removed. We do not re-run the cascade after removing
  events to find newly orphaned notes.
- **Cleaning noteref elements**: Dangling `<noteref hlink="..."/>` refs in
  remaining people, families, events, etc. are left as-is. Gramps handles these
  gracefully on import.
- **Deleting notes via the Python Gramps backend**: The Python backend remains
  people-only. Notes are removed via the streaming XML filter, same as events.
- **New CLI subcommand**: This is not exposed as a separate `gramps-gen clean-notes`
  command.
- **Combined single-pass event+note cleaning**: Events and notes are cleaned in
  separate passes, producing two output files. A single combined pass would
  be more efficient but would make it harder to inspect intermediate results
  and would complicate the existing step-by-step pipeline contract.

## 10. Implementation Order

| Step | Description | Tests |
|---|---|---|
| 1 | Refactor `CleanStats`: rename `events_removed` → `elements_removed`, `events_not_found` → `elements_not_found` | Update all existing field references; `cargo test -p cli` passes |
| 2 | Extract `clean_elements_xml` core function with `element_name` parameter | All existing event tests pass with generalized function |
| 3 | Add `clean_notes_xml` thin wrapper | New unit tests for note removal |
| 4 | Add `derive_deleted_3_path` helper in `delete.rs` | Unit test for path derivation |
| 5 | Add note cleaning step in `delete.rs` pipeline | Integration tests: `-deleted_3.gramps` has no pending notes |
| 6 | Update manifest re-save after note cleaning | Integration test: manifest has notes marked `Deleted` |
| 7 | Run full end-to-end test with real data | Verify `-deleted_2.gramps` and `-deleted_3.gramps` both valid |
