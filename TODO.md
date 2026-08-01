# Implementation Plan: Fix Gender Serialized as Attribute Instead of Child Element

Source: `docs/research/fix-gender-xml-serialization.md`

## Summary

The generated Gramps XML writes gender as an integer attribute on `<person>`
(e.g. `gender="1"`) instead of as a child element (`<gender>F</gender>`).
Gramps ignores the unrecognized attribute and defaults every person to Unknown.
This fix moves gender from an attribute to a child element and maps integer
values (0,1,2,3) to single-letter strings (M,F,U).

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: write gender as child element instead of attribute in Gramps XML` | Gender child-element serialization | `crates/output/src/serialization_map.rs`, `crates/output/src/xml.rs` | — (logic and map are committed together; tests updated in step 2) |
| 2 | `test: add serialization and E2E tests for gender child-element format` | Gender serialization tests | `crates/output/src/xml.rs` (tests), `crates/cli/tests/e2e.rs` | Unit, Regression |
| 3 | `chore: verify full test suite and clippy pass` | Verification | — (run `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`) | — |
| 4 | Manual verification with Gramps 5.1.6 GUI | Manual verification | — | — |

## Step Details

### Step 1 — Gender child-element serialization logic

**Files:**

- `crates/output/src/serialization_map.rs`
- `crates/output/src/xml.rs`

**Changes in `serialization_map.rs`:**

1. Remove the `XmlAttribute { field: "gender", attr_name: "gender" }` entry from the Person type's `attributes` vector.
2. Add a new `XmlChild` at position 0 of the Person type's `children` vector (gender must be the first child element per the DTD):

   ```rust
   XmlChild {
       element_name: "gender".to_string(),
       source: XmlChildSource::InlineStruct("gender".to_string()),
   },
   ```

   Before the existing `primary_name` child.

**Changes in `xml.rs`:**

1. In `has_inline_struct_value()` — add a `"gender"` case that returns `true` for Person nodes.
2. In `write_inline_struct()` — add a `"gender"` match arm that maps integer values to `M`/`F`/`U` and writes `<gender>X</gender>`.
3. In `get_field_value()` — remove the `"gender"` arm from the `Node::Person` match (gender is no longer an attribute).

**Deliverables:** `serialization_map.rs` updated, `xml.rs` updated with three changes.

### Step 2 — Update and add tests

**Files:**

- `crates/output/src/xml.rs` (test section)
- `crates/cli/tests/e2e.rs`

**Unit test changes in `xml.rs`:**

1. Update `serialize_person_element` — tighten the opening-tag assertion and assert the child element:

   ```rust
   assert!(xml.contains(r#"<person handle="p1" id="I0001">"#));
   assert!(xml.contains("<gender>M</gender>"));
   ```

2. Add `serialize_person_gender_values` — verifies gender values 0→M, 1→F, 2→U.
3. Add `serialize_person_gender_other_maps_to_u` (behind `#[cfg(not(feature = "schema-5-1"))]`) — verifies value 3→U.
4. Add `serialize_person_no_gender_attribute` — regression test that `gender="` attribute is absent and `<gender>M</gender>` child is present.

**E2E test changes in `e2e.rs`:**

- Update `e2e_51_gender_distribution` — change assertions from `gender="1"` (attribute) to `<gender>F</gender>` (child element) patterns.

**Deliverables:** Updated and new unit tests in `xml.rs`, updated E2E test in `e2e.rs`.

### Step 3 — Verify full test suite and clippy

**Commands:**

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

Fix any issues found. No code changes beyond what's needed to pass.

### Step 4 — Manual verification with Gramps 5.1.6 GUI

**Commands:**

```bash
cargo build --release --features schema-5-1
target/release/gramps-gen generate --schema-version 5.1 \
  --output /tmp/fixed_gender.gramps --count 16 --seed 2026 --depth 4

cargo build --release
target/release/gramps-gen generate --schema-version default \
  --output /tmp/regression.gramps --count 200 --seed 42 --depth 3
```

Verify output XML has `<gender>M</gender>`, `<gender>F</gender>`, or `<gender>U</gender>`
as the first child of each `<person>` element with no `gender="..."` attribute.
Open in Gramps 5.1.6 GUI and confirm genders are correctly displayed.
