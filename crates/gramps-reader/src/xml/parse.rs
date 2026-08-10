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
use crate::xml::{read_handle_attr, read_hlink_attr, read_id_attr, strip_prefix};
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
    PersonFamily,
    PersonParentFamily,
    FamilyFather,
    FamilyMother,
    FamilyCitation,
    FamilyNote,
    FamilyTag,
    EventPlace,
    EventCitation,
    EventNote,
    EventTag,
    PersonCitation,
    PersonNote,
    PersonTag,
    PlaceCitation,
    PlaceNote,
    PlaceTag,
    PlacePlaceRef,
    SourceNote,
    SourceTag,
    CitationNote,
    CitationTag,
    CitationRef,
    CitationSource,
    MediaCitation,
    MediaNote,
    MediaTag,
    NoteCitation,
    NoteTag,
    RepositoryNote,
    RepositoryTag,
    TagTag,
    NoteRef,
    MediaRef,
    TagRef,
    PersonMediaRef,
    EventMediaRef,
    FamilyMediaRef,
    CitationMediaRef,
    SourceMediaRef,
    PlaceMediaRef,
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
            TopLevel,
            Header,
            Tags,
            Events,
            People,
            Families,
            Citations,
            Sources,
            Places,
            Objects,
            Repositories,
            Notes,
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
                            Section::Places if name == b"place" => {
                                self.parse_place(&mut reader, e)?;
                                Section::Places
                            }
                            Section::Sources if name == b"source" => {
                                self.parse_source(&mut reader, e)?;
                                Section::Sources
                            }
                            Section::Citations if name == b"citation" => {
                                self.parse_citation(&mut reader, e)?;
                                Section::Citations
                            }
                            Section::Repositories if name == b"repository" => {
                                self.parse_repository(&mut reader, e)?;
                                Section::Repositories
                            }
                            Section::Objects if name == b"object" => {
                                self.parse_media(&mut reader, e)?;
                                Section::Objects
                            }
                            Section::Notes if name == b"note" => {
                                self.parse_note(&mut reader, e)?;
                                Section::Notes
                            }
                            Section::Tags if name == b"tag" => {
                                self.parse_tag(&mut reader, e)?;
                                Section::Tags
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
                        | b"citations" | b"sources" | b"places" | b"objects" | b"repositories"
                        | b"notes" => {
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
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Person(PersonData {
                                    gramps_id,
                                    handle,
                                    ..PersonData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"family" && matches!(section, Section::Families) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Family(FamilyData {
                                    gramps_id,
                                    handle,
                                    ..FamilyData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"event" && matches!(section, Section::Events) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Event(EventData {
                                    gramps_id,
                                    handle,
                                    ..EventData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"place" && matches!(section, Section::Places) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Place(PlaceData {
                                    gramps_id,
                                    handle,
                                    ..PlaceData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"source" && matches!(section, Section::Sources) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Source(SourceData {
                                    gramps_id,
                                    handle,
                                    ..SourceData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"citation" && matches!(section, Section::Citations) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Citation(CitationData {
                                    gramps_id,
                                    handle,
                                    ..CitationData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"repository" && matches!(section, Section::Repositories) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Repository(RepositoryData {
                                    gramps_id,
                                    handle,
                                    ..RepositoryData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"object" && matches!(section, Section::Objects) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Media(MediaData {
                                    gramps_id,
                                    handle,
                                    ..MediaData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"note" && matches!(section, Section::Notes) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Note(NoteData {
                                    gramps_id,
                                    handle,
                                    ..NoteData::default()
                                }),
                            )
                            .map_err(graph_error)?;
                    } else if name == b"tag" && matches!(section, Section::Tags) {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        self.graph
                            .add_node(
                                handle.clone(),
                                Node::Tag(TagData {
                                    gramps_id,
                                    handle,
                                    ..TagData::default()
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
        let gramps_id = read_id_attr(start);

        let mut person = PersonData {
            handle: handle.clone(),
            gramps_id,
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
        let mut in_postal = false;
        let mut in_locality = false;
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
                                    lds.type_field = Some(parse_lds_ord_type(&val));
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
                                    current_url.href = Some(val);
                                } else if key == b"type" || key.ends_with(b":type") {
                                    url_type = Some(val);
                                }
                            }
                        }
                        b"desc" if in_url => in_url_desc = true,
                        b"eventref" | b"citationref" | b"noteref" | b"tagref" | b"personref"
                        | b"mediaref" => {
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
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"relation" || key.ends_with(b":relation") {
                                    relation = parse_family_rel_type(&val);
                                }
                            }
                            let person_ref = PersonRef {
                                ref_field: hlink.clone(),
                                relation,
                                ..Default::default()
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
                                person.media_list.push(MediaRef {
                                    ref_field: h.clone(),
                                    ..Default::default()
                                });
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
                                    href: Some(href),
                                    type_field: url_type_val,
                                    desc: None,
                                    path: None,
                                });
                            }
                        }
                        b"lds_ord" => {
                            let mut lds = LdsOrd::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    lds.type_field = Some(parse_lds_ord_type(&val));
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
                            self.graph
                                .add_node(handle.clone(), Node::Person(person))
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
                                ..Default::default()
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
        let gramps_id = read_id_attr(start);

        let mut family = FamilyData {
            handle: handle.clone(),
            gramps_id,
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
                                family.media_list.push(MediaRef {
                                    ref_field: h.clone(),
                                    ..Default::default()
                                });
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
        let gramps_id = read_id_attr(start);

        let mut event = EventData {
            handle: handle.clone(),
            gramps_id,
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
                                event.media_list.push(MediaRef {
                                    ref_field: h.clone(),
                                    ..Default::default()
                                });
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

    /// Parse a `<place>` element and its children.
    ///
    /// Reads the place's name (stored in the `name.city` field of the
    /// `Location` struct), hierarchy parent references via `<placeref>`,
    /// and all handle references (citations, notes, media, tags,
    /// attributes). Accumulates referenced handles into pending edge
    /// lists for the second pass.
    fn parse_place(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();
        let gramps_id = read_id_attr(start);

        let mut place = PlaceData {
            handle: handle.clone(),
            gramps_id,
            ..PlaceData::default()
        };
        let mut current_attr_type = String::new();
        let mut current_attr_value = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"name" => {
                            // <name value="..." type="..." lang="..."/>
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"value" || key.ends_with(b":value") {
                                    place.name.city = Some(val);
                                }
                            }
                        }
                        b"attribute" => {
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
                                place.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                                current_attr_type.clear();
                                current_attr_value.clear();
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"name" => {
                            // Self-closing <name value="..."/>
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"value" || key.ends_with(b":value") {
                                    place.name.city = Some(val);
                                }
                            }
                        }
                        b"placeref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                place.place_ref_list.push(PlaceRef {
                                    ref_field: h.clone(),
                                    date: None,
                                });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PlacePlaceRef,
                                });
                            }
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                place.citation_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PlaceCitation,
                                });
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                place.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PlaceNote,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                place.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PlaceTag,
                                });
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                place.media_list.push(MediaRef {
                                    ref_field: h.clone(),
                                    ..Default::default()
                                });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::PlaceMediaRef,
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
                                place.attribute_list.push(Attribute {
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
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"place" => {
                            self.graph
                                .add_node(handle.clone(), Node::Place(place))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"name" => {}
                        b"attribute" => {
                            if !current_attr_type.is_empty() || !current_attr_value.is_empty() {
                                place.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                            current_attr_type.clear();
                            current_attr_value.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing place".to_string(),
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

    /// Parse a `<source>` element and its children.
    ///
    /// Reads the source's title, author, publication info, repository
    /// references, and all handle references (notes, media, tags,
    /// attributes). Accumulates referenced handles into pending edge
    /// lists for the second pass.
    fn parse_source(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();
        let gramps_id = read_id_attr(start);

        let mut source = SourceData {
            handle: handle.clone(),
            gramps_id,
            ..SourceData::default()
        };
        let mut in_title = false;
        let mut in_author = false;
        let mut in_pubinfo = false;
        let mut current_attr_type = String::new();
        let mut current_attr_value = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"title" => in_title = true,
                        b"author" => in_author = true,
                        b"pubinfo" => in_pubinfo = true,
                        b"attribute" => {
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
                                source.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                                current_attr_type.clear();
                                current_attr_value.clear();
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"reporef" => {
                            let hlink = read_hlink_attr(e).unwrap_or_default();
                            let mut call_number = None;
                            let mut media_type = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"callnumber" || key.ends_with(b":callnumber") {
                                    call_number = Some(val);
                                } else if key == b"mediatype" || key.ends_with(b":mediatype") {
                                    media_type = parse_source_media_type(&val);
                                }
                            }
                            let repo_ref = RepoRef {
                                call_number,
                                media_type,
                                ref_field: hlink.clone(),
                                note_list: vec![],
                            };
                            source.reporef_list.push(repo_ref.clone());
                            self.pending.push(PendingEdge::SourceRepoRef {
                                source: handle.clone(),
                                target: hlink,
                                metadata: repo_ref,
                            });
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                source.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::SourceNote,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                source.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::SourceTag,
                                });
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                source.media_list.push(MediaRef {
                                    ref_field: h.clone(),
                                    ..Default::default()
                                });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::SourceMediaRef,
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
                                source.attribute_list.push(Attribute {
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
                        if in_title {
                            source.title = text.to_string();
                        } else if in_author {
                            source.author = Some(text.to_string());
                        } else if in_pubinfo {
                            source.pubinfo = Some(text.to_string());
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"source" => {
                            self.graph
                                .add_node(handle.clone(), Node::Source(source))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"title" => in_title = false,
                        b"author" => in_author = false,
                        b"pubinfo" => in_pubinfo = false,
                        b"attribute" => {
                            if !current_attr_type.is_empty() || !current_attr_value.is_empty() {
                                source.attribute_list.push(Attribute {
                                    type_field: parse_attribute_type(&current_attr_type),
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                            }
                            current_attr_type.clear();
                            current_attr_value.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing source".to_string(),
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

    /// Parse a `<citation>` element and its children.
    ///
    /// Reads the citation's source reference, page, confidence, and all
    /// handle references (notes, media, tags). Accumulates referenced
    /// handles into pending edge lists for the second pass.
    fn parse_citation(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();
        let gramps_id = read_id_attr(start);

        let mut citation = CitationData {
            handle: handle.clone(),
            gramps_id,
            ..CitationData::default()
        };
        let mut in_page = false;
        let mut in_confidence = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"page" => in_page = true,
                        b"confidence" => in_confidence = true,
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"sourceref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                citation.source_handle = into_source_handle_field(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::CitationSource,
                                });
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                citation.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::CitationNote,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                citation.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::CitationTag,
                                });
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                citation.media_list.push(MediaRef {
                                    ref_field: h.clone(),
                                    ..Default::default()
                                });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::CitationMediaRef,
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
                        if in_page {
                            citation.page = Some(text.to_string());
                        } else if in_confidence {
                            citation.confidence = text.parse::<i32>().ok();
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"citation" => {
                            self.graph
                                .add_node(handle.clone(), Node::Citation(citation))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"page" => in_page = false,
                        b"confidence" => in_confidence = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing citation".to_string(),
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

    /// Parse a `<repository>` element and its children.
    ///
    /// Reads the repository's name, type, addresses, URLs, and all
    /// handle references (notes, media, tags). Accumulates referenced
    /// handles into pending edge lists for the second pass.
    fn parse_repository(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();
        let gramps_id = read_id_attr(start);

        let mut repo = RepositoryData {
            handle: handle.clone(),
            gramps_id,
            ..RepositoryData::default()
        };
        let mut in_name = false;
        let mut in_type = false;
        let mut in_address = false;
        let mut in_address_location = false;
        let mut current_location = Location::default();
        let mut in_city = false;
        let mut in_country = false;
        let mut in_county = false;
        let mut in_state = false;
        let mut in_street = false;
        let mut in_postal = false;
        let mut in_locality = false;
        let mut in_phone = false;
        let mut in_url = false;
        let mut current_url = Url::default();
        let mut in_url_desc = false;
        let mut url_type = None;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"name" => in_name = true,
                        b"type" => in_type = true,
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
                                    current_url.href = Some(val);
                                } else if key == b"type" || key.ends_with(b":type") {
                                    url_type = Some(val);
                                }
                            }
                        }
                        b"desc" if in_url => in_url_desc = true,
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                repo.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::RepositoryNote,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                repo.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::RepositoryTag,
                                });
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                repo.media_list.push(MediaRef {
                                    ref_field: h.clone(),
                                    ..Default::default()
                                });
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::RepositoryMediaRef,
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
                                repo.url_list.push(Url {
                                    href: Some(href),
                                    type_field: url_type_val,
                                    desc: None,
                                    path: None,
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
                        if in_name {
                            repo.name = Some(text.to_string());
                        } else if in_type {
                            repo.type_field = parse_repository_type(text);
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
                        } else if in_url_desc {
                            current_url.desc = Some(text.to_string());
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"repository" => {
                            self.graph
                                .add_node(handle.clone(), Node::Repository(repo))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"name" => in_name = false,
                        b"type" => in_type = false,
                        b"address" => {
                            repo.address_list.push(Address {
                                location: Some(current_location.clone()),
                                ..Default::default()
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
                            repo.url_list.push(current_url.clone());
                            in_url = false;
                            in_url_desc = false;
                        }
                        b"desc" => in_url_desc = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing repository".to_string(),
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

    /// Parse a `<object>` element (Gramps media object) and its children.
    ///
    /// Reads the object's handle, file metadata (src, mime), description,
    /// checksum, attributes, and all handle references (noterefs, citationrefs,
    /// tagrefs).
    fn parse_media(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();
        let gramps_id = read_id_attr(start);

        let mut media = MediaData {
            handle: handle.clone(),
            gramps_id,
            ..MediaData::default()
        };
        let mut in_desc = false;
        let mut in_checksum = false;
        let mut in_attribute = false;
        let mut current_attr_type = AttributeType::Custom;
        let mut current_attr_value = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"description" => in_desc = true,
                        b"checksum" => in_checksum = true,
                        b"attribute" => {
                            in_attribute = true;
                            current_attr_type = AttributeType::Custom;
                            current_attr_value = String::new();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    current_attr_type = parse_attribute_type(&val);
                                } else if key == b"value" || key.ends_with(b":value") {
                                    current_attr_value = val;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"file" => {
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"src" || key.ends_with(b":src") {
                                    media.path = Some(val);
                                } else if key == b"mime" || key.ends_with(b":mime") {
                                    media.mime_type = Some(val);
                                }
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                media.note_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::MediaNote,
                                });
                            }
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                media.citation_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::MediaCitation,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                media.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::MediaTag,
                                });
                            }
                        }
                        b"attribute" => {
                            let mut attr_type = AttributeType::Custom;
                            let mut attr_value = String::new();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == b"type" || key.ends_with(b":type") {
                                    attr_type = parse_attribute_type(&val);
                                } else if key == b"value" || key.ends_with(b":value") {
                                    attr_value = val;
                                }
                            }
                            media.attribute_list.push(Attribute {
                                type_field: attr_type,
                                value: attr_value,
                                citation_list: vec![],
                                note_list: vec![],
                            });
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
                        if in_desc {
                            media.desc = Some(text.to_string());
                        } else if in_checksum {
                            media.checksum = Some(text.to_string());
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"object" => {
                            self.graph
                                .add_node(handle.clone(), Node::Media(media))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"description" => in_desc = false,
                        b"checksum" => in_checksum = false,
                        b"attribute" => {
                            if in_attribute {
                                media.attribute_list.push(Attribute {
                                    type_field: current_attr_type,
                                    value: current_attr_value.clone(),
                                    citation_list: vec![],
                                    note_list: vec![],
                                });
                                in_attribute = false;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing media object".to_string(),
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

    /// Parse a `<note>` element and its children.
    ///
    /// Reads the note's handle, text content, format, type, and all handle
    /// references (noterefs, citationrefs, tagrefs).
    fn parse_note(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();
        let gramps_id = read_id_attr(start);

        let mut note = NoteData {
            handle: handle.clone(),
            gramps_id,
            ..NoteData::default()
        };
        let mut in_text = false;
        let mut in_format = false;
        let mut in_type = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"text" => in_text = true,
                        b"format" => in_format = true,
                        b"type" => in_type = true,
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                note.citation_list.push(h.clone());
                                // Note->Note references use the NoteRef (deduplicated) edge
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::NoteRef,
                                });
                            }
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                note.citation_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::NoteCitation,
                                });
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                note.tag_list.push(h.clone());
                                self.pending.push(PendingEdge::Simple {
                                    source: handle.clone(),
                                    target: h,
                                    kind: SimpleEdgeKind::NoteTag,
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
                        if in_text {
                            note.text = text.to_string();
                        } else if in_format {
                            if let Ok(f) = text.parse::<i32>() {
                                note.format = Some(f);
                            }
                        } else if in_type {
                            note.type_field = parse_note_type(text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"note" => {
                            self.graph
                                .add_node(handle.clone(), Node::Note(note))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"text" => in_text = false,
                        b"format" => in_format = false,
                        b"type" => in_type = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing note".to_string(),
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

    /// Parse a `<tag>` element and its children.
    ///
    /// Reads the tag's handle, name, color, and priority.
    fn parse_tag(
        &mut self,
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart,
    ) -> Result<(), Error> {
        let handle = read_handle_attr(start).unwrap_or_default();
        let gramps_id = read_id_attr(start);

        let mut tag = TagData {
            handle: handle.clone(),
            gramps_id,
            ..TagData::default()
        };
        let mut in_name = false;
        let mut in_color = false;
        let mut in_priority = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"name" => in_name = true,
                        b"color" => in_color = true,
                        b"priority" => in_priority = true,
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    if name == b"tagref" {
                        if let Some(h) = read_hlink_attr(e) {
                            tag.tag_list.push(h.clone());
                            self.pending.push(PendingEdge::Simple {
                                source: handle.clone(),
                                target: h,
                                kind: SimpleEdgeKind::TagTag,
                            });
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Ok(text) = e.unescape() {
                        let text = text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        if in_name {
                            tag.name = text.to_string();
                        } else if in_color {
                            tag.color = Some(text.to_string());
                        } else if in_priority {
                            if let Ok(p) = text.parse::<i32>() {
                                tag.priority = Some(p);
                            }
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);
                    match name {
                        b"tag" => {
                            self.graph
                                .add_node(handle.clone(), Node::Tag(tag))
                                .map_err(graph_error)?;
                            return Ok(());
                        }
                        b"name" => in_name = false,
                        b"color" => in_color = false,
                        b"priority" => in_priority = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => {
                    return Err(Error::XmlParseError {
                        message: "unexpected end of file while parsing tag".to_string(),
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
            let target_kind = target_kind_for_edge(&edge);
            match edge {
                PendingEdge::Simple {
                    source,
                    target,
                    kind,
                } => {
                    ensure_target_exists(&mut self.graph, &target, target_kind)?;
                    let e = simple_edge(kind, source, target);
                    self.graph.add_edge(e).map_err(graph_error)?;
                }
                PendingEdge::PersonEventRef {
                    source,
                    target,
                    metadata,
                } => {
                    ensure_target_exists(&mut self.graph, &target, target_kind)?;
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
                    ensure_target_exists(&mut self.graph, &target, target_kind)?;
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
                    ensure_target_exists(&mut self.graph, &target, target_kind)?;
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
                    ensure_target_exists(&mut self.graph, &target, target_kind)?;
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
                    ensure_target_exists(&mut self.graph, &target, target_kind)?;
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
/// parse-and-build pipeline, and returns the graph.
///
/// # Validation
///
/// Structural and referential validation errors are logged as warnings
/// (via `log::warn!`) rather than returned as errors. This allows files
/// with dangling references or missing required fields — common in real
/// Gramps databases — to be parsed successfully for diff analysis.
///
/// Parse errors (malformed XML, unsupported schema, I/O errors) remain
/// fatal and are returned as [`Error`].
pub fn parse_graph(content: &str) -> Result<Graph, Error> {
    // Detect the schema version from the header.
    let version = detect_schema_version(content)?;
    let schema = Schema::for_version(&version).ok_or_else(|| Error::UnsupportedSchema {
        version: version.clone(),
        schema_version: version.clone(),
    })?;

    let mut parser = Parser::new(schema);
    parser.parse_all(content)?;
    parser.build_edges()?;
    // Validation errors are non-fatal: some are expected for placeholder
    // nodes (missing required fields). Log warnings so the user can see
    // integrity issues without blocking the diff.
    let validation_errors = parser.graph.validate(schema);
    for err in &validation_errors {
        log::warn!("validation warning: {}", err);
    }
    Ok(parser.into_graph())
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Convert a [`GraphError`] into a reader [`Error`].
/// Map a `PendingEdge` variant to the `NodeKind` of its target node.
///
/// The compiler enforces exhaustiveness: every variant of both
/// `SimpleEdgeKind` and `PendingEdge` must be covered.
fn target_kind_for_edge(edge: &PendingEdge) -> NodeKind {
    match edge {
        PendingEdge::Simple { kind, .. } => target_kind_for_simple(kind),
        PendingEdge::PersonEventRef { .. } => NodeKind::Event,
        PendingEdge::FamilyChildRef { .. } => NodeKind::Person,
        PendingEdge::FamilyEventRef { .. } => NodeKind::Event,
        PendingEdge::PersonPersonRef { .. } => NodeKind::Person,
        PendingEdge::SourceRepoRef { .. } => NodeKind::Repository,
    }
}

/// Map a `SimpleEdgeKind` to the `NodeKind` of its target node.
fn target_kind_for_simple(kind: &SimpleEdgeKind) -> NodeKind {
    match kind {
        SimpleEdgeKind::PersonFamily | SimpleEdgeKind::PersonParentFamily => NodeKind::Family,
        SimpleEdgeKind::FamilyFather | SimpleEdgeKind::FamilyMother => NodeKind::Person,
        SimpleEdgeKind::FamilyCitation
        | SimpleEdgeKind::EventCitation
        | SimpleEdgeKind::PersonCitation
        | SimpleEdgeKind::PlaceCitation
        | SimpleEdgeKind::MediaCitation
        | SimpleEdgeKind::NoteCitation
        | SimpleEdgeKind::CitationRef => NodeKind::Citation,
        SimpleEdgeKind::FamilyNote
        | SimpleEdgeKind::EventNote
        | SimpleEdgeKind::PersonNote
        | SimpleEdgeKind::PlaceNote
        | SimpleEdgeKind::SourceNote
        | SimpleEdgeKind::CitationNote
        | SimpleEdgeKind::MediaNote
        | SimpleEdgeKind::NoteRef
        | SimpleEdgeKind::RepositoryNote => NodeKind::Note,
        SimpleEdgeKind::FamilyTag
        | SimpleEdgeKind::EventTag
        | SimpleEdgeKind::PersonTag
        | SimpleEdgeKind::PlaceTag
        | SimpleEdgeKind::SourceTag
        | SimpleEdgeKind::CitationTag
        | SimpleEdgeKind::MediaTag
        | SimpleEdgeKind::NoteTag
        | SimpleEdgeKind::RepositoryTag
        | SimpleEdgeKind::TagTag
        | SimpleEdgeKind::TagRef => NodeKind::Tag,
        SimpleEdgeKind::EventPlace | SimpleEdgeKind::PlacePlaceRef => NodeKind::Place,
        SimpleEdgeKind::CitationSource => NodeKind::Source,
        SimpleEdgeKind::PersonMediaRef
        | SimpleEdgeKind::EventMediaRef
        | SimpleEdgeKind::FamilyMediaRef
        | SimpleEdgeKind::CitationMediaRef
        | SimpleEdgeKind::SourceMediaRef
        | SimpleEdgeKind::PlaceMediaRef
        | SimpleEdgeKind::RepositoryMediaRef
        | SimpleEdgeKind::MediaRef => NodeKind::Media,
    }
}

/// Create a minimal default-constructed node of the given kind,
/// with the handle field set to the supplied handle.
fn placeholder_node(kind: NodeKind, handle: &str) -> Node {
    let h = handle.to_string();
    match kind {
        NodeKind::Person => Node::Person(PersonData {
            handle: h,
            ..PersonData::default()
        }),
        NodeKind::Family => Node::Family(FamilyData {
            handle: h,
            ..FamilyData::default()
        }),
        NodeKind::Event => Node::Event(EventData {
            handle: h,
            ..EventData::default()
        }),
        NodeKind::Place => Node::Place(PlaceData {
            handle: h,
            ..PlaceData::default()
        }),
        NodeKind::Source => Node::Source(SourceData {
            handle: h,
            ..SourceData::default()
        }),
        NodeKind::Citation => Node::Citation(CitationData {
            handle: h,
            ..CitationData::default()
        }),
        NodeKind::Repository => Node::Repository(RepositoryData {
            handle: h,
            ..RepositoryData::default()
        }),
        NodeKind::Media => Node::Media(MediaData {
            handle: h,
            ..MediaData::default()
        }),
        NodeKind::Note => Node::Note(NoteData {
            handle: h,
            ..NoteData::default()
        }),
        NodeKind::Tag => Node::Tag(TagData {
            handle: h,
            ..TagData::default()
        }),
    }
}

/// Ensure the target node exists in the graph. If it does not, create a
/// placeholder node and record it as inferred. If a placeholder already
/// exists but the new edge expects a different kind, log a warning.
fn ensure_target_exists(graph: &mut Graph, target: &Handle, kind: NodeKind) -> Result<(), Error> {
    if graph.get_node(target).is_none() {
        let node = placeholder_node(kind, target);
        graph.add_node(target.clone(), node).map_err(graph_error)?;
        graph.record_inferred_handle(target.clone());
    } else if graph.is_inferred_handle(target) {
        let expected_kind = kind;
        let actual_kind = graph::node_kind(graph.get_node(target).unwrap());
        if expected_kind != actual_kind {
            log::warn!(
                "kind conflict for inferred handle '{}': edge expects {:?}, but {:?} placeholder already exists",
                target, expected_kind, actual_kind
            );
        }
    }
    Ok(())
}

fn graph_error(err: GraphError) -> Error {
    Error::XmlParseError {
        message: format!("graph error: {}", err),
    }
}
/// Construct a simple (no metadata) edge from a `SimpleEdgeKind`.
fn simple_edge(kind: SimpleEdgeKind, source: Handle, target: Handle) -> Edge {
    match kind {
        SimpleEdgeKind::PlacePlaceRef => {
            let meta_target = target.clone();
            Edge::PlacePlaceRef {
                source,
                target,
                metadata: Box::new(PlaceRef {
                    ref_field: meta_target,
                    date: None,
                }),
            }
        }
        SimpleEdgeKind::PersonMediaRef => {
            let meta_target = target.clone();
            Edge::PersonMediaRef {
                source,
                target,
                metadata: Box::new(MediaRef {
                    ref_field: meta_target,
                    ..Default::default()
                }),
            }
        }
        SimpleEdgeKind::EventMediaRef => {
            let meta_target = target.clone();
            Edge::EventMediaRef {
                source,
                target,
                metadata: Box::new(MediaRef {
                    ref_field: meta_target,
                    ..Default::default()
                }),
            }
        }
        SimpleEdgeKind::FamilyMediaRef => {
            let meta_target = target.clone();
            Edge::FamilyMediaRef {
                source,
                target,
                metadata: Box::new(MediaRef {
                    ref_field: meta_target,
                    ..Default::default()
                }),
            }
        }
        SimpleEdgeKind::CitationMediaRef => {
            let meta_target = target.clone();
            Edge::CitationMediaRef {
                source,
                target,
                metadata: Box::new(MediaRef {
                    ref_field: meta_target,
                    ..Default::default()
                }),
            }
        }
        SimpleEdgeKind::SourceMediaRef => {
            let meta_target = target.clone();
            Edge::SourceMediaRef {
                source,
                target,
                metadata: Box::new(MediaRef {
                    ref_field: meta_target,
                    ..Default::default()
                }),
            }
        }
        SimpleEdgeKind::PlaceMediaRef => {
            let meta_target = target.clone();
            Edge::PlaceMediaRef {
                source,
                target,
                metadata: Box::new(MediaRef {
                    ref_field: meta_target,
                    ..Default::default()
                }),
            }
        }
        SimpleEdgeKind::RepositoryMediaRef => {
            let meta_target = target.clone();
            Edge::RepositoryMediaRef {
                source,
                target,
                metadata: Box::new(MediaRef {
                    ref_field: meta_target,
                    ..Default::default()
                }),
            }
        }
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

/// Parse a RepositoryType from a string value.
fn parse_repository_type(s: &str) -> Option<RepositoryType> {
    match s.trim().to_lowercase().as_str() {
        "archive" => Some(RepositoryType::Archive),
        "cemetery" => Some(RepositoryType::Cemetery),
        "church" => Some(RepositoryType::Church),
        "court house" | "court_house" => Some(RepositoryType::CourtHouse),
        "historical society" | "historical_society" => Some(RepositoryType::HistoricalSociety),
        "library" => Some(RepositoryType::Library),
        "museum" => Some(RepositoryType::Museum),
        "other" => Some(RepositoryType::Other),
        _ => None,
    }
}

/// Parse a note type string losslessly.
///
/// Note types are stored as `String` in `NoteData` to preserve the exact
/// XML value from the input file (the `NoteType` enum variant names do not
/// match Gramps XML strings). Returns the trimmed raw string.
fn parse_note_type(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn parse_source_media_type(s: &str) -> Option<SourceMediaType> {
    match s.trim().to_lowercase().as_str() {
        "audio" => Some(SourceMediaType::Audio),
        "book" => Some(SourceMediaType::Book),
        "cd" => Some(SourceMediaType::CD),
        "card" => Some(SourceMediaType::Card),
        "electronic" => Some(SourceMediaType::Electronic),
        "fiche" => Some(SourceMediaType::Fiche),
        "film" => Some(SourceMediaType::Film),
        "magazine" => Some(SourceMediaType::Magazine),
        "manuscript" => Some(SourceMediaType::Manuscript),
        "map" => Some(SourceMediaType::Map),
        "newspaper" => Some(SourceMediaType::Newspaper),
        "other" => Some(SourceMediaType::Other),
        "photo" => Some(SourceMediaType::Photo),
        "tombstone" => Some(SourceMediaType::Tombstone),
        "video" => Some(SourceMediaType::Video),
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
        parser
            .graph
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
        assert_eq!(p.gender, Some(1));
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
        assert_eq!(p.alternate_names[0].first_name.as_deref(), Some("Janey"));
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
        assert_eq!(p.url_list[0].href.as_deref(), Some("https://example.com"));
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
        assert_eq!(p.person_ref_list[0].relation, Some(FamilyRelType::Birth));
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
        assert_eq!(p.lds_ord_list[0].type_field, Some(LdsOrdType::Baptism));
        assert_eq!(p.lds_ord_list[0].status.as_deref(), Some("completed"));
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
        // Event e0001 doesn't exist — placeholder Event node is created
        let graph = parse_graph(&xml).expect("should succeed with placeholder");
        assert!(
            graph.is_inferred_handle(&"e0001".to_string()),
            "e0001 should be inferred"
        );
        assert_eq!(graph.inferred_handle_count(), 1);
        // The person node still exists
        assert!(graph.contains_node(&"p0001".to_string()));
        // The edge should exist linking p0001 to the placeholder event
        let edges = graph.edges_from(&"p0001".to_string());
        assert!(
            !edges.is_empty(),
            "should have an edge to the placeholder event"
        );
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
        let xml = with_db(
            r#"  <families>
    <family handle="f0003"/>
  </families>"#,
        );
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
        assert_eq!(e.event_type, Some(EventType::Marriage));
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
        assert_eq!(e.event_type, Some(EventType::Birth));
        let date = e.date.as_ref().expect("date should be set");
        assert_eq!(date.year, 1850);
        assert_eq!(date.month, None);
        assert_eq!(date.day, None);
    }

    #[test]
    fn parse_event_minimal() {
        let xml = with_db(
            r#"  <events>
    <event handle="e0003"/>
  </events>"#,
        );
        let e = single_event_from_parser(&xml);
        assert_eq!(e.handle, "e0003");
        // When no event type is specified in XML, event_type is None.
        assert_eq!(e.event_type, None);
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

    // -----------------------------------------------------------------------
    // Place helpers
    // -----------------------------------------------------------------------

    /// Parse places from XML using the Parser directly (no edge validation).
    fn places_from_parser(xml: &str) -> Vec<PlaceData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Place)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Place(p)) => p.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_place_from_parser(xml: &str) -> PlaceData {
        let mut ps = places_from_parser(xml);
        assert_eq!(ps.len(), 1, "expected exactly one place");
        ps.remove(0)
    }

    // -----------------------------------------------------------------------
    // Place parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_place_full() {
        let xml = with_db(
            r#"  <places>
    <place handle="pl0001">
      <name value="Springfield"/>
      <placeref hlink="pl0000"/>
      <citationref hlink="c0001"/>
      <noteref hlink="n0001"/>
      <tagref hlink="t0001"/>
      <mediaref hlink="m0001"/>
      <attribute type="Description" value="county seat"/>
    </place>
  </places>"#,
        );
        let p = single_place_from_parser(&xml);
        assert_eq!(p.handle, "pl0001");
        assert_eq!(p.name.city.as_deref(), Some("Springfield"));
        assert_eq!(p.place_ref_list.len(), 1);
        assert_eq!(p.place_ref_list[0].ref_field, "pl0000");
        assert_eq!(p.citation_list, vec!["c0001"]);
        assert_eq!(p.note_list, vec!["n0001"]);
        assert_eq!(p.tag_list, vec!["t0001"]);
        assert_eq!(p.media_list.len(), 1);
        assert_eq!(p.media_list[0].ref_field, "m0001");
        assert_eq!(p.attribute_list.len(), 1);
        assert_eq!(p.attribute_list[0].value, "county seat");
        assert_eq!(p.attribute_list[0].type_field, AttributeType::Description);
    }

    #[test]
    fn parse_place_minimal() {
        let xml = with_db(
            r#"  <places>
    <place handle="pl0002"/>
  </places>"#,
        );
        let p = single_place_from_parser(&xml);
        assert_eq!(p.handle, "pl0002");
        assert!(p.name.city.is_none());
        assert!(p.place_ref_list.is_empty());
    }

    #[test]
    fn parse_place_malformed_xml() {
        let xml = with_db(
            r#"  <places>
    <place handle="pl0003">
      <name value="Broken">
  </places>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    // -----------------------------------------------------------------------
    // Source helpers
    // -----------------------------------------------------------------------

    /// Parse sources from XML using the Parser directly (no edge validation).
    fn sources_from_parser(xml: &str) -> Vec<SourceData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Source)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Source(s)) => s.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_source_from_parser(xml: &str) -> SourceData {
        let mut ss = sources_from_parser(xml);
        assert_eq!(ss.len(), 1, "expected exactly one source");
        ss.remove(0)
    }

    // -----------------------------------------------------------------------
    // Source parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_source_full() {
        let xml = with_db(
            r#"  <sources>
    <source handle="s0001">
      <title>Marriage Records of Springfield</title>
      <author>Jane Clerk</author>
      <pubinfo>City Hall, 1850</pubinfo>
      <reporef hlink="r0001" callnumber="AB-123" mediatype="Book"/>
      <noteref hlink="n0001"/>
      <tagref hlink="t0001"/>
      <mediaref hlink="m0001"/>
      <attribute type="Description" value="microfilm copy"/>
    </source>
  </sources>"#,
        );
        let s = single_source_from_parser(&xml);
        assert_eq!(s.handle, "s0001");
        assert_eq!(s.title, "Marriage Records of Springfield");
        assert_eq!(s.author.as_deref(), Some("Jane Clerk"));
        assert_eq!(s.pubinfo.as_deref(), Some("City Hall, 1850"));
        assert_eq!(s.reporef_list.len(), 1);
        assert_eq!(s.reporef_list[0].ref_field, "r0001");
        assert_eq!(s.reporef_list[0].call_number.as_deref(), Some("AB-123"));
        assert_eq!(s.reporef_list[0].media_type, Some(SourceMediaType::Book));
        assert_eq!(s.note_list, vec!["n0001"]);
        assert_eq!(s.tag_list, vec!["t0001"]);
        assert_eq!(s.media_list.len(), 1);
        assert_eq!(s.media_list[0].ref_field, "m0001");
        assert_eq!(s.attribute_list.len(), 1);
        assert_eq!(s.attribute_list[0].value, "microfilm copy");
    }

    #[test]
    fn parse_source_minimal() {
        let xml = with_db(
            r#"  <sources>
    <source handle="s0002"/>
  </sources>"#,
        );
        let s = single_source_from_parser(&xml);
        assert_eq!(s.handle, "s0002");
        assert!(s.title.is_empty());
        assert!(s.author.is_none());
        assert!(s.reporef_list.is_empty());
    }

    #[test]
    fn parse_source_malformed_xml() {
        let xml = with_db(
            r#"  <sources>
    <source handle="s0003">
      <title>Broken
  </sources>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    // -----------------------------------------------------------------------
    // Citation helpers
    // -----------------------------------------------------------------------

    /// Parse citations from XML using the Parser directly (no edge validation).
    fn citations_from_parser(xml: &str) -> Vec<CitationData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Citation)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Citation(c)) => c.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_citation_from_parser(xml: &str) -> CitationData {
        let mut cs = citations_from_parser(xml);
        assert_eq!(cs.len(), 1, "expected exactly one citation");
        cs.remove(0)
    }

    // -----------------------------------------------------------------------
    // Citation parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_citation_full() {
        let xml = with_db(
            r#"  <citations>
    <citation handle="c0001">
      <sourceref hlink="s0001"/>
      <page>p. 42</page>
      <confidence>2</confidence>
      <noteref hlink="n0001"/>
      <tagref hlink="t0001"/>
      <mediaref hlink="m0001"/>
    </citation>
  </citations>"#,
        );
        let c = single_citation_from_parser(&xml);
        assert_eq!(c.handle, "c0001");
        assert_eq!(c.source_handle.as_deref(), Some("s0001"));
        assert_eq!(c.page.as_deref(), Some("p. 42"));
        assert_eq!(c.confidence, Some(2));
        assert_eq!(c.note_list, vec!["n0001"]);
        assert_eq!(c.tag_list, vec!["t0001"]);
        assert_eq!(c.media_list.len(), 1);
        assert_eq!(c.media_list[0].ref_field, "m0001");
    }

    #[test]
    fn parse_citation_minimal() {
        let xml = with_db(
            r#"  <citations>
    <citation handle="c0002"/>
  </citations>"#,
        );
        let c = single_citation_from_parser(&xml);
        assert_eq!(c.handle, "c0002");
        assert!(c.source_handle.as_deref().unwrap_or("").is_empty());
        assert!(c.page.is_none());
        assert!(c.confidence.is_none());
    }

    #[test]
    fn parse_citation_malformed_xml() {
        let xml = with_db(
            r#"  <citations>
    <citation handle="c0003">
      <page>Broken
  </citations>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    // -----------------------------------------------------------------------
    // Repository helpers
    // -----------------------------------------------------------------------

    /// Parse repositories from XML using the Parser directly (no edge validation).
    fn repositories_from_parser(xml: &str) -> Vec<RepositoryData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Repository)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Repository(r)) => r.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_repository_from_parser(xml: &str) -> RepositoryData {
        let mut rs = repositories_from_parser(xml);
        assert_eq!(rs.len(), 1, "expected exactly one repository");
        rs.remove(0)
    }

    // -----------------------------------------------------------------------
    // Repository parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_repository_full() {
        let xml = with_db(
            r#"  <repositories>
    <repository handle="r0001">
      <name>Springfield Public Library</name>
      <type>Library</type>
      <address>
        <location>
          <city>Springfield</city>
          <country>USA</country>
        </location>
      </address>
      <url href="https://library.example.com" type="Web Home"/>
      <noteref hlink="n0001"/>
      <tagref hlink="t0001"/>
      <mediaref hlink="m0001"/>
    </repository>
  </repositories>"#,
        );
        let r = single_repository_from_parser(&xml);
        assert_eq!(r.handle, "r0001");
        assert_eq!(r.name.as_deref(), Some("Springfield Public Library"));
        assert_eq!(r.type_field, Some(RepositoryType::Library));
        assert_eq!(r.address_list.len(), 1);
        assert_eq!(
            r.address_list[0]
                .location
                .as_ref()
                .and_then(|l| l.city.as_deref()),
            Some("Springfield")
        );
        assert_eq!(
            r.address_list[0]
                .location
                .as_ref()
                .and_then(|l| l.country.as_deref()),
            Some("USA")
        );
        assert_eq!(r.url_list.len(), 1);
        assert_eq!(
            r.url_list[0].href.as_deref(),
            Some("https://library.example.com")
        );
        assert_eq!(r.url_list[0].type_field, Some(UrlType::WebHome));
        assert_eq!(r.note_list, vec!["n0001"]);
        assert_eq!(r.tag_list, vec!["t0001"]);
        assert_eq!(r.media_list.len(), 1);
        assert_eq!(r.media_list[0].ref_field, "m0001");
    }

    #[test]
    fn parse_repository_minimal() {
        let xml = with_db(
            r#"  <repositories>
    <repository handle="r0002"/>
  </repositories>"#,
        );
        let r = single_repository_from_parser(&xml);
        assert_eq!(r.handle, "r0002");
        assert!(r.name.is_none());
        assert!(r.type_field.is_none());
        assert!(r.address_list.is_empty());
        assert!(r.url_list.is_empty());
    }

    #[test]
    fn parse_repository_malformed_xml() {
        let xml = with_db(
            r#"  <repositories>
    <repository handle="r0003">
      <name>Broken
  </repositories>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    // -----------------------------------------------------------------------
    // Media (object) parsing helpers
    // -----------------------------------------------------------------------

    fn media_from_parser(xml: &str) -> Vec<MediaData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Media)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Media(m)) => m.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_media_from_parser(xml: &str) -> MediaData {
        let mut ms = media_from_parser(xml);
        assert_eq!(ms.len(), 1, "expected exactly one media object");
        ms.remove(0)
    }

    // -----------------------------------------------------------------------
    // Media (object) tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_media_full() {
        let xml = with_db(
            r#"  <objects>
    <object handle="m0001">
      <file src="/path/to/photo.jpg" mime="image/jpeg"/>
      <description>A family photo</description>
      <checksum>abc123</checksum>
      <attribute type="Description" value="Old photo"/>
      <noteref hlink="n0001"/>
      <citationref hlink="c0001"/>
      <tagref hlink="t0001"/>
    </object>
  </objects>"#,
        );
        let m = single_media_from_parser(&xml);
        assert_eq!(m.handle, "m0001");
        assert_eq!(m.path.as_deref(), Some("/path/to/photo.jpg"));
        assert_eq!(m.mime_type.as_deref(), Some("image/jpeg"));
        assert_eq!(m.desc.as_deref(), Some("A family photo"));
        assert_eq!(m.checksum.as_deref(), Some("abc123"));
        assert_eq!(m.attribute_list.len(), 1);
        assert_eq!(m.attribute_list[0].type_field, AttributeType::Description);
        assert_eq!(m.attribute_list[0].value, "Old photo");
        assert_eq!(m.note_list, vec!["n0001"]);
        assert_eq!(m.citation_list, vec!["c0001"]);
        assert_eq!(m.tag_list, vec!["t0001"]);
    }

    #[test]
    fn parse_media_minimal() {
        let xml = with_db(
            r#"  <objects>
    <object handle="m0002"/>
  </objects>"#,
        );
        let m = single_media_from_parser(&xml);
        assert_eq!(m.handle, "m0002");
        assert!(m.path.is_none());
        assert!(m.mime_type.is_none());
        assert!(m.desc.is_none());
    }

    #[test]
    fn parse_media_malformed_xml() {
        let xml = with_db(
            r#"  <objects>
    <object handle="m0003">
      <description>Broken
  </objects>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    // -----------------------------------------------------------------------
    // Note parsing helpers
    // -----------------------------------------------------------------------

    fn notes_from_parser(xml: &str) -> Vec<NoteData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Note)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Note(n)) => n.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_note_from_parser(xml: &str) -> NoteData {
        let mut ns = notes_from_parser(xml);
        assert_eq!(ns.len(), 1, "expected exactly one note");
        ns.remove(0)
    }

    // -----------------------------------------------------------------------
    // Note tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_note_full() {
        let xml = with_db(
            r#"  <notes>
    <note handle="n0001">
      <text>This is a note about the family.</text>
      <format>0</format>
      <type>General</type>
      <noteref hlink="n0002"/>
      <citationref hlink="c0001"/>
      <tagref hlink="t0001"/>
    </note>
  </notes>"#,
        );
        let n = single_note_from_parser(&xml);
        assert_eq!(n.handle, "n0001");
        assert_eq!(n.text, "This is a note about the family.");
        assert_eq!(n.format, Some(0));
        assert_eq!(n.type_field, Some("General".to_string()));
        // note_list is stored in citation_list for noteref
        assert_eq!(n.citation_list, vec!["n0002", "c0001"]);
        assert_eq!(n.tag_list, vec!["t0001"]);
    }

    #[test]
    fn parse_note_with_text_only() {
        let xml = with_db(
            r#"  <notes>
    <note handle="n0002">
      <text>A simple note</text>
    </note>
  </notes>"#,
        );
        let n = single_note_from_parser(&xml);
        assert_eq!(n.handle, "n0002");
        assert_eq!(n.text, "A simple note");
        assert!(n.format.is_none());
        assert!(n.type_field.is_none());
    }

    #[test]
    fn parse_note_malformed_xml() {
        let xml = with_db(
            r#"  <notes>
    <note handle="n0003">
      <text>Broken
  </notes>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    // -----------------------------------------------------------------------
    // Tag parsing helpers
    // -----------------------------------------------------------------------

    fn tags_from_parser(xml: &str) -> Vec<TagData> {
        let version = detect_schema_version(xml).unwrap();
        let schema = Schema::for_version(&version).unwrap();
        let mut parser = Parser::new(schema);
        parser.parse_all(xml).unwrap();
        parser
            .graph
            .nodes_by_kind(NodeKind::Tag)
            .iter()
            .map(|h| match parser.graph.get_node(h) {
                Some(Node::Tag(t)) => t.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn single_tag_from_parser(xml: &str) -> TagData {
        let mut ts = tags_from_parser(xml);
        assert_eq!(ts.len(), 1, "expected exactly one tag");
        ts.remove(0)
    }

    // -----------------------------------------------------------------------
    // Tag tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_tag_full() {
        let xml = with_db(
            r#"  <tags>
    <tag handle="t0001">
      <name>Favorite</name>
      <color>#ff0000</color>
      <priority>1</priority>
    </tag>
  </tags>"#,
        );
        let t = single_tag_from_parser(&xml);
        assert_eq!(t.handle, "t0001");
        assert_eq!(t.name, "Favorite");
        assert_eq!(t.color.as_deref(), Some("#ff0000"));
        assert_eq!(t.priority, Some(1));
    }

    #[test]
    fn parse_tag_minimal() {
        let xml = with_db(
            r#"  <tags>
    <tag handle="t0002">
      <name>Unfiled</name>
    </tag>
  </tags>"#,
        );
        let t = single_tag_from_parser(&xml);
        assert_eq!(t.handle, "t0002");
        assert_eq!(t.name, "Unfiled");
        assert!(t.color.is_none());
        assert!(t.priority.is_none());
    }

    #[test]
    fn parse_tag_malformed_xml() {
        let xml = with_db(
            r#"  <tags>
    <tag handle="t0003">
      <name>Broken
  </tags>"#,
        );
        let result = parse_graph(&xml);
        assert!(matches!(result, Err(Error::XmlParseError { .. })));
    }

    #[test]
    fn parse_tag_with_tagref() {
        let xml = with_db(
            r#"  <tags>
    <tag handle="t0004">
      <name>Parent</name>
      <tagref hlink="t0005"/>
    </tag>
  </tags>"#,
        );
        let t = single_tag_from_parser(&xml);
        assert_eq!(t.handle, "t0004");
        assert_eq!(t.name, "Parent");
        assert_eq!(t.tag_list, vec!["t0005"]);
    }
}
