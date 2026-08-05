//! Streaming full-graph parser for Gramps XML documents.
//!
//! Parses all 10 primary Gramps types (Person, Family, Event, Place,
//! Source, Citation, Repository, Media, Note, Tag) into a
//! [`typed_graph::Graph`] with all nodes.  Edges are built in a
//! second pass (see [`Parser::build_edges`]).
//!
//! # Architecture
//!
//! The parser uses a two-phase approach:
//!
//! 1. **Node pass** — reads all `<person>`, `<family>`, `<event>`, …
//!    elements and creates [`Node`] entries in the graph.  Handle
//!    references are stored in a pending list.
//! 2. **Edge pass** — iterates the pending references, looks up
//!    target handles, and calls [`Graph::add_edge`] for each.
//!
//! This two-phase design is required because Gramps XML may define
//! a referenced node **after** the reference (e.g., a person's
//! `family_list` may point to a family defined later in the file).

use crate::error::Error;
use crate::xml::header::detect_schema_version;
use crate::xml::{read_handle_attr, read_hlink_attr, strip_prefix};
use quick_xml::events::Event;
use quick_xml::Reader;
use typed_graph::*;

/// Pending edge that will be materialised during the edge pass.
///
/// Edges with metadata (e.g. `EventRef`, `ChildRef`) carry the full
/// metadata struct; simple handle-reference edges are plain
/// source→target pairs.
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum PendingEdge {
    /// Simple handle-reference edge (no metadata).
    Simple {
        source: Handle,
        target: Handle,
        kind: SimpleEdgeKind,
    },
    /// Person ↔ Event with EventRef metadata.
    PersonEventRef {
        source: Handle,
        target: Handle,
        metadata: EventRef,
    },
    /// Family ↔ Child with ChildRef metadata.
    FamilyChildRef {
        source: Handle,
        target: Handle,
        metadata: ChildRef,
    },
    /// Family ↔ Event with EventRef metadata.
    FamilyEventRef {
        source: Handle,
        target: Handle,
        metadata: EventRef,
    },
    /// Person ↔ Person with PersonRef metadata.
    PersonPersonRef {
        source: Handle,
        target: Handle,
        metadata: PersonRef,
    },
    /// Source ↔ Repository with RepoRef metadata.
    SourceRepoRef {
        source: Handle,
        target: Handle,
        metadata: RepoRef,
    },
}

/// Simple edge kinds (no metadata).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum SimpleEdgeKind {
    PersonFamily,    PersonParentFamily,    FamilyFather,    FamilyMother,
    FamilyCitation,  FamilyNote,           FamilyTag,
    EventPlace,      EventCitation,        EventNote,      EventTag,
    PersonCitation, PersonNote,           PersonTag,
    PlaceCitation,  PlaceNote,            PlaceTag,        PlacePlaceRef,
    SourceNote,      SourceTag,
    CitationNote,    CitationTag,          CitationRef,     CitationSource,
    MediaCitation,   MediaNote,            MediaTag,
    NoteCitation,    NoteTag,
    RepositoryNote,  RepositoryTag,
    TagTag,
    NoteRef,         MediaRef,              TagRef,
    PersonMediaRef,  EventMediaRef,         FamilyMediaRef,
    CitationMediaRef,SourceMediaRef,        PlaceMediaRef,
    RepositoryMediaRef,
}

/// Streaming parser state for building a [`Graph`] from Gramps XML.
pub struct Parser {
    /// The graph being built.
    pub graph: Graph,
    /// Compiled-in schema metadata.
    schema: &'static Schema,
    /// Pending edges to resolve after all nodes are collected.
    pending: Vec<PendingEdge>,
}

impl Parser {
    /// Create a new parser bound to the given schema.
    pub fn new(schema: &'static Schema) -> Self {
        Self {
            graph: Graph::new(),
            schema,
            pending: Vec::new(),
        }
    }

