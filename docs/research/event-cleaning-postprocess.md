# Event Cleaning Post-Process

> Status: Plan  
> Created: 2026-08-11  
> Target: `gramps-gen delete` command — automatic post-processing step

## 1. Problem Statement

The `gramps-gen delete` command uses the Gramps Python database API
(`scripts/delete_backend.py`) to delete selected people. Gramps'
`delete_person_from_database` handles family ref cleanup and family deletion
internally, but it does **not** delete orphaned events — events that were
referenced only by the now-deleted people remain in the database and thus
survive into the output `.gramps` file.

Our Rust cascade engine correctly identifies these orphaned events. After
reconciliation, they appear in the manifest with `status: "pending"` (meaning
Gramps kept them). The manifest already has the exact set of event handles
that need to be removed.

**Example** from a real run (3 people deleted, 1 family auto-deleted by
Gramps, 3 orphaned events survived):

```json
{
  "plan": {
    "people":   { "to_delete": [3 items, status: "deleted"] },
    "families": { "to_delete": [1 item,  status: "deleted"] },
    "events":   { "to_delete": [3 items, status: "pending"] }
  }
}
```

The goal is to remove these pending events from the `.gramps` XML output as
an automatic post-processing step, producing a clean output file.

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Integration point | Inside the existing `delete` command, executed **by default** | User expectation: `delete` should produce a clean output without extra steps |
| Output file | `<input-stem>-no-events.gramps` (alongside the existing `-cleaned.gramps`) | Preserves both files; the `-cleaned` file is the Gramps DB export, `-no-events` is the fully cleaned version |
| Cascade depth | Events only (remove pending events, stop there) | The manifest already records the full cascade result. Events are the only type Gramps doesn't delete. Places/citations/etc. that the cascade flagged are either already deleted by Gramps or were never orphaned in the first place. Re-cascading after event removal could reveal new orphans, but in practice this is vanishingly rare (places cited by multiple events, etc.) and adds significant complexity |
| Implementation | Streaming XML filter using `quick-xml` | No need to parse into `Graph`, no schema dependency, preserves exact original XML structure, fast and memory-efficient |
| Module location | New module `crates/cli/src/commands/clean.rs` | Clean separation from the delete orchestration; callable as a library function |

## 3. Architecture

### 3.1 Pipeline (updated)

```
Before:
  Parse → Cascade → Review → Manifest → Python backend → Reconcile → Output.gramps

After:
  Parse → Cascade → Review → Manifest → Python backend → Reconcile
       → [Checkpoint: save -cleaned.gramps + manifest to disk]
       → Clean Events (remove pending events from XML) → Output-no-events.gramps
```

A **checkpoint** is established after reconciliation but before event
cleaning: the `-cleaned.gramps` file (Gramps DB export) and the manifest are
already saved to disk at this point. If the clean step fails, the user still
has both intermediate outputs — a valid `.gramps` file and a manifest
recording what was and wasn't deleted.

On success, the manifest is **re-saved** with pending events marked
`Deleted` and the `-no-events.gramps` file is written. The
`-cleaned.gramps` is preserved alongside it.

### 3.2 Component Diagram

```
┌──────────────────────────────────────────────────────────────┐
│  delete.rs (orchestrator)                                     │
│                                                               │
│  After reconcile() succeeds:                                  │
│  1. Collect pending event handles from manifest               │
│  2. Call clean_events_xml(input_path, output_path, handles)   │
│  3. Log result                                                │
└──────────────────────────┬───────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  clean.rs (new module)                                        │
│                                                               │
│  pub fn clean_events_xml(                                     │
│      input: &Path,                                            │
│      output: &Path,                                           │
│      event_handles: &HashSet<String>,                         │
│  ) -> Result<CleanStats, CleanError>                          │
│                                                               │
│  Uses quick-xml Reader + Writer to:                           │
│  1. Stream through the input .gramps file                     │
│  2. Detect <event> elements by handle attribute               │
│  3. Skip elements whose handle is in event_handles            │
│  4. Pass everything else through verbatim                     │
│  5. Handle gzip transparently (read via flate2, write .gramps) │
└──────────────────────────────────────────────────────────────┘
```

## 4. Implementation Plan

### 4.1 New module: `crates/cli/src/commands/clean.rs`

#### Types

```rust
/// Statistics from an event cleaning run.
pub struct CleanStats {
    /// Number of events removed.
    pub events_removed: usize,
    /// Number of event handles that were not found in the XML.
    pub events_not_found: usize,
}

/// Errors from event cleaning.
pub enum CleanError {
    Io { path: String, source: std::io::Error },
    XmlParse { message: String },
}
```

#### Core function

```rust
pub fn clean_events_xml(
    input: &Path,
    output: &Path,
    event_handles: &HashSet<String>,
) -> Result<CleanStats, CleanError>
```

**Algorithm:**

