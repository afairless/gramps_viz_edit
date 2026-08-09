# Improve Delete Tool Display — Implementation Plan

## Purpose

Address two issues in the `gramps-gen delete` interactive review CLI:

1. **Insufficient identifying information** — Many Gramps object types show
    only handles, leaving the user unable to tell what each item is.
2. **Incorrect step counter** — The progress header always says `Step X/10`
    even when only a subset of types have deletion candidates.

---

## Problem Analysis

### Problem 1: Poor descriptions for several node types

The `describe_node` function in `crates/delete/src/review.rs` (lines ~244-314)
generates one-line descriptions for each delete candidate. Its current output
per type:

| Type | Current output | What's missing |
|---|---|---|
| Person | `John Smith` | Birth/death dates |
| Family | `Family (f0001)` | Parent names, sample children |
| Event | `Birth event (1850)` | Associated person name(s) |
| Place | `New York` (title) | OK — has gramps title |
| Source | `Census Record` or `Unnamed Source` | Author/pubinfo fallback when title empty |
| Citation | `Citation (page: 5)` | Source ID & name, citing objects |
| Repository | `National Archives` (name) | OK |
| Media | `portrait.jpg` or `Unnamed Media` | Path/date fallback when desc empty |
| Note | `Note: "The quick brown fox..."` (60 chars) | Too short — need 150 chars |
| Tag | `MyTag` (name) | OK |

Additionally, **no gramps_id** (e.g., `I0001`, `E0001`) is shown for any type.
The user requested these be displayed after the handle.

### Problem 2: Step counter is hard-coded to 10

In `run_interactive_review` (lines ~76-85), `total_types` is always 10
(`type_order.len()`). The loop increments `current_step` only for types that
have candidates, producing sequences like `1/10, 2/10, …, 7/10`. The user
chose a **dynamic total** that reflects only types with candidates.

---

## Data-Field Reference

All primary types have an optional `gramps_id: Option<String>` field (e.g.
`I0001` for Person, `F0001` for Family, `E0001` for Event, `S0001` for
Source, `C0001` for Citation, `M0001` for Media, `N0001` for Note, `R0001`
for Repository, `P0001` for Place, `T0001` for Tag).

Key fields used by the plan:

| Type | Field | Rust type | Notes |
|---|---|---|---|
| Person | `primary_name.first_name` | `Option<String>` | |
| Person | `primary_name.surname_list[0].surname` | `Option<String>` | |
| Person | `birth_ref_index` | `Option<i32>` | Index into `event_ref_list` |
| Person | `death_ref_index` | `Option<i32>` | Index into `event_ref_list` |
| Person | `event_ref_list` | `Vec<EventRef>` | `EventRef.ref_field` = handle |
| Event | `event_type` | `Option<EventType>` | Enum: Birth, Death, etc. |
| Event | `date` | `Option<DateValue>` | Has `year`, `text`, etc. |
| Family | `father_handle` | `Option<Handle>` | |
| Family | `mother_handle` | `Option<Handle>` | |
| Family | `child_ref_list` | `Vec<ChildRef>` | `ChildRef.ref_field` = handle |
| Source | `title` | `String` | Required — may be empty |
| Source | `author` | `Option<String>` | |
| Source | `pubinfo` | `Option<String>` | Publication info |
| Citation | `source_handle` | `String` (5.2) / `Option<String>` (5.1) | Use `get_source_handle()` helper for portable access
| Citation | `page` | `Option<String>` | |
| Media | `desc` | `Option<String>` | The description/title |
| Media | `path` | `Option<String>` | File path |
| Media | `date` | `Option<DateValue>` | |
| Note | `text` | `String` | 60 chars → 150 chars |
| Tag | `name` | `String` | |
| Place | `title` | `Option<String>` | Place name |

---

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Gramps ID placement | After the handle, in brackets | `p0001 [I0001] — John Smith` |
| Missing gramps_id | Omit the brackets | `p0001 — John Smith` |
| Step counter | Dynamic total | `1/7 … 7/7` when 7 types have candidates |
| Person birth/death | Via `birth_ref_index` / `death_ref_index` | Direct field access, no graph walk needed |
| Event person names | Via `graph.edges_incident_to()` | Find Person/Family referers, get their names |
| Family children | 1–2 children from `child_ref_list` | Avoid overwhelming output; note count if >2 |
| Note text length | 150 chars | Up from 60; still reasonable for a single line |
| Media fallback chain | desc → path → date → "Unnamed Media" | |
| Source fallback chain | title → author → pubinfo → "Unnamed Source" | |

