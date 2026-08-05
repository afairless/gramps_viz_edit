# Implementation Plan: Gramps Diff Analyzer

Source: `docs/research/gramps-diff-design.md`

## Prerequisite Gap Analysis

The design document assumes XML files are parsed into the full `typed-graph::Graph`
model. The current `gramps-reader` crate only extracts simplified
`ParsedPerson`/`ParsedFamily`/`ParsedEvent` records — it does **not** parse all
10 primary types or construct edges. A full XML-to-Graph parser must be built
before any diff logic can operate.

## Dependencies Between Steps

```
Step 1  (full graph parser)     ──┐
                                  │
Step 2  (diff crate skeleton)    ──┤
Step 3  (similarity)             ──┤
Step 4  (normalization)          ──┤
Step 5  (report types)           ──┤
                                  ├──▶ Step 6 (compare) ──▶ Step 7 (matcher)
                                  │         │
                                  │         └──▶ Step 8 (cascading)
                                  │                  │
                                  │     ┌────────────┼──▶ Step 9 (output)
                                  │     │            │
                                  │     │            └──▶ Step 10 (visualizer index)
                                  │     │                      │
                                  └─────┼──────────────────────┼──▶ Step 11 (orchestrator)
                                        │                      │            │
                                        └──────────────────────┘            │
                                                                           ▼
                                                                   Step 12 (CLI)
                                                                           │
                                                                           ▼
                                                                   Step 14 (integration tests)
```

Steps 3–5 have no interdependencies and can be done in any order. Step 13
(interactive resolution) is independent and can be implemented after Step 12.
Steps 9–10 also depend on Step 5 (report types) for `DiffReport`/`ItemDiff`
serialization.

