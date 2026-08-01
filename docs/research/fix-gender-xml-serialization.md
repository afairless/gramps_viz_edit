# Fix Plan: Gender Serialized as Attribute Instead of Child Element

## Overview

The generated Gramps XML writes gender as an integer attribute on the `<person>`
element (`gender="0"`) instead of as a child element (`<gender>M</gender>`).
Gramps ignores the unrecognized attribute and, finding no `<gender>` child
element, defaults every person to Unknown. This affects all schema versions.

Two errors combine:

| # | Defect | Impact |
|---|--------|--------|
| 1 | Gender written as XML **attribute** (`gender="1"`) | Gramps ignores it — the DTD/RNG define no `gender` attribute |
| 2 | Gender value is an **integer** (`0`, `1`, `2`) | Gramps expects single-letter strings (`M`, `F`, `U`) |

---

## Problem Analysis

### What the Gramps XML format requires

The system-installed Gramps 5.1.6 RelaxNG schema (`/usr/share/gramps/grampsxml.rng`)
and DTD (`/usr/share/gramps/grampsxml.dtd`) define the `<person>` element as:

```xml
<!-- DTD (grampsxml.dtd) -->
<!ELEMENT person (gender, name*, eventref*, ...)>
<!ATTLIST person
        id        CDATA #IMPLIED
        handle    ID    #REQUIRED
        priv      (0|1) #IMPLIED
        change    CDATA #REQUIRED
>
<!-- GENDER has values of M, F, or U. -->
<!ELEMENT gender  (#PCDATA)>
```

```xml
<!-- RelaxNG (grampsxml.rng) -->
<define name="person-content">
  <ref name="primary-object"/>
  <element name="gender"><choice>
      <value>M</value>
      <value>F</value>
      <value>U</value>
  </choice></element>
  <zeroOrMore><element name="name"> ... </element></zeroOrMore>
  ...
</define>
```

Key facts:

- `<gender>` is a **child element** (first child of `<person>`), not an attribute.
- Valid values are single-letter strings: `M` (Male), `F` (Female), `U` (Unknown).
- The only attributes on `<person>` are `handle`, `id`, `priv`, and `change`.

### What gramps-gen currently writes (wrong)

```xml
<person handle="05ea2d7c-..." gender="1">
  <name><first>Chloer</first><surname>Stonemore</surname></name>
  ...
</person>
```

Two problems: `gender="1"` is (a) an attribute, not a child element, and (b) uses
integer `1` instead of string `F`.

### Why Gramps shows "Unknown"

Gramps 5.1.6 parses `<person>` elements looking for the `<gender>` child element.
The `gender="1"` attribute is silently ignored (no attribute of that name in the DTD).
With no `<gender>` child present, Gramps defaults the person's gender to Unknown.

### Why the existing code writes it as an attribute

The serialization pipeline has two layers: the **declarative map** in
`serialization_map.rs` and the **imperative writer** in `xml.rs`.

In the map, gender is listed as an attribute:

```rust
// serialization_map.rs — Person entry
attributes: vec![
    XmlAttribute { field: "handle".to_string(), attr_name: "handle".to_string() },
    XmlAttribute { field: "gramps_id".to_string(), attr_name: "id".to_string() },
    XmlAttribute { field: "gender".to_string(), attr_name: "gender".to_string() },  // ← wrong
],
```

And `get_field_value()` handles it as a plain field-to-string conversion:

```rust
// xml.rs — get_field_value
"gender" => Some(typed_graph::gender_value(p.gender).to_string()),
```

The `write_inline_struct()` function — which handles child elements like
`event_type` → `<eventtype><type>Birth</type></eventtype>` — has no `"gender"`
case. Gender was never wired as a child element.

### Gender value mapping

The internal graph model uses integers matching Gramps' Python constants:

| Integer | Meaning | Gramps XML |
|---------|---------|------------|
| 0 | Male | `M` |
| 1 | Female | `F` |
| 2 | Unknown | `U` |
| 3 | Other (5.2 only) | `U` (mapped — no DTD equivalent) |

---

## Fix Plan

The fix has three parts, all in the `output` crate.

### Part 1: Move gender from attributes to children in the serialization map

**File:** `crates/output/src/serialization_map.rs`

1. **Remove** the `XmlAttribute { field: "gender", ... }` entry from the Person
   type's `attributes` vector.