---

## Implementation Steps

### Step 1: Add helper functions (merged into Step 2)

The `format_gramps_id` helper and `person_full_name` helper (from Step 2b below)
are introduced in Step 2 as they are used. Step 1 and Step 2 are implemented
together since neither helper is independently testable as a standalone commit
— they only produce meaningful output when called from `describe_node`.

Step numbers in this plan are renumbered accordingly: 2a–2h below, then Step 3
(counter), Step 4 (tests), Step 5 (integration).

### Step 2: Enhance `describe_node` for each type

#### 2a. Person — add birth/death dates, extract `person_full_name` helper

Current:

```rust
Some(Node::Person(data)) => {
    let name = data.primary_name.first_name.as_deref().unwrap_or("Unknown");
    let surname = data.primary_name.surname_list.first()
        .and_then(|s| s.surname.as_deref()).unwrap_or("");
    if surname.is_empty() { name.to_string() }
    else { format!("{} {}", name, surname) }
}
```

**Refactoring first**: Extract the existing name-building logic into a
`person_full_name(p: &PersonData) -> String` helper (shown in Step 2b).
Update the Person branch to use the extracted helper, producing `full_name`.

New:

```rust
Some(Node::Person(data)) => {
    let full_name = person_full_name(data);
    // Birth/death date resolution
    let birth_year = data.birth_ref_index.and_then(|idx| {
        data.event_ref_list.get(idx as usize)
            .and_then(|eref| graph.get_node(&eref.ref_field))
            .and_then(|n| if let Node::Event(ed) = n { ed.date.as_ref() } else { None })
            .and_then(|d| d.text.clone().or_else(|| Some(format!("{:04}", d.year))))
    });
    let death_year = data.death_ref_index.and_then(|idx| {
        data.event_ref_list.get(idx as usize)
            .and_then(|eref| graph.get_node(&eref.ref_field))
            .and_then(|n| if let Node::Event(ed) = n { ed.date.as_ref() } else { None })
            .and_then(|d| d.text.clone().or_else(|| Some(format!("{:04}", d.year))))
    });
    match (birth_year, death_year) {
        (Some(b), Some(d)) => format!("{} ({}-{})", full_name, b, d),
        (Some(b), None)    => format!("{} (b. {})", full_name, b),
        (None, Some(d))    => format!("{} (d. {})", full_name, d),
        (None, None)       => full_name,
    }
}
```

**Note**: `describe_node` currently receives only `&Graph` and `&Handle`;
it already gets the node via `graph.get_node(handle)`, so accessing other
nodes (e.g., birth/death events) requires the graph.

#### 2b. Family — add parents + sample children

Current:

```rust
Some(Node::Family(_data)) => {
    format!("Family ({})", handle)
}
```

New:

```rust
Some(Node::Family(data)) => {
    // Get father name
    let father_name = data.father_handle.as_ref()
        .and_then(|h| graph.get_node(h))
        .and_then(|n| if let Node::Person(p) = n { Some(person_full_name(p)) } else { None })
        .unwrap_or_default();
    // Get mother name
    let mother_name = data.mother_handle.as_ref()
        .and_then(|h| graph.get_node(h))
        .and_then(|n| if let Node::Person(p) = n { Some(person_full_name(p)) } else { None })
        .unwrap_or_default();
    // 1-2 children
    let child_names: Vec<String> = data.child_ref_list.iter().take(2)
        .filter_map(|cr| graph.get_node(&cr.ref_field))
        .filter_map(|n| if let Node::Person(p) = n { Some(person_full_name(p)) } else { None })
        .collect();
    let child_count = data.child_ref_list.len();

    let parents = match (father_name.is_empty(), mother_name.is_empty()) {
        (true, true) => "no parents".to_string(),
        (false, true) => father_name,
        (true, false) => mother_name,
        (false, false) => format!("{} & {}", father_name, mother_name),
    };

    match child_names.len() {
        0 => format!("Family: {} ({})", parents, handle),
        1 if child_count == 1 => format!("Family: {} | child: {}", parents, child_names[0]),
        2 if child_count == 2 => format!("Family: {} | children: {}, {}", parents, child_names[0], child_names[1]),
        _ => format!("Family: {} | children: {}, {} (+{} more)", parents, child_names[0], child_names[1], child_count - 2),
    }
}
```