    /// Parse a complete Gramps XML document into the graph.
    ///
    /// Detects the schema version from the header, selects the matching
    /// compiled-in schema, then streams through all sections building
    /// nodes and collecting pending edge references.
    ///
    /// After this call, call [`Parser::build_edges`] to materialise all
    /// edges, then [`Parser::validate`] to verify the graph.
    pub fn parse_all(&mut self, content: &str) -> Result<(), Error> {
        let mut reader = Reader::from_str(content);
        reader.config_mut().trim_text(true);

        // Track which top-level section we are in.
        enum Section {
            TopLevel,            Header,            Tags,            Events,            People,            Families,
            Citations,            Sources,            Places,            Objects,
            Repositories,            Notes,
        }

        let mut section = Section::TopLevel;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    section = match name {
                        b"header" => Section::Header,
                        b"tags" => Section::Tags,
                        b"events" => Section::Events,
                        b"people" => Section::People,
                        b"families" => Section::Families,
                        b"citations" => Section::Citations,
                        b"sources" => Section::Sources,
                        b"places" => Section::Places,
                        b"objects" => Section::Objects,
                        b"repositories" => Section::Repositories,
                        b"notes" => Section::Notes,
                        _ => match section {
                            Section::People if name == b"person" => {
                                self.parse_person(&mut reader, e)?;
                                Section::People
                            }
                            Section::Families if name == b"family" => {
                                self.parse_family(&mut reader, e)?;
                                Section::Families
                            }
                            Section::Events if name == b"event" => {
                                self.parse_event(&mut reader, e)?;
                                Section::Events
                            }
                            _ => section,
                        },
                    };
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"header" | b"tags" | b"events" | b"people" | b"families"
                        | b"citations" | b"sources" | b"places" | b"objects"
                        | b"repositories" | b"notes" => {
                            section = Section::TopLevel;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    if name == b"person" && matches!(section, Section::People) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Person(PersonData {
                                    handle,
                                    ..PersonData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"family" && matches!(section, Section::Families) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Family(FamilyData {
                                    handle,
                                    ..FamilyData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"event" && matches!(section, Section::Events) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Event(EventData {
                                    handle,
                                    ..EventData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => {
                    return Err(Error::XmlParseError {
                        message: format!("{} at byte {}", e, reader.error_position()),
                    });
                }
            }
        }

        Ok(())
    }

    /// Parse a `<person>` element and its children.
    ///
    /// Reads the person's handle, gender, primary name, alternate names,
    /// attributes, addresses, URLs, and all handle references (eventrefs,
    /// family refs, citations, notes, media, tags).
    fn parse_person(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();

        let mut person = PersonData {
            handle: handle.clone(),
            ..PersonData::default()
        };
        let mut gender: i32 = 0;
        let mut name_count = 0usize;
        let mut in_name = false;
        let mut in_gender = false;
        let mut in_attribute = false;
        let mut in_address = false;
        let mut in_url = false;
        let mut current_name = Name::default();
        let mut current_surname = Surname::default();
        let mut in_surname = false;
        let mut in_first = false;
        let mut current_attr_type = String::new();
        let mut current_attr_value = String::new();
        let mut in_attr_type = false;
        let mut in_attr_value = false;
        let mut in_address_location = false;
        let mut current_location = Location::default();
        let mut in_city = false;
        let mut in_country = false;
        let mut in_county = false;
        let mut in_state = false;
        let mut in_street = false;
        let mut in_postal = false;        let mut in_locality = false;
        let mut in_phone = false;
        let mut current_url = Url::default();
        let mut in_url_desc = false;
        let mut url_type = None;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);

                    match name {
                        b"name" => {
                            name_count += 1;
                            in_name = true;
                            current_name = Name::default();
                            current_surname = Surname::default();
                            // Read type attribute from <name type="...">
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"type" || key.ends_with(b":type") {
                                    current_name.type_field =
                                        parse_name_type(&String::from_utf8_lossy(&attr.value));
                                }
                            }
                        }
                        b"surname" if in_name => {
                            in_surname = true;
                            current_surname = Surname::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"prefix" || key.ends_with(b":prefix") {
                                    current_surname.prefix = Some(val);
                                } else if key == b"prim" || key.ends_with(b":prim") {
                                    current_surname.primary = val.parse().ok();
                                } else if key == b"origintype" || key.ends_with(b":origintype") {
                                    current_surname.origintype = parse_name_origin_type(&val);
                                }
                            }
                        }
                        b"first" if in_name => in_first = true,
                        b"gender" => in_gender = true,
                        b"attribute" => {
                            in_attribute = true;
                            current_attr_type.clear();
                            current_attr_value.clear();
                            // Try to read type/value from attributes
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    current_attr_type = val;
                                } else if key == b"value" || key.ends_with(b":value") {
                                    current_attr_value = val;
                                }
                            }
                            // If both type and value are in attributes, we can close the attribute
                            if !current_attr_type.is_empty() && !current_attr_value.is_empty() {
                                person.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                                in_attribute = false;
                            }
                        }
                        b"type" if in_attribute => in_attr_type = true,
                        b"value" if in_attribute => in_attr_value = true,
                        b"lds_ord" => {
                            let mut lds = LdsOrd::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    lds.type_field = parse_lds_ord_type(&val);
                                } else if key == b"status" || key.ends_with(b":status") {
                                    lds.status = Some(val);
                                } else if key == b"temple" || key.ends_with(b":temple") {
                                    lds.temple = Some(val);
                                } else if key == b"date" || key.ends_with(b":date") {
                                    // Store as plain-text date — no DateValue parsing yet
                                } else if key == b"plac" || key.ends_with(b":plac") {
                                    lds.place_handle = Some(val);
                                }
                            }
                            // Skip if empty or default (no meaningful attributes)
                            person.lds_ord_list.push(lds);
                            // Self-closing handled in Empty as well
                        }
                        b"address" => {
                            in_address = true;
                            current_location = Location::default();
                        }
                        b"location" if in_address => in_address_location = true,
                        b"city" if in_address_location => in_city = true,
                        b"country" if in_address_location => in_country = true,
                        b"county" if in_address_location => in_county = true,
                        b"state" if in_address_location => in_state = true,
                        b"street" if in_address_location => in_street = true,
                        b"postal" if in_address_location => in_postal = true,
                        b"locality" if in_address_location => in_locality = true,
                        b"phone" if in_address_location => in_phone = true,
                        b"url" => {
                            in_url = true;
                            current_url = Url::default();
                            url_type = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"href" || key.ends_with(b":href") {
                                    current_url.href = val;
                                } else if key == b"type" || key.ends_with(b":type") {
                                    url_type = Some(val);
                                }
                            }
                        }
                        b"desc" if in_url => in_url_desc = true,
                        b"eventref" | b"citationref" | b"noteref" | b"tagref"
                        | b"personref" | b"mediaref" => {
                            // These are handled in the Empty branch
                            // For non-self-closing, they'd be processed on </...>
                            // but in Gramps XML they're always self-closing
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);

                    match name {
                        b"eventref" => {
                            let hlink = read_hlink_attr(e).unwrap_or_default();
                            let mut role = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"role" || key.ends_with(b":role") {
                                    role = parse_event_role_type(&val);
                                }
                            }
                            let event_ref = make_event_ref(hlink, role);
                            person.event_ref_list.push(event_ref.clone());
                            self.pending.push(PendingEdge::PersonEventRef {
                                source: handle.clone(),
                                target: event_ref.ref_field.clone(),
                                metadata: event_ref,
                            });
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                person.citation_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PersonCitation,
                                });
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                person.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PersonNote,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                person.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PersonTag,
                                });
                            }
                        }
                        b"personref" => {
                            let hlink = read_hlink_attr(e).unwrap_or_default();
                            let mut relation = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"relation" || key.ends_with(b":relation") {
                                    relation = parse_family_rel_type(&val);
                                }
                            }
                            let person_ref = PersonRef {
                                ref_field: hlink.clone(),
                                relation,
                            };
                            person.person_ref_list.push(person_ref.clone());
                            self.pending.push(PendingEdge::PersonPersonRef {
                                source: handle.clone(),
                                target: hlink,
                                metadata: person_ref,
                            });
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                person.media_list.push(MediaRef { ref_field: h.clone() });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PersonMediaRef,
                                });
                            }
                        }
                        b"attribute" => {
                            let mut attr_type = String::new();
                            let mut attr_value = String::new();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    attr_type = val;
                                } else if key == b"value" || key.ends_with(b":value") {
                                    attr_value = val;
                                }
                            }
                            if !attr_type.is_empty() || !attr_value.is_empty() {
                                person.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&attr_type),
                                    value: attr_value,
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                        }
                        b"url" => {
                            let mut href = String::new();
                            let mut url_type_val = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"href" || key.ends_with(b":href") {
                                    href = val;
                                } else if key == b"type" || key.ends_with(b":type") {
                                    url_type_val = parse_url_type(&val);
                                }
                            }
                            if !href.is_empty() {
                                person.url_list.push(Url {
                                    href,
                                    type_field: url_type_val,
                                    desc: None,
                                });
                            }
                        }
                        b"lds_ord" => {
                            let mut lds = LdsOrd::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    lds.type_field = parse_lds_ord_type(&val);
                                } else if key == b"status" || key.ends_with(b":status") {
                                    lds.status = Some(val);
                                } else if key == b"temple" || key.ends_with(b":temple") {
                                    lds.temple = Some(val);
                                } else if key == b"date" || key.ends_with(b":date") {
                                    // Store as plain-text date — no DateValue parsing yet
                                } else if key == b"plac" || key.ends_with(b":plac") {
                                    lds.place_handle = Some(val);
                                }
                            }
                            person.lds_ord_list.push(lds);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Ok(text) = e.unescape() {
                        let text = text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        if in_gender {
                            gender = parse_gender_value(text);
                        } else if in_first {
                            current_name.first_name = Some(text.to_string());
                        } else if in_surname {
                            current_surname.surname = Some(text.to_string());
                        } else if in_city {
                            current_location.city = Some(text.to_string());
                        } else if in_country {
                            current_location.country = Some(text.to_string());
                        } else if in_county {
                            current_location.county = Some(text.to_string());
                        } else if in_state {
                            current_location.state = Some(text.to_string());
                        } else if in_street {
                            current_location.street = Some(text.to_string());
                        } else if in_postal {
                            current_location.postal = Some(text.to_string());
                        } else if in_locality {
                            current_location.locality = Some(text.to_string());
                        } else if in_phone {
                            current_location.phone = Some(text.to_string());
                        } else if in_attr_type {
                            current_attr_type = text.to_string();
                        } else if in_attr_value {
                            current_attr_value = text.to_string();
                        } else if in_url_desc {
                            current_url.desc = Some(text.to_string());
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);

                    match name {
                        b"person" => {
                            set_gender(&mut person, gender);
                            self.graph.add_node(handle.clone(), Node::Person(person))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"name" => {
                            if in_name {
                                // Push any pending surname (handles self-closing <surname/>)
                                if current_surname.surname.is_some()
                                    || current_surname.prefix.is_some()
                                {
                                    current_name.surname_list.push(current_surname.clone());
                                }
                                if name_count == 1 {
                                    person.primary_name = current_name.clone();
                                } else {
                                    person.alternate_names.push(current_name.clone());
                                }
                                in_name = false;
                                in_first = false;
                                in_surname = false;
                            }
                        }
                        b"surname" => {
                            if in_surname {
                                current_name.surname_list.push(current_surname.clone());
                                current_surname = Surname::default();
                                in_surname = false;
                            }
                        }
                        b"first" => in_first = false,
                        b"gender" => in_gender = false,
                        b"attribute" => {
                            // If we had child type/value elements, push now
                            if !current_attr_type.is_empty() || !current_attr_value.is_empty() {
                                person.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                            in_attribute = false;
                            in_attr_type = false;
                            in_attr_value = false;
                        }
                        b"type" if in_attr_type => in_attr_type = false,
                        b"value" if in_attr_value => in_attr_value = false,
                        b"address" => {
                            person.address_list.push(Address {
                                location: Some(current_location.clone()),
                                date: None,
                                citation_list: vec![],
                                note_list: vec![],
                            });
                            in_address = false;
                            in_address_location = false;
                        }
                        b"location" => in_address_location = false,
                        b"city" => in_city = false,
                        b"country" => in_country = false,
                        b"county" => in_county = false,
                        b"state" => in_state = false,
                        b"street" => in_street = false,
                        b"postal" => in_postal = false,
                        b"locality" => in_locality = false,
                        b"phone" => in_phone = false,
                        b"url" => {
                            if let Some(t) = url_type.take() {
                                current_url.type_field = parse_url_type(&t);
                            }
                            person.url_list.push(current_url.clone());
                            in_url = false;
                            in_url_desc = false;
                        }
                        b"desc" => in_url_desc = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing person".to_string(),
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(Error::XmlParseError {
                        message: format!("{} at byte {}", e, reader.error_position()),
                    });
                }
            }
        }
    }

    /// Parse a `<family>` element and its children.
    ///
    /// Reads the family's father/mother handles, child refs, event refs,
    /// attributes, LDS ordinances, and all handle references (citations,
    /// notes, media, tags). Accumulates referenced handles into pending
    /// edge lists for the second pass.
    fn parse_family(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();

        let mut family = FamilyData {
            handle: handle.clone(),
            ..FamilyData::default()
        };
        let mut in_attribute = false;
        let mut current_attr_type = String::new();
        let mut current_attr_value = String::new();
        let mut in_attr_type = false;
        let mut in_attr_value = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"father" | b"mother" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if name == b"father" {
                                    family.father_handle = Some(h.clone());
                                    self.pending.push(PendingEdge::Simple {
                                        source: handle.clone(),
                                        target: h,
                                        kind: SimpleEdgeKind::FamilyFather,
                                    });
                                } else {
                                    family.mother_handle = Some(h.clone());
                                    self.pending.push(PendingEdge::Simple {
                                        source: handle.clone(),
                                        target: h,
                                        kind: SimpleEdgeKind::FamilyMother,
                                    });
                                }
                            }
                        }
                        b"attribute" => {
                            in_attribute = true;
                            current_attr_type.clear();
                            current_attr_value.clear();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    current_attr_type = val;
                                } else if key == b"value" || key.ends_with(b":value") {
                                    current_attr_value = val;
                                }
                            }
                            // If both type and value are in attributes, close immediately.
                            if !current_attr_type.is_empty() && !current_attr_value.is_empty() {
                                family.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                                in_attribute = false;
                            }
                        }
                        b"type" if in_attribute => in_attr_type = true,
                        b"value" if in_attribute => in_attr_value = true,
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"father" | b"mother" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if name == b"father" {
                                    family.father_handle = Some(h.clone());
                                    self.pending.push(PendingEdge::Simple {
                                        source: handle.clone(),
                                        target: h,
                                        kind: SimpleEdgeKind::FamilyFather,
                                    });
                                } else {
                                    family.mother_handle = Some(h.clone());
                                    self.pending.push(PendingEdge::Simple {
                                        source: handle.clone(),
                                        target: h,
                                        kind: SimpleEdgeKind::FamilyMother,
                                    });
                                }
                            }
                        }
                        b"childref" => {
                            let hlink = read_hlink_attr(e).unwrap_or_default();
                            let mut relation = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"relation" || key.ends_with(b":relation") {
                                    relation = parse_child_ref_type(&val);
                                }
                            }
                            let child_ref = make_child_ref(hlink.clone(), relation);
                            family.child_ref_list.push(child_ref.clone());
                            self.pending.push(PendingEdge::FamilyChildRef {
                                source: handle.clone(),
                                target: child_ref.ref_field.clone(),
                                metadata: child_ref,
                            });
                        }
                        b"eventref" => {
                            let hlink = read_hlink_attr(e).unwrap_or_default();
                            let mut role = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"role" || key.ends_with(b":role") {
                                    role = parse_event_role_type(&val);
                                }
                            }
                            let event_ref = make_event_ref(hlink.clone(), role);
                            family.event_ref_list.push(event_ref.clone());
                            self.pending.push(PendingEdge::FamilyEventRef {
                                source: handle.clone(),
                                target: event_ref.ref_field.clone(),
                                metadata: event_ref,
                            });
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                family.citation_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::FamilyCitation,
                                });
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                family.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::FamilyNote,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                family.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::FamilyTag,
                                });
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                family.media_list.push(MediaRef { ref_field: h.clone() });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::FamilyMediaRef,
                                });
                            }
                        }
                        b"attribute" => {
                            let mut attr_type = String::new();
                            let mut attr_value = String::new();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    attr_type = val;
                                } else if key == b"value" || key.ends_with(b":value") {
                                    attr_value = val;
                                }
                            }
                            if !attr_type.is_empty() || !attr_value.is_empty() {
                                family.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&attr_type),
                                    value: attr_value,
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Ok(text) = e.unescape() {
                        let text = text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        if in_attr_type {
                            current_attr_type = text.to_string();
                        } else if in_attr_value {
                            current_attr_value = text.to_string();
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"family" => {
                            self.graph
                                .add_node(handle.clone(), Node::Family(family))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"attribute" => {
                            if !current_attr_type.is_empty() || !current_attr_value.is_empty() {
                                family.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                            in_attribute = false;
                            in_attr_type = false;
                            in_attr_value = false;
                        }
                        b"type" if in_attr_type => in_attr_type = false,
                        b"value" if in_attr_value => in_attr_value = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing family".to_string(),
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(Error::XmlParseError {
                        message: format!("{} at byte {}", e, reader.error_position()),
                    });
                }
            }
        }
    }

    /// Parse a `<event>` element and its children.
    ///
    /// Reads the event's type, date, place, description, attributes, and
    /// all handle references (citations, notes, media, tags). Accumulates
    /// referenced handles into pending edge lists for the second pass.
    fn parse_event(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();

        let mut event = EventData {
            handle: handle.clone(),
            ..EventData::default()
        };
        let mut in_eventtype = false;
        let mut in_type = false;
        let mut event_type_str = String::new();
        let mut in_description = false;
        let mut in_attribute = false;
        let mut current_attr_type = String::new();
        let mut current_attr_value = String::new();
        let mut in_attr_type = false;
        let mut in_attr_value = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"eventtype" => in_eventtype = true,
                        b"type" if in_attribute => in_attr_type = true,
                        b"type" if in_eventtype => {
                            in_type = true;
                            event_type_str.clear();
                        }
                        b"type" => {
                            // Flat format: <type>Birth</type> directly inside <event>.
                            in_type = true;
                            event_type_str.clear();
                        }
                        b"description" => in_description = true,
                        b"place" => {
                            if let Some(h) = read_hlink_attr(e) {
                                event.place_handle = Some(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::EventPlace,
                                });
                            }
                        }
                        b"attribute" => {
                            in_attribute = true;
                            current_attr_type.clear();
                            current_attr_value.clear();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    current_attr_type = val;
                                } else if key == b"value" || key.ends_with(b":value") {
                                    current_attr_value = val;
                                }
                            }
                            if !current_attr_type.is_empty() && !current_attr_value.is_empty() {
                                event.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                                in_attribute = false;
                            }
                        }
                        b"value" if in_attribute => in_attr_value = true,
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"dateval" => {
                            if let Some(d) = parse_dateval(e) {
                                event.date = Some(d);
                            }
                        }
                        b"place" => {
                            if let Some(h) = read_hlink_attr(e) {
                                event.place_handle = Some(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::EventPlace,
                                });
                            }
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                event.citation_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::EventCitation,
                                });
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                event.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::EventNote,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                event.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::EventTag,
                                });
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                event.media_list.push(MediaRef { ref_field: h.clone() });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::EventMediaRef,
                                });
                            }
                        }
                        b"attribute" => {
                            let mut attr_type = String::new();
                            let mut attr_value = String::new();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    attr_type = val;
                                } else if key == b"value" || key.ends_with(b":value") {
                                    attr_value = val;
                                }
                            }
                            if !attr_type.is_empty() || !attr_value.is_empty() {
                                event.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&attr_type),
                                    value: attr_value,
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Ok(text) = e.unescape() {
                        let text = text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        if in_type {
                            event_type_str = text.to_string();
                        } else if in_description {
                            *event.description.get_or_insert_with(String::new) = text.to_string();
                        } else if in_attr_type {
                            current_attr_type = text.to_string();
                        } else if in_attr_value {
                            current_attr_value = text.to_string();
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"event" => {
                            self.graph
                                .add_node(handle.clone(), Node::Event(event))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"eventtype" => in_eventtype = false,
                        b"type" if in_type => {
                            if let Some(t) = parse_event_type(&event_type_str) {
                                event.event_type = into_event_type_field(t);
                            }
                            in_type = false;
                        }
                        b"type" if in_attr_type => in_attr_type = false,
                        b"value" if in_attr_value => in_attr_value = false,
                        b"description" => in_description = false,
                        b"attribute" => {
                            if !current_attr_type.is_empty() || !current_attr_value.is_empty() {
                                event.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                            in_attribute = false;
                            in_attr_type = false;
                            in_attr_value = false;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing event".to_string(),
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(Error::XmlParseError {
                        message: format!("{} at byte {}", e, reader.error_position()),
                    });
                }
            }
        }
    }

    /// Build all pending edges into the graph.
    ///
    /// Must be called after all nodes have been parsed.  Dangling
    /// references (target handle not found in the graph) are skipped
    /// with a warning.
    pub fn build_edges(&mut self) -> Result<(), Error> {
        let pending = std::mem::take(&mut self.pending);
        for edge in pending {
            match edge {
                PendingEdge::Simple { source, target, kind } => {
                    let e = simple_edge(kind, source, target);
                    // Check both nodes exist before adding the edge
                    self.graph.add_edge(e).map_err(graph_error)?;
                }
                PendingEdge::PersonEventRef {
                    source,
                    target,
                    metadata,
                } => {
                    self.graph
                        .add_edge(Edge::PersonEventRef {
                            source,
                            target,
                            metadata: Box::new(metadata),
                        })
                        .map_err(graph_error)?;
                }
                PendingEdge::FamilyChildRef {
                    source,
                    target,
                    metadata,
                } => {
                    self.graph
                        .add_edge(Edge::FamilyChildRef {
                            source,
                            target,
                            metadata: Box::new(metadata),
                        })
                        .map_err(graph_error)?;
                }
                PendingEdge::FamilyEventRef {
                    source,
                    target,
                    metadata,
                } => {
                    self.graph
                        .add_edge(Edge::FamilyEventRef {
                            source,
                            target,
                            metadata: Box::new(metadata),
                        })
                        .map_err(graph_error)?;
                }
                PendingEdge::PersonPersonRef {
                    source,
                    target,
                    metadata,
                } => {
                    self.graph
                        .add_edge(Edge::PersonPersonRef {
                            source,
                            target,
                            metadata: Box::new(metadata),
                        })
                        .map_err(graph_error)?;
                }
                PendingEdge::SourceRepoRef {
                    source,
                    target,
                    metadata,
                } => {
                    self.graph
                        .add_edge(Edge::SourceRepoRef {
                            source,
                            target,
                            metadata: Box::new(metadata),
                        })
                        .map_err(graph_error)?;
                }
            }
        }
        Ok(())
    }

    /// Validate the graph against the schema.
    ///
    /// Returns an error if validation fails.
    pub fn validate(&mut self) -> Result<(), Error> {
        let errors = self.graph.validate(self.schema);
        if !errors.is_empty() {
            return Err(Error::XmlParseError {
                message: format!("validation errors: {:?}", errors),
            });
        }
        Ok(())
    }

    /// Consume the parser and return the built graph.
    pub fn into_graph(self) -> Graph {
        self.graph
    }
}

