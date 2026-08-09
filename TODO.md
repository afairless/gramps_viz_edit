# Implementation Plan: Improve Delete Tool Display

Source: `docs/research/improve-delete-display.md`

## Overview

Improve the `gramps-gen delete` interactive review CLI by: (1) adding identifying
information (birth/death dates, parent/child names, source info, etc.) to all
node type descriptions; (2) showing `gramps_id` (e.g. `[I0001]`) after each
handle; and (3) fixing the step counter to show a dynamic total reflecting only
types that have deletion candidates.

## Structure

Each step is a small, testable unit. Steps build on previous ones — helpers and
signature changes come first; branch enhancements follow; edge-case tests and
verification come last.

## Steps

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat(delete): extract person_full_name, format_gramps_id, and fallback_source helpers` | Helper extraction | `crates/delete/src/review.rs` — add `person_full_name()`, `format_gramps_id()`, `fallback_source()` functions | Unit (3 helpers) |
| 2 | `feat(delete): change describe_node to return (description, gramps_id) tuple` | Signature change | `crates/delete/src/review.rs` — change `describe_node` return type to `(String, Option<String>)`, update all match arms to return `gramps_id`, update both call sites in `review_type_prompt`, update all 6 existing `describe_node` tests to destructure tuple | Unit (6 existing tests updated) |
| 3 | `feat(delete): enhance Person description with birth/death dates` | Person branch | `crates/delete/src/review.rs` — Person match arm: use `person_full_name` helper, resolve birth/death years via `birth_ref_index`/`death_ref_index` | Unit (`describe_person_node` updated) |
| 4 | `feat(delete): enhance Family description with parents and children` | Family branch | `crates/delete/src/review.rs` — Family match arm: show father/mother names (via `person_full_name`), up to 2 children, "+N more" suffix | Unit (`describe_family_node`) |
| 5 | `feat(delete): enhance Event description with associated person names` | Event branch | `crates/delete/src/review.rs` — Event match arm: use `edges_incident_to` to find up to 3 associated people via `PersonEventRef` edges | Unit (`describe_event_with_people`) |
| 6 | `feat(delete): enhance Citation description with source info` | Citation branch | `crates/delete/src/review.rs` — Citation match arm: use `typed_graph::get_source_handle()` for portable access, show source title/author/pubinfo, show page | Unit (`describe_citation_with_source`, `describe_citation_no_source`) |
| 7 | `feat(delete): enhance Source description with fallback fields` | Source branch | `crates/delete/src/review.rs` — Source match arm: use `fallback_source` helper for title → author → pubinfo → "Unnamed Source" chain | Unit (`describe_source_fallback`) |
| 8 | `feat(delete): enhance Media description with path/date fallbacks` | Media branch | `crates/delete/src/review.rs` — Media match arm: desc → path → date → "Unnamed Media" fallback chain | Unit (`describe_media_fallback`) |
| 9 | `feat(delete): extend Note preview to 150 chars with word-boundary break` | Note branch | `crates/delete/src/review.rs` — Note match arm: 60→150 char preview with `rfind(' ')` word-boundary break and fallback to exact truncation | Unit (`describe_note_long_text`) |
| 10 | `fix(delete): make step counter show dynamic total` | Step counter | `crates/delete/src/review.rs` — `run_interactive_review`: compute `total_types` by counting only types with non-empty candidate lists | Unit (`step_counter_dynamic_total`) |
| 11 | `test(delete): add comprehensive edge-case tests for enhanced descriptions` | Edge-case tests | `crates/delete/src/review.rs` — add tests for: Person with both/only-birth/only-death/no dates; Family with children "+N more"; Event with 0/1/3+ people; Source with each fallback level; Media with each fallback level; Note truncation with/without word-boundary; every type with `gramps_id = None` | Unit (~15 new tests) |
| 12 | `test(delete): run full test suite to verify no regressions` | Integration verification | `crates/cli/tests/e2e.rs` — run `cargo test --workspace` to confirm no regressions | Integration (existing E2E suite) |

## Key design decisions

1. **`format_gramps_id` as a helper** — standalone function rather than inline at
   call site, so it can be tested independently and reused across branches.
2. **Return tuple `(String, Option<String>)`** — `describe_node` returns both
   description body and optional gramps_id. Call sites compose the full display
   line. Existing tests destructure the tuple and continue to work.
3. **`person_full_name` extracted before use** — defined in Step 1, used by
   Person (Step 3), Family (Step 4), and Event (Step 5) branches.
4. **`fallback_source` as standalone helper** — extracted in Step 1 for
   testability, used in Step 7.
5. **Signature change (Step 2) before branch enhancements (Steps 3–9)** — avoids
   touching every test multiple times. Each branch enhancement then only modifies
   its own match arm and its own test(s).
