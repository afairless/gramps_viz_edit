# Implementation Plan: Place Name Display in Delete Review

Source: `docs/research/place-name-display-in-delete-review.md`

## Summary

Show real place names (`Furida`, `Furmany`) instead of `"Unnamed Place"` in the `gramps-gen delete` interactive review. The root cause is two-fold: (1) the Gramps 5.1 XML parser never reads `<pname value="..."/>` or `<ptitle>` elements, so `PlaceData.title` is always `None` for 5.1 files; (2) the review display only falls back to `data.title.as_deref().unwrap_or("Unnamed Place")`.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: parse pname/ptitle in gramps-reader XML graph parser` | Parsing: extend `PlaceBuilder` with `place_names`, read `<pname>` (Empty event) and `<ptitle>` (Start event), populate `alt_names` on `into_data()` | `crates/gramps-reader/src/xml/graph.rs` | Unit: `parse_place_pname`, `parse_place_ptitle`, `parse_place_pname_plus_ptitle`, `parse_place_multiple_pnames` |
| 2 | `feat: display real place names in delete review` | Display: add `location_display_name` and `place_display_name` helpers, update `Node::Place` arm in `describe_node` | `crates/delete/src/review.rs` | Unit: `describe_place_node_name_first`, `describe_place_node_title_fallback`, `describe_place_node_location_fallback`, `describe_place_node_unnamed`, `location_display_name_empty` |
| 3 | `test: add end-to-end parser→display integration test for place names` | Integration: cross-crate test via `gramps-reader` dev-dep on `delete`, parse XML and check `describe_node` output | `crates/gramps-reader/Cargo.toml`, `crates/gramps-reader/src/xml/graph.rs` | Integration: `parse_and_describe_place_pname` |
| 4 | `chore: run full workspace test and verify all outputs` | Full workspace verification | — | Workspace: `cargo test --workspace`, `cargo clippy --all-targets --all-features -- -D warnings` |

## Details

### Step 1 — Parse pname/ptitle in gramps-reader

**File: `crates/gramps-reader/src/xml/graph.rs`**

**1a. Extend `PlaceBuilder`** (≈ line 1533) with a `place_names: Vec<PlaceName>` field:

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
    place_names: Vec<PlaceName>,
}
```

**1b. Add `<ptitle>` text-element case** in the `Start` event block (next to the `code` case, ≈ line 564). Read the element text into `p.title`:

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

**1c. Add `<pname>` case** in the `Ok(Event::Empty(ref e))` match block (≈ line 974). Read the `value` attribute via `read_attr` and push a `PlaceName`:

```rust
b"pname" => {
    if let Some(value) = read_attr(e, b"value").filter(|v| !v.is_empty()) {
        if let Some(ref mut p) = current_place {
            p.place_names.push(PlaceName { value: Some(value), date: None });
        }
    }
}
```

**1d. Populate `alt_names` in `into_data()`** (≈ line 1546):

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
        alt_names: self.place_names,
        ..PlaceData::default()
    }
}
```

**Unit tests** (in `#[cfg(test)] mod tests` at end of `graph.rs`):

- `parse_place_pname` — single `<pname value="Furida"/>` → `alt_names[0].value == Some("Furida")`, `title == None`
- `parse_place_ptitle` — `<ptitle>Furida</ptitle>` → `title == Some("Furida")`
- `parse_place_pname_plus_ptitle` — both present; both preserved
- `parse_place_multiple_pnames` — two `<pname>` → `alt_names == [primary, alternate]`

### Step 2 — Display place names in delete review

**File: `crates/delete/src/review.rs`**

**2a. Add `location_display_name` helper** (near `fallback_source`, ≈ line 316):

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

**2b. Add `place_display_name` helper**:

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

**2c. Replace the `Node::Place` arm** (≈ line 509):

```rust
Some(Node::Place(data)) => {
    let desc = place_display_name(data);
    (desc, data.gramps_id.clone())
}
```

**Unit tests** (in existing `#[cfg(test)] mod tests`):

- `describe_place_node_name_first` — `alt_names[0] = Some("Furida")`, `title = Some("Furida title")` → renders `"Furida"`
- `describe_place_node_title_fallback` — empty `alt_names`, `title = Some("New York")` → renders `"New York"` (preserves existing behavior)
- `describe_place_node_location_fallback` — `name: Location { city: Some("Ur"), country: Some("Mesopotamia") }` → renders `"Ur, Mesopotamia"`
- `describe_place_node_unnamed` — all empty → `"Unnamed Place"`
- `location_display_name_empty` — default `Location` → `None`

### Step 3 — Integration test (parser → display)

**Dependency note**: `gramps-reader` does not currently depend on `delete`. Add `delete` as a **dev-dependency** in `crates/gramps-reader/Cargo.toml` (safe because `delete` does not depend on `gramps-reader`, so no cycle).

**File: `crates/gramps-reader/src/xml/graph.rs`** (in the unit test module, or as a separate `#[test]`):

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

### Step 4 — Full workspace verification

Run:

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```