2. **Add** a new `XmlChild` at position 0 of the Person type's `children` vector
   (gender must be the first child element per the DTD):

   ```rust
   XmlChild {
       element_name: "gender".to_string(),
       source: XmlChildSource::InlineStruct("gender".to_string()),
   },
   ```

   Before the existing `primary_name` child. The children list becomes:
   `[gender, name (primary), name (alternate), eventref, personref,
     parentin, childin, citationref, noteref, mediaref, tagref]`

### Part 2: Add gender writing logic to the XML writer

**File:** `crates/output/src/xml.rs`

Three spots need changes:

**A. `write_inline_struct()`** — add a `"gender"` match arm:

```rust
"gender" => {
    if let Node::Person(p) = node {
        let gender_str = match typed_graph::gender_value(p.gender) {
            1 => "F",
            2 | 3 => "U",
            _ => "M",
        };
        writeln!(
            writer,
            "      <gender>{}</gender>",
            gender_str
        )?;
    }
}
```

Value 3 (Other, schema-5.2 only) is silently mapped to `U` per decision above.
The 6-space indent is consistent with all other `write_inline_struct` match arms.

**B. `has_inline_struct_value()`** — add a `"gender"` case that returns `true`
for Person nodes (gender is always present):

```rust
"gender" => matches!(node, Node::Person(_)),
```

Gender is always present on Person nodes, so this arm always returns `true`.
This has a deliberate side effect: `has_children_for_node` will always return
`true` for Person nodes (even when all edges and alternate names are empty),
so the `<person>` element will never self-close.  This is correct per the DTD,
which requires `<gender>` as the first child of every `<person>` element.

**C. `get_field_value()`** — remove the `"gender"` arm from the `Node::Person`
match, since gender is no longer an attribute:

```rust
Node::Person(p) => match field_name {
    "handle" => Some(p.handle.clone()),
    "gramps_id" => p.gramps_id.clone(),
    // "gender" line removed
    _ => None,
},
```

### Part 3: Update tests

**A. Update `serialize_person_element`** (`xml.rs` tests):

Tighten the opening-tag assertion and assert the child element:

```rust
// Old (remove):
assert!(xml.contains(r#"<person handle="p1" id="I0001""#));

// New:
assert!(xml.contains(r#"<person handle="p1" id="I0001">"#));
assert!(xml.contains("<gender>M</gender>"));
```

Since `make_person` always uses `gender: 0` (Male), the expected output is `<gender>M</gender>`.

The old assertion also passes after the fix (the opening tag still contains those strings);
the change to `>` is a style improvement that makes the assertion more precise.  The
`serialize_person_no_gender_attribute` test (Part 3C) is the actual regression guard.

**B. Add two new tests for gender value mappings:**

**B1. `serialize_person_gender_values`** — verifies the three shared gender values
(common to all schema versions):

```rust
#[test]
fn serialize_person_gender_values() {
    for (gender_int, expected_char) in [(0, "M"), (1, "F"), (2, "U")] {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        let node = Node::Person(PersonData {
            handle: "p1".to_string(),
            gramps_id: None,
            gender: typed_graph::graph::into_gender_field(gender_int),
            primary_name: Name {
                first_name: Some("Test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        });
        graph.add_node("p1".to_string(), node).unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(
            xml.contains(&format!("<gender>{}</gender>", expected_char)),
            "gender {} should serialize as <gender>{}</gender>, got: {}",
            gender_int, expected_char, xml
        );
    }
}
```

**B2. `serialize_person_gender_other_maps_to_u`** — verifies that value 3 (Other,
schema-5.2 only) maps to `U`.  Gated behind `#[cfg(not(feature = "schema-5-1"))]`
because the `Other` variant only exists in the 5.2 Gender enum:

```rust
#[cfg(not(feature = "schema-5-1"))]
#[test]
fn serialize_person_gender_other_maps_to_u() {
    let map = SerializationMap::new();
    let writer = GraphXmlWriter::new(map, "5.2.0");
    let mut graph = Graph::new();
    let node = Node::Person(PersonData {
        handle: "p1".to_string(),
        gramps_id: None,
        gender: typed_graph::graph::into_gender_field(3),
        primary_name: Name {
            first_name: Some("Test".to_string()),
            ..Default::default()
        },
        ..Default::default()
    });
    graph.add_node("p1".to_string(), node).unwrap();

    let mut output = Vec::new();
    writer.write(&graph, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();

    assert!(
        xml.contains("<gender>U</gender>"),
        "gender 3 (Other) should serialize as <gender>U</gender>, got: {}",
        xml
    );
}
```

