# Implementation Plan: Phase 4 — XML Serializer

Source: `docs/research/design.md`

## Phase 4 Scope

Phase 4 builds the XML serializer on top of the completed Phases 1–3 (schema extraction/codegen, graph core/validation, GraphBuilder API). The serializer walks a validated in-memory `Graph` and produces Gramps XML (`.gramps` format) following the RelaxNG schema. Since no RelaxNG schema file is available in this project, a **hand-coded fallback `SerializationMap`** is used (per design Decision 5).

**New crate**: `crates/output/` (workspace member with `quick-xml` dependency).

**Key design references**: design §8 (XML Serialization Design), §4 Decision 5 (RelaxNG-driven serialization mapping), §5 (Schema types), §6 (Graph model), §10 Phase 4.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: add output crate with quick-xml dependency` | Output crate scaffold | `crates/output/Cargo.toml`, `crates/output/src/lib.rs` | Smoke |
| 2 | `feat: define SerializationMap types for XML element mapping` | SerializationMap types | `crates/output/src/serialization_map.rs` | Unit |
| 3 | `feat: add hand-coded SerializationMap constants for Gramps XML` | SerializationMap data | `crates/output/src/serialization_map.rs` | Unit |
| 4 | `feat: implement XML header and database section structure writer` | XML document structure | `crates/output/src/xml.rs` | Unit |
| 5 | `feat: implement Person, Family, and Event node serialization` | Core type serialization | `crates/output/src/xml.rs` | Unit |
| 6 | `feat: implement remaining primary type XML serialization` | Other type serialization | `crates/output/src/xml.rs` | Unit |
| 7 | `feat: implement handle reference and edge serialization in XML output` | Edge/reference output | `crates/output/src/xml.rs` | Unit |
| 8 | `feat: implement Graph::to_xml orchestrator with validation check` | Public serialization API | `crates/output/src/xml.rs`, `crates/output/src/lib.rs` | Unit, Integration |
| 9 | `test: add comprehensive edge-case tests for XML serializer` | Edge-case tests | `crates/output/src/xml.rs` | Unit |

### Step Details

#### Step 1 — Output crate scaffold

- Create `crates/output/Cargo.toml`:

  ```toml
  [package]
  name = "output"
  version = "0.1.0"
  edition = "2021"

  [dependencies]
  quick-xml = "0.36"
  typed-graph = { path = "../typed-graph" }
  serde = { workspace = true }
  ```

- Register `crates/output` in workspace `Cargo.toml` (`members = ["crates/typed-graph", "crates/output"]`).
- Create `crates/output/src/lib.rs`:

  ```rust
  pub mod serialization_map;
  pub mod xml;
  ```

  - No codegen or complex logic yet — just module declarations and re-exports.
- Verify: `cargo build -p output` compiles.

  **Tests**: Smoke — `cargo build -p output` succeeds.

#### Step 2 — SerializationMap types

- In `crates/output/src/serialization_map.rs`, define the mapping types from design §8:

  ```rust
  use std::collections::HashMap;

  /// Maps Gramps schema types to XML element/attribute names.
  pub struct SerializationMap {
      /// Maps primary type name to its XML element info.
      pub type_map: HashMap<String, XmlTypeInfo>,
      /// Maps edge variant name to the XML nesting and attributes.
      pub edge_map: HashMap<String, XmlEdgeInfo>,
      /// Order in which type sections appear in the XML output.
      pub section_order: Vec<String>,
  }

  pub struct XmlTypeInfo {
      pub element_name: String,
      pub section_name: String,
      pub attributes: Vec<XmlAttribute>,
      pub children: Vec<XmlChild>,
  }

  pub struct XmlEdgeInfo {
      pub parent_element: String,
      pub element_name: String,
      pub attributes: Vec<(String, String)>,
  }

  pub struct XmlAttribute {
      pub field: String,
      pub attr_name: String,
  }

  pub struct XmlChild {
      pub element_name: String,
      pub source: XmlChildSource,
  }

  pub enum XmlChildSource {
      /// A direct field on the struct.
      Field(String),
      /// An array field on the struct.
      ArrayField(String),
      /// An edge kind whose target handle appears as an attribute.
      Edge(String),
  }
  ```

- Define `SerializationError` in `crates/output/src/serialization_map.rs` or in a new module:

  ```rust
  use std::fmt;

  #[derive(Clone, Debug, PartialEq)]
  pub enum SerializationError {
      Io(String),
      UnsupportedType(String),
      MissingField(String),
      GraphNotValidated,
  }

  impl fmt::Display for SerializationError { ... }
  impl std::error::Error for SerializationError {}
  impl From<std::io::Error> for SerializationError { ... }
  ```

- Write unit tests:
  - `serialization_map_types_construct`: Create an `XmlTypeInfo`, `XmlEdgeInfo`, `XmlAttribute`, `XmlChild`, `XmlChildSource` — verify fields match.
  - `serialization_error_display`: Error messages are user-actionable.
  - `serialization_error_from_io`: `IoError` conversion works.
  - `graph_not_validated_error_message`: Contains "not validated".

#### Step 3 — Hand-coded SerializationMap constants

- In `crates/output/src/serialization_map.rs`, add a function that builds the static `SerializationMap`:

  ```rust
  impl SerializationMap {
      /// Build the hand-coded fallback SerializationMap.
      ///
      /// ⚠ This is a hand-coded fallback based on the Gramps 5.2 RelaxNG schema.
      /// It may drift from the actual schema. If the RelaxNG schema becomes
      /// available, replace this with an auto-extracted version.
      pub fn new() -> Self { ... }
  }
  ```

  The map must cover all 10 primary types with:
  - **Person**: element `"person"`, section `"people"`, attributes `handle → "handle"`, `gramps_id → "id"`, `gender → "gender"`. Children: `primary_name → "name"`, `alternate_names` as `"name"`, `event_ref_list` as edge `"eventref"`, `family_list` as edge `"famc"`/`"fams"`, etc.
  - **Family**: element `"family"`, section `"families"`, attributes `handle → "handle"`, `gramps_id → "id"`. Children: `father_handle → "father"` via edge `FamilyFather`, `mother_handle → "mother"` via edge `FamilyMother`, `child_ref_list → "childref"` via edge `FamilyChildRef`, `event_ref_list → "eventref"` via edge `FamilyEventRef`.
  - **Event**: element `"event"`, section `"events"`, attributes `handle → "handle"`, `gramps_id → "id"`, `event_type → "type"` (as a child `<type>` element). Children: `date → "date"` via field, `description → "description"`, `place_handle → "place"` via edge `EventPlace`.
  - **Place**: element `"place"`, section `"places"`, attributes `handle → "handle"`, `gramps_id → "id"`, `type_field → "type"`. Children: `name → "name"` (PlaceName data).
  - **Source**: element `"source"`, section `"sources"`, attributes `handle → "handle"`, `gramps_id → "id"`. Children: `title → "stitle"` (or `"title"`).
  - **Citation**: element `"citation"`, section `"citations"`, attributes `handle → "handle"`, `gramps_id → "id"`. Children: `source_handle → "sourceref"` via edge `CitationSource`, `confidence → "confidence"`.
  - **Repository**: element `"repository"`, section `"repositories"`, attributes `handle → "handle"`, `gramps_id → "id"`.
  - **Media**: element `"object"`, section `"objects"`, attributes `handle → "handle"`, `gramps_id → "id"`.
  - **Note**: element `"note"`, section `"notes"`, attributes `handle → "handle"`, `gramps_id → "id"`. Children: `text → "text"`.
  - **Tag**: element `"tag"`, section `"tags"`, attributes `handle → "handle"`, `name → "name"`.

  Section order per design §8:

  ```rust
  vec!["tags", "events", "people", "families", "citations",
       "sources", "places", "objects", "repositories", "notes"]
  ```

- Write unit tests:
  - `map_has_all_primary_types`: `type_map` contains entries for all 10 primary types.
  - `map_section_order_complete`: All 10 section names appear in `section_order`.
  - `map_person_element_name`: `type_map["Person"].element_name == "person"`.
  - `map_person_has_handle_attribute`: Person's `attributes` contains `("handle", "handle")`.
  - `map_family_section_name`: `type_map["Family"].section_name == "families"`.
  - `map_event_children_not_empty`: Event has at least one `XmlChild`.
  - `map_edge_map_has_person_family`: `edge_map` has entry for `"PersonFamily"`.
  - `map_edge_map_has_citation_ref`: `edge_map` has entry for `"CitationRef"`.
  - `map_empty_serialization_map`: A fresh `SerializationMap::new()` has all entries populated.

#### Step 4 — XML header and database section structure

- In `crates/output/src/xml.rs`, implement the XML document structure writer:

  ```rust
  use quick_xml::Writer;
  use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
  use std::io::Write;

  use typed_graph::Graph;
  use crate::serialization_map::{SerializationMap, SerializationError};

  /// Write the XML declaration (`<?xml ...?>`).
  fn write_declaration<W: Write>(writer: &mut Writer<W>) -> Result<(), SerializationError> { ... }

  /// Write the `<database>` opening tag with namespace.
  fn write_database_open<W: Write>(writer: &mut Writer<W>) -> Result<(), SerializationError> { ... }

  /// Write the `<database>` closing tag.
  fn write_database_close<W: Write>(writer: &mut Writer<W>) -> Result<(), SerializationError> { ... }

  /// Write the `<header>` section with creator metadata.
  fn write_header<W: Write>(writer: &mut Writer<W>) -> Result<(), SerializationError> { ... }

  /// Write an empty `<{section}>...</{section}>` wrapper for a type section.
  fn write_empty_section<W: Write>(
      writer: &mut Writer<W>,
      section_name: &str,
  ) -> Result<(), SerializationError> { ... }
  ```

  Use `quick_xml::Writer` for all output. The XML target format:

  ```xml
  <?xml version="1.0" encoding="UTF-8"?>
  <database xmlns="http://gramps-project.org/xml/1.7.2/">
    <header>
      <created date="2026-07-27" version="5.2"/>
      <researcher>
        <resname>Generated by gramps-gen</resname>
      </researcher>
    </header>
    <tags>...</tags>
    <events>...</events>
    ...
  </database>
  ```

- Write unit tests:
  - `write_declaration_ok`: Renders `<?xml version="1.0" encoding="UTF-8"?>`.
  - `write_database_open_and_close`: Renders `<database ...>` and `</database>`.
  - `write_header_contains_gramps_gen`: Header contains "Generated by gramps-gen".
  - `write_empty_section_tags`: Renders `<tags/>` or `<tags></tags>`.
  - `write_empty_section_events`: Renders `<events/>`.
  - `write_multiple_empty_sections`: All 10 sections written in order, wrapped in database.

#### Step 5 — Person, Family, and Event node serialization

- In `crates/output/src/xml.rs`, implement functions to serialize primary node types:

  ```rust
  use typed_graph::{Node, PersonData, FamilyData, EventData, Name, Surname, DateValue};

  /// Serialize a single Person node as XML.
  fn write_person<W: Write>(
      writer: &mut Writer<W>,
      handle: &str,
      person: &PersonData,
      map: &SerializationMap,
  ) -> Result<(), SerializationError> { ... }

  /// Serialize a single Family node as XML.
  fn write_family<W: Write>(
      writer: &mut Writer<W>,
      handle: &str,
      family: &FamilyData,
      map: &SerializationMap,
  ) -> Result<(), SerializationError> { ... }

  /// Serialize a single Event node as XML.
  fn write_event<W: Write>(
      writer: &mut Writer<W>,
      handle: &str,
      event: &EventData,
      map: &SerializationMap,
  ) -> Result<(), SerializationError> { ... }
  ```

  For each type, follow the `SerializationMap`:
  - Open element with `element_name` (e.g., `<person`).
  - Write attributes from `XmlTypeInfo.attributes`: map field names to XML attribute names, look up values in the data struct.
  - For `handle` attribute, use the handle string.
  - For `gramps_id`, write `id` attribute (if `Some`).
  - For `gender` (Person), write as `"M"`, `"F"`, `"U"`, `"N"` mapping (0→M, 1→F, 2→U, 3→N).
  - Close the opening tag.
  - Write child elements from `XmlTypeInfo.children`:
    - For `primary_name` (Person), emit `<name>` with `<first>` and `<surname>` sub-elements, handling `Option` fields.
    - For dates, emit `<dateval>` with `val`, `type`, `qualifier` attributes from `DateValue` fields.
  - Write `</person>` closing tag.

  Helper functions:

  ```rust
  fn write_date_value<W: Write>(
      writer: &mut Writer<W>,
      date: &DateValue,
  ) -> Result<(), SerializationError> { ... }

  fn write_name<W: Write>(
      writer: &mut Writer<W>,
      name: &Name,
  ) -> Result<(), SerializationError> { ... }
  ```

  XML escaping is handled by `quick_xml::Writer` automatically.

- Write unit tests:
  - `write_person_basic`: Person with handle, gramps_id, gender → correct XML element.
  - `write_person_with_name`: Person with first name and surname → `<name><first>John</first><surname>Smith</surname></name>`.
  - `write_person_with_date`: Person with birth DateValue → `<eventref>` with dateval child.
  - `write_family_basic`: Family with handle → `<family handle="..."/>`.
  - `write_family_with_father_mother`: Family with father/mother handles → hlink attributes.
  - `write_event_basic`: Event with type, handle → `<event handle="..." type="..."/>`.
  - `write_event_with_date_place`: Event with date and place handle.
  - `write_gender_mapping`: Gender 0→"M", 1→"F", 2→"U", 3→"N".
  - `write_person_without_optional_fields`: Person with no gramps_id or dates → no id attribute, no date elements.

#### Step 6 — Remaining primary type serialization

- In `crates/output/src/xml.rs`, implement serialization for the remaining 7 types:

  ```rust
  use typed_graph::{PlaceData, SourceData, CitationData, RepositoryData, MediaData, NoteData, TagData, ...};

  fn write_place<W: Write>(...) -> Result<(), SerializationError> { ... }
  fn write_source<W: Write>(...) -> Result<(), SerializationError> { ... }
  fn write_citation<W: Write>(...) -> Result<(), SerializationError> { ... }
  fn write_repository<W: Write>(...) -> Result<(), SerializationError> { ... }
  fn write_media<W: Write>(...) -> Result<(), SerializationError> { ... }
  fn write_note<W: Write>(...) -> Result<(), SerializationError> { ... }
  fn write_tag<W: Write>(...) -> Result<(), SerializationError> { ... }
  ```

  Follow the `SerializationMap` for each type:
  - **Place**: `<place handle="..." id="..." type="...">` with `<name>` child containing `city`, `county`, `state`, `country` fields.
  - **Source**: `<source handle="..." id="...">` with `<stitle>` child.
  - **Citation**: `<citation handle="..." id="...">` with `<confidence>` child. Source handle reference output deferred to Step 7.
  - **Repository**: `<repository handle="..." id="...">` — minimal, name from `name` field.
  - **Media**: `<object handle="..." id="...">` — minimal, with `<file>` child from `path`/`mime` fields if they exist (or empty).
  - **Note**: `<note handle="..." id="...">` with `<text>` child containing note text.
  - **Tag**: `<tag handle="..." id="...">` with `name` attribute.

  Handle reference edges (e.g., `citation.source_handle` → `<sourceref hlink="...">`) are deferred to Step 7. For now, serialize only the node's direct data fields.

- Write unit tests:
  - `write_place_with_name`: Place with city/county/state → correct `<name>` children.
  - `write_source_with_title`: Source with title → `<stitle>` element.
  - `write_citation_basic`: Citation with handle → `<citation handle="..."/>`.
  - `write_note_with_text`: Note with text → `<text>` child.
  - `write_tag_with_name`: Tag with name attribute.
  - `write_repository_basic`: Repository with handle.
  - `write_media_basic`: Media/object with handle → `<object handle="..."/>`.
  - `write_all_types`: Serialize one node of each type, verify each appears in output.

#### Step 7 — Handle reference and edge serialization

- In `crates/output/src/xml.rs`, implement functions that write edges as XML reference elements:

  ```rust
  /// Write all edges from the graph that belong to a given node's section.
  /// This emits inline reference elements (e.g., `<eventref hlink="...">`
  /// inside `<person>`, `<childref hlink="...">` inside `<family>`).
  fn write_node_edges<W: Write>(
      writer: &mut Writer<W>,
      graph: &Graph,
      node_handle: &str,
      section_name: &str,
      map: &SerializationMap,
  ) -> Result<(), SerializationError> { ... }
  ```

  For each edge type, determine which section it belongs to from `XmlEdgeInfo.parent_element`:
  - `PersonFamily` → `<famc hlink="..."/>` (parent family) or `<fams hlink="..."/>` (own family) — need to distinguish based on field origin. Use `Person.parent_family_list` → `PersonParentFamily` edge → `<famc>`; `Person.family_list` → `PersonFamily` edge → `<fams>`.
  - `PersonParentFamily` → `<famc hlink="..."/>`.
  - `PersonEventRef` → `<eventref hlink="..." role="...">` with role metadata.
  - `FamilyFather` → `<father hlink="..."/>`.
  - `FamilyMother` → `<mother hlink="..."/>`.
  - `FamilyChildRef` → `<childref hlink="..." relation="...">` with relation metadata.
  - `FamilyEventRef` → `<eventref hlink="..."/>`.
  - `EventPlace` → `<place hlink="..."/>`.
  - `CitationSource` → `<sourceref hlink="..."/>`.
  - Mixin edges (`CitationRef`, `NoteRef`, `MediaRef`, `TagRef`) → `<citationref hlink="..."/>`, `<noteref hlink="..."/>`, `<objref hlink="..."/>`, `<tagref hlink="..."/>`.
  - Handle ref arrays like `Person.person_ref_list` → `PersonPersonRef` → `<personref hlink="..."/>`.

  Each edge's metadata (where applicable) must be serialized as child elements or attributes:
  - `EventRef` metadata: `role` → `role` attribute.
  - `ChildRef` metadata: `relation` → `relation` attribute.
  - `PersonRef` metadata: `relation` → `relation` attribute.
  - `RepoRef` metadata: `call_number` → child `<callno>`, `media_type` → child `<media-type>`.

- Write unit tests:
  - `write_person_family_edge`: Person has `<famc hlink="f1"/>` for parent family.
  - `write_person_event_ref_edge`: Person has `<eventref hlink="e1" role="Primary"/>`.
  - `write_family_child_ref_edge`: Family has `<childref hlink="p2" relation="Birth"/>`.
  - `write_family_father_edge`: Family has `<father hlink="p1"/>`.
  - `write_citation_ref_edge`: Person has `<citationref hlink="c1"/>`.
  - `write_note_ref_edge`: Person has `<noteref hlink="n1"/>`.
  - `write_media_ref_edge`: Person has `<objref hlink="m1"/>`.
  - `write_tag_ref_edge`: Person has `<tagref hlink="t1"/>`.
  - `write_edges_not_written_for_wrong_section`: Person edges don't appear in Family section.
  - `write_node_with_no_edges`: Node with no outgoing edges produces no ref elements.

#### Step 8 — Graph::to_xml orchestrator

- In `crates/output/src/xml.rs`, implement the public serialization API:

  ```rust
  /// Serialize a Graph to Gramps XML.
  ///
  /// The graph must have [`ValidationState::Valid`] set, otherwise
  /// [`SerializationError::GraphNotValidated`] is returned.
  ///
  /// Uses the hand-coded [`SerializationMap`] for element/attribute mapping.
  /// Output is written to the provided [`Write`] target.
  pub fn graph_to_xml<W: Write>(
      graph: &Graph,
      writer: W,
      map: &SerializationMap,
  ) -> Result<(), SerializationError> {
      // 1. Check validation_state — must be Valid
      // 2. Create quick_xml::Writer
      // 3. Write declaration and database open
      // 4. Write header
      // 5. For each section in section_order:
      //    a. Find the corresponding node kind
      //    b. If no nodes of that kind, write empty section
      //    c. Otherwise, open section element and iterate nodes
      //       - For each node: call type-specific write function
      //       - Call write_node_edges for outgoing edges
      //    d. Write section closing element
      // 6. Write database close
      // 7. Flush writer
  }
  ```

  The orchestrator must map section names to `NodeKind` values:
  - `"tags"` → `NodeKind::Tag`
  - `"events"` → `NodeKind::Event`
  - `"people"` → `NodeKind::Person`
  - `"families"` → `NodeKind::Family`
  - `"citations"` → `NodeKind::Citation`
  - `"sources"` → `NodeKind::Source`
  - `"places"` → `NodeKind::Place`
  - `"objects"` → `NodeKind::Media`
  - `"repositories"` → `NodeKind::Repository`
  - `"notes"` → `NodeKind::Note`

  Update `crates/output/src/lib.rs`:

  ```rust
  pub use xml::graph_to_xml;
  pub use serialization_map::{SerializationMap, SerializationError, ...};
  ```

- Write unit tests:
  - `graph_to_xml_empty_graph`: Empty graph produces valid XML skeleton with all empty sections.
  - `graph_to_xml_unvalidated_graph`: Graph with `Unvalidated` state returns `Err(GraphNotValidated)`.
  - `graph_to_xml_invalid_graph`: Graph with `Invalid` state returns `Err(GraphNotValidated)`.
  - `graph_to_xml_single_person`: Graph with one validated person produces XML with `<people><person ...>`.
  - `graph_to_xml_single_event`: Graph with one valid event.
  - `graph_to_xml_single_family`: Graph with one valid family.
  - `graph_to_xml_full_gramps_database`: Graph with nodes of all 10 types, all validated → complete valid Gramps XML document.
  - `graph_to_xml_contains_valid_xml_declaration`: Output begins with `<?xml version`.
  - `graph_to_xml_contains_namespace`: Database element has correct namespace.
  - `graph_to_xml_section_order`: Sections appear in the correct order from the `SerializationMap`.

#### Step 9 — Comprehensive edge-case tests

- Add tests in `crates/output/src/xml.rs`:
  - `serialize_person_special_chars_in_name`: Name contains `&`, `<`, `>`, `"`, `'` — verify proper XML escaping.
  - `serialize_person_empty_handle`: Person with empty handle → still serialized (structural validation catches this, but serializer should not panic).
  - `serialize_family_with_only_father`: Family with father but no mother → XML output.
  - `serialize_event_with_all_optional_fields_absent`: Event with no date, no place, no description → valid minimal event.
  - `serialize_graph_with_many_nodes`: Graph with 100 persons → output is valid, no truncation.
  - `serialize_graph_with_all_edge_types`: Graph with one of every edge kind → output contains all reference elements.
  - `serialize_io_error`: Writer that returns I/O errors → `Err(SerializationError::Io(...))`.
  - `serialize_date_with_modifier`: DateValue with `Before` modifier → `<dateval type="before" ...>`.
  - `serialize_date_with_range_modifier`: DateValue with `Range` modifier → two `<dateval>` children.
  - `serialize_date_text_fallback`: DateValue with `text` set → `<dateval val="text value" ...>`.
  - `serialize_note_empty_text`: Note with empty text → `<text></text>` or `<text/>`.
  - `graph_to_xml_database_close`: Ensure `</database>` is the last element.
  - `serialize_roundtrip_graphbuilder_to_xml`: Use GraphBuilder to construct a small family tree, validate, serialize to XML, verify output contains expected handles and structure.

