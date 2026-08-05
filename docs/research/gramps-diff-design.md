# Gramps Diff Analyzer — Design Document

## Overview

The `gramps-gen diff` tool compares two `.gramps` XML files and produces a
detailed difference record spanning all 10 Gramps primary types (Person, Family,
Event, Place, Source, Citation, Repository, Media, Note, Tag), their embedded
secondary types (Name, Location, DateValue, Attribute, Url, LdsOrd,
Surname, EventRef, ChildRef, PersonRef, PlaceRef, MediaRef, RepoRef, Address),
and their inter-object relationships (edges).

The tool parses both files into the existing `typed-graph` `Graph` model (nodes

+ edges), reusing `gramps-reader` for XML parsing, then operates entirely at the
graph level.

### Data flow

```
File A ──parse──▶ Graph A ──┐
                            ├──▶ Matcher ──▶ Diff Engine ──▶ Diff Report
File B ──parse──▶ Graph B ──┘                              ──▶ Visualizer Index
```

### Crate placement

| Component | Location |
|---|---|
| Diff engine (library) | New crate `crates/diff/` |
| CLI subcommand | `crates/cli/src/commands/diff.rs` |
| XML parsing | Reuses `crates/gramps-reader/` |
| Graph model | Reuses `crates/typed-graph/` |

---

## Matching Algorithm (Two Passes)

### Pass 1a — Exact Handle Match

Index both graphs by handle. For each primary type (Person, Family, Event,
Place, Source, Citation, Repository, Media, Note, Tag):

1. For handles present in both files:
   + Compare all scalar and embedded fields. Any difference → candidate `MODIFIED`.
   + Compare all array fields. Handle-ref arrays use set comparison (order-independent).
     Embedded-ref arrays (EventRef, ChildRef, etc.) compare by the `ref` field within
     each item and their associated metadata (role, relation).
   + For text-bearing fields (note text, source title, place name, citation page,
     person name display), compute a continuous similarity score (see §Text Similarity
     Scoring). Do NOT classify the pair yet — record the score.
   + If every field is exactly identical (all text fields at 1.0 similarity) → `SAME`.
2. Handles only in file A → candidate `REMOVED`.
3. Handles only in file B → candidate `ADDED`.

At the end of Pass 1a we have:

+ Definite `SAME` pairs
+ Pairs with non-text differences → `MODIFIED`
+ Pairs where only text fields differ → held pending threshold selection
+ Unmatched items from A and B

### Pass 1b — Fuzzy Content Match

For unmatched items, attempt to find a content match in the other file using
only the item's own scalar and embedded fields. Edge relationships are NOT used
for automatic matching (they are presented during interactive resolution; see
§Interactive Resolution).

#### Fuzzy match keys per type

| Type | Strong Match Keys | Weak Match Keys |
|---|---|---|
| **Person** | Normalized primary name (first_name + surnames) + birth year | Given name + surname + approximate birth year |
| **Family** | Father name + mother name (resolved from handles) | — |
| **Event** | event_type + date.year + resolved place name | event_type + date.year only |
| **Place** | Normalized location (country + state + city + locality) | Locality name only |
| **Source** | Normalized title + normalized author | Title only |
| **Citation** | Resolved source title + normalized page | Page only |
| **Repository** | Normalized name | — |
| **Media** | checksum (if present) or normalized path | Path basename |
| **Note** | Text similarity > threshold | — |
| **Tag** | Normalized name | — |

Fuzzy match outcomes:

+ **Exactly 1 strong match** → `MATCHED_FUZZY` (the pair proceeds to field-level diff)
+ **Multiple candidates** → `NEEDS_REVIEW` (all candidates recorded in `AmbiguousCase`)
+ **No match** → `REMOVED` or `ADDED` (depending on direction)

### Pass 2 — Extrinsic (Cascading) Difference Resolution

