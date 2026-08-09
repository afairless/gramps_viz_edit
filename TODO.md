# Implementation Plan: Delete Tool — Display & Cascade Fixes

Source: `docs/research/delete-display-cascade-fixes.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat(delete): add sourceref parsing to citation handler` | Parser: sourceref | `crates/gramps-reader/src/xml/graph.rs` | Unit (parser creates CitationSource edge, sets source_handle) |
| 2 | `feat(delete): add place hlink parsing inside event` | Parser: event place | `crates/gramps-reader/src/xml/graph.rs` | Unit (parser creates EventPlace edge, sets place_handle) |
| 3 | `fix(delete): use edge traversal for family member display` | Display: Family | `crates/delete/src/review.rs` | Unit (update `family_graph` helper, test edge-based father/mother/children display) |
| 4 | `fix(delete): show family-event people in event descriptions` | Display: Event | `crates/delete/src/review.rs` | Unit (test FamilyEventRef resolution and display) |
| 5 | `fix(delete): check CitationSource edge for citation display` | Display: Citation | `crates/delete/src/review.rs` | Unit (test edge-based source fallback, no-source fallback) |
| 6 | `fix(delete): extend place orphan rule with citation/media/note/tag edges` | Cascade: Place | `crates/delete/src/cascade.rs` | Unit (test PlaceCitation, PlaceMediaRef, PlaceNote, PlaceTag orphan detection) |
| 7 | `docs(delete): update orphan rules table for place edges` | Documentation | `docs/delete-tool.md` | — |
| 8 | `chore(delete): final workspace lint and test pass` | Verification | — | Integration (full `cargo test --workspace`, `cargo clippy --all-targets --all-features -- -D warnings`) |

## Notes

- **Parser tests** (Steps 1–2) use real XML snippets to verify edge creation. The existing test infrastructure in `gramps-reader` already supports streaming parse checks.
- **Display tests** (Steps 3–5) update `review.rs` test helpers to create edges alongside data fields, then verify the displayed string.
- **Cascade tests** (Step 6) add scenarios for each additional place edge type.
- **Dependency order:** Steps 1–2 are independent of each other; Steps 3–5 depend on the parser being correct only for integration, but are testable independently of Steps 1–2 at the unit level. Step 6 depends on Step 2 for end-to-end but cascade rule logic is independently testable.
