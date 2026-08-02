# Revised Plan: Family Group Statistics

## Terminology Distinction

The current code and documentation conflate two concepts under the name
"family". This document formally distinguishes them:

| Term | What it is | Gramps XML element | How it's determined |
|---|---|---|---|
| **Gramps family** | A nuclear family: parents + children in a single `<family>` element | `<family>` | Directly counted from XML |
| **Family group** | A connected component of the person graph — all people linked by any family relationship (parent, child, marriage, etc.) | N/A (computed) | Disjoint-set union over person handles shared across Gramps families |

In graph terms:

- **Gramps families** are edges in the person graph (they connect parents to children).
- **Family groups** are the connected components of that graph.

A family group contains one or more Gramps families. For example, a
grandparent → parent → child chain requires at least 2 Gramps families
but forms a single family group.

---

## Problem: Current Generation Table

The current `Family size × generation table` (implemented per the
original `family-generation-table.md` design) has a fundamental
inconsistency:

- **Rows** count **Gramps families** (nuclear families by size).
- **Columns** show the **generation span of the connected component**
  (family group) that each Gramps family belongs to.

This produces impossible-looking combinations. For example, a Gramps
family of size 1 or 2 is shown spanning 7 generations — which is
impossible for a single nuclear family. What's actually happening is
that the family is part of a larger connected component that spans 7
generations, but the table labels the column as if it were the family's
own span.

### Concrete example: `exp01.gramps`

```
Family size × generation table
  # generations │1   7  total
  ──────────────┼────────────
  # people 1    │1  12     13
  2             │5  19     24
  3             │0   2      2
  4             │0   7      7
  5             │0   3      3
  ──────────────┼────────────
  total         │6  43     49
```

The generation span values are correct for the **components**:

- 43 families belong to a single component spanning 7 generations.
- 6 families belong to components spanning 1 generation.

But the table claims these are properties of the individual families.
A family of size 1 cannot span 7 generations, yet 12 of them appear in
that column.

---

## Root Cause

In `compute_generation_table` (in `crates/cli/src/commands/stats/count.rs`),
the generation span is computed per **connected component** (family group)
and then assigned to every Gramps family in that component:

```rust
// Generation span per component = max generation + 1.
let mut span_by_root: HashMap<String, usize> = HashMap::new();
for (root, component) in &components {
    let max_gen = component.iter().map(|h| gen[h]).max().unwrap_or(0);
    span_by_root.insert(root.clone(), max_gen + 1);
}

// Tabulate families by (size, span).
for record in family_records {
    let size = record.size();
    // ... finds the component root for any member ...
    span = span_by_root.get(root).copied();  // <-- component's span, not family's
    // ...
}
```

The original design document (`family-generation-table.md`) made this
choice deliberately in Decision 1:

> **Decision 1: Units of the table are nuclear families** ... The
> generation count for a nuclear family is the **genealogical depth of
> the connected extended-family group (connected component) that the
> nuclear family belongs to**, not the span of the nuclear family's own
> members. This is the "extended lineage depth" interpretation —
> confirmed by user choice.

However, the user's reaction to the actual output shows this was the
wrong interpretation. The table is confusing because it mixes two
different units of analysis.

---

## Revised Plan: Table Counts Family Groups

### 1. Rename the table to "Family group size × generation table"

The table now counts **family groups (connected components)**, not
Gramps families. This makes the table self-consistent: each row is a
family group with the given size, and the generation span column
reflects that group's own min-to-max generation range.

### 2. New data: What the proposed table would show

For `exp01.gramps` (64 people, 49 Gramps families, 12 components):

| Component | People | Gramps Families | Gen Span | Description |
|---|---|---|---|---|
| 0 | 48 | 43 | 7 | The big tree (7 levels: 0–6) |
| 1 | 3 | 3 | 1 | Small polygamous subgraph (no children) |
| 2–4 | 2 each | 1 each | 1 | Three married couples, no children |
| 5–11 | 1 each | 0 | 1 | Seven isolated people |

**Proposed table (including all components, per user decision):**

```
Family group size × generation table
  # generations │1   7  total
  ──────────────┼────────────
  # people 1    │7   0      7
  2             │3   0      3
  3             │1   0      1
  48            │0   1      1
  ──────────────┼────────────
  total         │11  1     12
```

This table is now self-consistent: no impossible combinations. A family
group of size 48 spanning 7 generations is perfectly plausible (it's
the big tree). A family group of size 1 spanning 1 generation is an
isolated person. The 7 isolated people are included as family groups of
size 1.

### 3. Keep existing "Family size distribution" as-is

The existing `Family size distribution` section counts Gramps families
by size. This is correct and useful — it tells you how many nuclear
families of each size exist. It should remain unchanged.

### 4. Add a new "Family group distribution" section (confirmed)

A new section between "Family size distribution" and the generation
table, showing the distribution of family groups by size:

```
Family group distribution
  size  1: 7 groups (7 people)
  size  2: 3 groups (6 people)
  size  3: 1 group (3 people)
  size 48: 1 group (48 people)
```

### 5. Remove the old table entirely (confirmed)

The old `Family size × generation table` is removed. The new `Family
group size × generation table` replaces it. The `Family size
distribution` section (counting Gramps families by size) remains
unchanged as a separate summary.

### 6. Disposition of the existing `FamilyGroupGenerationTable` type

The type alias `FamilyGenerationTable = BTreeMap<String, BTreeMap<String, usize>>`
can remain structurally the same, but its doc comment and the
`StatsReport` field name should be updated to reflect the new semantics
(family groups, not Gramps families).

---

## Changes Required

### `crates/cli/src/commands/stats/count.rs`

