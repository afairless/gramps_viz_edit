# Integrate Tool User Guide

The `gramps-gen integrate` command merges the output of two other tools —
a diff CSV report and a visualizer selections JSON file — into a single,
combined report. This lets you cross-reference *what changed* (from the diff)
with *who was selected* (from the visualizer).

## Table of Contents

- [Quick Start](#quick-start)
- [How the Integration Works](#how-the-integration-works)
- [Understanding the Output](#understanding-the-output)
  - [Row Kinds](#row-kinds)
  - [CSV Output Schema](#csv-output-schema)
  - [JSON Output Schema](#json-output-schema)
- [Command Reference](#command-reference)
  - [Arguments](#arguments)
  - [Options](#options)
- [How Matching Works](#how-matching-works)
- [Tips and Common Workflows](#tips-and-common-workflows)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

```bash
# Basic integration — merge diff CSV with visualizer selections
gramps-gen integrate diff-viz \
  --diff diff.csv \
  --selections selections.json

# CSV output (default) with explicit output file
gramps-gen integrate diff-viz \
  --diff diff.csv \
  --selections selections.json \
  --output-file merged.csv

# JSON output
gramps-gen integrate diff-viz \
  --diff diff.csv \
  --selections selections.json \
  --format json \
  --output-file merged.json
```

At minimum you provide a diff CSV (from `gramps-gen diff --output csv`)
and a selections JSON (from the visualizer's Export button).

---

## How the Integration Works

The integration pipeline runs in three stages:

```
┌──────────────┐    ┌─────────────────┐    ┌──────────────────┐
│  Parse diff  │ →  │  Parse viz JSON │ →  │  Full outer join │ →  Format output
│  CSV         │    │  (selections)   │    │  by handle       │
└──────────────┘    └─────────────────┘    └──────────────────┘
```

1. **CSV reader** (`csv_reader.rs`) — Parses the diff CSV into `DiffRow`
   structs, extracting handle, classification, field changes, and metadata
   for each person

2. **JSON reader** (`json_reader.rs`) — Parses the visualizer selections
   JSON into `Selection` structs, extracting handle, name, birth/death
   dates, gender, and family group for each selected person

3. **Merge engine** (`merge.rs`) — Performs a **full outer join** on
   person handles between the two datasets, producing three row kinds

---

## Understanding the Output

### Row Kinds

Each row in the merged output has a `row_kind` column (CSV) or `row_kind`
field (JSON) indicating the match status:

| Row kind | Meaning | Diff data present? | Viz data present? |
|---|---|---|---|
| `Matched` | Person appears in both diff and selections | Yes | Yes |
| `DiffOnly` | Person appears only in the diff CSV | Yes | No (empty viz fields) |
| `VizOnly` | Person appears only in the selections JSON | No (empty diff fields) | Yes |

### CSV Output Schema

The CSV output includes all diff columns **plus** selection metadata:

| Column | Source | Description |
|---|---|---|
| `classification` | Diff | `SAME`, `MODIFIED`, `ADDED`, `REMOVED`, `NEEDS REVIEW`, `EXTRINSIC ONLY` |
| `item_type` | Diff | Type of Gramps item (Person, Family, Event, …) |
| `handle_a` | Diff | Handle in file A (baseline) |
| `gramps_id_a` | Diff | Gramps ID in file A |
| `display_name_a` | Diff | Display name in file A |
| `handle_b` | Diff | Handle in file B (target) |
| `gramps_id_b` | Diff | Gramps ID in file B |
| `display_name_b` | Diff | Display name in file B |
| `confidence` | Diff | Match confidence (0.0–1.0) |
| `field_name` | Diff | Name of changed field (empty for non-MODIFIED) |
| `field_kind` | Diff | Kind of field that changed |
| `old_value` | Diff | Value in file A |
| `new_value` | Diff | Value in file B |
| `similarity` | Diff | Similarity score for this field change |
| `side` | Merge | Which dataset(s) the person came from: `both`, `diff_only`, `viz_only` |
| `row_kind` | Merge | One of `Matched`, `DiffOnly`, `VizOnly` |
| `viz_name` | Viz | Person's display name from selections |
| `viz_birth_date` | Viz | Birth date from selections |
| `viz_death_date` | Viz | Death date from selections |
| `viz_gender` | Viz | Gender from selections |
| `viz_family_group` | Viz | Family group index from selections |

### JSON Output Schema

The JSON output is a structured object:

```json
{
  "diff_file": "diff.csv",
  "selection_file": "selections.json",
  "row_count": 150,
  "matched_count": 80,
  "diff_only_count": 40,
  "viz_only_count": 30,
  "rows": [
    {
      "row_kind": "Matched",
      "diff": {
        "classification": "Modified",
        "handle_a": "abc123",
        "display_name_a": "John Smith",
        "field_changes": [
          { "field_name": "surname", "old_value": "Smith", "new_value": "Smyth" }
        ]
      },
      "viz": {
        "name": "John Smith",
        "birth_date": "1850-03-15",
        "gender": "Male",
        "family_group": 2
      }
    }
  ]
}
```

---

## Command Reference

```text
gramps-gen integrate diff-viz [OPTIONS] --diff <CSV> --selections <JSON>
```

### Arguments

| Argument | Description |
|---|---|
| `diff-viz` | Subcommand: merge diff CSV with visualizer selections |

### Options

| Option | Default | Description |
|---|---|---|
| `--diff <PATH>` | (required) | Path to diff CSV file (from `gramps-gen diff --output csv`) |
| `--selections <PATH>` | (required) | Path to visualizer selections JSON file |
| `--format <FORMAT>` | `csv` | Output format: `csv` or `json` |
| `--output-file <PATH>` | stdout | Write output to a file instead of printing |

---

## How Matching Works

The merge engine uses **handle-based matching only** — no fuzzy matching,
no name comparison, no similarity scoring. The algorithm:

1. Build a set of handles from the diff CSV (using `handle_a` first, then
   `handle_b`)
2. Build a set of handles from the selections JSON
3. **Matched**: handles present in both sets — row includes all diff and
   viz data
4. **DiffOnly**: handles present only in the diff CSV — viz fields are
   empty/null
5. **VizOnly**: handles present only in the selections JSON — diff fields
   are empty/null

This means matching is **deterministic and fast**. If a person's handle
differs between the two files (e.g., the diff uses `handle_b` from
regeneration while the selections use `handle_a` from the original),
they will not match. In practice, if both files were produced from the
same `.gramps` source, handles should match.

No cross-mapping or handle translation is performed — the merge relies
on handles being preserved across the diff and visualization workflows.

---

## Tips and Common Workflows

### Finding the overlap between changes and selections

```bash
# 1. Diff the two files
gramps-gen diff before.gramps after.gramps --output csv --output-file changes.csv

# 2. In the visualizer, select people of interest → Export → selections.json

# 3. Integrate — Matched rows are people who both changed AND were selected
gramps-gen integrate diff-viz --diff changes.csv --selections selections.json
```

### Cross-referencing who changed vs. who was selected

```bash
gramps-gen integrate diff-viz \
  --diff changes.csv \
  --selections selections.json \
  --output-file cross-ref.csv

# Open cross-ref.csv in a spreadsheet
# Filter by row_kind to see:
#   Matched = these people changed AND you selected them
#   DiffOnly = these people changed but you didn't select them
#   VizOnly = you selected these people but they didn't change
```

### Spreadsheet analysis

```bash
# CSV output is designed for spreadsheet import
gramps-gen integrate diff-viz \
  --diff changes.csv \
  --selections selections.json \
  --output-file analysis.csv

# Import into Excel / LibreOffice Calc / Google Sheets
# Create pivot tables: row_kind vs classification, count by family_group, etc.
```

### Programmatic processing with JSON

```bash
# JSON output for scripted analysis
gramps-gen integrate diff-viz \
  --diff changes.csv \
  --selections selections.json \
  --format json \
  --output-file merged.json

# Query with jq
jq '.rows[] | select(.row_kind == "Matched") | {name: .viz.name, classification: .diff.classification}' merged.json
```

---

## Troubleshooting

### "no data to integrate"

One or both input files produced zero rows after parsing. Verify:

- The diff CSV has rows (check with `wc -l changes.csv`)
- The selections JSON has a non-empty `selections` array
- Both files are in the expected format (not empty, not truncated)

### 0% handle match (all DiffOnly and VizOnly, no Matched)

The handles in the diff CSV don't match the handles in the selections
JSON. Common causes:

- The diff was run on a regenerated `.gramps` file (handles changed)
- The selections JSON came from a different `.gramps` file than the diff
- The diff CSV uses `handle_b` values that differ from the selections

Solutions:

- Run the diff and visualizer on the **same** `.gramps` file
- If handles differ intentionally, use `--output json` and write a custom
  script to cross-reference by display name instead

### Empty selections

The visualizer selections JSON has an empty `selections` array. Select
some nodes in the visualizer and export again.

### "csv parse error"

The diff CSV format doesn't match what the reader expects. Ensure the CSV
was produced by `gramps-gen diff --output csv` (not hand-edited or from
a different tool).

### "json parse error: selections"

The selections file is not valid JSON or doesn't match the expected
schema. Export again from the visualizer — do not hand-edit the JSON
unless you maintain the exact schema.