Handle-reference fields (e.g., Citation's `source_handle`, Event's `place_handle`,
Family's `father_handle`) carry handles that may differ between files even when the
referenced item is semantically identical. We distinguish two classes of change:

| Class | Meaning | Example |
|---|---|---|
| **Intrinsic** | The item's own data changed | Note text changed, person's gender changed |
| **Extrinsic / Referential** | Only a referenced handle changed; the referenced item's content is identical | Citation points to Source `SXYZ` instead of `S001`, but `S001` and `SXYZ` were matched as `SAME` in Pass 1 |

To resolve extrinsic changes, build a **handle-remapping table** from all matches
established in Pass 1:

```
handle_map: B-handle → A-handle    (for every matched pair)
```

Then for every matched pair, re-evaluate each handle-reference field:

```
if handle_map.lookup(field_value_in_B) == field_value_in_A:
    → EXTRINSIC_CHANGE  (handle differs but resolves to same matched item)
else:
    → INTRINSIC_CHANGE  (genuinely different reference)
```

Both intrinsic and extrinsic changes are always present in the output. Downstream
consumers (report formatter, visualizer index, scripts) decide independently
whether to surface or collapse extrinsic changes.

---

## Text Similarity Scoring

For text-bearing fields, the diff engine computes a continuous similarity score
but does NOT classify the pair. Classification comes later when the user selects
per-type thresholds.

### Similarity methods by field type

| Field | Method | Rationale |
|---|---|---|
| `note.text` | Levenshtein (character-level) | Free text; catches typos, rewrites, formatting changes |
| `source.title` | Token Jaccard + Levenshtein as tiebreaker | "1850 US Census" ≈ "U.S. Census, 1850" — word order varies |
| `source.author` | Tokenized Levenshtein | "J. Smith" and "John Smith" share initials |
| `place.name.*` | Levenshtein on normalized form | After abbreviation expansion |
| `person.primary_name.display` | Levenshtein | Display names vary in formatting |
| `citation.page` | Levenshtein after prefix stripping | "p. 12" vs "12" vs "folio 12" |

### Score formula

```
normalized_similarity = 1 - (levenshtein_distance / max(len(a), len(b)))
```

Range: 0.0 (completely different) to 1.0 (identical).

### Threshold-based classification

The diff report includes a distribution of similarity scores for each text field
type. The user selects per-type thresholds; pairs above the threshold are
classified as `SIMILAR` (not modified enough to count as `MODIFIED`) and pairs
below are `MODIFIED`.

A future interactive threshold-tuning UI could show a histogram and let the user
slide thresholds while watching classification counts update in real time.

---

## Per-Type Normalization Rules

### Person

| Field | Normalization |
|---|---|
| `primary_name.first_name` | Case-fold, trim, collapse whitespace |
| `primary_name.surname_list[*].surname` | Case-fold, trim |
| `primary_name.surname_list[*].prefix` | Case-fold, trim |
| `primary_name.display` | Compare structurally (not as display string), or Levenshtein with similarity score |
| `alternate_names` | Set comparison by normalized name content (every sub-field) |
| `gender` | `None` or `0` ≡ unknown. Otherwise exact integer match |
| Birth/death dates | Resolved via `event_ref_list` → actual event date, not the event handle |

### Name (secondary type)

Compare sub-fields independently: `type`, `first_name`, `surname_list` (set comparison
by `surname` + `prefix` + `primary`), `suffix`, `title`, `date`. Do NOT rely on the
`display` field for matching — it is a derived rendering.

### Family

| Field | Handling |
|---|---|
| `father_handle`, `mother_handle` | Handle refs; subject to extrinsic resolution in Pass 2 |
| `child_ref_list` | Set comparison by `ChildRef.ref`. `ChildRef.relation` compared per child when matched by ref |
| `event_ref_list` | Set comparison by `EventRef.ref`. `EventRef.role` compared per event when matched by ref |

### Event

| Field | Normalization |
|---|---|
| `event_type` | Exact enum match (Birth ≢ Death) |
| `date` | Compare year, month, day individually. `quality` and `modifier` differences are reported but do not prevent matching |
| `place_handle` | Extrinsic resolution in Pass 2; compare resolved place name, not the handle |

### Place

| Field | Normalization |
|---|---|
| `name.locality`, `name.city`, `name.county`, `name.state`, `name.country` | Case-fold, diacritic-insensitive |
| `name.street` | Abbreviation expansion: "Street" → "St", "Avenue" → "Ave", "Road" → "Rd", etc. |
| `place_ref_list` | Extrinsic resolution; hierarchical position difference is an extrinsic change |

### Source

| Field | Normalization |
|---|---|
| `title` | Token Jaccard similarity + Levenshtein as tiebreaker |
| `author` | Case-fold, trim. Tokenize and compare; "J. Smith" and "John Smith" match on surname + initial |
| `pubinfo` | Levenshtein with similarity score |
| `reporef_list` | Set comparison by `RepoRef.ref`. `RepoRef.call_number` and `media_type` compared per repo |

### Citation

| Field | Normalization |
|---|---|
| `source_handle` | Extrinsic resolution |
| `page` | Strip "p.", "pp.", "page", "folio", "f." prefixes. Normalize whitespace. Then Levenshtein |
| `confidence` | Exact integer comparison |

### Repository

| Field | Normalization |
|---|---|
| `name` | Case-fold, trim. Primary fuzzy-match key |
| `type` | Exact enum match |
| `address_list` | Set comparison by normalized `Location` content |

### Media

| Field | Normalization |
|---|---|
| `checksum` | If present in both, exact match is dispositive. If only one has it, report as `MISSING_CHECKSUM` info item |
| `path` | Compare basename and extension separately from directory. Media may move between directories |
| `mime_type` | Exact match after case-fold |

### Note

| Field | Normalization |
|---|---|
| `text` | Strip HTML tags (if `format` indicates HTML), normalize whitespace, then Levenshtein. Record similarity score |
| `format` | Exact integer match |
| `type` | Exact enum match |

### Tag

| Field | Normalization |
|---|---|
| `name` | Case-fold, trim |
| `color` | Case-fold, strip leading `#`. Normalize to 6-char hex: `#FF0000` → `ff0000`, `#f00` → `ff0000` |
| `priority` | Exact integer |
| `tag_list` | Set comparison |

---

## Output Data Model

### `DiffReport` (top-level)

```
DiffReport {
  file_a: Path,
  file_b: Path,
  schema_version: String,
  generated_at: DateTime,

  summary: DiffSummary {
    same: usize,
    modified: usize,         // intrinsic changes (non-text fields)
    extrinsic_only: usize,   // only extrinsic (handle-remapping) changes
    similar: usize,          // text above threshold, otherwise identical
    added: usize,
    removed: usize,
    needs_review: usize,
  },

  by_type: HashMap<TypeName, DiffSummary>,

  diffs: Vec<ItemDiff>,

  ambiguous: Vec<AmbiguousCase>,

  /// B-handle → A-handle for every matched pair
  handle_map: HashMap<Handle, Handle>,

  /// Similarity scores per text field path, for threshold selection
  text_similarities: HashMap<FieldPath, Vec<f64>>,
}
```

### `ItemDiff`

```
ItemDiff {
  type: PrimaryType,
  handle_a: Option<Handle>,   // None if ADDED
  handle_b: Option<Handle>,   // None if REMOVED
  classification: Classification,

  intrinsic_changes: Vec<FieldChange>,
  extrinsic_changes: Vec<FieldChange>,

  /// Text similarity scores for this item's text fields
  text_scores: HashMap<FieldPath, f64>,
}
```

### `Classification` enum

```
SAME           — all fields identical (text at 1.0)
MODIFIED       — at least one non-text field differs, or text below threshold
SIMILAR        — only text fields differ, all above threshold
EXTRINSIC_ONLY — only extrinsic changes (handle remappings)
ADDED          — present only in file B
REMOVED        — present only in file A
NEEDS_REVIEW   — ambiguous fuzzy match, requires human resolution
```

### `FieldChange`

```
FieldChange {
  path: String,           // e.g. "primary_name.first_name"
  kind: FieldKind,        // Scalar | Array | Embedded | HandleRef | Text
  old_value: JsonValue,
  new_value: JsonValue,
  similarity: Option<f64>, // only for Text kind
}
```

### `AmbiguousCase`

```
AmbiguousCase {
  type: PrimaryType,
  handle: Handle,              // the unmatched handle
  direction: Direction,        // FromA | FromB
  candidates: Vec<Candidate>,
  context: AmbiguousContext,   // edge-derived related items for user review
}

Candidate {
  handle: Handle,
  similarity: f64,
  matched_fields: Vec<String>,
  mismatched_fields: Vec<FieldChange>,
}

AmbiguousContext {
  related_items: Vec<RelatedItem>,
  // For Person: spouse name + handle, child names + handles, associated events
  // For Family: father/mother/child names
  // For Event: associated person/family names
  // For Citation: resolved source title
}
```

### Visualizer Index Format

A compact JSON output for the `visualize` crate:

```json
{
  "file_a": "path/to/a.gramps",
  "file_b": "path/to/b.gramps",
  "handle_map": { "P7890": "I0042", "SXYZ": "S001" },
  "items": {
    "I0001": { "class": "SAME" },
    "I0002": { "class": "MODIFIED", "intrinsic_fields": ["gender", "primary_name"] },
    "I0003": { "class": "ADDED" },
    "I0004": { "class": "REMOVED" },
    "I0042": { "class": "SIMILAR", "text_scores": { "note_list[0].text": 0.87 } }
  }
}
```

The visualizer can color-code nodes by classification and provide hover details
on which fields changed. The `handle_map` lets the visualizer resolve
cross-references when loading a single file but displaying diff status from
the other.

---

## Interactive Resolution CLI

When `--resolve` is passed, the diff tool enters an interactive loop after Pass 2.
For each `AmbiguousCase`, the user sees side-by-side details including edge
context (which was NOT used for automatic matching, only for display):

```
┌──────────────────────────────────────────────────────────────┐
│ Ambiguous Match 3/5 — Person                                 │
│                                                              │
│ File A: I0042                          File B: P7890         │
│ ─────────────────                      ─────────────────     │
│ Name:    John Smith                    Name:    John Smith   │
│ Birth:   1850-07-13                    Birth:   about 1850   │
│ Gender:  Male                          Gender:  Male         │
│                                                              │
│ Related:                               Related:              │
│   Spouse: Mary Jones (I0043)             Spouse: Mary Jones  │
│   Child:  Robert Smith (I0100)           Child:  Robt Smith  │
│                                                              │
│ Similarity: 0.91                                            │
│                                                              │
│ [Y] Match    [N] Different    [S] Skip    [D] Field diff    │
└──────────────────────────────────────────────────────────────┘
```

Resolved matches are saved to a `.gramps-diff-resolve.json` file. On subsequent
runs, `--resolutions-file` loads these to skip re-resolution.

---

## CLI Interface

```
gramps-gen diff <FILE_A> <FILE_B> [OPTIONS]

Arguments:
  FILE_A    First .gramps file
  FILE_B    Second .gramps file

Options:
  --output FORMAT           Output format: text (default), json, or visualizer
  --output-file PATH        Write output to file instead of stdout
  --resolve                 Enter interactive resolution for NEEDS_REVIEW cases
  --resolutions-file PATH   Load/save manual resolutions (default: <FILE_A>.gramps-diff-resolve.json)
  --threshold TYPE:VALUE    Set similarity threshold per type (e.g. --threshold Note:0.75).
                            May be specified multiple times. Without this, no threshold
                            classification is applied and raw scores are reported.
  --no-normalize            Skip text normalization (whitespace, case, HTML stripping)
  --include-extrinsic       Include extrinsic changes in text output (on by default for json)
  --summary-only            Print only the summary table, not per-item diffs

Examples:
  gramps-gen diff tree_v1.gramps tree_v2.gramps
  gramps-gen diff tree_v1.gramps tree_v2.gramps --output json --threshold Note:0.7
  gramps-gen diff tree_v1.gramps tree_v2.gramps --resolve --output visualizer
```

---

## Tricky Scenarios

| Scenario | How Handled |
|---|---|
| Same person, different UUID | Fuzzy match by name + birth. If ambiguous, flagged for review with edge context |
| Note with minor text edits | Levenshtein similarity score. User controls threshold |
| Citation same, source handle changed | Extrinsic change detected in Pass 2. Both intrinsic and extrinsic classes reported |
| Place hierarchy differs (Springfield, IL vs Springfield, Illinois, USA) | Place matched by normalized location fields. Hierarchy difference is extrinsic |
| "p. 12" vs "12" vs "folio 12" | Page prefix stripping before comparison |
| HTML vs plain text notes | HTML tags stripped by default (`--no-normalize` to disable) |
| Gender missing (schema 5.1) vs explicit "Unknown" (5.2) | Normalize: missing ≡ 0 |
| Same source, different title formatting | Token Jaccard handles word-order differences |
| Family with reordered children | Set comparison, order-independent |
| Date "about 1850" vs exact "1850-07-13" | Year matches; month/day recorded as field-level diff but does not prevent fuzzy match |
| Media moved between directories | Path compared by basename + extension, not full directory |
| Tag color "red" vs "#FF0000" | Color normalized to lowercase 6-char hex |
| Same event, different place handle | Place handle resolved through handle_map in Pass 2 |
| Person's birth event handle changed | If the resolved event (type + date + place) matches via fuzzy match, this is extrinsic |

---

## Crate Structure

```
crates/diff/
├── Cargo.toml
└── src/
    ├── lib.rs                 # Re-exports, run_diff() entry point
    ├── matcher.rs             # Pass 1: exact handle match + fuzzy content match
    ├── compare.rs             # Field-level comparison dispatch for each primary type
    ├── normalize.rs           # Text normalization, abbreviation expansion, HTML stripping
    ├── similarity.rs          # Levenshtein, Jaccard, token-based similarity functions
    ├── cascading.rs           # Pass 2: extrinsic diff resolution via handle map
    ├── report.rs              # DiffReport, ItemDiff, FieldChange, serialization
    ├── output.rs              # Text, JSON, and visualizer-index output formatters
    ├── resolve.rs             # Interactive CLI resolution (behind 'resolve' feature gate)
    └── visualizer_index.rs    # Compact output format consumed by the visualize crate
```

### New workspace dependencies

| Crate | Purpose |
|---|---|
| `strsim` | Levenshtein and Jaro-Winkler distance |
| `crossterm` or `ratatui` | Terminal UI for interactive resolution (optional, behind feature gate) |

### Existing dependencies reused

| Crate | Use |
|---|---|
| `typed-graph` | Graph, Node, Edge, schema types |
| `gramps-reader` | Streaming XML parse for both files |
| `serde` / `serde_json` | Output serialization |
| `clap` | CLI argument parsing (in `cli` crate) |