| Change | Detail |
|---|---|
| Rename `FamilyGenerationTable` | `FamilyGroupGenerationTable` (or just update doc comment) |
| Rename field on `StatsReport` | `family_generation_table` → `family_group_generation_table` |
| Rewrite `compute_generation_table` | Change the final tabulation loop to count **components** by (size, span) instead of iterating over `family_records`. **Important: components with zero family records (isolated people) must also be tabulated** — the loop must iterate over all components, not just those reachable via `family_records`. The DSU and component-building code in the first half of the function remains unchanged; only the final tabulation loop changes. |
| Add `family_group_distribution` | New field on `StatsReport`: `BTreeMap<usize, usize>` mapping component size → count of components. Populate during the same component iteration loop that builds the generation table: for each component, increment `distribution[component.len()]`. |
| Update doc comments | Clarify the Gramps-family vs. family-group distinction throughout. In particular, add a doc comment on `StatsReport` distinguishing `family_size_distribution` (counts Gramps `<family>` elements by nuclear-family size) from `family_group_distribution` (counts connected components by number of people). |
| Clarify `people_not_in_family` | Keep this field unchanged. It still correctly counts people whose handle never appears in any `<family>` element. These people now additionally appear in the generation table as size-1 family groups. |

### `crates/cli/src/commands/stats/mod.rs`

| Change | Detail |
|---|---|
| Update `format_text_report` | Add "Family group distribution" section; update table title to "Family group size × generation table"; update field references |
| Rename rendering functions | `render_generation_table` → `render_family_group_table` (or similar) |
| Update table header label | `# generations` → stays (correct), but row label changes from `# people` to `# people` (same label, different data) |

### Tests

| Test file | Change |
|---|---|
| `count.rs` tests | Update `compute_generation_table` tests to reflect component-level counting; fix `json_output_contains_generation_table` (JSON field renamed); add tests for `family_group_distribution`; add tests for multi-family-to-single-component collapsing; add tests for isolated-person components appearing in the table |
| `mod.rs` tests | Update `format_text_report` tests to match new field names and new section; update `format_text_report_contains_generation_table_section` for new table title |
| `e2e.rs` | Rename `family_generation_table` → `family_group_generation_table` in JSON assertions; add `family_group_distribution` assertion; update text output assertions for new table title |
| `integration.rs` | Update `stats_count_known_graph` to use new field name `family_group_generation_table` |

#### Test implementation notes

**Key behavioral changes to account for:**

- **Multi-family collapsing**: Tests like `generation_table_pedigree_collapse` currently have 4 Gramps families in one component producing 2 rows (size 3 and size 4). Under the new semantics, the component has 9 people and span 4, producing a single row: size 9, span 4, count 1.
- **Disconnected components**: Tests like `generation_table_disconnected_components` will produce the same numerical output, but the semantics change: "2 family groups of size 3 and 1 family group of size 2" instead of "2 families of size 3 and 1 family of size 2."
- **Isolated persons**: The existing `generation_table_isolated_person` test has an isolated person with no family records. Currently the table is empty. Under the new semantics, the table should have a row: size 1, span 1, count 1.
- **Empty graph**: The `generation_table_empty` test must remain passing (empty components → empty table).

**New tests to add:**

| Test | Description |
|---|---|
| `generation_table_multi_family_collapse` | Multiple Gramps families in one component collapse to a single family-group row with the correct size and span |
| `generation_table_isolated_person_components` | Isolated persons (no family records) appear as family groups of size 1, span 1 |
| `family_group_distribution_empty` | Empty graph → empty map |
| `family_group_distribution_single` | Single component → one entry with correct size |
| `family_group_distribution_multiple` | Multiple components of varying sizes → correct entries; verify sum of `size × count` equals total people |

### JSON output

The JSON field `family_generation_table` should be renamed to
`family_group_generation_table` for consistency. A new field
`family_group_distribution` is added. The existing
`people_not_in_family` field is retained unchanged.

---

## Backward Compatibility

| Aspect | Impact |
|---|---|
| Text output | Breaking: table content is different (different row sizes, different counts, different column distribution). Existing scripts parsing text output will break. |
| JSON output | Breaking: field renamed `family_generation_table` → `family_group_generation_table`. New field `family_group_distribution` added. No dual-publishing transition period — this is a pre-1.0 tool and breaking changes are expected. |
| `StatsReport` struct | Breaking: field renamed. Code compiling against the struct must be updated. |
| Unit tests | Most tests need updating to match new semantics. |

This is a conscious break. The current table is misleading, so
backward compatibility is not worth preserving. No dual-publishing
of old and new JSON field names is planned.

## Suggested Implementation Order

For an incremental-development workflow where each step is independently
testable, implement in this order:

| Step | Description | Testable? |
|---|---|---|
| 1 | Rename types and fields: `FamilyGenerationTable` → `FamilyGroupGenerationTable`, `family_generation_table` → `family_group_generation_table` on `StatsReport`; add `family_group_distribution` field | Yes — compiles if field renames are correct; existing tests break as expected |
| 2 | Rewrite `compute_generation_table` tabulation loop to iterate over **components** (including isolated-person components) instead of `family_records`; populate `family_group_distribution` in the same loop | Yes — update unit tests for new semantics |
| 3 | Update `format_text_report` in `mod.rs`: add "Family group distribution" section, update table title, update field references; rename rendering functions | Yes — update `format_text_report` tests |
| 4 | Update unit tests in `count.rs` and `mod.rs` for new field names, new table content, and new sections | Yes — all unit tests pass |
| 5 | Update integration and E2E tests for new field names and output format | Yes — full test suite passes |
