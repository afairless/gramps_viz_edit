# Delete Tool User Guide

The `gramps-gen delete` command safely removes selected people and all
orphaned dependencies from a Gramps XML file. It uses a fixed-point cascade
engine to determine which families, events, places, citations, sources,
repositories, media, notes, and tags become unreachable after deletion.

## Table of Contents

- [Quick Start](#quick-start)
- [How the Delete Pipeline Works](#how-the-delete-pipeline-works)
- [How the Cascade Works](#how-the-cascade-works)
  - [Phase A — Pre-Connectivity Recording](#phase-a--pre-connectivity-recording)
  - [Phase B — Fixed-Point Orphan Detection](#phase-b--fixed-point-orphan-detection)
  - [Phase C — Per-Type Orphan Rules](#phase-c--per-type-orphan-rules)
- [Dependency Chain](#dependency-chain)
- [Per-Type Orphan Rules](#per-type-orphan-rules)
- [Command Reference](#command-reference)
  - [Arguments](#arguments)
  - [Options](#options)
- [Interactive Review](#interactive-review)
- [Manifest Format](#manifest-format)
- [Save/Load Manifests](#saveload-manifests)
- [Dry Run Mode](#dry-run-mode)
- [Tips and Common Workflows](#tips-and-common-workflows)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

```bash
# Basic deletion with selections from the visualizer
gramps-gen delete data.gramps --selections picks.json

# Dry run — compute cascade without writing output
gramps-gen delete data.gramps --selections picks.json --dry-run

# Non-interactive deletion (skip review, auto-approve)
gramps-gen delete data.gramps --selections picks.json --yes

# Save a manifest for audit
gramps-gen delete data.gramps --selections picks.json \
  --yes --save-manifest delete-manifest.json

# Re-run from a saved manifest
gramps-gen delete data.gramps \
  --load-manifest delete-manifest.json --output cleaned.gramps

# Custom output path
gramps-gen delete data.gramps --selections picks.json \
  --output cleaned.gramps
```

---

## How the Delete Pipeline Works

```
.gramps file
    │
    ▼
┌────────────────────────────┐
│  1. Parse                  │  gramps-reader → typed_graph::Graph
└──────────┬─────────────────┘
           │
           ▼
┌────────────────────────────┐
│  2. Load selections        │  Parse visualizer JSON → set of seed handles
└──────────┬─────────────────┘
           │
           ▼
┌────────────────────────────┐
│  3. Cascade engine         │  Three-phase fixed-point cascade
│     (read-only on graph)   │  Phase A: Record pre-connectivity
│                            │  Phase B: Fixed-point orphan detection
│                            │  Phase C: Per-type orphan rules
└──────────┬─────────────────┘
           │  DeletePlan
           ▼
┌────────────────────────────┐
│  4. Interactive review     │  Terminal TUI (skip with --yes)
│     (optional)             │  Review per-type, y/n/l/r/s/q
└──────────┬─────────────────┘
           │  Reviewed set → JSON manifest
           ▼
┌────────────────────────────┐
│  5. Python backend         │  scripts/delete_backend.py (subprocess)
│     (XML I/O)              │  Temp Gramps DB → import → delete → export
│                            │  Uses Gramps' own import/export libraries
└────────────────────────────┘
```

> **Note:** Step 5 delegates all XML I/O to `scripts/delete_backend.py`, a
> Python subprocess that uses Gramps' own `gramps.plugins.importer.importxml`
> and `gramps.plugins.export.exportxml` libraries. This eliminates the entire
> class of XML round-trip bugs that arose from the previous Rust-based
> `GraphXmlWriter` filter path. The Rust cascade engine (steps 1–4) remains
> unchanged.

---

## How the Cascade Works

The cascade engine operates on a **read-only** Graph — it never
mutates the in-memory graph. Instead, it computes a `DeletePlan` that
records which handles should be removed, then the output stage filters
them during serialization.

### Phase A — Pre-Connectivity Recording

Records all edges (forward and reverse) for every node in the graph.
This snapshot is used in later phases to determine which nodes lose
all incoming connections.

### Phase B — Fixed-Point Orphan Detection

After removing seed people (Phase C applies the seed removal as input),
the engine iteratively detects orphaned nodes:

1. Find all nodes with **zero remaining incoming edges** from surviving
   nodes
2. Mark them as orphaned → remove their outgoing edges
3. Repeat until no new orphans are detected

This is a **fixed-point** algorithm — it continues iterating until the
set of orphaned nodes stabilizes.

### Phase C — Per-Type Orphan Rules

After the fixed-point detection, per-type rules are applied following
the **dependency chain** (see below). Each type has a specific condition
that determines whether it should be deleted.

---

## Dependency Chain

The cascade follows a strict ordering. When a person is deleted, the
following types are checked in order:

```
People → Families → Events → Places → Citations → Sources → Repositories → Media → Notes → Tags
```

Each type depends on the survival of the types to its left. For example,
an Event is only deleted if no surviving Person or Family references it.
A Source is only deleted if no surviving Citation references it.

---

## Per-Type Orphan Rules

| Type | Orphaned when… |
|---|---|
| **Person** | Explicitly selected as a seed for deletion |
| **Family** | No remaining father/mother connections to surviving people, **and** no remaining children (via `ChildRef` edges) |
| **Event** | No remaining `PersonEventRef` or `FamilyEventRef` edges from surviving nodes |
| **Place** | No remaining `EventPlace`, `PlacePlaceRef`, `PlaceCitation`, `PlaceMediaRef`, `PlaceNote`, or `PlaceTag` edges from surviving nodes |
| **Citation** | No remaining `CitationRef` edges from surviving nodes |
| **Source** | No remaining `source_handle` references from surviving citations, and no remaining `RepoRef` edges |
| **Repository** | No remaining `repo_handle` references from surviving sources |
| **Media** | No remaining `MediaRef` edges from surviving nodes |
| **Note** | No remaining `NoteRef` edges from surviving nodes |
| **Tag** | No remaining `TagRef` edges from surviving nodes |

All rules are evaluated **after** seed people and previously detected
orphans have been conceptually removed (their edges are excluded from
connectivity checks).

---

## Command Reference

```text
gramps-gen delete [OPTIONS] <FILE> --selections <JSON>
```

### Arguments

| Argument | Description |
|---|---|
| `FILE` | Path to the input `.gramps` file |

### Options

| Option | Default | Description |
|---|---|---|
| `--selections <PATH>` | (required) | Path to visualizer selections JSON file containing seed people |
| `--output <PATH>` | stdout / auto-generated | Write cleaned output to this file |
| `--yes` | `false` | Skip interactive review and auto-approve all deletions |
| `--dry-run` | `false` | Compute cascade and print plan, but do not write output |
| `--save-manifest <PATH>` | (none) | Save the deletion plan as an auditable JSON manifest |
| `--load-manifest <PATH>` | (none) | Load a previously saved manifest instead of computing cascade |

---

## Interactive Review

When `--yes` is not specified, the tool presents an interactive terminal
UI for reviewing deletion candidates **type by type**:

```
Type: Person (3 to delete, 47 kept)
  abc123  John Smith     (seed)
  def456  Jane Doe       (seed)
  ghi789  Robert Jones   (seed)

[y]es  [n]o  [l]ist  [r]eview  [s]kip type  [q]uit
```

| Action | Key | Description |
|---|---|---|
| **Yes** | `y` | Approve deletion of this type's candidates |
| **No** | `n` | Keep this type's candidates (remove from plan) |
| **List** | `l` | Show the full list of candidates with handles |
| **Review** | `r` | Show detailed info for each candidate |
| **Skip type** | `s` | Skip to the next type (current type remains in plan) |
| **Quit** | `q` | Abort the review (remaining types stay in plan, already-reviewed decisions are kept) |

The review progresses through types in dependency order: People → Families
→ Events → Places → Citations → Sources → Repositories → Media → Notes
→ Tags. Types with zero candidates are skipped automatically.

---

## Manifest Format

Deletion manifests are **version 1** JSON files with the following
structure:

```json
{
  "version": 1,
  "source_file": "data.gramps",
  "selections_file": "picks.json",
  "created_at": "2025-08-08T14:30:00Z",
  "seed_people": ["abc123", "def456", "ghi789"],
  "plan": {
    "people": {
      "to_delete": ["abc123", "def456", "ghi789"],
      "kept": ["jkl012", "mno345"]
    },
    "families": {
      "to_delete": ["fam001"],
      "kept": ["fam002"]
    },
    "events": {
      "to_delete": ["evt001", "evt002", "evt003"],
      "kept": []
    }
  }
}
```

| Field | Description |
|---|---|
| `version` | Manifest schema version (always `1`) |
| `source_file` | Path to the input `.gramps` file |
| `selections_file` | Path to the selections JSON used as input |
| `created_at` | ISO 8601 timestamp of manifest creation |
| `seed_people` | Array of person handles explicitly selected for deletion |
| `plan` | Per-type object with `to_delete` (handles to remove) and `kept` (handles to keep) |

The manifest serves as an **audit trail** — it records exactly what was
deleted and when. Empty `to_delete` arrays for a type mean no nodes of
that type were orphaned.

> **Note:** The manifest JSON keys use **snake_case** plural type names
> (`people`, `families`, `events`, etc.) matching the Rust
> `NodeKindLabel::plural()` output. This is the format consumed by the
> Python backend script.

---

## Save/Load Manifests

### Saving

`--save-manifest` writes the computed `DeletePlan` to a JSON file **before**
any interactive review. The manifest includes:

- All seed people
- All cascade-detected orphans across all 10 types
- Metadata (version, source file, timestamp)

The saved manifest reflects the **computed plan**, not the post-review plan.
This means you can save a manifest, review interactively, and the saved
manifest still contains the full cascade for audit purposes.

### Loading

`--load-manifest` bypasses the cascade engine entirely. The tool reads
the manifest, validates it against the source file, and uses the recorded
`to_delete` arrays directly.

Validation checks:

- Manifest version is supported (1)
- Source file exists and is a valid `.gramps` file
- All handles in `to_delete` exist in the source file

A **warning** is printed if the manifest's `source_file` doesn't match the
current input file path — the manifest still loads, but the user is alerted
to the mismatch.

### Audit trail workflow

```bash
# Step 1: Compute and save
gramps-gen delete data.gramps --selections picks.json \
  --dry-run --save-manifest plan.json

# Step 2: Review the manifest
cat plan.json | jq '.plan.people.to_delete | length'

# Step 3: Execute from manifest
gramps-gen delete data.gramps \
  --load-manifest plan.json --yes --output cleaned.gramps

# Step 4: Archive manifest with the cleaned file
```

---

## Dry Run Mode

`--dry-run` computes the full cascade but **does not write output**.
Use it to preview what would be deleted:

```bash
gramps-gen delete data.gramps --selections picks.json --dry-run
```

The dry run prints:

- Number of seed people
- Per-type counts of nodes that would be deleted
- Total cascade size (seed + orphans)

Combine with `--save-manifest` to inspect the full plan:

```bash
gramps-gen delete data.gramps --selections picks.json \
  --dry-run --save-manifest plan.json

# Inspect the plan
jq '.plan | to_entries[] | "\(.key): \(.value.to_delete | length) to delete"' plan.json
```

---

## Tips and Common Workflows

### Cleaning up after visualizer pruning

```bash
# 1. Open file in visualizer
gramps-gen visualize data.gramps

# 2. Select people to remove → Export → picks.json

# 3. Dry run to preview
gramps-gen delete data.gramps --selections picks.json --dry-run

# 4. Execute with review
gramps-gen delete data.gramps --selections picks.json --output cleaned.gramps
```

### Reviewing before deletion

```bash
# Save manifest first for audit
gramps-gen delete data.gramps --selections picks.json \
  --save-manifest audit.json

# Interactive review — approve/deny per type
gramps-gen delete data.gramps --selections picks.json --output cleaned.gramps
```

### Saving manifests for audit

```bash
# Always save a manifest alongside the cleaned file
gramps-gen delete data.gramps --selections picks.json \
  --yes --save-manifest "$(date +%Y%m%d)-deletion-manifest.json" \
  --output "cleaned-$(date +%Y%m%d).gramps"
```

### Re-running from a saved manifest

```bash
# If you need to re-apply the same deletion to a different file
# (warning: handles may differ)
gramps-gen delete another.gramps --load-manifest manifest.json --dry-run

# If handles match, execute
gramps-gen delete another.gramps --load-manifest manifest.json --yes
```

---

## Troubleshooting

### 0% handle match between selections and graph

The handles in the selections JSON don't exist in the `.gramps` file.
Common causes:

- The selections JSON was exported from a different `.gramps` file
- The `.gramps` file was regenerated (handles changed)
- The file path is incorrect

Verify with:

```bash
# Check how many selection handles exist in the graph
gramps-gen stats data.gramps --json | jq '.persons | length'
# Compare with selection count
jq '.selections | length' picks.json
```

### Manifest version error

The manifest has an unsupported `version` field. Only version `1` is
supported. Check the manifest:

```bash
jq '.version' manifest.json
```

### Source file mismatch warning

The manifest's `source_file` doesn't match the current input file. This
is a **warning**, not an error — the manifest still loads. But be aware
that the handles might not match the current file's content.

### "no seed people provided"

The selections JSON has an empty `selections` array. Select some people
in the visualizer and export again.

### "json parse error: selections"

The selections file is not valid JSON or doesn't match the expected
format. Export again from the visualizer — do not hand-edit the JSON.

### Cascade deleted more than expected

The cascade is designed to be **conservative** — it only deletes nodes
that are truly orphaned. If more nodes were deleted than expected:

- Check which types had unexpected orphans (use `--dry-run` to preview)
- Review the per-type orphan rules in [Per-Type Orphan Rules](#per-type-orphan-rules)
- Consider using a different set of seed people

### Cascade deleted fewer than expected

Some nodes you expected to be deleted were kept. Causes:

- The node is still referenced by a surviving node (e.g., an Event
  referenced by a surviving Family)
- The node is referenced indirectly through a chain that was not fully
  orphaned

Use `--dry-run --save-manifest` and inspect the manifest to understand
which nodes were kept and why.
