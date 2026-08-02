# Design Plan: Family-Size × Generations Contingency Table

## Problem Statement

The `gramps-gen stats` command currently reports how many nuclear families
have each size (number of people). For example:

```
Family size distribution
  size  1: 13 families (13 people)
  size  2: 24 families (48 people)
  size  3: 2 families (6 people)
  size  4: 7 families (28 people)
  size  5: 3 families (15 people)
```

This tells us how many people are in each family, but says nothing about
**generational depth** — how many genealogical generations each family
group spans. Two families of the same size can have very different
structures: one might be a single couple with no children (1 generation),
while another might be a three-generation chain (grandparent → parent →
child) spanning the same number of people.

We want to augment the size distribution with a second dimension:
**how many genealogical generations does each family's extended group
span?** The output is a contingency table with family size along the rows
and generation count along the columns, including marginal sums.

### Desired Output

Placement: the table appears as a **new section** beneath the existing
"Family size distribution" section. The existing lines are preserved.

```
Family size × generation table
  # generations │  1   2   3  total
  ──────────────┼──────────────────
  # people  1   │  5   0   0      5
             2  │  4   2   0      6
             3  │  1   8   2     11
  ──────────────┼──────────────────
  total         │ 10  10   2     22
```

The table is present in both text and JSON output.

---

## Design Decisions

### Decision 1: Units of the table are nuclear families

**Context**: The existing "Family size distribution" counts `<family>`
elements (nuclear families). The new table should augment this, not
redefine it.

**Decision**: Each row is a nuclear-family size. The generation count
for a nuclear family is the **genealogical depth of the connected
extended-family group (connected component) that the nuclear family
belongs to**, not the span of the nuclear family's own members. This
is the "extended lineage depth" interpretation — confirmed by user
choice.

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Table unit | Nuclear family (same as existing size distribution) | Keeps the table consistent with the existing output; existing marginal totals sum to the number of nuclear families |
| Generation source | Generations of the connected component (extended family group) | A nuclear family spans at most 2 generations (parents + children); the interesting statistic is how deep the lineage goes |
| How generations are measured | Longest-path layering of the parent→child DAG within each component | Genealogically meaningful; handles real data with remarriage, step-families, and pedigree collapse |

### Decision 2: Build the person graph after the streaming pass