Extract a `person_full_name` helper:

```rust
fn person_full_name(p: &PersonData) -> String {
    let first = p.primary_name.first_name.as_deref().unwrap_or("Unknown");
    let surname = p.primary_name.surname_list.first()
        .and_then(|s| s.surname.as_deref()).unwrap_or("");
    if surname.is_empty() { first.to_string() }
    else { format!("{} {}", first, surname) }
}
```

#### 2c. Event — add associated person names

Current:

```rust
Some(Node::Event(data)) => {
    let event_type = data.event_type.as_ref()
        .map(|t| format!("{:?}", t)).unwrap_or_else(|| "Unknown".to_string());
    let date_str = if let Some(ref d) = data.date {
        d.text.clone().unwrap_or_else(|| format!("{:04}", d.year))
    } else { "no date".to_string() };
    format!("{} event ({})", event_type, date_str)
}
```

New:

```rust
Some(Node::Event(data)) => {
    let event_type = data.event_type.as_ref()
        .map(|t| format!("{:?}", t)).unwrap_or_else(|| "Unknown".to_string());
    let date_str = if let Some(ref d) = data.date {
        d.text.clone().unwrap_or_else(|| format!("{:04}", d.year))
    } else { "no date".to_string() };
    // Find associated people (up to 3) by checking PersonEventRef edges
    let people: Vec<String> = graph.edges_incident_to(handle).iter()
        .filter_map(|e| match e {
            Edge::PersonEventRef { source, target, .. } if target == handle => Some(source),
            _ => None,
        })
        .take(3)
        .filter_map(|h| graph.get_node(h))
        .filter_map(|n| if let Node::Person(p) = n { Some(person_full_name(p)) } else { None })
        .collect();
    match people.len() {
        0 => format!("{} event ({})", event_type, date_str),
        1 => format!("{} event ({}) — {}", event_type, date_str, people[0]),
        _ => format!("{} event ({}) — {}", event_type, date_str, people.join(", ")),
    }
}
```

**Note on FamilyEventRef**: Events can also be referenced by families via
`Edge::FamilyEventRef` (e.g., Marriage events). This code only traverses
`PersonEventRef` edges to show direct person associations. For Marriage
events, two additional hops through the Family → father/mother would be
needed, which is deliberately avoided here — the parent names are already
visible when reviewing related Family candidates. If a future user reports
that Marriage events show "no associated person", this can be revisited.

#### 2d. Citation — add source info + citing objects

Current:

```rust
Some(Node::Citation(data)) => {
    let page = data.page.as_deref().unwrap_or("no page");
    format!("Citation (page: {})", page)
}
```

New:

```rust
Some(Node::Citation(data)) => {
    // Source info — use get_source_handle() for portable access across
    // schema versions (String in 5.2, Option<String> in 5.1)
    let source_handle = {
        #[cfg(feature = "schema-5-1")]
        { data.source_handle.clone().unwrap_or_default() }
        #[cfg(not(feature = "schema-5-1"))]
        { data.source_handle.clone() }
    };
    let source_desc = if !source_handle.is_empty() {
        graph.get_node(&source_handle)
            .map(|n| {
                if let Node::Source(s) = n {
                    let id = format_gramps_id(&s.gramps_id);
                    if s.title.is_empty() {
                        format!("source({})", s.handle)
                    } else {
                        format!("source{} \"{}\"", id, s.title)
                    }
                } else { String::new() }
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    let source_str = if source_desc.is_empty() {
        "no source".to_string()
    } else {
        source_desc
    };
    // Page if available
    let page_str = data.page.as_deref()
        .map(|p| format!(", p. {}", p))
        .unwrap_or_default();
    format!("Citation → {}{}", source_str, page_str)
}
```