---

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add full XML-to-Graph parser for all 10 Gramps primary types` | Full graph XML parser | `crates/gramps-reader/src/xml/parse.rs` — streaming parser that reads all 10 primary types (Person, Family, Event, Place, Source, Citation, Repository, Media, Note, Tag) and constructs a `typed-graph::Graph` with all nodes + edges. Exposes `parse_graph(xml: &str, schema: &Schema) -> Result<Graph, Error>`. Uses the schema version detected from the XML `<header>` element; returns `Error::UnsupportedSchema` if the version isn't compiled in. Adds `typed-graph` as a new dependency of `gramps-reader` (coexists with existing lightweight `extract.rs` types). Depends on `quick-xml` (already in workspace). | Unit (one `.gramps` snippet per primary type, round-trip: parse → serialize → re-parse → compare graphs, malformed XML per type, one mixed-type integration: Person with attached Events, Citations, Notes, Media). Acceptance: round-trip graphs are identical; malformed elements produce `XmlParseError`. |
| 2 | `feat: scaffold diff crate with dependencies` | Diff crate skeleton | `crates/diff/Cargo.toml` — new workspace member depending on `typed-graph`, `gramps-reader`, `serde`, `serde_json`. `crates/diff/src/lib.rs` — re-exports, `run_diff()` stub, `DiffError` enum (`ParseError`, `SchemaMismatch`, `EmptyGraph`, `InternalError`). Update root `Cargo.toml` members and add `strsim = "0.11"` to `[workspace.dependencies]`. | Smoke (crate compiles, `run_diff` returns unimplemented, `DiffError` derives `Debug + Display + Error`) |
| 3 | `feat: add text similarity functions` | Similarity module | `crates/diff/src/similarity.rs` — `levenshtein(a, b) -> f64`, `token_jaccard(a, b) -> f64`, `tokenized_levenshtein(a, b) -> f64`. All return normalized 0.0–1.0 scores. `jaro_winkler` from `strsim` is available but excluded from the initial module — it underperformed on short genealogical text in prototyping and can be added later if needed. | Unit (identical strings → 1.0, completely different → 0.0, partial overlap, empty strings, Unicode). Property-based (∀ scores ∈ [0.0, 1.0]; identical inputs → 1.0). |
| 4 | `feat: add text normalization utilities` | Normalization module | `crates/diff/src/normalize.rs` — `case_fold`, `trim_whitespace`, `collapse_whitespace`, `strip_html_tags`, `expand_street_abbreviations`, `strip_page_prefix`, `normalize_tag_color`, `diacritic_insensitive_eq`. Each function is a pure `&str -> String` transform. | Unit (one test per function, edge cases for empty/HTML/abbrev). Property-based (idempotency: `normalize(normalize(x)) == normalize(x)` for all functions except `strip_html_tags` which is one-pass). |
| 5 | `feat: add diff report data types` | Report types | `crates/diff/src/report.rs` — `DiffReport`, `DiffSummary`, `ItemDiff`, `Classification` enum, `FieldChange`, `FieldKind` enum, `AmbiguousCase`, `Candidate`, `AmbiguousContext`, `RelatedItem`. All types derive `Serialize, Deserialize, Debug, Clone`. | Unit (type shape existence, serde round-trip) |
| 6 | `feat: add field-level comparison per primary type` | Compare module | `crates/diff/src/compare.rs` — `compare_person(a, b) -> Vec<FieldChange>`, `compare_family(...)`, `compare_event(...)`, ..., `compare_tag(...)`. One function per primary type. Delegates text fields to similarity scoring; uses set comparison for handle-ref arrays. Uses `normalize` module. | Unit (identical → empty vec, one field differs → one FieldChange, array reorder → empty, text field → similarity in FieldChange) |
| 7 | `feat: add Pass 1 matcher with exact handle + fuzzy content matching` | Matcher module | `crates/diff/src/matcher.rs` — `match_graphs(graph_a, graph_b, schema) -> MatchResult`. Pass 1a: exact handle match → SAME/MODIFIED. Pass 1b: fuzzy content match for unmatched items using per-type match keys (strong + weak). Produces `HashMap<Handle, Handle>` handle map (direction: B-handle → A-handle) and `Vec<ItemDiff>` with classifications SAME, MODIFIED, ADDED, REMOVED, NEEDS_REVIEW. For large graphs, unmatched items are indexed by strong match key for O(1) lookup instead of O(n×m) pairwise comparison. | Unit (exact match, one-field diff, fuzzy match by name+birth, no match → REMOVED/ADDED, ambiguous → NEEDS_REVIEW with candidates; edge cases: Person with no name fields → no fuzzy match possible, Person with Unicode-only name → matched by normalized Unicode comparison, empty graphs for a given type → all ADDED/REMOVED) |
| 8 | `feat: add Pass 2 extrinsic/cascading diff resolution` | Cascading module | `crates/diff/src/cascading.rs` — `resolve_extrinsic(diffs, handle_map) -> Vec<ItemDiff>`. For each matched pair, re-evaluate handle-reference fields through the handle map. Produces EXTRINSIC_ONLY classification when only handle remappings differ. | Unit (extrinsic-only case, intrinsic + extrinsic mix, no remap needed) |
| 9 | `feat: add text and JSON output formatters` | Output formatters | `crates/diff/src/output.rs` — `format_text(report) -> String` (summary table + per-item details, optional extrinsic filtering) and `format_json(report) -> String` (compact JSON). | Unit (text output contains expected headings, JSON parses and matches report data, empty report, summary-only flag) |
| 10 | `feat: add visualizer index output format` | Visualizer index | `crates/diff/src/visualizer_index.rs` — `format_visualizer(report) -> String` producing the compact JSON format with `handle_map` + per-handle `{class, intrinsic_fields?, text_scores?}`. | Unit (round-trip JSON parse, all classifications represented, handle_map correctness) |
| 11 | `feat: wire run_diff orchestrator` | Diff orchestrator | `crates/diff/src/lib.rs` — `run_diff(file_a, file_b, config, schema) -> Result<DiffReport, DiffError>` where `config: DiffConfig { thresholds, include_extrinsic, normalize_enabled }`. Orchestrates: parse both files → match (Pass 1) → cascade (Pass 2) → apply thresholds → return report. Parse failures return `DiffError::ParseError` (no partial results — both files must parse). Schema version mismatch: diff proceeds using the higher version's merged schema with a warning in the report. Initial implementation loads both graphs in memory; future optimization can stream one graph while indexing the other. | Integration (fixtures generated via `generate_random()` with fixed seeds: identical → all SAME; add one person → one ADDED; modify note text → one MODIFIED; change handle refs matched as SAME → EXTRINSIC_ONLY) |
| 12 | `feat: add gramps-gen diff CLI subcommand` | CLI subcommand | `crates/cli/src/commands/diff.rs` — `DiffArgs` with `file_a`, `file_b`, `--output`, `--output-file`, `--threshold`, `--no-normalize`, `--include-extrinsic`, `--summary-only`. `run(args)` maps `--no-normalize` to `DiffConfig::normalize_enabled` and threads into `diff::run_diff()`. Writes to stdout/file. Update `crates/cli/src/commands/mod.rs` and `crates/cli/src/main.rs`. | Smoke (compiles, help text works). Integration (subprocess E2E test with two generated files) |
| 13 | `feat: add interactive resolution for ambiguous matches` | Interactive resolution | `crates/diff/src/resolve.rs` (behind `resolve` feature gate) — `run_interactive_resolution(ambiguous_cases, resolutions_file) -> ResolvedMatches`. Terminal UI loop presenting side-by-side candidate details with edge context. Saves/loads `.gramps-diff-resolve.json`. Depends on `crossterm` (optional dep). | Unit (resolution file save/load round-trip). Smoke (feature compiles). Manual (interactive TUI behavior) |
| 14 | `test: add integration tests for diff pipeline` | Diff integration tests | `crates/diff/tests/integration.rs` — full pipeline: generate two `.gramps` files with known differences, run diff, verify classification counts and field change details. | Integration (identical files → all SAME, add one person → one ADDED, modify note text → SIMILAR/MODIFIED, change handle refs → EXTRINSIC) |

---

## Notes

- **Step 1 (full graph parser)** is the largest and riskiest step. It requires
  understanding every Gramps XML element for all 10 primary types. The existing
  `extract.rs` provides a pattern to follow for the streaming quick-xml approach.
  The parser mixes ingestion (XML → data) with transformation (data → Graph with
  edges) in one step — this is an intentional tradeoff to avoid an intermediate
  representation, but the data contract between parsing and the diff matcher is
  the `typed-graph::Graph` struct, which is already validated by `graph.validate()`.
- **Steps 3–5** are independent small utilities that can be implemented in
  parallel or any order.
- **Step 6 (compare)** will be verbose — one function per primary type with many
  fields — but each function is straightforward delegation to the similarity and
  normalization modules. The data contract between compare (Step 6) and matcher
  (Step 7) is: each primary type's compare function receives two `XxxData` structs
  and produces `Vec<FieldChange>`; the matcher uses strong-match keys derived from
  the per-type normalization rules in the design doc.
- **Step 11 (orchestrator)** is the integration point: it wires Steps 1, 7, 8,
  and 9–10 together. It depends on all of them and must be implemented after
  they are committed.
- **Step 13 (interactive resolution)** is feature-gated and can be deferred
  without blocking the core diff pipeline.
- The `strsim` crate provides `levenshtein` and `jaro_winkler`; the similarity
  module wraps these with normalization to 0.0–1.0 scores. `jaro_winkler` is
  excluded from the initial similarity module (see Step 3 notes).
