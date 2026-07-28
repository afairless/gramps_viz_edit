# Implementation Plan: Phase 3 — GraphBuilder API

Source: `docs/research/design.md`

## Phase 3 Scope

Phase 3 builds the GraphBuilder API on top of the completed Phase 1 (schema extraction / codegen) and Phase 2 (graph core / validation). The GraphBuilder is a **separate struct** from `Graph` that provides a fluent, chainable API for constructing graphs programmatically. It checks required fields and handle resolution at build time, guaranteeing that graphs it produces pass structural and referential validation.

**Key design references**: design §7.3 (Fluent builder API), §7.1 (DateValue), §10 Phase 3 (Implementation plan).

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add DateValue struct and date types for GraphBuilder` | Date types | `crates/typed-graph/src/date.rs` | Unit |
| 2 | `feat: implement GraphBuilder scaffold and PersonBuilder` | GraphBuilder + PersonBuilder | `crates/typed-graph/src/generate/builder.rs`, `crates/typed-graph/src/generate/mod.rs` | Unit |
| 3 | `feat: add date- and family-relation setters to PersonBuilder` | PersonBuilder date/ref methods | `crates/typed-graph/src/generate/builder.rs` | Unit |
| 4 | `feat: implement FamilyBuilder with parent/child linking` | FamilyBuilder | `crates/typed-graph/src/generate/builder.rs` | Unit |
| 5 | `feat: implement EventBuilder and remaining primary type builders` | Other type builders | `crates/typed-graph/src/generate/builder.rs` | Unit |
| 6 | `feat: add build-time validation for required fields and handle resolution` | Build-time validation | `crates/typed-graph/src/generate/builder.rs` | Unit |
| 7 | `test: add comprehensive integration tests for GraphBuilder` | Integration tests | `crates/typed-graph/src/generate/builder.rs`, `tests/` | Unit, Integration |

### Step Details

#### Step 1 — DateValue, DateQuality, DateModifier (foundation)

- Create `crates/typed-graph/src/date.rs`:
  - `DateQuality` enum: `Exact`, `Estimated`, `Calculated` — derives `Clone, Copy, Debug, PartialEq, Default` (default: `Exact`).
  - `DateModifier` enum: `None`, `Before`, `After`, `About`, `Range { start: DateValue, end: DateValue }`, `Span { start: DateValue, end: DateValue }` — derives `Clone, Debug, PartialEq` (note: `Range` and `Span` contain `DateValue` so we need to define `DateValue` first).
  - `DateValue` struct:

    ```rust
    pub struct DateValue {
        pub year: u16,
        pub month: Option<u8>,
        pub day: Option<u8>,
        pub quality: DateQuality,
        pub modifier: DateModifier,
        pub text: Option<String>,
    }
    ```

    Derive `Clone, Debug, PartialEq`. Implement `DateValue::new(year: u16) -> Self` (year-only, exact, no modifier) and `DateValue::new_ymd(year: u16, month: u8, day: u8) -> Self` (full date).
  - Implement `DateValue::display_text()` returning a synthesized string ("about 1870", "estimated 1805", "between 1890 and 1900", etc.) matching the design §7.1 spec.
  - Implement `DateValue::is_valid()` — checks year in [1, 9999], month in [1, 12] if Some, day in valid range for month/year if both present.
  - Update `crates/typed-graph/src/lib.rs` to `pub mod date;` and re-export key types.
  - Write unit tests:
    - `date_value_new_year`: Creates year-only DateValue, year is correct, month/day are None.
    - `date_value_new_ymd`: Creates full DateValue, all fields match.
    - `date_quality_default_is_exact`.
    - `date_value_is_valid`: Valid dates pass, invalid year/month/day fail.
    - `date_value_display_text_exact`: "1870" for exact year, "1870-06-15" for exact YMD.
    - `date_value_display_text_about`: "about 1870" for About modifier.
    - `date_value_display_text_estimated`: "estimated 1870" for Estimated quality.
    - `date_value_display_text_before`: "before 1900" for Before modifier.
    - `date_value_display_text_after`: "after 1950" for After modifier.
    - `date_value_display_text_range`: "between 1890 and 1900" for Range.
    - `date_value_display_text_span`: "from 1890 to 1900" for Span.

#### Step 2 — GraphBuilder scaffold + PersonBuilder

- Create `crates/typed-graph/src/generate/mod.rs`:

  ```rust
  pub mod builder;
  pub use builder::*;
  ```

- Update `crates/typed-graph/src/lib.rs`:
  - Add `pub mod generate;`
  - Re-export key builder types.

- Create `crates/typed-graph/src/generate/builder.rs`:
  - `GraphBuilder` struct:

    ```rust
    pub struct GraphBuilder<'a> {
        graph: &'a mut Graph,
    }
    ```

    - `GraphBuilder::new(graph: &'a mut Graph) -> Self` — takes `&mut Graph` reference, per design §7.3.
    - `GraphBuilder::into_graph(self) -> &'a mut Graph` — returns the inner graph reference (consumes builder, useful for validation/serialization after building).
    - `GraphBuilder::add_person(&mut self, handle: impl Into<Handle>) -> PersonBuilder<'a, '_>` — creates a PersonBuilder.
    - `GraphBuilder::add_person_auto(&mut self) -> PersonBuilder<'a, '_>` — auto-generates a UUID v4 handle (requires `uuid::Uuid::new_v4()`).

  - `PersonBuilder` struct (separate builder per type, per design §7.3):

    ```rust
    pub struct PersonBuilder<'a, 'b> {
        builder: &'b mut GraphBuilder<'a>,
        data: PersonData,
    }
    ```

    - `PersonBuilder::with_handle(mut self, handle: impl Into<Handle>) -> Self` — sets the handle (overrides the one from `add_person`).
    - `PersonBuilder::with_gramps_id(mut self, id: impl Into<String>) -> Self`.
    - `PersonBuilder::with_gender(mut self, gender: i32) -> Self` — accepts raw i32 (0-3) matching the codegen Gender enum.
    - `PersonBuilder::with_name(mut self, first_name: impl Into<String>, surname: impl Into<String>) -> Self` — sets `primary_name` with a `Name` struct containing the given first name and a single `Surname`.
    - `PersonBuilder::with_primary_name(mut self, name: Name) -> Self` — sets the full `Name` struct directly.
    - `PersonBuilder::build(self) -> Handle` — inserts the node into the graph via `graph.add_node(...)`, returns the handle. NOTE: in this step, `build()` does NOT yet validate required fields (that comes in Step 6). For now, it just inserts.
    - Handle: if the user called `add_person("p1")`, the handle is "p1". If they called `add_person_auto()`, the handle is auto-generated UUID v4.

  - Write unit tests:
    - `builder_new_is_empty`: Create GraphBuilder, graph is unchanged.
    - `builder_add_person_basic`: Add person with handle, name, gender; verify node is in graph.
    - `builder_add_person_auto_handle`: Add person without explicit handle; verify it gets a UUID.
    - `builder_build_returns_handle`: `build()` returns the correct handle string.
    - `builder_add_person_with_gramps_id`: Person has correct gramps_id.
    - `builder_into_graph_returns_graph`: After building, `into_graph()` returns the graph with the person.
    - `builder_add_multiple_persons`: Add 2 persons, both exist in graph.

#### Step 3 — Date setters + parent family reference on PersonBuilder

- In `crates/typed-graph/src/generate/builder.rs`, add to `PersonBuilder`:
  - `PersonBuilder::with_birth_date(mut self, date: DateValue) -> Self` — sets the birth date (stores as a text field or structured date depending on how PersonData stores dates; Gramps stores dates as a `DateVal` struct, but the codegen generates `date: Option<String>` or similar from schema.json — check the actual schema. If the schema stores dates as `Option<String>`, use `DateValue::display_text()` to set the string).
  - `PersonBuilder::with_death_date(mut self, date: DateValue) -> Self` — same as above.
  - `PersonBuilder::with_parent_family(mut self, family_handle: &Handle) -> Self` — adds the family handle to `parent_family_list` and records a `PersonParentFamily` edge. NOTE: does NOT yet validate that the family handle exists in the graph (that comes in Step 6).
  - `PersonBuilder::with_family(mut self, family_handle: &Handle) -> Self` — adds to `family_list` and records a `PersonFamily` edge.
  - `PersonBuilder::add_alternate_name(mut self, name: Name) -> Self` — pushes to `alternate_names`.

  - Write unit tests:
    - `builder_person_with_birth_date`: Person has correct birth date text.
    - `builder_person_with_death_date`: Person has correct death date text.
    - `builder_person_with_parent_family`: Person has correct parent_family_list and edge exists.
    - `builder_person_with_family`: Person has correct family_list and edge exists.
    - `builder_person_with_alternate_name`: Person has correct alternate_names count.
    - `builder_person_with_multiple_families`: Person in multiple families, all edges present.

#### Step 4 — FamilyBuilder

- In `crates/typed-graph/src/generate/builder.rs`, add to `GraphBuilder`:
  - `GraphBuilder::add_family(&mut self, handle: impl Into<Handle>) -> FamilyBuilder<'a, '_>`.
  - `GraphBuilder::add_family_auto(&mut self) -> FamilyBuilder<'a, '_>`.

  - `FamilyBuilder` struct:

    ```rust
    pub struct FamilyBuilder<'a, 'b> {
        builder: &'b mut GraphBuilder<'a>,
        data: FamilyData,
    }
    ```

    - `FamilyBuilder::with_handle(mut self, handle: impl Into<Handle>) -> Self`.
    - `FamilyBuilder::with_gramps_id(mut self, id: impl Into<String>) -> Self`.
    - `FamilyBuilder::with_father(mut self, father_handle: &Handle) -> Self` — sets `father_handle` and records a `FamilyFather` edge. Checks that a Person node with the handle exists (defers to Step 6 for validation; in this step just inserts).
    - `FamilyBuilder::with_mother(mut self, mother_handle: &Handle) -> Self` — sets `mother_handle` and records a `FamilyMother` edge.
    - `FamilyBuilder::add_child(mut self, child_handle: &Handle, relation: ChildRefType) -> Self` — adds to `child_ref_list` and records a `FamilyChildRef` edge with metadata.
    - `FamilyBuilder::add_child_birth(mut self, child_handle: &Handle) -> Self` — convenience: adds child with `ChildRefType::Birth`.
    - `FamilyBuilder::with_marriage_date(mut self, date: DateValue) -> Self` — sets the marriage event date. Since Family has an `event_ref_list`, this creates a new Event node (Birth type as placeholder) or sets the date on an existing marriage event. Per design §7.3, this takes a `DateValue`.
    - `FamilyBuilder::build(self) -> Handle` — inserts the Family node.

  - Write unit tests:
    - `builder_family_basic`: Add family, verify node exists.
    - `builder_family_with_father_mother`: Family has correct father and mother handles and edges.
    - `builder_family_with_child`: Family has child in child_ref_list and edge exists.
    - `builder_family_with_multiple_children`: Family has correct number of children.
    - `builder_family_with_marriage_date`: Family has marriage event with correct date.
    - `builder_family_auto_handle`: Auto-generated UUID handle.
    - `builder_person_belongs_to_family`: Person's family_list updated after family build.

#### Step 5 — EventBuilder + remaining primary type builders

- In `crates/typed-graph/src/generate/builder.rs`, add to `GraphBuilder`:
  - `GraphBuilder::add_event(&mut self, handle: impl Into<Handle>) -> EventBuilder<'a, '_>`.
  - `GraphBuilder::add_place(&mut self, handle: impl Into<Handle>) -> PlaceBuilder<'a, '_>`.
  - `GraphBuilder::add_source(&mut self, handle: impl Into<Handle>) -> SourceBuilder<'a, '_>`.
  - `GraphBuilder::add_citation(&mut self, handle: impl Into<Handle>) -> CitationBuilder<'a, '_>`.
  - `GraphBuilder::add_note(&mut self, handle: impl Into<Handle>) -> NoteBuilder<'a, '_>`.
  - `GraphBuilder::add_media(&mut self, handle: impl Into<Handle>) -> MediaBuilder<'a, '_>`.
  - `GraphBuilder::add_repository(&mut self, handle: impl Into<Handle>) -> RepositoryBuilder<'a, '_>`.
  - `GraphBuilder::add_tag(&mut self, handle: impl Into<Handle>) -> TagBuilder<'a, '_>`.

  - `EventBuilder`:
    - `with_handle`, `with_gramps_id`, `with_event_type(EventType)`, `with_date(DateValue)`, `with_place(place_handle)`, `with_description(text)`, `build() -> Handle`.

  - `PlaceBuilder`:
    - `with_handle`, `with_name(name: PlaceName)`, `build() -> Handle`.

  - `SourceBuilder`:
    - `with_handle`, `with_title(title)`, `build() -> Handle`.

  - `CitationBuilder`:
    - `with_handle`, `with_source(source_handle)`, `build() -> Handle`.

  - `NoteBuilder`:
    - `with_handle`, `with_text(text)`, `build() -> Handle`.

  - `MediaBuilder`:
    - `with_handle`, `build() -> Handle`.

  - `RepositoryBuilder`:
    - `with_handle`, `build() -> Handle`.

  - `TagBuilder`:
    - `with_handle`, `with_name(name)`, `build() -> Handle`.

  - Write unit tests:
    - `builder_event_basic`: Add event, verify node in graph.
    - `builder_event_with_type_date_place`: Event has correct type, date, place.
    - `builder_place_basic`: Add place, verify node.
    - `builder_source_basic`: Add source with title.
    - `builder_citation_with_source`: Citation linked to source.
    - `builder_note_with_text`: Note with text.
    - `builder_all_types`: Add one of each primary type, verify all exist.

#### Step 6 — Build-time validation (required fields, handle resolution)

- In `crates/typed-graph/src/generate/builder.rs`, add validation logic to `build()` methods:
  - Define `BuilderError` enum:

    ```rust
    #[derive(Clone, Debug, PartialEq)]
    pub enum BuilderError {
        MissingRequiredField { builder_type: &'static str, field: &'static str },
        InvalidHandle { builder_type: &'static str, handle: Handle, target_type: &'static str },
    }
    ```

    Implement `Display` and `Error`.

  - Modify `PersonBuilder::build(self) -> Result<Handle, BuilderError>`:
    - Check required fields: `handle` must be non-empty, `gender` must be in 0..=3, `primary_name` must have at least a first name or surname.
    - Check that all handle references in `family_list`, `parent_family_list`, `person_ref_list` resolve to existing nodes in the graph.
    - Return `Err(BuilderError::MissingRequiredField)` for missing required fields.
    - Return `Err(BuilderError::InvalidHandle)` for unresolvable handle references.
    - On success, insert the node via `graph.add_node(...)` (which may still fail with `DuplicateHandle` — map that to a `BuilderError`).

  - Modify `FamilyBuilder::build(self) -> Result<Handle, BuilderError>`:
    - Check required fields: `handle` must be non-empty.
    - Check that `father_handle` and `mother_handle` (if set) resolve to Person nodes.
    - Check that child handles resolve to Person nodes.
    - On success, insert the node.

  - Modify `EventBuilder::build(self) -> Result<Handle, BuilderError>`:
    - Check required fields: `handle` must be non-empty, `event_type` must be set.
    - Check `place_handle` (if set) resolves to a Place node.

  - Modify all other `build()` methods to check required fields and handle resolution.

  - Note: `build()` now returns `Result<Handle, BuilderError>` instead of `Handle`. All unit tests in previous steps that called `.build()` expecting a `Handle` must be updated to use `.build().unwrap()`.

  - Write unit tests:
    - `builder_person_missing_name`: Person without name returns `MissingRequiredField`.
    - `builder_person_invalid_family_handle`: Person with non-existent parent family returns `InvalidHandle`.
    - `builder_family_invalid_father_handle`: Family with non-existent father returns `InvalidHandle`.
    - `builder_family_invalid_child_handle`: Family with non-existent child returns `InvalidHandle`.
    - `builder_event_invalid_place_handle`: Event with non-existent place returns `InvalidHandle`.
    - `builder_duplicate_handle`: Adding two nodes with the same handle returns error.
    - `builder_valid_person_with_family_ok`: Person with valid family handle passes.
    - `builder_error_display`: Error messages are user-actionable.

#### Step 7 — Comprehensive integration tests

- Add integration tests in `crates/typed-graph/src/generate/builder.rs` (or in `tests/` if cross-crate):
  - `builder_complex_tree`: Build a 3-generation family tree:
    - Grandfather + Grandmother → Family1
    - Family1 → Child1 (Father), Child2 (Aunt)
    - Father + Mother → Family2
    - Family2 → Grandchild1, Grandchild2
    - Verify: correct node counts, correct edge counts, all edges reference existing nodes.
  - `builder_cross_references`: Build persons with alternating family relationships.
  - `builder_person_with_all_fields`: Person with gramps_id, gender, name, birth/death dates, parent family, own family, alternate names, all edges present.
  - `builder_graph_validates_after_build`: Build a graph with the builder, then run `graph.validate(&schema)` — should pass with zero errors.
  - `builder_roundtrip_from_builder_to_validation`: Build a graph, validate, then serialize (stub — no serializer yet, just verify validation passes).
  - `builder_error_cases_comprehensive`: Test all error types from Step 6 in one test.
  - `builder_empty_graph_after_new`: GraphBuilder::new does not modify the graph until build() is called.
  - `builder_mixed_auto_and_explicit_handles`: Mix of auto-generated and explicit handles, all unique.

### Key design references

- **GraphBuilder API**: design §7.3 — separate `GraphBuilder` struct, per-type builder methods, `build()` returns `Handle`, chainable `with_*` setters.
- **DateValue struct**: design §7.1 — `DateValue { year, month, day, quality, modifier, text }`, `DateQuality`, `DateModifier` enums.
- **Builder validation**: design §7.3 — "builder guarantees that graphs it produces pass structural and referential validation: it checks required fields at build time and ensures all handle references resolve to existing nodes."
- **PersonBuilder setters**: design §7.3 — `with_name`, `with_gender`, `with_birth_date`, `with_death_date`, `with_parent_family`.
- **FamilyBuilder setters**: design §7.3 — `with_father`, `with_mother`, `with_marriage_date`, `add_child`.
- **Auto-generated handles**: design §7.3 — "add_person("p1") takes a string handle (or auto-generates a UUID v4 if omitted)."
- **Edge recording**: design §5 — `PersonFamily`, `PersonParentFamily`, `FamilyFather`, `FamilyMother`, `FamilyChildRef`, `PersonEventRef`, `EventPlace`, etc. are generated Edge variants.
- **Graph add_node/add_edge**: design §6 — `graph.add_node(handle, Node)` and `graph.add_edge(Edge)` return `Result<(), GraphError>`.