**Alternative**: Use the `typed_graph::get_source_handle()` helper which
abstracts over the version difference with a `#[cfg]` dispatch:
`let handle = typed_graph::get_source_handle(&data.source_handle);`
This is preferred over inline `#[cfg]` blocks for readability.

#### 2e. Source — add fallback fields

Current:

```rust
Some(Node::Source(data)) => {
    if data.title.is_empty() { "Unnamed Source".to_string() }
    else { data.title.clone() }
}
```

New:

```rust
Some(Node::Source(data)) => {
    if !data.title.is_empty() {
        data.title.clone()
    } else if let Some(ref author) = data.author {
        if !author.is_empty() { format!("Source by {}", author) }
        else { fallback_source(data) }
    } else {
        fallback_source(data)
    }
}
```

Where `fallback_source`:

```rust
fn fallback_source(data: &SourceData) -> String {
    if let Some(ref pubinfo) = data.pubinfo {
        if !pubinfo.is_empty() { return format!("Source: {}", pubinfo); }
    }
    "Unnamed Source".to_string()
}
```

#### 2f. Media — add fallback fields

Current:

```rust
Some(Node::Media(data)) => {
    data.desc.as_deref().unwrap_or("Unnamed Media").to_string()
}
```

New:

```rust
Some(Node::Media(data)) => {
    if let Some(ref desc) = data.desc {
        if !desc.is_empty() { return desc.clone(); }
    }
    if let Some(ref path) = data.path {
        if !path.is_empty() { return format!("Media: {}", path); }
    }
    if let Some(ref date) = data.date {
        let d = date.text.clone().unwrap_or_else(|| format!("{:04}", date.year));
        return format!("Media from {}", d);
    }
    "Unnamed Media".to_string()
}
```

#### 2g. Note — extend text preview to 150 chars

Current (60 chars):

```rust
let text_preview = if data.text.len() > 60 {
    format!("{}...", &data.text[..57])
} else if data.text.is_empty() {
    "empty note".to_string()
} else {
    data.text.clone()
};
format!("Note: \"{}\"", text_preview)
```

New (150 chars with word-boundary break):

```rust
let text_preview = if data.text.len() > 150 {
    let trunc = &data.text[..150];
    // Break at last space within the 150-char window if possible
    if let Some(last_space) = trunc.rfind(' ') {
        format!("{}...", &trunc[..last_space])
    } else {
        format!("{}...", trunc)
    }
} else if data.text.is_empty() {
    "empty note".to_string()
} else {
    data.text.clone()
};
```

#### 2h. Wrap all descriptions with gramps_id

Every branch in `describe_node` should prepend the gramps_id after the
handle. At the call site in `review_type_prompt` (line ~143), the display is:

```
  • {handle} — {description}
```

Change to:

```
  • {handle}{gramps_id} — {description}
```

**Design choice**: Change `describe_node` signature to return a tuple
`(String, Option<String>)` where the first element is the description body
and the second is the gramps_id:

```rust
fn describe_node(graph: &Graph, handle: &Handle) -> (String, Option<String>)
```

Each match arm extracts the `gramps_id` from its destructured node data and
returns it alongside the description.

At the call site, compose the full display line:

```rust
.map(|h| {
    let (desc, gramps_id) = describe_node(graph, h);
    let id_str = gramps_id
        .as_ref()
        .map(|id| format!(" [{}]", id))
        .unwrap_or_default();
    format!("  • {}{} — {}", h, id_str, desc)
})
```

**Rationale**: This approach keeps existing tests passing since the
description body returned by each match arm is unchanged. Only the
call site in `review_type_prompt` (the `list` action handler and the
3-sample display) needs updating. Existing `describe_node` unit tests
that assert on the description body continue to work — they just need
to also assert on the second tuple element.

**Impact on existing tests**: Each existing `describe_node` test must
be updated to destructure the tuple. Example:

```rust
let (desc, gramps_id) = describe_node(&graph, &handle);
assert_eq!(desc, "John Smith");
assert_eq!(gramps_id, Some("I0001".to_string()));
```

### Step 3: Fix the step counter

In `run_interactive_review`, compute `total_types` dynamically:

Current:

