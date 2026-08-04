# Architecture & CLI Review — gramps-gen

> Review date: 2025-07-16
> Scope: Workspace architecture, crate boundaries, CLI surface, and readiness for the upcoming import component.

---

## Strengths: Architecture

### 1. Crate boundary discipline

The workspace has five crates with crisp, non-leaky boundaries:

| Crate | Responsibility | Depends on |
|---|---|---|
| `typed-graph` | Core model, codegen, validation, generation | Nothing (leaf) |
| `output` | XML serialization | `typed-graph` |
| `gramps-reader` | Streaming XML parsing, DSU, generation layering | Nothing (leaf) |
| `cli` | CLI orchestration, scenario parsing | All above |
| `visualize` | Tauri app, D3 rendering | `gramps-reader` |

This is textbook modular architecture. No crate depends on a binary. The `gramps-reader` crate is shared by both `cli` and `visualize` without either knowing about the other. The `visualize` feature gate isolates system dependencies (WebKit2GTK/WebView2) from the core CLI. This pattern makes adding a third component straightforward — it would depend on `gramps-reader` and possibly `typed-graph`, without touching `cli` or `visualize`.

### 2. Schema-driven codegen

The `build.rs` → `generated_schema.rs` pipeline is the strongest technical asset in the project. Schemas are the source of truth; Rust types (Node, Edge, XxxData, enums, Schema metadata) are derived facts. The multi-version support via Cargo features + union merge algorithm is genuinely impressive — it handles per-version optionality, conflict detection (type mismatch across versions), and enum integer-to-name mapping. The JSON Schema → flat format converter for 5.1 is a pragmatic compatibility shim that doesn't contaminate the on-disk schema file.

### 3. Graph model invariants

Concrete (non-generic) typed graph with forward/reverse indexes, validation-state tracking on mutation, and edge validation on add. The separation of `GraphBuilder` (fluent API, validates at build time) from `Graph` (storage, queries) is clean and well-executed.

### 4. Five-stage pipeline with validation gates

Generation → Validate → Adversarial → Validate → Serialize. Every data-altering stage is bracketed by validation. The adversarial Category A/B distinction (during-generation vs. post-generation) is well thought out and avoids tainting the generator with post-transform concerns.

### 5. Test quality

Tests live co-located with source, cover normal/edge/error/shape cases, and use `tempfile` for filesystem-based tests. E2E tests run the compiled binary as a subprocess. The `gramps-reader` functions are pure over `&str`, so they test without filesystem access. Overall coverage is good.

---

## Strengths: CLI

### 1. Rich generation config surface

The `generate` command exposes person count, depth, family ratio, seed, adversarial strategies, feature toggles (places/citations/notes/media/tags), schema version, densification controls (connectivity target, max parent roles), and a YAML scenario path. Good coverage for a domain-specific power-user tool.

### 2. Stats command is genuinely useful

The `stats` command produces a rich report: per-type counts, family-size histogram, family-group-size distribution, a two-dimensional contingency table (group size × generation span), orphan/dangling counts, and cycle-detection warnings. Both human-readable (with Unicode box-drawing) and machine-readable (`--json`) output. This is diagnostic gold for understanding generated data.

### 3. Schema management is well-handled

`schema list` / `schema download` handle the external dependency on Python gracefully — warnings about code execution, fallback to static tag map when GitHub is unreachable, caching with 1-hour TTL. The download flow (clone → run extractor → validate output → clean up) is careful and defensive.

### 4. YAML scenario support

The `-c` flag for reproducible configurations is essential. The `Scenario` type handles partial overrides with `Option` fields and defaults from `RandomConfig::default()`, so scenario files are concise (not required to specify every field).

### 5. Selection export format

The visualizer's `export_selections` writes plain JSON (`Vec<SelectedPerson>`). This is a clean, implementation-agnostic contract that the upcoming import component can consume without depending on any Rust crate.

---

## Weaknesses: Architecture

### 1. No full roundtrip: `.gramps` → `Graph` is impossible