### Key design references

- **SerializationMap types**: design §8 — `SerializationMap`, `XmlTypeInfo`, `XmlEdgeInfo`, `XmlAttribute`, `XmlChild` structs. Hand-coded fallback per Decision 5.
- **XML output ordering**: design §8 — Gramps XML groups objects by type in fixed order: tags → events → people → families → citations → sources → places → objects → repositories → notes.
- **Serialization strategy**: design §8 — `graph.to_xml()` walks nodes by type, emits in Gramps' expected order, uses `SerializationMap` for element/attribute names, handle references emit as `hlink` attributes, embedded refs emit inline elements. Streams directly to writer via `Write`-based API.
- **Validation check**: design §8 — serializer checks `graph.validation_state` (must be `Valid`) and returns `SerializationError::GraphNotValidated` if not.
- **Error handling**: design §8 — `SerializationError` covers I/O errors, unsupported types, missing required fields.
- **XML escaping**: design §8 — handled by `quick-xml`; all string output is properly escaped for XML.
- **Section-to-type mapping**: design §8 — tag→`tags`, person→`people`, family→`families`, event→`events`, place→`places`, source→`sources`, citation→`citations`, media→`objects`, repository→`repositories`, note→`notes`.
- **Edge-to-XML mapping**: design §6 Edge enum — every edge variant maps to a specific XML element name and attribute set; metadata fields (role, relation, call_number) are serialized as attributes or child elements.
- **Graph API**: design §6 — `graph.nodes_by_kind(NodeKind)`, `graph.edges_from(handle)`, `graph.validation_state()`, `graph.assert_valid()`.
- **Validation pipeline**: design §11 — "Serialization checks `graph.validation_state` (must be `Valid`) and returns an error if not set."