**Context**: The current `count_gramps_xml` is a single streaming pass
over the XML. Computing connected components and generation layering
requires cross-family data (a person's role in multiple families).

**Decision**: Keep the streaming pass for element counting, but also
record per-family adjacency data during the stream. After the stream
finishes, build the person graph, find connected components, compute
generation layers, and tabulate the contingency table. This is a
**two-phase** approach:

- **Phase 1 (streaming)**: Count element types; record family member
  sets with role separation (parents vs. children); record which
  persons are parents in which families.
- **Phase 2 (post-stream)**: Build DSU (disjoint set union) over
  people connected through shared families; compute genealogical
  layers via longest-path layering; build the contingency table.

| Alternative | Rejected because |
|---|---|
| Two-pass XML | Requires re-reading the file; unnecessary when the first pass can collect enough data |
| Full graph construction | Building the full `Graph` (all nodes, edges, events) is heavy and not needed for stats |
| Streaming-only generation counting | Cannot compute cross-family generation depth; limits table to 1–2 columns |

### Decision 3: Generation layering uses longest-path DAG layering

**Context**: Within a connected component, people form a DAG via
parent→child edges. A person's generation is the longest path length
from any root (person with no known parents).

**Decision**: For each connected component, assign generation numbers
via iterative relaxation:

1. Find all people in the component with no known parents (roots) → gen 0.
2. For each person, `gen = 1 + max(gen of all parents)`.
3. After all people are assigned, the component's generation span =
   `max(gen) - min(gen) + 1` = `max(gen) + 1` (since min is always 0
   in a connected component following only parent-child edges).

**Edge cases**:

| Case | Handling |
|---|---|
| Person with no parents and no children (isolated) | gen 0; component span = 1 |
| Person with no parents but has children (root of a lineage) | gen 0; children gen 1, etc. |
| Pedigree collapse (cousins marry — person's parents share an ancestor) | Still a DAG; longest-path works correctly |
| Cycle (person is their own ancestor — data error) | Detect cycles via visited-set; cap layering at a configurable maximum (default 50); emit a warning |
| Person appears as child in one family and parent in another (normal) | Correctly assigned gen ≥ 1 via parent link |

### Decision 4: Table format uses Unicode box-drawing or ASCII

**Context**: The table should be "neater and prettier" than the plain
whitespace example.

**Decision**: Use Unicode box-drawing characters (`│`, `─`, `┼`) for
the column separators and header rules. Fall back to pure ASCII when
the output is not a terminal (pipe/redirect) or when a `--no-unicode`
flag is set. The `--json` flag produces structured JSON instead.

### Decision 5: Table is additive — existing output is preserved

**Context**: The user said "Let's include a count" — implying addition,
not replacement.

**Decision**: The existing "Family size distribution" section with its
per-size lines remains unchanged. The new table is a separate section
printed below it. This is backward-compatible for both text and JSON
output (new JSON fields are added, not changed).

---

## Specification

### 1. Data structures

```rust
/// New field on StatsReport.
pub struct StatsReport {
    // ... existing fields ...

    /// Family-size × generation contingency table.
    pub family_generation_table: FamilyGenerationTable,
}

/// Contingency table: family size (rows) × generations (columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyGenerationTable {
    /// Matrix: row_sizes[i] = family size for row i,
    ///           col_spans[j] = generation span for column j,
    ///           cells[i][j] = count of families.
    pub row_sizes: Vec<usize>,
    pub col_spans: Vec<usize>,
    pub cells: Vec<Vec<usize>>,  // cells[row][col]
    pub row_totals: Vec<usize>,  // marginal sums
    pub col_totals: Vec<usize>,  // marginal sums
    pub grand_total: usize,      // same as sum(family_size_distribution values)
}

// Alternative simpler representation: nested map
// "family_generation_table": { "1": { "1": 5, "2": 0, "3": 0, "total": 5 }, ... }
// This is more JSON-friendly and avoids the Vec-of-Vecs problem.
```

After considering JSON ergonomics, use a **nested map** representation
for the serialized form, with a helper that builds the text table:

```rust
/// Cross-classification of nuclear families by size and generation span.
///
/// In JSON: { "size": { "span": count, "total": row_total }, ... }
/// The outer map key is family size (as string), inner map key is
/// generation span (as string) plus "total" for the row marginal.
pub type FamilyGenerationTable = BTreeMap<String, BTreeMap<String, usize>>;
```

The `grand_total` is implicit (sum of all `"total"` values, equals
`sum(family_size_distribution.values())`).

### 2. Phase 1: Streaming pass additions

During the existing streaming pass, additionally record:

```rust
// Per-family data: track who is a parent and who is a child.
struct FamilyRecord {
    /// Distinct count of unique person handles across both
    /// parent_handles and child_handles (deduplicates when a
    /// person appears as both parent and child in the same family).
    size: usize,
    parent_handles: Vec<String>,  // father + mother
    child_handles: Vec<String>,   // from childref elements
}

// All families, indexed by order of appearance.
let mut family_records: Vec<FamilyRecord> = Vec::new();

// Per-person data: joins families across the file.
// Person → families where they are a parent.
let mut person_parent_families: HashMap<String, Vec<usize>> = HashMap::new();
// Person → families where they are a child.
let mut person_child_families: HashMap<String, Vec<usize>> = HashMap::new();
```

The existing `family_members: Vec<HashSet<String>>` is replaced by
`family_records: Vec<FamilyRecord>` which preserves role separation.

### 3. Phase 2: Connected components and generation layering

After the streaming pass completes:

```
1. Build DSU over all person handles.
   For each family_record:
     union all members of the family (parents + children) into one set.

   **Dangling refs**: Only include person handles that actually exist
   in the document (present in `all_handles`). Dangling references
   (handles that appear in a family but have no matching `<person>`
   element) are **skipped** — they do not participate in the DSU.
   This means the generation span of a family with dangling refs
   reflects only its existing-person subset.

2. For each DSU set (connected component), assign generation numbers:
   a. Build parent→child edges:
      For each family_record:
        For each parent_handle in family_record.parent_handles:
          For each child_handle in family_record.child_handles:
            edge parent_handle → child_handle.
   b. Collect all people in the component.
   c. Find roots: people with no incoming edges (no parents recorded).
   d. Topological (longest-path) layering from roots: assign gen[root] = 0.
      For each child of a person p with gen = g:
        child.gen = max(child.gen, g + 1).
      (This uses iterative relaxation, not pure BFS — the `max()`
      ensures correct layering when a person has multiple parent paths
      of different depths.)
   e. Handle cycles: if a cycle is detected (visited set + back edges),
      break the cycle by treating all members of the cycle as the same
      generation (set to min of their tentative generations) and emit
      a warning.
   f. Component span = max(person.gen for person in component) + 1.
      (All components have at least one root, so min gen = 0.)

3. For each nuclear family (family_record):
   - family_size = family_record.size (distinct handles across
     parent_handles + child_handles, deduplicated via a HashSet).
   - family_gen_span = component span of any member of the family
     (all members share the same component span).
   - Increment table[family_size][family_gen_span].

4. Fill in row and column marginals.
```

**DSU implementation detail**: Use a simple `HashMap<String, String>`
mapping handle → parent handle in the DSU tree, with path compression.
No external crate needed.

### 4. Text output format

The table is printed below the existing "Family size distribution"
section, separated by a blank line:

```
Family size × generation table
  # generations │  1   2   3  total
  ──────────────┼──────────────────
  # people  1   │  5   0   0      5
             2  │  4   2   0      6
             3  │  1   8   2     11
  ──────────────┼──────────────────
  total         │ 10  10   2     22
```

**Formatting rules:**

- Column widths are auto-sized to fit the largest value in each column
  (including headers).
- The header row shows " # generations" (left-aligned, with a leading
  space) and column labels "1", "2", … right-aligned.
- Row labels show "# people" on the first row (left-aligned), then
  just the numeric size for subsequent rows (left-aligned, same
  indent width).
- Box-drawing characters (`│`, `─`, `┼`) are used for separators.
  When `--json` is set, no text table is produced (JSON replaces
  the entire text output).
- A `--no-unicode` flag (or auto-detection of non-TTY output) causes
  pure ASCII (`|`, `-`, `+`).

**Warnings in text output**: The `format_text_report()` function
currently does not render the `warnings` field. To ensure cycle
warnings are visible, add a "Warnings" section to the text report:

```
Warnings:
  - WARNING: detected 1 cycle(s) in the family graph; generation layering may be approximate
```

This section should appear at the end of the report, after the
generation table.

### 5. JSON output additions

The JSON output gains a new top-level field:

```json
{
  "file": "family.gramps",
  "counts": { ... },
  "family_size_distribution": { "1": 13, "2": 24, ... },
  "family_generation_table": {
    "1": { "1": 5, "total": 5 },
    "2": { "1": 4, "2": 2, "total": 6 },
    "3": { "1": 1, "2": 8, "3": 2, "total": 11 }
  },
  "people_not_in_family": 8,
  "dangling_refs": 0,
  "warnings": []
}
```

The `family_generation_table` is a JSON object where:

- Each key is a family size (as a string).
- Each value is an object with generation-span keys (as strings) plus
  a `"total"` key.
- The grand total is implicit (sum of all `"total"` values).

This representation is backward-compatible: existing consumers that
ignore unknown fields continue to work.

### 6. Warnings

If cycles are detected in the parent→child graph, add a warning:

```
"WARNING: detected {n} cycle(s) in the family graph; generation layering may be approximate"
```

Warnings are appended to the existing `warnings: Vec<String>` field.

---

## Implementation Plan

### Step 1: Add `FamilyRecord` struct and update streaming pass

**Files:** `crates/cli/src/commands/stats/count.rs`

- Add `FamilyRecord` struct with `size`, `parent_handles`, `child_handles`.
- Replace `family_members: Vec<HashSet<String>>` with
  `family_records: Vec<FamilyRecord>`.
- In the streaming pass, separate parent refs (`father`/`mother`) from
  child refs (`childref`) when recording family data.
- When a self-closing `<family/>` is encountered (no child elements),
  push a `FamilyRecord { size: 0, parent_handles: vec![],
  child_handles: vec![] }` alongside the existing histogram increment.
  This ensures every family has a corresponding record for the
  generation table.
- Keep `histogram: HashMap<usize, usize>` for the existing size
  distribution (unchanged logic).
- Add `person_parent_families` and `person_child_families` hashmaps
  (optional optimization — not strictly needed for the algorithm in
  Phase 2, but useful for debugging).

**Tests:** Existing tests must still pass (family size distribution
  unchanged). The existing `family_members` → `HashSet` deduplication
  is now done via `FamilyRecord.size` which is the number of distinct
  handles across parents + children.

### Step 2: Implement connected-components and generation layering

**Note:** The `FamilyGenerationTable` type alias (defined in Step 3)
is needed for the return type of `compute_generation_table()`.
Define the type alias at the top of `count.rs` at the beginning of
this step — it's a one-liner (`pub type FamilyGenerationTable =
BTreeMap<String, BTreeMap<String, usize>>`) and does not depend on
any other Step 3 work.

**Files:** `crates/cli/src/commands/stats/count.rs`

- Add `fn compute_generation_table(family_records: &[FamilyRecord],
  all_handles: &HashSet<String>) -> FamilyGenerationTable`.
- Implement DSU (Disjoint Set Union) over person handles:
  `struct Dsu { parent: HashMap<String, String> }` with `find()` and
  `union()`.
- Implement parent→child edge collection and longest-path layering
  per component.
- Handle cycles with a visited-set check; emit warning.
- Build the `FamilyGenerationTable` (nested `BTreeMap`).

**New tests:**

- `generation_table_empty` — no families → empty table.
- `generation_table_single_family_no_children` — size 2, 1 gen.
- `generation_table_single_family_parents_children` — size 3+, 2 gens.
- `generation_table_two_family_chain` — two families forming a
  parent→child chain → 3 generations.
- `generation_table_two_family_chain_with_extra_children` — chain
  - extra children in same families.
- `generation_table_three_generation_chain` — grandparent→parent→child.
- `generation_table_isolated_person` — 1 person, 1 gen.
- `generation_table_disconnected_components` — two independent
  components in the same file.
- `generation_table_pedigree_collapse` — cousins marry; DAG still
  valid; longest-path works.
- `generation_table_cycle` — pathological cycle; covered by warning;
  layering caps at 50.
- `generation_table_duplicate_handles_across_families` — same person
  in multiple families.

### Step 3: Add `FamilyGenerationTable` to `StatsReport`

**Files:** `crates/cli/src/commands/stats/count.rs`

- Define `FamilyGenerationTable` type alias.
- Add `pub family_generation_table: FamilyGenerationTable` field to
  `StatsReport` (default: empty `BTreeMap`).
- Derive `Serialize`, `Deserialize` on the new field (the type alias
  inherits these from `BTreeMap`).
- Update `StatsReport::default()` and `StatsReport` construction in
  `count_gramps_xml()` to populate the table.

**Tests:**

- `json_output_contains_generation_table` — JSON round-trip includes
  the new field.
- `report_default_empty_table` — default report has empty table.
- `generation_table_integration` — generate a known graph (via
  `GraphBuilder` + `GraphXmlWriter`), count via `count_gramps_xml`,
  verify the table matches expected values.

### Step 4: Render the table in text output

**Files:** `crates/cli/src/commands/stats/mod.rs`

- Add `render_generation_table(table: &FamilyGenerationTable) -> String`.
- Implement Unicode box-drawing formatting with auto-width columns.
- Add `--no-unicode` flag to `StatsArgs` (or auto-detect TTY).
- Call `render_generation_table` from `format_text_report` and append
  the result after the "Family size distribution" section.
- For `--json` mode, the table is already part of the serialized
  `StatsReport` — no changes needed.

**Tests:**

- `render_generation_table_empty` — no rows → "No data" or empty
  section.
- `render_generation_table_single_row` — one row.
- `render_generation_table_multi_row` — multiple rows, column widths.
- `render_generation_table_unicode_ascii` — both modes produce correct
  alignment.
- `format_text_report_contains_generation_table_section` — the full
  report text includes the new section with expected text.

### Step 5: Update E2E tests

**Files:** `crates/cli/tests/e2e.rs`

- `e2e_stats_text_output` — add assertion that the generation table
  section appears in stdout.
- `e2e_stats_json_output` — add assertion that
  `family_generation_table` is present in parsed JSON.
- Existing assertions (File:, Object counts, Family size distribution,
  etc.) must still pass.

### Step 6: Update integration tests

**Files:** `crates/cli/tests/integration.rs`

- `stats_count_known_graph` — add assertions for the generation table
  (the existing test builds a 3-person family; the generation table
  should have one entry: size 3, span 2 generations).

### Step 7: Run full test suite

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Testing Strategy

### Unit tests (`count.rs`)

| Test | What it verifies |
|------|-----------------|
| `generation_table_empty` | Empty family list → empty table |
| `generation_table_single_family_no_children` | 2 parents, 0 children → size 2, 1 gen (parents only, no children to create a 2nd gen) |
| `generation_table_single_family_with_children` | 2 parents + 1 child → size 3, 2 gens |
| `generation_table_two_family_chain` | Family A (parents + child P), Family B (P + spouse + child) → A size 3, B size 3, both in 3-gen component |
| `generation_table_three_generation_chain` | Grandparent → parent → child → grandchild chain → component spans 4 gens |
| `generation_table_isolated_person` | Single person, no families → table empty (no nuclear families) |
| `generation_table_disconnected_components` | Two independent components → table has entries from both |
| `generation_table_pedigree_collapse` | Cousins marry → DAG preserved → correct layering |
| `generation_table_cycle` | Artificial cycle → warning emitted, layering capped at 50 |
| `generation_table_duplicate_handles` | Same person in multiple families → person correctly unified across families |
| `generation_table_single_parent_family` | Single parent + child → size 2, parent gen 0, child gen 1 → component span 2 |
| `generation_table_child_only_family` | Only children, no parents → all gen 0 → component span 1 |
| `generation_table_single_member_family` | Family with one parent and no children → size 1, component span 1 |

### Integration tests (`integration.rs`)

| Test | What it verifies |
|------|-----------------|
| `stats_count_known_graph` | Existing test updated: known graph → correct generation table |

### E2E tests (`e2e.rs`)

| Test | What it verifies |
|------|-----------------|
| `e2e_stats_text_output` | Text output contains the generation table section |
| `e2e_stats_json_output` | JSON output contains `family_generation_table` field |

### Property-based tests (future, not in v1)

- Generate random graphs via `generate_random`, serialize to XML,
  stats → `count_gramps_xml`, verify that the generation table is
  internally consistent (row totals sum to `family_size_distribution`
  values, grand total = total families).

### Warnings rendering (text output)

Add a test for warning display in `format_text_report`:

| Test | What it verifies |
|------|-----------------|
| `format_text_report_warnings` | Report with cycle warnings renders them in the text output |

---

## Backward Compatibility

| Aspect | Compatible? | Detail |
|--------|-------------|--------|
| Text output | Yes | New section added below existing output; existing lines unchanged |
| JSON output | Yes | New field `family_generation_table` added; existing fields unchanged |
| `StatsReport` struct | Yes | New field added with `Default` (empty table); existing code compiling against the struct continues to compile |
| `--json` consumers | Yes | New field added; existing consumers that ignore unknown fields (or that deserialize the full struct) continue to work |
| Unit tests | Minor | Some tests may need minor updates if they construct `StatsReport` directly (new field uses `Default`) |

---

## Future Extensions (not in scope)

- **`--no-unicode` auto-detection**: Detect when stdout is not a TTY
  and switch to ASCII automatically. (v1 scope: explicit `--no-unicode`
  flag implemented in Step 4; TTY auto-detection is a future refinement.)
- **Property-based test**: Random graph → serialize → stats → verify
  generation table invariants.
- **Generation-span histogram**: A separate histogram showing how many
  families span each number of generations (1D, collapsing the size
  dimension).
- **Visualization**: ASCII sparklines or bar charts next to the table
  cells.
- **Cycle repair**: In the rare case of a cycle, attempt to break it
  in a principled way (e.g., remove the edge with the lowest confidence)
  instead of capping at 50.