```rust
let type_order = [ ... ]; // 10 types
let total_types = type_order.len(); // always 10
```

New — count only types with candidates:

```rust
let type_order = [ ... ];
let total_types = type_order.iter()
    .filter(|nk| plan.per_type.get(nk).is_some_and(|h| !h.is_empty()))
    .count();
```

This ensures the header `Step 1/7` through `Step 7/7` when only 7 types
have deletion candidates. If all 10 have items, it naturally shows `1/10`.

**Edge case**: When `total_types` is 0 (no types have candidates), the
loop body never runs and the user sees no prompts — that's correct
behavior already handled by the empty loop skip.

### Step 4: Update unit tests

Add or modify tests in `crates/delete/src/review.rs`:

1. **Existing tests updated** — Every test calling `describe_node` must
    destructure the `(String, Option<String>)` tuple. Assert both the
    description body (unchanged format) and the gramps_id value.
2. **`person_full_name` tests** — With/without surname.
3. **`describe_node` for Person with both dates** — birth + death years.
4. **`describe_node` for Person with only birth** — `(Some(b), None)` case.
5. **`describe_node` for Person with only death** — `(None, Some(d))` case.
6. **`describe_node` for Person with no dates** — `(None, None)` case.
7. **`describe_node` for Family** — With father, mother, 1–2 children,
    and with more children than shown (testing the "+N more" suffix).
8. **`describe_node` for Event** — With associated people.
9. **`describe_node` for Event with FamilyEventRef** — Only PersonEventRef
    is traversed; no people should appear for a marriage event.
10. **`describe_node` for Citation** — With valid source handle.
11. **`describe_node` for Citation** — With empty/missing source handle
    (tests the "no source" fallback).
12. **`describe_node` for Source fallback** — Empty title → author →
    pubinfo → "Unnamed Source".
13. **`describe_node` for Media fallback** — Empty desc → path → date →
    "Unnamed Media".
14. **`describe_node` for Note** — 150-char truncation with word boundary.
15. **`describe_node` for Note** — 150+ chars with NO space in first 150
    (tests fallback to exact 150-char truncation).
16. **`describe_node` for Note** — Exact 150 chars (no truncation needed).
17. **`describe_node` for each type with `gramps_id` = None** — verify
    brackets are omitted.
18. **Step counter** — Test with 3 types → `3/3`.

### Step 5: Integration test

The E2E test in `crates/cli/tests/e2e.rs` already covers delete operations.
Run the existing suite to verify no regressions. Then manually test with a
real `.gramps` file to confirm display quality.

---

## Files to Modify

| File | Changes |
|---|---|
| `crates/delete/src/review.rs` | `describe_node` function: all branches enhanced; `run_interactive_review`: dynamic step counter; call sites update handle/gramps_id composition |
| `crates/delete/Cargo.toml` | No dependency changes needed |

No other crates are affected. The `describe_node` function is the single
point of display-format control.

---

## Open Questions / Risks

1. **Null-safe graph access**: `graph.get_node()` returns `Option<&Node>`.
    All cross-reference lookups (birth events, father/mother handles, event
    referers) must handle missing handles gracefully. This is expected in
    delete-scope cascades where connected nodes might already be deleted.

2. **Performance**: `edges_incident_to()` for events is O(1) per call
    (uses the reverse edge index). With 3-person per-event display, each
    event prompt requires 1 graph call. Acceptable for interactive use.

3. **Schema version handling**: The `gramps_id` field is `Option<String>`
    consistently across both schema-5.1 and schema-5.2, so no `#[cfg]`
    gating needed. The `title` field on `PlaceData` is `Option<String>`
    (same across versions).

4. **Word-boundary truncation for notes**: `rfind(' ')` may not find a
    space if the first 150 chars are a single word or non-breaking content.
    The fallback is to truncate at exactly 150 chars. This is acceptable.

5. **`source_handle` version handling**: In schema-5.2, `source_handle` on
    `CitationData` is `String` (required: true). In schema-5.1, it's
    `Option<String>`. The plan originally used `.as_ref().and_then(...)`
    which compiles only for `Option<String>`. Step 2d now uses
    `get_source_handle()` or inline `#[cfg]` dispatch for portable access.
