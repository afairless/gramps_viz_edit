# Implementation Plan: Note Deletion Post-Process

Source: `docs/research/note-deletion-postprocess.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `refactor: rename CleanStats fields to generic element names` | CleanStats field rename | `crates/cli/src/commands/clean.rs` — rename `events_removed` → `elements_removed`, `events_not_found` → `elements_not_found`; update all uses in tests | Unit (all existing tests pass with renamed fields) |
| 2 | `refactor: extract clean_elements_xml with element_name parameter` | Generalized clean core | `crates/cli/src/commands/clean.rs` — extract `clean_elements_xml(input, output, element_name, handles)` from `clean_events_xml` body; reimplement `clean_events_xml` as thin wrapper | Unit (existing event tests pass via wrapper) |
| 3 | `feat: add clean_notes_xml wrapper and note-specific unit tests` | Note cleaning function | `crates/cli/src/commands/clean.rs` — add `clean_notes_xml` wrapper; add note-specific tests (single, self-closing, unrelated, empty, text body, style, type attr, not-found, namespace-prefixed, mixed, section preserved) | Unit, Property-based (idempotency) |
| 4 | `feat: add derive_deleted_3_path helper for note-cleaned output` | Path helper | `crates/cli/src/commands/delete.rs` — add `derive_deleted_3_path()` mirroring `derive_no_events_path` | Unit (path derivation for various inputs) |
| 5 | `feat: integrate note cleaning step into delete pipeline` | Pipeline integration | `crates/cli/src/commands/delete.rs` — add note cleaning step after event cleaning; collect pending note handles from manifest; call `clean_notes_xml`; mark notes `Deleted` in manifest; re-save manifest | Integration (e2e_delete.rs: note cleaning, manifest re-save, `-deleted_2.gramps` preserved, no-op when no pending notes) |
| 6 | `test: add end-to-end integration test for note deletion` | Full E2E test | `crates/cli/tests/e2e_delete.rs` — add test with real data that verifies `-deleted_3.gramps` has no pending notes, `-deleted_2.gramps` and `-cleaned.gramps` are valid, and manifest is updated | Integration (full pipeline) |
