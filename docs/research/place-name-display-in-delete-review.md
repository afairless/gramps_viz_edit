# Place Name Display in `delete` Review — Implementation Plan

> Status: Plan
> Created: 2026-08-13
> Target: `gramps-gen delete` — show real place names instead of "Unnamed Place" in the interactive review

## 1. Problem Statement

When `gramps-gen delete` lists deletion candidates, place entries show only the
handle and Gramps ID, never the place name:

```text
═══ Step 4/5: Places ═══
2 places would be deleted.

Sample candidates:
  • _103fac9b16892f1145fa0c356fba [P0002] — Unnamed Place
  • _103faca733d53f00cff933dddb95 [P0003] — Unnamed Place
```

The user expects the actual names (`Furida`, `Furmany`) to appear after the
`—`, matching the behavior already present for Person, Source, Repository, etc.

## 2. Root Cause Analysis

### 2.1 The input is Gramps 5.1 XML, and the name lives in `<pname>`

The reported file is `version="5.1.6"`. In Gramps 5.1, a `Place` has both a
free-text `title` and a structured primary `name` (a `PlaceName` with a
`value`). The XML exporter serializes them as:

```xml
<placeobj handle="_103fac9b16892f1145fa0c356fba" id="P0002" type="State">
  <pname value="Furida"/>
</placeobj>
```

`<ptitle>` is only written when `place.title` is non-empty (the exporter's
`write_line_nofix` skips empty values), so many real-world files — including
the user's — have only `<pname value="..."/>`.

### 2.2 The graph parser never reads `<pname>` or `<ptitle>`

The `delete` command loads its graph via
`gramps_reader::xml::graph::parse_gramps_xml`
(`crates/gramps-reader/src/xml/graph.rs`). Its `PlaceBuilder`
(≈ lines 1533–1563) has `title` and `name: Location` fields, and its child
parser (≈ lines 564–700) reads `code`, `lat`, `long`, `street`, `locality`,
`city`, `county`, `state`, `country`, `postal`, and `phone` — but **not**
`ptitle` and **not** `pname`. Therefore `PlaceData.title` ends up `None` and
`PlaceData.name` (the 5.2 `Location`) stays empty for 5.1 files.

### 2.3 The merged `PlaceData` model has no 5.1 primary-name field

The build-time merge of `schema-5.1.json` and `schema-5.2.json` unions fields
by name. 5.1 defines `Place.name` as an embedded `PlaceName`; 5.2 defines
`Place.name` as an embedded `Location`. The merge keeps the *last* version's
field info for colliding names, and 5.2 sorts after 5.1, so the merged struct
wins 5.2's `Location` (for the `Place.name` field).

However, secondary types like `Location` are merged **per-field** — 5.1's
`Location` has a `parish` field that 5.2's does not, so the merged `Location`
contains `parish: Option<String>` (the field is optional because only one
version defines it). This is why `location_display_name` in §4.2.1 can safely
access `loc.parish`.

```rust
pub struct PlaceData {
    pub title: Option<String>,     // 5.1 <ptitle>
    pub name: Location,            // 5.2 structured name
    pub alt_names: Vec<PlaceName>, // 5.1 alternate <pname> values
    // ... (the 5.1 PRIMARY <pname> has no home)
}
```

`PlaceName { value: Option<String>, date: Option<DateValue> }` survives as a
secondary type, and `alt_names` is otherwise unused by the delete path.

### 2.4 The review display only reads `title`

`describe_node` in `crates/delete/src/review.rs` (≈ line 509) renders a place
as:

```rust
Some(Node::Place(data)) => {
    let desc = data.title.as_deref().unwrap_or("Unnamed Place").to_string();
    (desc, data.gramps_id.clone())
}
```

With `title == None`, it falls straight to `"Unnamed Place"`.

## 3. Decisions

| Decision | Choice |
|---|---|
| Scope | Delete review path only (`xml/graph.rs` + `delete/src/review.rs`) |
| Storage of the 5.1 primary name | Reuse `PlaceData.alt_names`, primary `<pname>` first |
| Display precedence | **Name first**: `pname` → `title` → joined `Location` → `"Unnamed Place"` |

Out of scope (recorded for follow-up, not implemented here):

- `xml/parse.rs` (used by the `diff` command) has the same `<pname>` gap and
  reads a non-existent `<name value>` element instead.
- The `output` crate serializer (`serialization_map.rs`) does not round-trip
  `<pname>`/`<location>` for places.
- A first-class `PlaceData.place_name: Option<PlaceName>` field would be the
  most correct long-term model, but requires a schema-conversion rename and a
  wider blast radius.

## 4. Proposed Changes