This is the single biggest architectural gap. The tool can generate a `Graph` → serialize → `.gramps`. But it cannot read `.gramps` → `Graph` for modification, re-validation, or re-serialization. The `gramps-reader` crate is streaming-only: it produces `ParsedPerson`/`ParsedFamily`/`StatsReport`, not a `Graph` or `Node`/`Edge` types.

**Consequences:**

- The `validate` command cannot run structural+referential validation (the same validation that `generate` applies internally). It only checks XML well-formedness.
- There is no way to read a `.gramps` file, apply adversarial transforms or densification, and write it back.
- The upcoming import component cannot rely on `typed_graph::Graph` as an interchange format.

**Severity: Medium-High.** The gap is acknowledged in the code ("Full `.gramps` file parsing…is a significant effort beyond Phase 7's scope"), but it constrains what the architecture can express. The import component will need at least partial graph reconstruction, and doing it once in `gramps-reader` (or a new bridge crate) benefits both `validate` and the import pipeline.

### 2. Adversarial strategy name parsing is duplicated verbatim

`strategy_from_name` exists identically in:

- `crates/cli/src/commands/generate.rs`
- `crates/cli/src/scenario.rs`

Adding a new strategy requires updating both locations or they diverge. The function should live once — either in `scenario.rs` with a re-export, or better, in `typed-graph` next to the `AdversarialStrategy` enum.

**Severity: Low.** Easy fix, no behavior change, but a clear DRY violation.

### 3. `validate` command is misleadingly named

It only checks: well-formed XML, `<database>` root element, namespace, `<header>` section. It does **not** check: required fields, cardinality constraints, dangling references, or plausibility — all of which `typed-graph::validate` checks on an in-memory `Graph`. A user running `gramps-gen validate real_family.gramps` would reasonably expect the same validation that `gramps-gen generate` applies to its own output.

**Severity: Medium.** The command name promises more than it delivers.

### 4. `extract-schema` command is a dead stub

It accepts a path, prints it, and succeeds. It serves no purpose. This is clutter in `--help` output.

**Severity: Low.** Either implement it or remove it from the CLI surface.

### 5. Stats and visualize parse the same file twice

When `gramps-gen visualize` → `get_stats` is called from the frontend, the file is re-read and re-parsed via `count_gramps_xml`. The `load_graph_data` call already parsed much of the same data via `extract_persons`/`extract_families`/`extract_events`. The code acknowledges OS caching mitigates this, but it's still logically redundant.

**Severity: Low.** Acceptable for now. If the import component also re-reads the file, consider caching the parsed result or combining the passes.

---

## Weaknesses: CLI

### 1. The `generate` command has too many flags

Total flags: 19 (counting short and long forms). A user who just wants "200 people, 3 generations" is exposed to:

```
-n, -d, -o, --seed, --strict, --adversarial, --progress-interval,
-c, --with-places, --with-citations, --with-notes, --with-media, --with-tags,
--schema-version, --family-ratio, --max-parent-roles, --no-densify,
--connectivity-target, --densify-max-parent-roles
```

The feature toggles (`--with-places`, `--with-citations`, etc.) are particularly good candidates for collapsing into a single `--features` flag with comma-separated values (e.g., `--features places,citations,notes`).

**Severity: Low-Medium.** It's a power-user tool, so some flag volume is expected. But discoverability suffers and `--help` output is intimidatingly long.

### 2. Densification is opt-out, not opt-in

`--no-densify` disables it; the default is to densify with `connectivity_target=0.85`. Densification fundamentally rewires the graph (cross-component marriage, orphan adoption, remarriage). A new user who types `gramps-gen generate -n 200 -d 3` gets densified output without knowing what that means. For a tool that generates "valid, plausible family trees," silently altering topology is surprising.

**Severity: Medium.** Consider defaulting to densification off and making it opt-in via `--densify`.

### 3. No `--help` examples