1. Open input file with transparent gzip detection:
   - Read the first two bytes (gzip magic: `0x1f 0x8b`) to detect compression
   - If gzip: wrap `File` in `flate2::read::GzDecoder<BufReader<File>>`
   - If plain XML: wrap `File` in `BufReader<File>`
   - Produce a `Box<dyn BufRead>` to feed to `quick_xml::Reader`
   - This is a true streaming approach — the file is never loaded into memory as a single `String`
2. Create output file (always plain `.gramps`, no gzip)
3. Initialize `quick_xml::Reader` from the `Box<dyn BufRead>` and `quick_xml::Writer` from the output `File`
3a. Handle the XML declaration: if the first event from the reader is `Event::Decl`, write it to the writer before entering the main event loop. This preserves `<?xml version="1.0" encoding="UTF-8"?>` in the output
4. Track state:
   - `in_event: bool` — are we inside an `<event>` element?
   - `skip_depth: usize` — nesting depth when skipping (0 = not skipping)
   - `current_event_handle: Option<String>` — handle of the event being read
   - `events_removed: usize`, `events_not_found: usize`
5. For each XML event:
   - **Start** `<event handle="...">`: read handle attribute, if in deletion set → set `skip_depth = 1`, increment `events_removed`, skip writing
   - **Start** while skipping: increment `skip_depth`, skip writing
   - **End** while skipping: decrement `skip_depth`, if reaches 0 → resume writing
   - **Empty** `<event handle="..." />`: if handle in set → skip, increment `events_removed`
   - **Everything else**: write through verbatim
6. Return `CleanStats`

#### Namespace handling

Gramps XML may use namespace prefixes (e.g. `<ns:event ns:handle="...">`).
We detect the element name by stripping the namespace prefix (using the
existing `gramps_reader::xml::strip_prefix` utility or a local equivalent).
For the handle attribute, we check both `handle` and `*:handle` (any
namespace-qualified handle attribute).

#### Gzip handling

The input `.gramps` file may be gzip-compressed (`.gramps.gz`). Use
`flate2::read::GzDecoder` for transparent decompression on read. The output
is always written as plain `.gramps` (no compression), since the user will
likely import it into Gramps which accepts both formats.

### 4.2 Integration into `delete.rs`

After reconciliation succeeds (step 12 in the current flow), add a new step:

```rust
// After reconciliation, clean pending events from the output XML
let pending_events: HashSet<String> = manifest
    .plan
    .get("events")
    .map(|p| {
        p.to_delete
            .iter()
            .filter(|e| e.status == HandleStatus::Pending)
            .map(|e| e.handle.clone())
            .collect()
    })
    .unwrap_or_default();

if !pending_events.is_empty() {
    let no_events_path = /* derive from input_path: <stem>-no-events.gramps */;
    log::info!(
        "Cleaning {} pending events from output → {}",
        pending_events.len(),
        no_events_path.display()
    );
    let stats = clean::clean_events_xml(&output_path, &no_events_path, &pending_events)?;
    log::info!(
        "Event cleaning complete: {} removed, {} not found in XML",
        stats.events_removed,
        stats.events_not_found,
    );
    // Mark these events as Deleted in the manifest
    for entry in &mut manifest.plan.get_mut("events").unwrap().to_delete {
        if entry.status == HandleStatus::Pending {
            entry.status = HandleStatus::Deleted;
        }
    }
    // Persist the updated manifest to disk (overwrites checkpoint copy)
    save_manifest(&manifest, &manifest_path)?;
} else {
    log::info!("No pending events to clean.");
}
```

### 4.3 Output file naming

Derive from the *input* file stem (not the `-cleaned` output path):

```
input:      family.gramps
output:     family-cleaned.gramps     (existing, Gramps DB export)
no-events:  family-no-events.gramps   (new, with events cleaned)

input:      family.gramps.gz
output:     family-cleaned.gramps.gz  (existing)
no-events:  family-no-events.gramps   (new, always plain)
```

The `-no-events.gramps` path always uses a plain `.gramps` extension (no
`.gz`) since the cleaning step decompresses the input and writes plain XML.

Implementation:

```rust
fn derive_no_events_path(input_path: &Path) -> PathBuf {
    let stem = strip_gramps_extensions(input_path);
    PathBuf::from(format!("{}-no-events.gramps", stem))
}
```

**Note:** `strip_gramps_extensions` is currently a private function in `delete.rs`. It must be made `pub(crate)` and imported from `clean.rs`, or extracted to a shared utility module (e.g., `crates/cli/src/path_utils.rs`). The implementation is a ~5-line string manipulation, so duplicating it in `clean.rs` is also acceptable if extraction feels premature.

### 4.4 Checkpoint and manifest update

**Before** the clean step begins, the `-cleaned.gramps` and the manifest are
already saved to disk by the existing pipeline (steps 7–14 in `delete.rs`).
This is the checkpoint that guarantees intermediate outputs survive a clean
failure.

**After** successful cleaning, the manifest is **re-saved** with pending
events now marked `Deleted`. This is a second save that overwrites the
checkpoint copy with the final, enriched manifest. It keeps the manifest as
the authoritative record of what was removed.