### 4.1 `crates/gramps-reader/src/xml/graph.rs` — parse `ptitle` and `pname`

**4.1.1 Extend `PlaceBuilder`** (≈ line 1533) with a list that captures every
`<pname>` in document order (primary first, alternates after):

```rust
#[derive(Default)]
struct PlaceBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    title: Option<String>,
    code: Option<String>,
    lat: Option<String>,
    long: Option<String>,
    name: Location,
    place_names: Vec<PlaceName>,   // <pname value="..."/> in document order
}
```

**4.1.2 Add a `ptitle` text-element case** next to the existing `code` case
(≈ line 564), reading the element text into `p.title`:

```rust
b"ptitle" if current_place.is_some() => {
    let name_q = e.name().to_owned();
    if let Ok(text) = reader.read_text(name_q) {
        let text = text.trim().to_string();
        if !text.is_empty() {
            if let Some(ref mut p) = current_place {
                p.title = Some(text);
            }
        }
    }
}
```

**4.1.3 Add a `pname` case** in the `Ok(Event::Empty(ref e))` match block
(≈ line 974), alongside the existing `placeref`/`eventref` arms. Gramps
emits `<pname value="..."/>` as a self-closing element, which quick-xml
delivers as `Event::Empty` — not `Event::Start`. Read the `value` attribute
via the existing `read_attr` helper and push a `PlaceName`:

```rust
b"pname" => {
    if let Some(value) = read_attr(e, b"value").filter(|v| !v.is_empty()) {
        if let Some(ref mut p) = current_place {
            p.place_names.push(PlaceName { value: Some(value), date: None });
        }
    }
}
```

> `read_attr` is already imported in `graph.rs` and handles bare/namespaced
> attributes. The `date` on `PlaceName` is deliberately left `None` for now —
> the delete review only needs the text value.

**4.1.4 Populate `alt_names` in `PlaceBuilder::into_data()`** (≈ line 1546):

```rust
fn into_data(self) -> PlaceData {
    PlaceData {
        handle: self.handle,
        gramps_id: self.gramps_id,
        title: self.title,
        code: self.code,
        lat: self.lat,
        long: self.long,
        name: self.name,
        alt_names: self.place_names, // primary <pname> first, then alternates
        ..PlaceData::default()
    }
}
```

### 4.2 `crates/delete/src/review.rs` — render the place name

**4.2.1 Add a `Location`-join helper** (near `fallback_source`, ≈ line 316).
Join the populated fields in a stable order and return `None` when empty:

```rust
fn location_display_name(loc: &typed_graph::Location) -> Option<String> {
    let parts = [
        loc.street.as_deref(),
        loc.locality.as_deref(),
        loc.city.as_deref(),
        loc.parish.as_deref(),
        loc.county.as_deref(),
        loc.state.as_deref(),
        loc.country.as_deref(),
    ];
    let parts: Vec<&str> = parts.into_iter().flatten().filter(|s| !s.is_empty()).collect();
    if parts.is_empty() { None } else { Some(parts.join(", ")) }
}
```

**4.2.2 Add a `place_display_name` helper** implementing the decided
precedence — **name first**:

```rust
fn place_display_name(data: &typed_graph::PlaceData) -> String {
    // 1. Primary 5.1 name (<pname value>), stored as alt_names[0]
    if let Some(name) = data
        .alt_names
        .first()
        .and_then(|n| n.value.as_deref())
        .filter(|v| !v.is_empty())
    {
        return name.to_string();
    }
    // 2. 5.1 descriptive title (<ptitle>)
    if let Some(title) = data.title.as_deref().filter(|t| !t.is_empty()) {
        return title.to_string();
    }
    // 3. 5.2 structured Location (joined parts)
    if let Some(loc) = location_display_name(&data.name) {
        return loc;
    }
    "Unnamed Place".to_string()
}
```

**4.2.3 Replace the `Node::Place` arm** (≈ line 509):

```rust
Some(Node::Place(data)) => {
    let desc = place_display_name(data);
    (desc, data.gramps_id.clone())
}
```

## 5. Test Plan

### 5.1 `crates/gramps-reader/src/xml/graph.rs`

Add unit tests in the existing `#[cfg(test)] mod tests` (near the place tests,
≈ line 2000):

- **`parse_place_pname`** — a 5.1 `<placeobj>` with a single
  `<pname value="Furida"/>` yields `alt_names.len() == 1` and
  `alt_names[0].value == Some("Furida")`, `title == None`.
- **`parse_place_ptitle`** — a `<placeobj><ptitle>Furida</ptitle></placeobj>`
  yields `title == Some("Furida")`.
