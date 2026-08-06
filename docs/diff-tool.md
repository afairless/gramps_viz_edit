# Diff Tool User Guide

The `gramps-gen diff` command compares two Gramps XML (`.gramps`) files and
produces a structured report showing what changed between them, item by item
and field by field.

## Table of Contents

- [Quick Start](#quick-start)
- [How the Diff Pipeline Works](#how-the-diff-pipeline-works)
- [Understanding the Output](#understanding-the-output)
  - [Summary Table](#summary-table)
  - [Item Classifications](#item-classifications)
  - [Field-Level Changes](#field-level-changes)
- [Command Reference](#command-reference)
  - [Arguments](#arguments)
  - [Options](#options)
  - [Output Formats](#output-formats)
- [How Matching Works](#how-matching-works)
  - [Pass 1a — Exact Handle Match](#pass-1a--exact-handle-match)
  - [Pass 1b — Fuzzy Content Match](#pass-1b--fuzzy-content-match)
  - [Pass 2 — Cascading Extrinsic Resolution](#pass-2--cascading-extrinsic-resolution)
- [Output Formats in Detail](#output-formats-in-detail)
  - [Text Format](#text-format)
  - [JSON Format](#json-format)
  - [CSV Format](#csv-format)
- [Per-Type Field Comparison](#per-type-field-comparison)
- [Interactive Resolution](#interactive-resolution)
- [Visualizer Integration](#visualizer-integration)
- [Using the Diff Library from Rust](#using-the-diff-library-from-rust)
- [Text Normalization](#text-normalization)
- [Similarity Scoring](#similarity-scoring)
- [Tips and Common Workflows](#tips-and-common-workflows)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

```bash
# Basic comparison (text output to stdout)
gramps-gen diff original.gramps modified.gramps

# Comparison with JSON output, saved to a file
gramps-gen diff original.gramps modified.gramps --output json --output-file diff.json

# Summary only (no per-item details)
gramps-gen diff original.gramps modified.gramps --summary-only

# CSV output for spreadsheet analysis
gramps-gen diff original.gramps modified.gramps --output csv --output-file diff.csv
```

At minimum you provide two `.gramps` files: the first is the **baseline** (A)
and the second is the **target** (B). The diff compares items in A against
items in B.

---

## How the Diff Pipeline Works

The diff tool runs a three-stage pipeline:

```
┌──────────────┐    ┌──────────────────┐    ┌───────────────────┐
│  Parse both  │ →  │  Pass 1: Match   │ →  │  Pass 2: Cascade  │ → Report
│  .gramps     │    │  (1a exact +     │    │  (extrinsic       │
│  files       │    │   1b fuzzy)      │    │   resolution)     │
└──────────────┘    └──────────────────┘    └───────────────────┘
```

1. **Parse** — Both files are read and parsed into typed in-memory graphs
   (`typed_graph::Graph`). Gzip-compressed files are detected and
   transparently decompressed.

2. **Pass 1: Match** — Items are paired between the two graphs (see
   [How Matching Works](#how-matching-works)). Produces a handle map and
   per-item classifications.

3. **Pass 2: Cascade** — Handle-reference changes are re-evaluated through
   the handle map. Items whose only changes are extrinsic handle remappings
   are reclassified from `MODIFIED` to `EXTRINSIC ONLY`.

4. **Report** — The final report is formatted in the requested output format.

---

## Understanding the Output

### Summary Table

Every diff run produces a summary with counts across six classification
categories:

```text
Gramps Diff Report
==================

Summary
-------
Total (A):        250
Total (B):        252
Same:             200
Modified:          20
Added:             22
Removed:           18
Needs Review:       2
Extrinsic Only:    10
```

| Field            | Meaning                                                                               |
|------------------|---------------------------------------------------------------------------------------|
| `Total (A)`      | Total items (nodes) in the first file                                                 |
| `Total (B)`      | Total items in the second file                                                        |
| `Same`           | Items identical in both files                                                         |
| `Modified`       | Items present in both files with changed fields                                       |
| `Added`          | Items present in B but not A                                                          |
| `Removed`        | Items present in A but not B                                                          |
| `Needs Review`   | Items that could not be matched unambiguously                                         |
| `Extrinsic Only` | Items whose only changes are handle-reference remappings (considered non-semantic)    |

### Item Classifications

Each item in the report receives one of six classifications:

#### `SAME`

The item exists in both files with identical fields. No changes to report.

```text
[SAME] Person (A: abc123 [I0001] "John Smith", B: abc123 [I0001] "John Smith")
```

#### `MODIFIED`

The item exists in both files but one or more fields have changed. Each
changed field is listed with its old value, new value, and similarity score.

```text
[MODIFIED] Person (A: abc123 [I0001] "John Smith", B: def456 [I0001] "John Jones")
  surname: "Smith" -> "Jones" (similarity 0.50)
```

#### `ADDED`

The item exists only in the second file (B). No field-level changes are shown
because there is no corresponding item in A to compare against.

```text
[ADDED] Note (B: note789 [N0003])
```

#### `REMOVED`

The item exists only in the first file (A). It was present in A but is absent
in B.

```text
[REMOVED] Tag (A: tag001)
```

#### `NEEDS REVIEW`

The matcher found multiple candidate matches for an item and could not
determine which one is correct. These cases require human judgment.

```text
[NEEDS REVIEW] Person (A: p001)
```

See [Interactive Resolution](#interactive-resolution) for how to resolve
these cases.

#### `EXTRINSIC ONLY`

The item's intrinsic fields (names, dates, text) match exactly between the
two files, but the item's **handle references** (links to other items) differ
because the referenced items were remapped during regeneration. These changes
are considered **non-semantic** — the item logically refers to the same
related entities, just using different handle values.

```text
[EXTRINSIC ONLY] Citation (A: c001, B: c001)
  source_handle: "SRC_A" -> "SRC_B" (similarity 1.00)
```

### Field-Level Changes

For `MODIFIED` items, each changed field is shown with:

- **Field name** — the path to the field (e.g., `surname`, `primary_name.display`,
  `event_ref_list[0].role`)
- **Old value** — the value in file A
- **New value** — the value in file B
- **Similarity score** — a number from 0.0 (completely different) to 1.0
  (identical after normalization)

#### Field Kinds

| Kind            | Description                                        | Examples                                          |
|-----------------|----------------------------------------------------|---------------------------------------------------|
| `Text`          | Free-text fields                                   | `surname`, `title`, `text`                        |
| `Date`          | Date values                                        | `date`, `birth_date`                              |
| `HandleRef`     | A single reference to another item                 | `father_handle`, `source_handle`                  |
| `HandleRefList` | A list of references to other items                | `citation_list`, `family_list`                    |
| `Enum`          | A categorical/choice field                         | `gender`, `event_type`, `type_field`              |
| `Boolean`       | A true/false field                                 | `primary`                                         |
| `Numeric`       | A numeric value                                    | `confidence`, `priority`, `format`                |

---

## Command Reference

```text
gramps-gen diff [OPTIONS] <FILE_A> <FILE_B>
```

### Arguments

| Argument | Description                                            |
|----------|--------------------------------------------------------|
| `FILE_A` | First Gramps XML file — the **baseline** (side A)      |
| `FILE_B` | Second Gramps XML file — the **target** (side B)       |

### Options

| Option                | Default              | Description                                                                 |
|-----------------------|----------------------|-----------------------------------------------------------------------------|
| `--output <FORMAT>`   | `text`               | Output format: `text`, `json`, or `csv`                                     |
| `--output-file <PATH>`| stdout (printed)     | Write output to a file instead of printing                                  |
| `--threshold <FLOAT>` | `0.8`                | (Reserved) Minimum similarity threshold for text field matches              |
| `--no-normalize`      | `false`              | (Reserved) Would disable text normalization before comparison               |
| `--include-extrinsic` | `false`              | Include extrinsic-only items in the report output                           |
| `--summary-only`      | `false`              | Print only the summary section, omitting per-item details                   |

> **Note on `--threshold` and `--no-normalize`**: These flags are accepted by
> the CLI and stored in the configuration struct, but the current
> implementation always normalizes text and always reports all field-level
> differences. The flags are reserved for a future release where the
> similarity threshold will control match sensitivity and normalization
> can be toggled off.

### Output Formats

| Format            | Best for                                           |
|-------------------|----------------------------------------------------|
| `text` (default)  | Human reading at the terminal                      |
| `json`            | Machine processing, programmatic consumption       |
| `csv`             | Spreadsheet import, pivot tables, further analysis |

---

## How Matching Works

The diff pipeline runs in three stages.

### Pass 1a — Exact Handle Match

Items that share the **same handle** (internal UUID) in both files are paired
first. They are compared field-by-field and classified as either `SAME` or
`MODIFIED`.

This pass handles the common case where a file was edited in Gramps and the
application preserved handles across the edit.

### Pass 1b — Fuzzy Content Match

Items that could **not** be matched by handle are compared using **strong
match keys** — distinctive combinations of fields that identify an item even
when its handle changes.

| Item Type | Strong Key Fields |
|---|---|
| Person | `surname \| first_name \| birth_date` |
| Family | `father_handle \| mother_handle` |
| Event | `event_type \| date \| description` |
| Place | `city \| street` |
| Source | `title` |
| Citation | `source_handle \| page` |
| Repository | `name` |
| Media | `path \| checksum` |
| Note | `text` |
| Tag | `name` |

When a single item from A matches exactly one item from B by strong key, they
are paired and classified as `SAME` or `MODIFIED` (confidence 0.9).

When **multiple** items from A have the same strong key as an item from B
(e.g., two people named "John Smith" born in 1850), the pair is flagged as
`NEEDS REVIEW` with the candidate list.

#### Match confidence scores

| Scenario | Confidence |
|---|---|
| Exact handle match (Pass 1a) | `1.0` |
| Strong key match (Pass 1b, unique) | `0.9` |
| Strong key match (Pass 1b, ambiguous) | `NEEDS REVIEW` |
| Unmatched item (ADDED / REMOVED) | `1.0` |

### Pass 2 — Cascading Extrinsic Resolution

After Pass 1 produces a **handle map** (mapping B-handles → A-handles for
matched items), Pass 2 re-evaluates handle-reference changes.

When a `MODIFIED` item's only changes are handle references that resolve
through the handle map — meaning the B-side handle refers to the same
underlying entity as the A-side handle — the item is reclassified as
`EXTRINSIC ONLY`.

For example:

- File A has `Citation("C001")` referencing `Source("SRC_A")`
- File B has `Citation("C001")` referencing `Source("SRC_B")`
- The handle map records `SRC_B → SRC_A` (same source, different handle)
- The citation's only change (the source handle remapping) is extrinsic
- The citation is classified as `EXTRINSIC ONLY`

Items with a mix of intrinsic changes (e.g., a text field changed) and
extrinsic changes remain `MODIFIED`.

Pass 2 also handles changes in handle-ref **lists** (e.g., `citation_list`,
`family_list`) using set comparison — B-side handles are remapped through the
handle map and compared as unordered sets against the A-side handles.

---

## Output Formats in Detail

### Text Format

The default `text` format produces a human-readable report with three sections:

1. **Header** — "Gramps Diff Report"
2. **Summary** — tabular counts of each classification
3. **Items** — per-item details with field-level changes
4. **Ambiguous Cases** (if any) — items needing review with their candidates

```text
Gramps Diff Report
==================

Summary
-------
Total (A):        10
Total (B):        12
Same:              5
Modified:          2
Added:             3
Removed:           1
Needs Review:      1
Extrinsic Only:    0

Items
-----

[SAME] Person (A: abc123 [I0001] "John Smith", B: abc123 [I0001] "John Smith")

[MODIFIED] Person (A: def456 [I0002] "Jane Doe", B: def456 [I0003] "Jane Jones")
  surname: "Doe" -> "Jones" (similarity 0.50)
  primary_name.display: "Jane Doe" -> "Jane Jones" (similarity 0.60)

[ADDED] Note (B: note789 [N0004])

[REMOVED] Tag (A: tag001)

Ambiguous Cases
---------------
[NEEDS REVIEW] Person (A: p001)
  John Smith: p001
  Candidate p002 (score 0.9): John Smith
  Candidate p003 (score 0.8): John Smith
```

When `--summary-only` is used, output stops after the summary section.

### JSON Format

The `json` format produces a compact single-line JSON object with the full
report structure. This is ideal for programmatic consumption.

```json
{
  "summary": {
    "total_a": 10,
    "total_b": 12,
    "same": 5,
    "modified": 2,
    "added": 3,
    "removed": 1,
    "needs_review": 1,
    "extrinsic_only": 0
  },
  "items": [
    {
      "handle_a": "abc123",
      "handle_b": "abc123",
      "gramps_id_a": "I0001",
      "gramps_id_b": "I0001",
      "display_name_a": "John Smith",
      "display_name_b": "John Smith",
      "item_type": "Person",
      "classification": "Same",
      "field_changes": [],
      "confidence": 1.0
    }
  ],
  "ambiguous_cases": []
}
```

The JSON output always includes all items regardless of classification,
including `EXTRINSIC ONLY` items. Use `--summary-only` in JSON format
to limit output to the summary object.

### CSV Format

The `csv` format produces one row per **field change** (for `MODIFIED` items)
or one row per item (for non-`MODIFIED` items). Columns:

| Column            | Description                                                                           |
| ----------------- | ------------------------------------------------------------------------------------- |
| `classification`  | One of `SAME`, `MODIFIED`, `ADDED`, `REMOVED`, `NEEDS REVIEW`, `EXTRINSIC ONLY`       |
| `item_type`       | The type of Gramps item (Person, Family, Event, etc.)                                 |
| `handle_a`        | Handle in file A (empty for ADDED)                                                    |
| `gramps_id_a`     | Gramps ID in file A (e.g., "I0002", "E0005")                                         |
| `display_name_a`  | Human-readable name in file A                                                         |
| `handle_b`        | Handle in file B (empty for REMOVED)                                                  |
| `gramps_id_b`     | Gramps ID in file B                                                                   |
| `display_name_b`  | Human-readable name in file B                                                         |
| `confidence`      | Match confidence (0.0–1.0)                                                            |
| `field_name`      | Name of the changed field (empty for non-MODIFIED)                                    |
| `field_kind`      | Kind of field that changed (Text, Date, Enum, etc.)                                   |
| `old_value`       | Value in file A                                                                       |
| `new_value`       | Value in file B                                                                       |
| `similarity`      | Similarity score for this field change                                                |

All CSV cells are quoted. Newlines within values are replaced with spaces.
Double-quotes inside values are escaped as `""`.

---

## Per-Type Field Comparison

The diff tool performs deep, field-by-field comparison for each Gramps item
type. Here is what gets compared for each type:

### Person

| Field path | Kind | Notes |
|---|---|---|
| `gramps_id` | Text | |
| `gender` | Enum | Compared as discriminant |
| `primary_name` | (nested) | See [Name comparison](#name-comparison) |
| `primary_name.display` | Text | Full display name |
| `primary_name.first_name` | Text | |
| `primary_name.surname_list[i].surname` | Text | Positional comparison |
| `primary_name.surname_list[i].prefix` | Text | |
| `primary_name.surname_list[i].primary` | Boolean | |
| `primary_name.surname_list[i].origintype` | Enum | |
| `primary_name.suffix` | Text | |
| `primary_name.title` | Text | |
| `primary_name.type_field` | Enum | Name type (Birth, Married, etc.) |
| `primary_name.date` | Date | |
| `alternate_names[i]` | (nested) | Same structure as `primary_name`, positional |
| `event_ref_list[i]` | HandleRefList | With role comparison |
| `person_ref_list[i]` | HandleRefList | With relation comparison |
| `family_list` | HandleRefList | Set comparison |
| `parent_family_list` | HandleRefList | Set comparison |
| `citation_list` | HandleRefList | Set comparison |
| `note_list` | HandleRefList | Set comparison |
| `tag_list` | HandleRefList | Set comparison |
| `media_list[i]` | HandleRefList | Set comparison |
| `address_list[i]` | (nested) | Positional, date + location + citations + notes |
| `attribute_list[i]` | (nested) | Positional, type + value + citations + notes |
| `lds_ord_list[i]` | (nested) | Positional, type + date + status + temple + place + citations + notes |
| `url_list[i]` | (nested) | Positional, description + href + type |

### Family

| Field path | Kind | Notes |
|---|---|---|
| `gramps_id` | Text | |
| `father_handle` | HandleRef | |
| `mother_handle` | HandleRef | |
| `child_ref_list[i]` | HandleRefList | With relation comparison |
| `event_ref_list[i]` | HandleRefList | With role comparison |
| `media_list[i]` | HandleRefList | |
| `citation_list` | HandleRefList | Set comparison |
| `note_list` | HandleRefList | Set comparison |
| `tag_list` | HandleRefList | Set comparison |
| `attribute_list[i]` | (nested) | Positional |

### Event

| Field path | Kind | Notes |
|---|---|---|
| `gramps_id` | Text | |
| `event_type` | Enum | Display-name comparison |
| `date` | Date | |
| `description` | Text | |
| `place_handle` | HandleRef | |
| `citation_list` | HandleRefList | Set comparison |
| `note_list` | HandleRefList | Set comparison |
| `tag_list` | HandleRefList | Set comparison |
| `media_list[i]` | HandleRefList | |
| `attribute_list[i]` | (nested) | Positional |

### Place, Source, Citation, Repository, Media, Note, Tag

Each type follows the same pattern: optional `gramps_id`, type-specific text
fields, handle-ref lists for related items, and nested struct arrays
(attribute lists, address lists, URL lists) compared positionally.

For the complete field list, see the `compare_*` functions in
`crates/diff/src/compare.rs`.

---

## Interactive Resolution

When the diff produces `NEEDS REVIEW` items, the diff library provides an
interactive terminal-based resolution loop (requires the `resolve` Cargo
feature at compile time).

```bash
# Build the CLI with the resolve feature enabled
cargo build --release --features resolve,cli
```

> There is no separate `gramps-gen-resolve` binary. The interactive
> resolution is part of the `diff` library crate, exposed through the
> `diff::resolve::run_interactive_resolution()` function. A future
> release may add a dedicated CLI subcommand.

The interactive terminal TUI presents each ambiguous case side-by-side with
its candidates. You can:

- Enter a **number** (1-indexed) to select that candidate
- Enter **`s`** to skip the case (leave unresolved)
- Enter **`q`** to quit early (remaining cases are left unresolved)

Resolutions are saved incrementally to a JSON file (`.gramps-diff-resolve.json`)
in the current directory, so you can resume interrupted sessions.

To use the resolution feature from your own Rust code:

```rust
use diff::report::AmbiguousCase;
use diff::resolve::run_interactive_resolution;

let cases: Vec<AmbiguousCase> = /* from a previous diff run */;
let result = run_interactive_resolution(&cases, ".gramps-diff-resolve.json");

for (handle_a, resolved) in &result.resolved {
    println!("{} → {} (confidence {})", handle_a, resolved.handle_b, resolved.confidence);
}
for handle in &result.unresolved {
    println!("{} remains unresolved", handle);
}
```

---

## Visualizer Integration

The diff library provides a `format_visualizer()` function in the
`visualizer_index` module that produces a compact JSON payload consumed by
the `gramps-gen-visualize` graph visualizer. The index maps each item's
handle to its classification and intrinsic field-change metadata.

The visualizer index structure:

```json
{
  "handle_map": {
    "B001": "A001",
    "B002": "A002"
  },
  "entries": {
    "A001": { "class": "Same" },
    "A002": {
      "class": "Modified",
      "intrinsic_fields": ["surname"],
      "text_scores": { "surname": 0.5 }
    },
    "B003": { "class": "Added" }
  }
}
```

This function is part of the `diff` library (`diff::visualizer_index::format_visualizer()`)
and can be used by Rust code that embeds the diff tool. The `gramps-gen visualize`
subcommand does **not** yet accept a `--diff` flag — this integration is
planned for a future release.

To generate the visualizer index programmatically:

```rust
use diff::run_diff;
use diff::visualizer_index::format_visualizer;
use diff::DiffConfig;

let report = run_diff("before.gramps", "after.gramps", &DiffConfig::default())?;
let visualizer_json = format_visualizer(&report);
// Pass visualizer_json to the visualize crate via IPC
```

---

## Using the Diff Library from Rust

The diff tool is also available as a Rust library crate (`diff`). The public
API provides:

```rust
use diff::{
    run_diff,              // Run the full diff pipeline
    DiffConfig,            // Configuration struct
    DiffError,             // Error type (ParseError, SchemaMismatch, EmptyGraph, InternalError)
    DiffReport,            // Top-level report
    DiffSummary,           // Summary statistics
    ItemDiff,              // Per-item diff result
    FieldChange,           // A single field-level change
    FieldKind,             // Kind of field (Text, Date, HandleRef, etc.)
    Classification,        // Item classification (Same, Modified, Added, etc.)
    AmbiguousCase,         // An ambiguous match case
    output::format_text,   // Render report as human-readable text
    output::format_json,   // Render report as compact JSON
    output::format_csv,    // Render report as CSV
    visualizer_index::format_visualizer, // Render report as visualizer index
};

// Feature-gated (requires "resolve" feature):
// resolve::run_interactive_resolution
```

The full report structure serializes to and from JSON via serde, enabling
you to save reports, load them later, or pass them between processes.

---

## Text Normalization

Before comparing text fields, the diff tool applies several normalization
steps to reduce false positives from cosmetic differences:

| Normalization                       | What it does                                         | Example                                           |
| ----------------------------------- | ---------------------------------------------------- | ------------------------------------------------- |
| **Case folding**                    | Converts all text to lowercase                       | `"Smith"` → `"smith"`                             |
| **Whitespace collapsing**          | Replaces runs of whitespace with a single space      | `"John   Smith"` → `"John Smith"`                 |
| **HTML tag stripping**             | Removes content between `<` and `>`                  | `"<b>Note</b>"` → `"Note"`                        |
| **Street abbreviation expansion**  | Expands common abbreviations                         | `"Main St"` → `"Main Street"`                     |
| **Page prefix stripping**          | Removes "p.", "pp.", "page ", "pages "           | `"p. 123"` → `"123"`                               |
| **Tag color normalization**        | Maps named colors to hex                             | `"red"` → `"#ff0000"`                              |
| **Diacritic-insensitive comparison**| Strips accents for equality checks                  | `"café"` ≈ `"cafe"`                                |

These normalizations are always applied in the current implementation.
The `--no-normalize` flag is accepted but does not yet skip normalization.

---

## Similarity Scoring

Three scoring functions are used for text field comparison:

| Function                    | How it works                                                                    | Used for                                                 |
| --------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------- |
| **Normalized Levenshtein**  | `1.0 - (edit_distance / max_length)`                                           | Single-word fields, exact text matching                  |
| **Tokenized Levenshtein**   | Tokenizes by whitespace, then averages per-token Levenshtein scores             | Multi-word text fields (names, descriptions)             |
| **Token Jaccard**           | `intersection / union` of word token sets                                      | Handle-ref list comparison, candidate ranking            |

All scores are in the range `[0.0, 1.0]`, where `1.0` means identical and
`0.0` means completely different.

---

## Tips and Common Workflows

### Finding what changed after editing in Gramps

```bash
# Save before and after, then diff
gramps-gen diff before.gramps after.gramps --output json --output-file changes.json
```

### Comparing two generated datasets

```bash
gramps-gen generate --count 100 --seed 42 --output run1.gramps
gramps-gen generate --count 100 --seed 99 --output run2.gramps
gramps-gen diff run1.gramps run2.gramps --output csv --output-file comparison.csv
```

### Getting a quick summary

```bash
gramps-gen diff large1.gramps large2.gramps --summary-only
```

### Exporting to a spreadsheet

```bash
gramps-gen diff file_a.gramps file_b.gramps --output csv --output-file diff.csv
# Then open diff.csv in Excel, LibreOffice Calc, or import into Google Sheets
```

### Inspecting specific field changes in JSON

```bash
gramps-gen diff before.gramps after.gramps --output json | jq '.items[] | select(.classification == "Modified") | {type: .item_type, fields: [.field_changes[] | {name: .field_name, old: .old_value, new: .new_value}]}'
```

### Processing diff CSV in a spreadsheet

The CSV output is designed for data analysis. Import it into a spreadsheet
to create pivot tables, count changes by field kind, or filter for specific
types of changes (e.g., all surname changes).

### Finding what was added or removed

```bash
# JSON is easiest to filter programmatically
gramps-gen diff a.gramps b.gramps --output json | jq '.items[] | select(.classification == "Added" or .classification == "Removed")'
```

---

## Troubleshooting

### "parse error: ..."

The tool could not read one or both files. Verify they exist, are valid
Gramps XML (`.gramps`) files, and are not corrupted. The tool handles
gzip-compressed `.gramps` files automatically.

### "empty graph: both files are empty"

Both files contain zero Gramps data items. Verify the files have content.

### "schema mismatch: ..."

The two files use Gramps schema versions that are incompatible. Ensure both
files were created with similar Gramps versions, or that the relevant schema
versions are compiled into the tool.

### No differences found but files are different

The diff uses normalized comparison (case-folded, whitespace-collapsed,
HTML-stripped text) which may hide purely cosmetic differences. If you
need to see every byte-level difference, be aware that `--no-normalize`
is reserved but not yet implemented — normalization is always applied
in the current version.

### Many items classified as "Needs Review"

The strong match key could not disambiguate between similar items. This is
common when a dataset has multiple people with the same name and birth date.
Possible workarounds:

- Use files where handles are preserved (e.g., edited within Gramps rather
  than regenerated from scratch)
- Write a custom script that uses the diff library to produce a partial
  handle map before running the full diff
- Use interactive resolution (requires the `resolve` feature) to manually
  resolve the ambiguous cases

### Extrinsic-only items cluttering the output

By default, extrinsic-only items are **hidden** from text output. Pass
`--include-extrinsic` to show them, or consult the JSON output which always
includes them.

### Understanding the confidence score

Confidence scores indicate how the match was made:

| Confidence | Meaning |
|---|---|
| `1.0` | Exact handle match, or ADDED/REMOVED singleton |
| `0.9` | Strong key match (fuzzy content-based) |
| `< 0.9` | Reserved for future scoring improvements |