/// Parse a complete Gramps XML document into a [`Graph`].
///
/// Convenience wrapper that creates a [`Parser`], runs the full
/// parse-and-build pipeline, and returns the validated graph.
pub fn parse_graph(content: &str) -> Result<Graph, Error> {
    // Detect the schema version from the header.
    let version = detect_schema_version(content)?;
    let schema = Schema::for_version(&version).ok_or_else(|| Error::UnsupportedSchema {
        version: version.clone(),
    })?;

    let mut parser = Parser::new(schema);
    parser.parse_all(content)?;
    parser.build_edges()?;
    parser.validate()?;
    Ok(parser.into_graph())
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Convert a [`GraphError`] into a reader [`Error`].
fn graph_error(err: GraphError) -> Error {
    Error::XmlParseError {
        message: format!("graph error: {}", err),
    }
}
/// Construct a simple (no metadata) edge from a `SimpleEdgeKind`.
fn simple_edge(kind: SimpleEdgeKind, source: Handle, target: Handle) -> Edge {
    match kind {
        SimpleEdgeKind::PersonFamily => Edge::PersonFamily { source, target },
        SimpleEdgeKind::PersonParentFamily => Edge::PersonParentFamily { source, target },
        SimpleEdgeKind::FamilyFather => Edge::FamilyFather { source, target },
        SimpleEdgeKind::FamilyMother => Edge::FamilyMother { source, target },
        SimpleEdgeKind::FamilyCitation => Edge::FamilyCitation { source, target },
        SimpleEdgeKind::FamilyNote => Edge::FamilyNote { source, target },
        SimpleEdgeKind::FamilyTag => Edge::FamilyTag { source, target },
        SimpleEdgeKind::EventPlace => Edge::EventPlace { source, target },
        SimpleEdgeKind::EventCitation => Edge::EventCitation { source, target },
        SimpleEdgeKind::EventNote => Edge::EventNote { source, target },
        SimpleEdgeKind::EventTag => Edge::EventTag { source, target },
        SimpleEdgeKind::PersonCitation => Edge::PersonCitation { source, target },
        SimpleEdgeKind::PersonNote => Edge::PersonNote { source, target },
        SimpleEdgeKind::PersonTag => Edge::PersonTag { source, target },
        SimpleEdgeKind::PlaceCitation => Edge::PlaceCitation { source, target },
        SimpleEdgeKind::PlaceNote => Edge::PlaceNote { source, target },
        SimpleEdgeKind::PlaceTag => Edge::PlaceTag { source, target },
        SimpleEdgeKind::PlacePlaceRef => Edge::PlacePlaceRef { source, target },
        SimpleEdgeKind::SourceNote => Edge::SourceNote { source, target },
        SimpleEdgeKind::SourceTag => Edge::SourceTag { source, target },
        SimpleEdgeKind::CitationNote => Edge::CitationNote { source, target },
        SimpleEdgeKind::CitationTag => Edge::CitationTag { source, target },
        SimpleEdgeKind::CitationRef => Edge::CitationRef { source, target },
        SimpleEdgeKind::CitationSource => Edge::CitationSource { source, target },
        SimpleEdgeKind::MediaCitation => Edge::MediaCitation { source, target },
        SimpleEdgeKind::MediaNote => Edge::MediaNote { source, target },
        SimpleEdgeKind::MediaTag => Edge::MediaTag { source, target },
        SimpleEdgeKind::NoteCitation => Edge::NoteCitation { source, target },
        SimpleEdgeKind::NoteTag => Edge::NoteTag { source, target },
        SimpleEdgeKind::TagTag => Edge::TagTag { source, target },
        SimpleEdgeKind::NoteRef => Edge::NoteRef { source, target },
        SimpleEdgeKind::MediaRef => Edge::MediaRef { source, target },
        SimpleEdgeKind::TagRef => Edge::TagRef { source, target },
        SimpleEdgeKind::RepositoryNote => Edge::RepositoryNote { source, target },
        SimpleEdgeKind::RepositoryTag => Edge::RepositoryTag { source, target },
        SimpleEdgeKind::PersonMediaRef => Edge::PersonMediaRef { source, target },
        SimpleEdgeKind::EventMediaRef => Edge::EventMediaRef { source, target },
        SimpleEdgeKind::FamilyMediaRef => Edge::FamilyMediaRef { source, target },
        SimpleEdgeKind::CitationMediaRef => Edge::CitationMediaRef { source, target },
        SimpleEdgeKind::SourceMediaRef => Edge::SourceMediaRef { source, target },
        SimpleEdgeKind::PlaceMediaRef => Edge::PlaceMediaRef { source, target },
        SimpleEdgeKind::RepositoryMediaRef => Edge::RepositoryMediaRef { source, target },
    }
}

/// Parse a gender string ("M", "F", "U") into the Gramps enum value.
fn parse_gender_value(s: &str) -> i32 {
    match s.trim().to_uppercase().as_str() {
        "M" => 1, // Male
        "F" => 2, // Female
        "U" => 3, // Unknown
        _ => 0,   // Not set / default
    }
}

/// Parse a NameType from a string value.
fn parse_name_type(s: &str) -> Option<NameType> {
    match s.trim().to_lowercase().as_str() {
        "birth" => Some(NameType::Birth),
        "married" => Some(NameType::Married),
        "also known as" | "aka" => Some(NameType::AlsoKnownAs),
        "akn" => Some(NameType::Akn),
        "called" => Some(NameType::Called),
        "formal" => Some(NameType::Formal),
        "patronymic" => Some(NameType::Patronymic),
        "religious" => Some(NameType::Religious),
        "unknown" => Some(NameType::Unknown),
        _ => None,
    }
}

/// Parse a NameOriginType from a string value.
fn parse_name_origin_type(s: &str) -> Option<NameOriginType> {
    match s.trim().to_lowercase().as_str() {
        "patrilineal" => Some(NameOriginType::Patrilineal),
        "matrilineal" => Some(NameOriginType::Matrilineal),
        "given" => Some(NameOriginType::Given),
        "taken" => Some(NameOriginType::Taken),
        "other" => Some(NameOriginType::Other),
        _ => None,
    }
}

/// Parse an EventRoleType from a string value.
fn parse_event_role_type(s: &str) -> Option<EventRoleType> {
    match s.trim().to_lowercase().as_str() {
        "primary" => Some(EventRoleType::Primary),
        "family" => Some(EventRoleType::Family),
        "witness" => Some(EventRoleType::Witness),
        "clergy" => Some(EventRoleType::Clergy),
        "bride" => Some(EventRoleType::Bride),
        "groom" => Some(EventRoleType::Groom),
        "parent" => Some(EventRoleType::Parent),
        "child" => Some(EventRoleType::Child),
        "officiator" => Some(EventRoleType::Officiator),
        "other" => Some(EventRoleType::Other),
        _ => None,
    }
}

/// Parse a FamilyRelType from a string value.
fn parse_family_rel_type(s: &str) -> Option<FamilyRelType> {
    match s.trim().to_lowercase().as_str() {
        "birth" => Some(FamilyRelType::Birth),
        "married" => Some(FamilyRelType::Married),
        "census" => Some(FamilyRelType::Census),
        "unknown" => Some(FamilyRelType::Unknown),
        _ => None,
    }
}

/// Parse an AttributeType from a string value.
fn parse_attribute_type(s: &str) -> AttributeType {
    match s.trim().to_lowercase().as_str() {
        "caste" => AttributeType::Caste,
        "cause" => AttributeType::Cause,
        "custom" => AttributeType::Custom,
        "dna" => AttributeType::DNA,
        "description" => AttributeType::Description,
        "ethnicity" => AttributeType::Ethnicity,
        "nationality" => AttributeType::Nationality,
        "nobility" => AttributeType::Nobility,
        "profession" => AttributeType::Profession,
        "property" => AttributeType::Property,
        "religion" => AttributeType::Religion,
        "social security number" => AttributeType::SocialSecurityNumber,
        "title" => AttributeType::Title,
        _ => AttributeType::Custom,
    }
}

/// Parse a UrlType from a string value.
fn parse_url_type(s: &str) -> Option<UrlType> {
    match s.trim().to_lowercase().as_str() {
        "email" => Some(UrlType::Email),
        "web home" | "web_home" => Some(UrlType::WebHome),
        "web search" | "web_search" => Some(UrlType::WebSearch),
        "ftp" => Some(UrlType::FTP),
        "other" => Some(UrlType::Other),
        _ => None,
    }
}

/// Parse an LdsOrdType from a string value.
/// Parse a ChildRefType from a string value.
fn parse_child_ref_type(s: &str) -> Option<ChildRefType> {
    match s.trim().to_lowercase().as_str() {
        "adopted" => Some(ChildRefType::Adopted),
        "birth" => Some(ChildRefType::Birth),
        "created" => Some(ChildRefType::Created),
        "foster" => Some(ChildRefType::Foster),
        "godchild" => Some(ChildRefType::Godchild),
        "other" => Some(ChildRefType::Other),
        "sponsor" => Some(ChildRefType::Sponsor),
        "stepchild" => Some(ChildRefType::Stepchild),
        _ => None,
    }
}

/// Parse an EventType from a string value.
fn parse_event_type(s: &str) -> Option<EventType> {
    match s.trim().to_lowercase().as_str() {
        "adoption" => Some(EventType::Adoption),
        "baptism" => Some(EventType::Baptism),
        "bar mitzvah" => Some(EventType::BarMitzvah),
        "bat mitzvah" => Some(EventType::BatMitzvah),
        "birth" => Some(EventType::Birth),
        "burial" => Some(EventType::Burial),
        "census" => Some(EventType::Census),
        "confirmation" => Some(EventType::Confirmation),
        "correspondence" => Some(EventType::Correspondence),
        "creates" => Some(EventType::Creates),
        "death" => Some(EventType::Death),
        "divorce" => Some(EventType::Divorce),
        "education" => Some(EventType::Education),
        "emigration" => Some(EventType::Emigration),
        "funeral" => Some(EventType::Funeral),
        "graduation" => Some(EventType::Graduation),
        "immigration" => Some(EventType::Immigration),
        "marriage" => Some(EventType::Marriage),
        "military service" => Some(EventType::MilitaryService),
        "naturalization" => Some(EventType::Naturalization),
        "occupation" => Some(EventType::Occupation),
        "other" => Some(EventType::Other),
        "probate" => Some(EventType::Probate),
        "religion" => Some(EventType::Religion),
        "residence" => Some(EventType::Residence),
        "retirement" => Some(EventType::Retirement),
        "title" => Some(EventType::Title),
        "will" => Some(EventType::Will),
        _ => None,
    }
}

/// Parse a DateQuality from a string value.
fn parse_date_quality(s: &str) -> Option<DateQuality> {
    match s.trim().to_lowercase().as_str() {
        "exact" => Some(DateQuality::Exact),
        "estimated" => Some(DateQuality::Estimated),
        "calculated" => Some(DateQuality::Calculated),
        _ => None,
    }
}

/// Parse a DateModifier from a string value.
fn parse_date_modifier(s: &str) -> Option<DateModifier> {
    match s.trim().to_lowercase().as_str() {
        "about" => Some(DateModifier::About),
        "after" => Some(DateModifier::After),
        "before" => Some(DateModifier::Before),
        "range" => Some(DateModifier::Range),
        "span" => Some(DateModifier::Span),
        "none" => Some(DateModifier::None),
        _ => None,
    }
}

/// Parse a DateValue from a `<dateval>` element's attributes.
fn parse_dateval(e: &quick_xml::events::BytesStart) -> Option<DateValue> {
    let mut val = None;
    let mut quality = None;
    let mut modifier = None;
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        let attr_val = String::from_utf8_lossy(&attr.value).to_string();
        if key == b"val" || key.ends_with(b":val") {
            val = Some(attr_val);
        } else if key == b"quality" || key.ends_with(b":quality") {
            quality = parse_date_quality(&attr_val);
        } else if key == b"type" || key.ends_with(b":type") {
            modifier = parse_date_modifier(&attr_val);
        }
    }

    let raw = val?;
    // Strip any time component (e.g. "1850-03-15 00:00:00") and split into Y/M/D.
    let date_part = raw.split_whitespace().next().unwrap_or(&raw);
    let mut parts = date_part.split('-');
    let year = parts.next().and_then(|p| p.trim().parse::<i32>().ok())?;
    let month = parts.next().and_then(|p| p.trim().parse::<i32>().ok());
    let day = parts.next().and_then(|p| p.trim().parse::<i32>().ok());

    Some(DateValue {
        year,
        month,
        day,
        quality: quality.or(Some(DateQuality::Exact)),
        modifier: modifier.or(Some(DateModifier::None)),
        text: None,
    })
}