- **`parse_place_pname_plus_ptitle`** — both elements present; both are
  preserved (primary `pname` in `alt_names[0]`, `title` set independently).
- **`parse_place_multiple_pnames`** — two `<pname>` elements produce
  `alt_names == [primary, alternate]` in document order.
- **`parse_place_pname_namespaced`** (optional) — `<pname value="X"/>` with a
  namespace prefix on the `value` attribute is still read via `read_attr`.

### 5.2 `crates/delete/src/review.rs`

Update/add tests in `describe_place_node` (≈ line 789) and neighbors:

- **`describe_place_node_name_first`** — `alt_names[0].value = Some("Furida")`
  with `title = Some("Furida title")` renders `"Furida"` (name wins).
- **`describe_place_node_title_fallback`** — empty `alt_names`, `title` set →
  renders the title (keeps the existing `New York` assertion semantics).
- **`describe_place_node_location_fallback`** —
  `name = Location { city: Some("Ur"), country: Some("Mesopotamia"), .. }`
  with no name/title → renders `"Ur, Mesopotamia"`.
- **`describe_place_node_unnamed`** — all empty → `"Unnamed Place"`.
- **`location_display_name_empty`** — default `Location` → `None`.

### 5.3 Integration test (parser → display)

Add a single end-to-end test in `crates/gramps-reader/src/xml/graph.rs` that
parses a 5.1 `<placeobj>` with a `<pname value="Furida"/>` and then runs
`describe_node` from the `delete` crate on the resulting graph, asserting the
description is `"Furida"`. This catches wiring mistakes (e.g., the `pname`
arm placed in the wrong event block) that the unit tests alone would miss.

```rust
#[test]
fn parse_and_describe_place_pname() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.1"/>
  </header>
  <places>
    <placeobj handle="pl0001" id="P0001">
      <pname value="Furida"/>
    </placeobj>
  </places>
</database>"#.to_string();
    let (graph, _) = parse_gramps_xml(&xml).unwrap();
    let (desc, _) = delete::review::describe_node(&graph, &"pl0001".to_string());
    assert_eq!(desc, "Furida");
}
```

## 6. Verification

```bash
# Build + lint + test the touched crates
cargo build --release
cargo test -p gramps-reader
cargo test -p delete
cargo clippy -p gramps-reader -p delete --all-targets -- -D warnings

# Reproduce the reported command
target/release/gramps-gen delete \
  ~/Documents/gramps02/data.gramps \
  -s ~/Documents/gramps02/people-to-delete.json \
  -o ~/Documents/gramps02/data-deleted.gramps
```

Expected `Step 4/5: Places` output after the fix:

```text
Sample candidates:
  • _103fac9b16892f1145fa0c356fba [P0002] — Furida
  • _103faca733d53f00cff933dddb95 [P0003] — Furmany
```

## 7. Risks & Mitigations

| Risk | Mitigation |
| --- | --- |
| `alt_names` now contains the primary name at index 0, which is a slight semantic overload ("alternate" names). | Documented at the storage site (`into_data`) and in this plan. No current consumer iterates place `alt_names` for display, so no behavior changes elsewhere. A follow-up can promote this to a dedicated `place_name` field. |
| Display precedence assumes the primary `<pname>` lands at `alt_names[0]` (Gramps writes `Place.name` before `alt_names` in document order). | Verified against the Gramps exporter's element order. If a file ever violates it, the fallback chain still renders `title`/`Location` rather than crashing; the assumption is documented at the storage site (`into_data`). |
| A place could have both `title` and `pname` with different text; name-first changes today's title-first display for such files. | This matches Gramps' primary "Name" column and the user's explicit request; covered by `describe_place_node_name_first`. |
| `Location` fallback joins only fields the parser already captures (it does not parse 5.1 `<location street="..."/>` attribute form). | Out of scope; the fallback is best-effort and 5.1 names are recovered from `<pname>`. Tracked as future work. |

## 8. Follow-up Work (not in this change)

1. Fix `crates/gramps-reader/src/xml/parse.rs` (diff command) to read
   `<pname>`/`<ptitle>` symmetrically (currently reads a non-existent
   `<name value>` into `name.city`).
2. Teach `crates/output/src/serialization_map.rs` (the `generate` pipeline's
   serializer) to round-trip `<pname>`/`<ptitle>`/`<location>`. The `delete`
   command's output is serialized by Gramps' Python backend, which already
   preserves `<pname>` correctly.
3. Consider a schema-level `place_name: Option<PlaceName>` field by renaming
   the 5.1 `name` during `schema_convert.rs` conversion, removing the
   `alt_names[0]` overload and centralizing the semantics.