## 5. Error Handling

| Scenario | Behavior |
|---|---|
| No pending events in manifest | Skip cleaning entirely, log info |
| Input file doesn't exist | `CleanError::Io` — propagate to user |
| Event handle in deletion set but not found in XML | Increment `events_not_found`, log warning, continue |
| Malformed XML | `CleanError::XmlParse` — propagate to user |
| Output file already exists | Overwrite (consistent with existing behavior) |
| Gzip decompression fails | `CleanError::Io` — propagate |

The clean step **fails fatally**: if it errors, the command exits with an
error. However, the `-cleaned.gramps` and manifest are **already saved to
disk** before the clean step begins (the checkpoint in §3.1), so the user
always has the intermediate outputs regardless of whether the clean step
succeeds.

To avoid leaving a partially-written file on disk, the clean function
writes to a temporary file first and renames it to the final
`-no-events.gramps` path only on success. If the clean step fails, the
temp file is deleted and the manifest is left as-is (with pending events
still marked `Pending`). The user can inspect the
`-cleaned.gramps` and manifest, then re-run the delete command with
`--load-manifest` to retry from the checkpoint.

## 6. Testing Strategy

### 6.1 Unit tests (`clean.rs`)

| Test | What it verifies |
|---|---|
| `remove_single_event` | A `<event>` with matching handle is removed |
| `remove_self_closing_event` | `<event handle="x"/>` self-closing form |
| `keep_unrelated_event` | Events not in deletion set pass through |
| `no_events_to_remove` | Empty deletion set → output = input |
| `handle_not_found` | Handle in set but not in XML → increments `events_not_found` |
| `namespace_prefixed_event` | `<ns:event ns:handle="x">` detected correctly |
| `nested_eventtype` | Event body with `<eventtype><type>Birth</type></eventtype>` skipped correctly |
| `flat_type` | Event body with flat `<type>Birth</type>` skipped correctly |
| `multiple_events_mixed` | Some removed, some kept, stats correct |
| `non_event_xml_preserved` | Header, people, families, etc. pass through unchanged |
| `gzip_input` | Transparent `.gramps.gz` decompression |

### 6.2 Integration test (`e2e.rs` or `integration.rs`)

| Test | What it verifies |
|---|---|
| `delete_with_event_clean` | Full pipeline: delete people → events auto-cleaned from XML |
| `no_pending_events_noop` | When all events already deleted by Gramps, `-no-events` file still written (identical to `-cleaned`) |
| `manifest_re_saved_after_clean` | After successful clean, manifest on disk has pending events marked `Deleted` |

## 7. Edge Cases & Considerations

### 7.1 Event references in other elements

When an event is removed from the XML, any `<eventref hlink="..."/>` elements
in `<person>`, `<family>`, or other elements that point to the deleted event
become **dangling references**. However, this is the same situation that
Gramps itself creates when it deletes people — the remaining people/families
simply lose those event references. Gramps handles dangling hlinks gracefully
on import (it skips them). We do NOT need to clean up eventref elements.

### 7.2 Already-orphaned events

The cascade engine in `delete::cascade` already handles the distinction
between "newly orphaned" and "already orphaned" (pre-connectivity check).
Events that were already orphaned before the deletion operation will NOT
appear in the manifest's `to_delete` list, so they will not be removed.
This is correct behavior.

### 7.3 Schema version compatibility

The streaming filter does not depend on the Gramps schema version. It only
looks at element names (`event`) and the `handle` attribute. Both 5.1 and
5.2 (and future versions) use the same `<event handle="...">` structure.

### 7.4 Large files

The streaming approach processes the file in a single pass with minimal
memory overhead (O(depth of XML nesting), typically < 10 elements). A
500 MB `.gramps` file will be processed efficiently.

### 7.5 Deterministic output

The output is deterministic: events are removed, everything else passes
through in original order. No reordering or reformatting occurs.

## 8. Files Changed

| File | Change |
|---|---|
| `crates/cli/src/commands/clean.rs` | **New file** — streaming XML event filter |
| `crates/cli/src/commands/mod.rs` | Add `pub mod clean;` |
| `crates/cli/src/commands/delete.rs` | Add clean step after reconciliation (~30 lines) |

## 9. Non-Goals

- **Removing non-event orphans**: Only events are cleaned. Places, citations,
  sources, etc. that might become orphaned after event removal are NOT
  re-cascaded. (See §2, cascade depth decision.)
- **Cleaning eventref elements**: Dangling `<eventref hlink="..."/>` refs
  in remaining people/families are left as-is. Gramps handles these
  gracefully on import.
- **New CLI subcommand**: This is not exposed as a separate `gramps-gen clean`
  command (though it could be added later by wrapping `clean_events_xml`).
- **Overwriting the `-cleaned.gramps` file**: The Gramps DB export is
  preserved as an intermediate artifact; the `-no-events.gramps` is a new,
  separate output.