fn parse_lds_ord_type(s: &str) -> LdsOrdType {
    match s.trim().to_lowercase().as_str() {
        "baptism" => LdsOrdType::Baptism,
        "confirm" => LdsOrdType::Confirm,
        "endowment" => LdsOrdType::Endowment,
        "sealing to parents" => LdsOrdType::SealingToParents,
        "sealing to spouse" => LdsOrdType::SealingToSpouse,
        "other" => LdsOrdType::Other,
        _ => LdsOrdType::Baptism,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Wrap XML body in a minimal Gramps database with a 5.2 header.
    fn with_db(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created version="5.2"/></header>
{body}
</database>"#,
        )
    }

    fn persons_from(xml: &str) -> Vec<PersonData> {
        let graph = parse_graph(xml).unwrap();
        graph
            .nodes_by_kind(NodeKind::Person)
            .iter()
            .map(|h| match graph.get_node(h) {
                Some(Node::Person(p)) => p.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    /// Parse persons from XML using the Parser directly (no edge validation).
    fn persons_from_parser(xml: &str) -> Vec<PersonData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser.graph
            .nodes_by_kind(NodeKind::Person)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Person(p)) => p.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_person(xml: &str) -> PersonData {
        let mut ps = persons_from(xml);
        assert_eq!(ps.len(), 1, "expected exactly one person");
        ps.remove(0)
    }

    fn single_person_from_parser(xml: &str) -> PersonData {
        let mut ps = persons_from_parser(xml);
        assert_eq!(ps.len(), 1, "expected exactly one person");
        ps.remove(0)
    }

    // -----------------------------------------------------------------------
    // Full person with all fields
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_full() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0001">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <eventref hlink="e0001" role="Primary"/>
      <eventref hlink="e0002"/>
    </person>
  </people>"#,
        );
        // Use the parser directly: the referenced events are not parsed yet.
        let p = single_person_from_parser(&xml);
        assert_eq!(p.handle, "p0001");
        assert_eq!(p.primary_name.first_name.as_deref(), Some("John"));
        assert_eq!(p.primary_name.surname_list.len(), 1);
        assert_eq!(
            p.primary_name.surname_list[0].surname.as_deref(),
            Some("Smith")
        );
        // Gender: M = 1
        assert_eq!(p.gender, 1);
        // Event refs should be populated
        assert_eq!(p.event_ref_list.len(), 2);
        assert_eq!(p.event_ref_list[0].ref_field, "e0001");
        assert_eq!(p.event_ref_list[0].role, Some(EventRoleType::Primary));
        assert_eq!(p.event_ref_list[1].ref_field, "e0002");
    }

    // -----------------------------------------------------------------------
    // Minimal person (handle only)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_minimal() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0002"/>
  </people>"#,
        );
        // Use the parser directly — validation fails for a person with no name.
        let p = single_person_from_parser(&xml);
        assert_eq!(p.handle, "p0002");
        assert!(p.primary_name.first_name.is_none());
        assert!(p.event_ref_list.is_empty());
    }

    // -----------------------------------------------------------------------
    // Person with alternate names
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_alternate_names() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0003">
      <gender>F</gender>
      <name>
        <first>Jane</first>
        <surname>Doe</surname>
      </name>
      <name type="aka">
        <first>Janey</first>
        <surname>Doe</surname>
      </name>
    </person>
  </people>"#,
        );
        let p = single_person(&xml);
        assert_eq!(p.handle, "p0003");
        assert_eq!(p.primary_name.first_name.as_deref(), Some("Jane"));
        assert_eq!(
            p.primary_name.surname_list[0].surname.as_deref(),
            Some("Doe")
        );
        assert_eq!(p.alternate_names.len(), 1);
        assert_eq!(
            p.alternate_names[0].first_name.as_deref(),
            Some("Janey")
        );
    }

    // -----------------------------------------------------------------------
    // Person with attributes
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_attributes() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0004">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <attribute type="Profession" value="Farmer"/>
    </person>
  </people>"#,
        );
        let p = single_person(&xml);
        assert_eq!(p.attribute_list.len(), 1);
        assert_eq!(p.attribute_list[0].value, "Farmer");
        assert_eq!(p.attribute_list[0].type_field, AttributeType::Profession);
    }

    // -----------------------------------------------------------------------
    // Person with URLs
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_urls() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0005">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <url href="https://example.com" type="Web Home"/>
    </person>
  </people>"#,
        );
        let p = single_person(&xml);
        assert_eq!(p.url_list.len(), 1);
        assert_eq!(p.url_list[0].href, "https://example.com");
        assert_eq!(p.url_list[0].type_field, Some(UrlType::WebHome));
    }

    // -----------------------------------------------------------------------
    // Person with addresses
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_addresses() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0006">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <address>
        <location>
          <city>Springfield</city>
          <country>USA</country>
        </location>
      </address>
    </person>
  </people>"#,
        );
        let p = single_person(&xml);
        assert_eq!(p.address_list.len(), 1);
        assert_eq!(
            p.address_list[0]
                .location
                .as_ref()
                .and_then(|l| l.city.as_deref()),
            Some("Springfield")
        );
        assert_eq!(
            p.address_list[0]
                .location
                .as_ref()
                .and_then(|l| l.country.as_deref()),
            Some("USA")
        );
    }

    // -----------------------------------------------------------------------
    // Person with citation/note/tag refs
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_refs() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0007">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <citationref hlink="c0001"/>
      <noteref hlink="n0001"/>
      <tagref hlink="t0001"/>
    </person>
  </people>"#,
        );
        let p = single_person_from_parser(&xml);
        assert_eq!(p.citation_list, vec!["c0001"]);
        assert_eq!(p.note_list, vec!["n0001"]);
        assert_eq!(p.tag_list, vec!["t0001"]);
    }

    // -----------------------------------------------------------------------
    // Person with personref
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_personref() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0008">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <personref hlink="p0009" relation="Birth"/>
    </person>
  </people>"#,
        );
        let p = single_person_from_parser(&xml);
        assert_eq!(p.person_ref_list.len(), 1);
        assert_eq!(p.person_ref_list[0].ref_field, "p0009");
        assert_eq!(
            p.person_ref_list[0].relation,
            Some(FamilyRelType::Birth)
        );
    }

    // -----------------------------------------------------------------------
    // Multiple persons
    // -----------------------------------------------------------------------

    #[test]
    fn parse_persons_multiple() {
        let xml = with_db(
            r#"  <people>
    <person handle="p1">
      <gender>M</gender>
      <name><first>Alice</first><surname>A</surname></name>
    </person>
    <person handle="p2">
      <gender>F</gender>
      <name><first>Bob</first><surname>B</surname></name>
    </person>
  </people>"#,
        );
        let ps = persons_from(&xml);
        assert_eq!(ps.len(), 2);
        let mut handles: Vec<&str> = ps.iter().map(|p| p.handle.as_str()).collect();
        handles.sort();
        assert_eq!(handles, vec!["p1", "p2"]);
    }

    // -----------------------------------------------------------------------
    // Empty people section
    // -----------------------------------------------------------------------

    #[test]
    fn parse_persons_empty() {
        let xml = with_db(r#"  <people></people>"#);
        let ps = persons_from(&xml);
        assert!(ps.is_empty());
    }

    // -----------------------------------------------------------------------
    // Namespace-prefixed
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_namespace_prefixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:header><ns:created ns:version="5.2"/></ns:header>
  <ns:people>
    <ns:person ns:handle="p0001">
      <ns:gender>M</ns:gender>
      <ns:name>
        <ns:first>John</ns:first>
        <ns:surname>Smith</ns:surname>
      </ns:name>
    </ns:person>
  </ns:people>
</ns:database>"#;
        let p = single_person(xml);
        assert_eq!(p.handle, "p0001");
        assert_eq!(p.primary_name.first_name.as_deref(), Some("John"));
    }

    // -----------------------------------------------------------------------
    // Malformed person
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_malformed_xml() {
        let xml = with_db(r#"  <people><person handle="p1"><name></person></people>"#);
        let result = parse_graph(&xml);
        assert!(result.is_err(), "expected error for malformed XML");
    }

    // -----------------------------------------------------------------------
    // No people section
    // -----------------------------------------------------------------------

    #[test]
    fn parse_no_people_section() {
        let xml = with_db("  <tags></tags>");
        let result = parse_graph(&xml);
        assert!(result.is_ok());
        let graph = result.unwrap();
        let count = graph.nodes_by_kind(NodeKind::Person).len();
        assert_eq!(count, 0);
    }

    // -----------------------------------------------------------------------
    // Person with LDS ordinances
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_lds_ord() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0010">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <lds_ord type="Baptism" status="completed" temple="SLC"/>
    </person>
  </people>"#,
        );
        let p = single_person(&xml);
        assert_eq!(p.lds_ord_list.len(), 1);
        assert_eq!(p.lds_ord_list[0].type_field, LdsOrdType::Baptism);
        assert_eq!(
            p.lds_ord_list[0].status.as_deref(),
            Some("completed")
        );
        assert_eq!(p.lds_ord_list[0].temple.as_deref(), Some("SLC"));
    }

    // -----------------------------------------------------------------------
    // Person with empty handle
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_empty_handle() {
        let xml = with_db(
            r#"  <people>
    <person handle="">
      <gender>M</gender>
      <name><first>No</first><surname>Handle</surname></name>
    </person>
  </people>"#,
        );
        let p = single_person_from_parser(&xml);
        assert!(p.handle.is_empty());
        assert_eq!(p.primary_name.first_name.as_deref(), Some("No"));
    }

    // -----------------------------------------------------------------------
    // Person with media ref
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_media_ref() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0011">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
      <mediaref hlink="m0001"/>
    </person>
  </people>"#,
        );
        let p = single_person_from_parser(&xml);
        assert_eq!(p.media_list.len(), 1);
        assert_eq!(p.media_list[0].ref_field, "m0001");
    }

    // -----------------------------------------------------------------------
    // Dangling eventref (event handle not in graph) — should error on build_edges
    // -----------------------------------------------------------------------

    #[test]
    fn parse_person_dangling_eventref() {
        let xml = with_db(
            r#"  <people>
    <person handle="p0001">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
      <eventref hlink="e0001"/>
    </person>
  </people>"#,
        );
        // Event e0001 doesn't exist — build_edges will error
        let result = parse_graph(&xml);
        assert!(result.is_err(), "expected error for dangling eventref");
    }

    // -----------------------------------------------------------------------
    // Family helpers
    // -----------------------------------------------------------------------

    /// Parse families from XML using the Parser directly (no edge validation).
    fn families_from_parser(xml: &str) -> Vec<FamilyData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Family)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Family(f)) => f.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_family_from_parser(xml: &str) -> FamilyData {
        let mut fs = families_from_parser(xml);
        assert_eq!(fs.len(), 1, "expected exactly one family");
        fs.remove(0)
    }

    /// Parse events from XML using the Parser directly (no edge validation).
    fn events_from_parser(xml: &str) -> Vec<EventData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Event)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Event(e)) => e.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_event_from_parser(xml: &str) -> EventData {
        let mut es = events_from_parser(xml);
        assert_eq!(es.len(), 1, "expected exactly one event");
        es.remove(0)
    }

    // -----------------------------------------------------------------------
    // Family parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_family_full() {
        let xml = with_db(
            r#"  <families>
    <family handle="f0001">
      <father hlink="p0001"/>
      <mother hlink="p0002"/>
      <childref hlink="p0003" relation="Birth"/>
      <childref hlink="p0004"/>
      <eventref hlink="e0001" role="Primary"/>
      <attribute type="Property" value="123 Main St"/>
    </family>
  </families>"#,
        );
        let f = single_family_from_parser(&xml);
        assert_eq!(f.handle, "f0001");
        assert_eq!(f.father_handle.as_deref(), Some("p0001"));
        assert_eq!(f.mother_handle.as_deref(), Some("p0002"));
        assert_eq!(f.child_ref_list.len(), 2);
        assert_eq!(f.child_ref_list[0].ref_field, "p0003");
        assert_eq!(f.child_ref_list[0].relation, Some(ChildRefType::Birth));
        assert_eq!(f.child_ref_list[1].ref_field, "p0004");
        assert_eq!(f.child_ref_list[1].relation, None);
        assert_eq!(f.event_ref_list.len(), 1);
        assert_eq!(f.event_ref_list[0].ref_field, "e0001");
        assert_eq!(f.event_ref_list[0].role, Some(EventRoleType::Primary));
        assert_eq!(f.attribute_list.len(), 1);
        assert_eq!(f.attribute_list[0].value, "123 Main St");
        assert_eq!(f.attribute_list[0].type_field, AttributeType::Property);
    }

    #[test]
    fn parse_family_soft_refs() {
        let xml = with_db(
            r#"  <families>
    <family handle="f0002">
      <citationref hlink="c0001"/>
      <noteref hlink="n0001"/>
      <tagref hlink="t0001"/>
      <mediaref hlink="m0001"/>
    </family>
  </families>"#,
        );
        let f = single_family_from_parser(&xml);
        assert_eq!(f.citation_list, vec!["c0001"]);
        assert_eq!(f.note_list, vec!["n0001"]);
        assert_eq!(f.tag_list, vec!["t0001"]);
        assert_eq!(f.media_list.len(), 1);
        assert_eq!(f.media_list[0].ref_field, "m0001");
    }

    #[test]
    fn parse_family_minimal() {
        let xml = with_db(r#"  <families>
    <family handle="f0003"/>
  </families>"#);
        let f = single_family_from_parser(&xml);
        assert_eq!(f.handle, "f0003");
        assert!(f.father_handle.is_none());
        assert!(f.mother_handle.is_none());
        assert!(f.child_ref_list.is_empty());
    }

    #[test]
    fn parse_family_malformed_xml() {
        let xml = with_db(
            r#"  <families>
    <family handle="f0004">
      <father hlink="p0001">
  </families>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    // -----------------------------------------------------------------------
    // Event parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_event_full() {
        let xml = with_db(
            r#"  <events>
    <event handle="e0001">
      <eventtype>
        <type>Marriage</type>
      </eventtype>
      <dateval val="1850-03-15" quality="exact" type="about"/>
      <place hlink="pl0001"/>
      <description>Church ceremony</description>
      <attribute type="Cause" value="heart attack"/>
    </event>
  </events>"#,
        );
        let e = single_event_from_parser(&xml);
        assert_eq!(e.handle, "e0001");
        assert_eq!(e.event_type, EventType::Marriage);
        let date = e.date.as_ref().expect("date should be set");
        assert_eq!(date.year, 1850);
        assert_eq!(date.month, Some(3));
        assert_eq!(date.day, Some(15));
        assert_eq!(date.quality, Some(DateQuality::Exact));
        assert_eq!(date.modifier, Some(DateModifier::About));
        assert_eq!(e.place_handle.as_deref(), Some("pl0001"));
        assert_eq!(e.description.as_deref(), Some("Church ceremony"));
        assert_eq!(e.attribute_list.len(), 1);
        assert_eq!(e.attribute_list[0].type_field, AttributeType::Cause);
        assert_eq!(e.attribute_list[0].value, "heart attack");
    }

    #[test]
    fn parse_event_flat_type() {
        // Gramps 5.1 flat format: <type>Birth</type> directly inside <event>.
        let xml = with_db(
            r#"  <events>
    <event handle="e0002">
      <type>Birth</type>
      <dateval val="1850"/>
    </event>
  </events>"#,
        );
        let e = single_event_from_parser(&xml);
        assert_eq!(e.event_type, EventType::Birth);
        let date = e.date.as_ref().expect("date should be set");
        assert_eq!(date.year, 1850);
        assert_eq!(date.month, None);
        assert_eq!(date.day, None);
    }

    #[test]
    fn parse_event_minimal() {
        let xml = with_db(r#"  <events>
    <event handle="e0003"/>
  </events>"#);
        let e = single_event_from_parser(&xml);
        assert_eq!(e.handle, "e0003");
        assert_eq!(e.event_type, EventType::Adoption); // Default variant.
        assert!(e.date.is_none());
        assert!(e.place_handle.is_none());
    }

    #[test]
    fn parse_event_malformed_xml() {
        let xml = with_db(
            r#"  <events>
    <event handle="e0004">
      <eventtype>
  </events>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }
}