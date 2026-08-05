# Implementation Plan: Steps 2–5 — Diff Crate (Skeleton, Similarity, Normalization, Report Types)

Source: `docs/research/gramps-diff-plan.md` (Steps 2, 3, 4, 5)

> **Scope:** This plan implements **Steps 2, 3, 4, and 5** of the Gramps Diff
> Analyzer plan. Step 1 (full graph parser) is already complete. These four steps
> create the `diff` crate skeleton, text similarity functions, text normalization
> utilities, and diff report data types. Steps 3–5 have no interdependencies
> and can be implemented in any order after Step 2 (scaffold).

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 2 | `feat: scaffold diff crate with dependencies` | Diff crate skeleton | `crates/diff/Cargo.toml` — new workspace member depending on `typed-graph`, `gramps-reader`, `serde`, `serde_json`. `crates/diff/src/lib.rs` — re-exports, `run_diff()` stub returning `Err(DiffError::Unimplemented)`, `DiffError` enum (`ParseError`, `SchemaMismatch`, `EmptyGraph`, `InternalError`, `Unimplemented`). Update root `Cargo.toml` `members` list and add `strsim = "0.11"` to `[workspace.dependencies]`. | Smoke (crate compiles, `run_diff` returns `Err(DiffError::Unimplemented)`, `DiffError` derives `Debug + Display + Error`) |
| 3 | `feat: add text similarity functions` | Similarity module | `crates/diff/src/similarity.rs` — `levenshtein(a, b) -> f64`, `token_jaccard(a, b) -> f64`, `tokenized_levenshtein(a, b) -> f64`. All return normalized 0.0–1.0 scores. `jaro_winkler` from `strsim` is excluded from the initial module. | Unit (identical strings → 1.0, completely different → 0.0, partial overlap, empty strings, Unicode). Property-based (∀ scores ∈ [0.0, 1.0]; identical inputs → 1.0). |
| 4 | `feat: add text normalization utilities` | Normalization module | `crates/diff/src/normalize.rs` — `case_fold`, `trim_whitespace`, `collapse_whitespace`, `strip_html_tags`, `expand_street_abbreviations`, `strip_page_prefix`, `normalize_tag_color`, `diacritic_insensitive_eq`. Each function is a pure `&str -> String` transform. | Unit (one test per function, edge cases for empty/HTML/abbrev). Property-based (idempotency: `normalize(normalize(x)) == normalize(x)` for all functions except `strip_html_tags` which is one-pass). |
| 5 | `feat: add diff report data types` | Report types | `crates/diff/src/report.rs` — `DiffReport`, `DiffSummary`, `ItemDiff`, `Classification` enum, `FieldChange`, `FieldKind` enum, `AmbiguousCase`, `Candidate`, `AmbiguousContext`, `RelatedItem`. All types derive `Serialize, Deserialize, Debug, Clone`. | Unit (type shape existence, serde round-trip) |

## Notes

- **Step 2** must come first — it creates the crate that Steps 3–5 live in.
- **Steps 3–5** are independent of each other; the table order (3 → 4 → 5) is
  arbitrary (logical: utilities first, then types that use them).
- `strsim` crate provides `levenshtein`; the similarity module wraps it with
  normalization to 0.0–1.0 scores. `jaro_winkler` is excluded from the initial
  similarity module (see Step 3 notes in the plan).
- **Step 5 (report types)** uses `serde` for serialization; all types derive
  `Serialize, Deserialize, Debug, Clone`.
- The `DiffError` enum in Step 2 includes `Unimplemented` (not in the original
  plan) so the `run_diff()` stub can return a concrete error that satisfies
  `std::error::Error` without depending on other modules.