Clap supports `#[command(after_help = "...")]` and `#[arg(verbatim_doc_comment)]` for examples. None of the subcommands provide usage examples. For a domain-specific tool, example invocations dramatically lower the barrier to entry.

**Severity: Low.** Nice-to-have, easy to add.

### 4. No compound/cross-cutting commands

The most natural workflow is: `generate → visualize → export selections → import-and-process`. The user currently runs multi-step sequences manually. There is no `generate --open` (generate and immediately visualize), no `visualize --stats` (show stats in the visualizer on load), no `import <selections.json> <source.gramps>`.

**Severity: Low (for now).** The import component doesn't exist yet, so workflow composition isn't urgent. But designing the import interface now — what does it consume? what does it produce? — will shape whether composition makes sense.

---

## Impact on the Upcoming Import Component

The third component — importing node selections and processing them further — needs clear answers to these questions:

| Question | Current state | Recommendation |
|---|---|---|
| **What format are selections in?** | JSON (`SelectedPerson[]`) — well-defined with `Serialize`/`Deserialize` | Keep this. It is clean and versionable. |
| **Does the import need the source `.gramps`?** | Unclear | Almost certainly yes — selections are handles + names, not the full graph context. The import component should read both the selection JSON and the source `.gramps` to reconstruct context. |
| **Does the import produce another `.gramps`?** | Unknown | If yes, full roundtrip (`.gramps` → `Graph` → `.gramps`) becomes essential. If it produces a different format (e.g., a report, a filtered subset, statistics), then `gramps-reader`'s streaming API may suffice. |
| **Should import be a separate binary or a subcommand?** | Open | A subcommand of `gramps-gen` is natural: `gramps-gen import <selections.json> <source.gramps>`. This keeps the workflow under one CLI surface. |

---

## Summary Table

| Aspect | Rating | Action |
|---|---|---|
| Crate separation | ★★★ | No change needed |
| Schema-driven codegen | ★★★ | No change needed |
| Graph model | ★★☆ | Solid; needs a deserialization path |
| Five-stage pipeline | ★★☆ | Well-structured; densification opt-out is surprising |
| Test coverage | ★★☆ | Good; no gaps flagged |
| CLI flag volume | ★★☆ | Collapse feature toggles; add examples |
| Validate command | ★☆☆ | Misleading scope — needs full graph validation or renaming |
| DRY violations | ★★☆ | Duplicate `strategy_from_name` — easy fix |
| Roundtrip capability | ★☆☆ | Blocking gap for import component and `validate` |
| Dead code | ★★☆ | `extract-schema` stub — remove or implement |
| Selection export contract | ★★★ | Clean JSON — ready for import component |

---

## No-Change Items (things that work well as-is)

- The two-binary approach for visualization (CLI spawns Tauri binary)
- The feature gate that keeps WebKit2GTK/WebView2 out of the core CLI build
- The `gramps-reader` shared library pattern
- The `stats --json` output format for machine readability
- The schema download caching (1-hour TTL)
- The GraphBuilder/Graph separation
- The error type hierarchy with `From` impls from all downstream errors

---

## Options to Consider (no decisions required now)

1. **Add `.gramps` → `Graph` deserialization to `gramps-reader`** — unblocks full `validate`, enables a `transform` subcommand, and gives the import component a rich data model. This is the single biggest bang-for-buck architectural improvement.

2. **Collapse feature toggles into `--features`** — reduces 5 flags to 1. Example: `--features places,citations,notes`.

3. **Make densification opt-in** — change `--no-densify` to `--densify` with a default of off. Less surprising for new users.

4. **Add `--open` flag to `generate`** — auto-launches the visualizer after generation. Simple composition of two existing capabilities.

5. **Remove the `extract-schema` stub from the CLI** — or implement it. A dead command erodes trust in `--help`.

6. **Design the import subcommand interface now** — even before implementation, defining the CLI contract (`gramps-gen import <selections.json> <source.gramps> [--output <path>]`) will inform whether the architecture needs upstream changes.