**C. Add a regression test that the `gender` attribute is absent:**

```rust
#[test]
fn serialize_person_no_gender_attribute() {
    let map = SerializationMap::new();
    let writer = GraphXmlWriter::new(map, "5.2.0");
    let mut graph = Graph::new();
    graph.add_node("p1".to_string(), make_person("p1", None)).unwrap();

    let mut output = Vec::new();
    writer.write(&graph, &mut output).unwrap();
    let xml = String::from_utf8(output).unwrap();

    // The string 'gender="' should NOT appear (it was an attribute, now a child element)
    assert!(!xml.contains(r#"gender=""#));
    // The child element should be present
    assert!(xml.contains("<gender>M</gender>"));
}
```

---

## Verification

### Manual verification

1. **Regenerate with the original command:**

   ```bash
   cargo build --release --features schema-5-1
   target/release/gramps-gen generate --schema-version 5.1 \
     --output /tmp/fixed_gender.gramps --count 16 --seed 2026 --depth 4
   ```

   Expected: `<person handle="..." id="...">` followed by `<gender>M</gender>`,
   `<gender>F</gender>`, or `<gender>U</gender>` as the first child.

2. **Open in Gramps 5.1.6 GUI** and verify genders are correctly displayed
   (Male/Female/Unknown, not all Unknown).

3. **Test with schema-5.2 to confirm no regression:**

   ```bash
   cargo build --release
   target/release/gramps-gen generate --schema-version default \
     --output /tmp/regression.gramps --count 200 --seed 42 --depth 3
   ```

   Expected: gender written as `<gender>` child element, not as attribute.

### Automated tests

```bash
cargo test -p output                    # All serialization tests
cargo test --workspace                  # Full workspace
cargo clippy --all-targets -- -D warnings
```

---

## Scope limitations

This plan addresses only the XML serialization format. It does **not** address:

- The `change` attribute (required by DTD but never written — Gramps tolerates its absence).
- The `priv` attribute (optional, never written — Gramps defaults to public).
- The internal graph model's gender representation (integers 0–3 are correct; only the
  serialization format is wrong).
- The existing plan `docs/research/fix-5.1-gender-and-family-connections.md`, which
  addresses a separate defect (missing Gender enum in schema-5.1 causing all persons
  to be generated as male).
- The `DoubleGender` adversarial strategy (Category B, swaps Male↔Female).  This strategy
  operates on the graph model's integer gender values and is unaffected — it swaps integers
  0↔1 before serialization, and those integers then map to `M↔F` via the new child-element
  writer.  No changes are needed.

---

## Implementation Steps

Steps are grouped into commit-sized units.  Steps 2–4 are interdependent (`has_inline_struct_value`
gates whether `write_inline_struct` is reached) and should be committed together.

1. **`serialization_map.rs`**: Remove gender from Person attributes, add as first child.
   → Commit this alone (it causes compile errors in tests until step 5, but the map change
   is a standalone logical unit).
2. **`xml.rs`**: Add `"gender"` case to `has_inline_struct_value()` (must be present before
   step 3 so the writer knows to emit the child element instead of self-closing).
3. **`xml.rs`**: Add `"gender"` case to `write_inline_struct()`.
4. **`xml.rs`**: Remove `"gender"` from `get_field_value()` Person match.
   → Commit steps 2–4 together (all `xml.rs` writer logic).
5. **`xml.rs` tests**: Update `serialize_person_element`, add `serialize_person_gender_values`,
   `serialize_person_gender_other_maps_to_u` (behind `#[cfg(not(feature = "schema-5-1"))]`),
   and `serialize_person_no_gender_attribute`.
   → Commit tests.
6. **Check CLI integration and E2E tests** in `crates/cli/tests/` for XML substring
   assertions that may reference the old `gender="` attribute.  Update any that do.
   → Commit any integration/E2E test updates.
7. **Run full test suite and clippy.**
8. **Manual verification with Gramps 5.1.6 GUI.**
