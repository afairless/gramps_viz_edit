//! Full-graph XML parser — reads a complete Gramps XML document into a
//! [`typed_graph::Graph`] with all 10 primary types and all edge relationships.
//!
//! # Usage
//!
//! ```no_run
//! use gramps_reader::xml::graph::parse_gramps_xml;
//!
//! let content = std::fs::read_to_string("family.gramps").unwrap();
//! let (graph, namespace) = parse_gramps_xml(&content).unwrap();
//! assert!(graph.node_count() > 0);
//! ```
//!
//! # Graph population order
//!
//! 1. All 10 types of nodes are parsed first
//! 2. Handle-ref fields within data structs are added as edges
//! 3. Hlink element references are added as edges
//!
//! This ensures all nodes exist before any edge references them, avoiding
//! [`GraphError::MissingNode`] errors.

use std::collections::HashMap;

use crate::error::Error;
use crate::xml::header::detect_schema_version;
use crate::xml::{read_handle_attr, read_hlink_attr, read_id_attr, strip_prefix};
use typed_graph::graph::*;
use typed_graph::*;

/// Helper: extract source and target handles from an Edge.
fn edge_source_target_helper(edge: &Edge) -> (Handle, Handle) {
    use Edge::*;
    match edge {
        CitationMediaRef { source, target, .. }
        | CitationNote { source, target }
        | CitationSource { source, target }
        | CitationTag { source, target }
        | CitationRef { source, target }
        | NoteRef { source, target }
        | MediaRef { source, target }
        | TagRef { source, target }
        | EventCitation { source, target }
        | EventMediaRef { source, target, .. }
        | EventNote { source, target }
        | EventPlace { source, target }
        | EventTag { source, target }
        | FamilyCitation { source, target }
        | FamilyFather { source, target }
        | FamilyMediaRef { source, target, .. }
        | FamilyMother { source, target }
        | FamilyNote { source, target }
        | FamilyTag { source, target }
        | MediaCitation { source, target }
        | MediaNote { source, target }
        | MediaTag { source, target }
        | NoteCitation { source, target }
        | NoteTag { source, target }
        | PersonCitation { source, target }
        | PersonFamily { source, target }
        | PersonMediaRef { source, target, .. }
        | PersonNote { source, target }
        | PersonParentFamily { source, target }
        | PersonTag { source, target }
        | PlaceCitation { source, target }
        | PlaceMediaRef { source, target, .. }
        | PlaceNote { source, target }
        | PlacePlaceRef { source, target, .. }
        | PlaceTag { source, target }
        | RepositoryMediaRef { source, target, .. }
        | RepositoryNote { source, target }
        | RepositoryTag { source, target }
        | SourceMediaRef { source, target, .. }
        | SourceNote { source, target }
        | SourceTag { source, target }
        | TagTag { source, target } => (source.clone(), target.clone()),
        FamilyChildRef { source, target, .. }
        | FamilyEventRef { source, target, .. }
        | PersonEventRef { source, target, .. }
        | PersonPersonRef { source, target, .. }
        | SourceRepoRef { source, target, .. } => (source.clone(), target.clone()),
    }
}

/// Parse a Gramps XML document string into a [`Graph`].
///
/// Returns the populated `Graph` and the XML namespace URI captured from
/// the `<database>` root element.
///
/// Handles both Gramps 5.1 (flat `<type>`) and 5.2 (nested `<eventtype>`)
/// formats transparently. Gzip-compressed input must be decompressed before
/// calling this function — use [`crate::io::read_gramps_file`] for that.
pub fn parse_gramps_xml(content: &str) -> Result<(Graph, String), Error> {
    // Detect schema version from header (validates it is compiled in).
    let _schema_version = detect_schema_version(content)?;

    // Parse the XML content into intermediate data structures.
    let mut parser = GraphParser::new();
    parser.parse(content)?;

    // Capture namespace before build moves the parser.
    let namespace = parser.namespace.clone();

    // Build the graph — add all nodes first, then edges.
    let graph = parser.build().map_err(|e| Error::XmlParseError {
        message: format!("graph build error: {}", e),
    })?;

    Ok((graph, namespace))
}

// ---------------------------------------------------------------------------
// Intermediate parser state
// ---------------------------------------------------------------------------

/// Intermediate data collected during XML parsing.
struct GraphParser {
    /// XML namespace from the `<database>` root element.
    namespace: String,
    /// Collected nodes by handle.
    nodes: HashMap<Handle, Node>,
    /// Collected edges (handle-ref and hlink-based).
    edges: Vec<Edge>,
    /// Handles that were inferred from hlink references to non-existent nodes.
    inferred_handles: Vec<Handle>,
    /// Per-parser state machine fields.
    state: ParserState,
}

/// Tracks which section and element we are currently inside.
struct ParserState {
    in_header: bool,
    in_section: bool,
    current_section: Section,
}

/// Enum for the major XML sections.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Section {
    None,
    People,
    Families,
    Events,
    Places,
    Sources,
    Citations,
    Repositories,
    Media,
    Notes,
    Tags,
}

impl Default for ParserState {
    fn default() -> Self {
        ParserState {
            in_header: false,
            in_section: false,
            current_section: Section::None,
        }
    }
}

impl GraphParser {
    fn new() -> Self {
        GraphParser {
            namespace: String::new(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            inferred_handles: Vec::new(),
            state: ParserState::default(),
        }
    }

    /// Parse the XML content, populating intermediate data.
    fn parse(&mut self, content: &str) -> Result<(), Error> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(content);
        reader.config_mut().trim_text(true);

        let mut current_person: Option<PersonBuilder> = None;
        let mut current_family: Option<FamilyBuilder> = None;
        let mut current_event: Option<EventBuilder> = None;
        let mut current_place: Option<PlaceBuilder> = None;
        let mut current_source: Option<SourceBuilder> = None;
        let mut current_citation: Option<CitationBuilder> = None;
        let mut current_repository: Option<RepositoryBuilder> = None;
        let mut current_media: Option<MediaBuilder> = None;
        let mut current_note: Option<NoteBuilder> = None;
        let mut current_tag: Option<TagBuilder> = None;

        // Context flags for nested elements.
        let mut in_eventtype = false;
        let mut in_name = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);

                    match name {
                        b"database" => {
                            // Capture xmlns attribute
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"xmlns" {
                                    self.namespace =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        }
                        b"header" => self.state.in_header = true,
                        b"people" => {
                            self.state.current_section = Section::People;
                            self.state.in_section = true;
                        }
                        b"families" => {
                            self.state.current_section = Section::Families;
                            self.state.in_section = true;
                        }
                        b"events" => {
                            self.state.current_section = Section::Events;
                            self.state.in_section = true;
                        }
                        b"places" => {
                            self.state.current_section = Section::Places;
                            self.state.in_section = true;
                        }
                        b"sources" => {
                            self.state.current_section = Section::Sources;
                            self.state.in_section = true;
                        }
                        b"citations" => {
                            self.state.current_section = Section::Citations;
                            self.state.in_section = true;
                        }
                        b"repositories" => {
                            self.state.current_section = Section::Repositories;
                            self.state.in_section = true;
                        }
                        b"objects" | b"media" => {
                            self.state.current_section = Section::Media;
                            self.state.in_section = true;
                        }
                        b"notes" => {
                            self.state.current_section = Section::Notes;
                            self.state.in_section = true;
                        }
                        b"tags" => {
                            self.state.current_section = Section::Tags;
                            self.state.in_section = true;
                        }
                        b"person" if self.state.current_section == Section::People => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_person = Some(PersonBuilder {
                                handle,
                                gramps_id,
                                ..PersonBuilder::default()
                            });
                        }
                        b"family" if self.state.current_section == Section::Families => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_family = Some(FamilyBuilder {
                                handle,
                                gramps_id,
                                ..FamilyBuilder::default()
                            });
                        }
                        b"event" if self.state.current_section == Section::Events => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_event = Some(EventBuilder {
                                handle,
                                gramps_id,
                                ..EventBuilder::default()
                            });
                        }
                        b"place" if self.state.current_section == Section::Places => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_place = Some(PlaceBuilder {
                                handle,
                                gramps_id,
                                ..PlaceBuilder::default()
                            });
                        }
                        b"source" if self.state.current_section == Section::Sources => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_source = Some(SourceBuilder {
                                handle,
                                gramps_id,
                                ..SourceBuilder::default()
                            });
                        }
                        b"citation" if self.state.current_section == Section::Citations => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_citation = Some(CitationBuilder {
                                handle,
                                gramps_id,
                                ..CitationBuilder::default()
                            });
                        }
                        b"repository" if self.state.current_section == Section::Repositories => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_repository = Some(RepositoryBuilder {
                                handle,
                                gramps_id,
                                ..RepositoryBuilder::default()
                            });
                        }
                        b"object" if self.state.current_section == Section::Media => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_media = Some(MediaBuilder {
                                handle,
                                gramps_id,
                                ..MediaBuilder::default()
                            });
                        }
                        b"note" if self.state.current_section == Section::Notes => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_note = Some(NoteBuilder {
                                handle,
                                gramps_id,
                                ..NoteBuilder::default()
                            });
                        }
                        b"tag" if self.state.current_section == Section::Tags => {
                            let handle = read_handle_attr(e).unwrap_or_default();
                            let gramps_id = read_id_attr(e);
                            current_tag = Some(TagBuilder {
                                handle,
                                gramps_id,
                                ..TagBuilder::default()
                            });
                        }
                        b"eventtype" if current_event.is_some() => {
                            in_eventtype = true;
                        }
                        b"gender" if current_person.is_some() => {
                            // Read gender text directly.
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Ok(val) = text.parse::<i32>() {
                                        if let Some(ref mut p) = current_person {
                                            p.gender = Some(val);
                                        }
                                    }
                                }
                            }
                        }
                        b"type" if in_eventtype || current_event.is_some() => {
                            // Read type text directly — works for both nested
                            // <eventtype><type>Birth</type></eventtype> (5.2)
                            // and flat <type>Birth</type> (5.1) formats.
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut ev) = current_event {
                                        ev.event_type_text = text;
                                    }
                                }
                            }
                        }
                        b"name" if current_person.is_some() => {
                            in_name = true;
                        }
                        b"first" if in_name => {
                            // Read given name text directly.
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_person {
                                        p.given_name = text;
                                    }
                                }
                            }
                        }
                        b"surname" if in_name => {
                            // Read surname text directly.
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_person {
                                        p.surname = text;
                                    }
                                }
                            }
                        }
                        b"title" if current_source.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut s) = current_source {
                                        s.title = text;
                                    } else if let Some(ref mut p) = current_place {
                                        p.title = Some(text);
                                    }
                                }
                            }
                        }
                        b"abbrev" if current_source.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut s) = current_source {
                                        s.abbrev = Some(text);
                                    }
                                }
                            }
                        }
                        b"author" if current_source.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut s) = current_source {
                                        s.author = Some(text);
                                    }
                                }
                            }
                        }
                        b"pubinfo" if current_source.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut s) = current_source {
                                        s.pubinfo = Some(text);
                                    }
                                }
                            }
                        }
                        b"page" if current_citation.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut c) = current_citation {
                                        c.page = Some(text);
                                    }
                                }
                            }
                        }
                        b"sourceref" if current_citation.is_some() => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref mut c) = current_citation {
                                    self.edges.push(Edge::CitationSource {
                                        source: c.handle.clone(),
                                        target: h.clone(),
                                    });
                                    c.source_handle = Some(h);
                                }
                            }
                        }
                        b"place" if current_event.is_some() => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref mut ev) = current_event {
                                    self.edges.push(Edge::EventPlace {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                    ev.place_handle = Some(h);
                                }
                            }
                        }
                        b"description" if current_event.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut ev) = current_event {
                                        ev.description = Some(text);
                                    }
                                }
                            }
                        }
                        b"desc" if current_media.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut m) = current_media {
                                        m.desc = Some(text);
                                    }
                                }
                            }
                        }
                        b"path" if current_media.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut m) = current_media {
                                        m.path = Some(text);
                                    }
                                }
                            }
                        }
                        b"mime" if current_media.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut m) = current_media {
                                        m.mime = Some(text);
                                    }
                                }
                            }
                        }
                        b"checksum" if current_media.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut m) = current_media {
                                        m.checksum = Some(text);
                                    }
                                }
                            }
                        }
                        b"text" if current_note.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut n) = current_note {
                                        n.text = text;
                                    }
                                }
                            }
                        }
                        b"color" if current_tag.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut t) = current_tag {
                                        t.color = Some(text);
                                    }
                                }
                            }
                        }
                        b"code" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.code = Some(text);
                                    }
                                }
                            }
                        }
                        b"lat" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.lat = Some(text);
                                    }
                                }
                            }
                        }
                        b"long" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.long = Some(text);
                                    }
                                }
                            }
                        }
                        b"rname" if current_repository.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut r) = current_repository {
                                        r.name = Some(text);
                                    }
                                }
                            }
                        }
                        b"street" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.street = Some(text);
                                    }
                                }
                            }
                        }
                        b"locality" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.locality = Some(text);
                                    }
                                }
                            }
                        }
                        b"city" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.city = Some(text);
                                    }
                                }
                            }
                        }
                        b"county" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.county = Some(text);
                                    }
                                }
                            }
                        }
                        b"state" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.state = Some(text);
                                    }
                                }
                            }
                        }
                        b"country" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.country = Some(text);
                                    }
                                }
                            }
                        }
                        b"postal" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.postal = Some(text);
                                    }
                                }
                            }
                        }
                        b"phone" if current_place.is_some() => {
                            let name_q = e.name().to_owned();
                            if let Ok(text) = reader.read_text(name_q) {
                                let text = text.trim().to_string();
                                if !text.is_empty() {
                                    if let Some(ref mut p) = current_place {
                                        p.name.phone = Some(text);
                                    }
                                }
                            }
                        }
                        b"placeref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_place {
                                    self.edges.push(edge_place_place_ref(p.handle.clone(), h));
                                }
                            }
                        }
                        b"eventref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                let role = None;
                                if let Some(ref p) = current_person {
                                    let event_ref = make_event_ref(h.clone(), role);
                                    self.edges.push(Edge::PersonEventRef {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(event_ref),
                                    });
                                } else if let Some(ref f) = current_family {
                                    let event_ref = make_event_ref(h.clone(), None);
                                    self.edges.push(Edge::FamilyEventRef {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(event_ref),
                                    });
                                }
                            }
                        }
                        b"childref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref f) = current_family {
                                    let child_ref = make_child_ref(h.clone(), None);
                                    self.edges.push(Edge::FamilyChildRef {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(child_ref),
                                    });
                                }
                            }
                        }
                        b"father" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyFather {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"mother" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyMother {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"personref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonPersonRef {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(PersonRef {
                                            ref_field: h,
                                            ..PersonRef::default()
                                        }),
                                    });
                                }
                            }
                        }
                        b"reporef" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceRepoRef {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(RepoRef {
                                            ref_field: h,
                                            ..RepoRef::default()
                                        }),
                                    });
                                }
                            }
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonCitation {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyCitation {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventCitation {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceCitation {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonNote {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyNote {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventNote {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceNote {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceNote {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref c) = current_citation {
                                    self.edges.push(Edge::CitationNote {
                                        source: c.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref r) = current_repository {
                                    self.edges.push(Edge::RepositoryNote {
                                        source: r.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref m) = current_media {
                                    self.edges.push(Edge::MediaNote {
                                        source: m.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonMediaRef {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyMediaRef {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventMediaRef {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceMediaRef {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceMediaRef {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref r) = current_repository {
                                    self.edges.push(Edge::RepositoryMediaRef {
                                        source: r.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                }
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonTag {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyTag {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventTag {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceTag {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceTag {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref c) = current_citation {
                                    self.edges.push(Edge::CitationTag {
                                        source: c.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref r) = current_repository {
                                    self.edges.push(Edge::RepositoryTag {
                                        source: r.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref m) = current_media {
                                    self.edges.push(Edge::MediaTag {
                                        source: m.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref n) = current_note {
                                    self.edges.push(Edge::NoteTag {
                                        source: n.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref t) = current_tag {
                                    self.edges.push(Edge::TagTag {
                                        source: t.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"call" | b"group" => {
                            // Skip these inner name elements
                        }
                        _ => {}
                    }
                }

                Ok(Event::Empty(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);

                    match name {
                        b"father" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyFather {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"mother" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyMother {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"eventref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    let event_ref = make_event_ref(h.clone(), None);
                                    self.edges.push(Edge::PersonEventRef {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(event_ref),
                                    });
                                } else if let Some(ref f) = current_family {
                                    let event_ref = make_event_ref(h.clone(), None);
                                    self.edges.push(Edge::FamilyEventRef {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(event_ref),
                                    });
                                }
                            }
                        }
                        b"childref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref f) = current_family {
                                    let child_ref = make_child_ref(h.clone(), None);
                                    self.edges.push(Edge::FamilyChildRef {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(child_ref),
                                    });
                                }
                            }
                        }
                        b"personref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonPersonRef {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(PersonRef {
                                            ref_field: h,
                                            ..PersonRef::default()
                                        }),
                                    });
                                }
                            }
                        }
                        b"reporef" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceRepoRef {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(RepoRef {
                                            ref_field: h,
                                            ..RepoRef::default()
                                        }),
                                    });
                                }
                            }
                        }
                        b"placeref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_place {
                                    self.edges.push(edge_place_place_ref(p.handle.clone(), h));
                                }
                            }
                        }
                        b"citationref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonCitation {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyCitation {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventCitation {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceCitation {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"noteref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonNote {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyNote {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventNote {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceNote {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceNote {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref c) = current_citation {
                                    self.edges.push(Edge::CitationNote {
                                        source: c.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref r) = current_repository {
                                    self.edges.push(Edge::RepositoryNote {
                                        source: r.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref m) = current_media {
                                    self.edges.push(Edge::MediaNote {
                                        source: m.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"mediaref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonMediaRef {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyMediaRef {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventMediaRef {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceMediaRef {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceMediaRef {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                } else if let Some(ref r) = current_repository {
                                    self.edges.push(Edge::RepositoryMediaRef {
                                        source: r.handle.clone(),
                                        target: h.clone(),
                                        metadata: Box::new(MediaRef {
                                            ref_field: h,
                                            ..MediaRef::default()
                                        }),
                                    });
                                }
                            }
                        }
                        b"tagref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref p) = current_person {
                                    self.edges.push(Edge::PersonTag {
                                        source: p.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref f) = current_family {
                                    self.edges.push(Edge::FamilyTag {
                                        source: f.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref ev) = current_event {
                                    self.edges.push(Edge::EventTag {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref pl) = current_place {
                                    self.edges.push(Edge::PlaceTag {
                                        source: pl.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref s) = current_source {
                                    self.edges.push(Edge::SourceTag {
                                        source: s.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref c) = current_citation {
                                    self.edges.push(Edge::CitationTag {
                                        source: c.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref r) = current_repository {
                                    self.edges.push(Edge::RepositoryTag {
                                        source: r.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref m) = current_media {
                                    self.edges.push(Edge::MediaTag {
                                        source: m.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref n) = current_note {
                                    self.edges.push(Edge::NoteTag {
                                        source: n.handle.clone(),
                                        target: h.clone(),
                                    });
                                } else if let Some(ref t) = current_tag {
                                    self.edges.push(Edge::TagTag {
                                        source: t.handle.clone(),
                                        target: h.clone(),
                                    });
                                }
                            }
                        }
                        b"dateval" => {
                            // Parse date from empty element attributes
                            let year = parse_year_from_val(e);
                            if let Some(ref mut ev) = current_event {
                                ev.date = Some(DateValue {
                                    quality: None,
                                    modifier: None,
                                    day: Some(year as i32),
                                    month: None,
                                    year: year as i32,
                                    text: None,
                                });
                            }
                        }
                        b"sourceref" => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref mut c) = current_citation {
                                    self.edges.push(Edge::CitationSource {
                                        source: c.handle.clone(),
                                        target: h.clone(),
                                    });
                                    c.source_handle = Some(h);
                                }
                            }
                        }
                        b"place" if current_event.is_some() => {
                            if let Some(h) = read_hlink_attr(e) {
                                if let Some(ref mut ev) = current_event {
                                    self.edges.push(Edge::EventPlace {
                                        source: ev.handle.clone(),
                                        target: h.clone(),
                                    });
                                    ev.place_handle = Some(h);
                                }
                            }
                        }
                        b"created" if self.state.in_header => {
                            // Already handled by detect_schema_version
                        }
                        b"gender" => {
                            // Handled by reading text: we don't need to handle
                            // it here since text reading happens in Start event.
                            // For gender elements specifically, read text directly.
                        }
                        _ => {}
                    }
                }

                Ok(Event::End(ref e)) => {
                    let raw = e.name().as_ref().to_vec();
                    let name = strip_prefix(&raw);

                    match name {
                        b"header" => {
                            self.state.in_header = false;
                        }
                        b"people" | b"families" | b"events" | b"places" | b"sources"
                        | b"citations" | b"repositories" | b"objects" | b"notes" | b"tags"
                        | b"media" => {
                            self.state.current_section = Section::None;
                            self.state.in_section = false;
                        }
                        b"person" => {
                            if let Some(p) = current_person.take() {
                                let data = p.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Person(data));
                            }
                        }
                        b"family" => {
                            if let Some(f) = current_family.take() {
                                let data = f.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Family(data));
                            }
                        }
                        b"event" => {
                            if let Some(e) = current_event.take() {
                                let data = e.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Event(data));
                            }
                        }
                        b"place" => {
                            if let Some(p) = current_place.take() {
                                let data = p.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Place(data));
                            }
                        }
                        b"source" => {
                            if let Some(s) = current_source.take() {
                                let data = s.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Source(data));
                            }
                        }
                        b"citation" => {
                            if let Some(c) = current_citation.take() {
                                let data = c.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Citation(data));
                            }
                        }
                        b"repository" => {
                            if let Some(r) = current_repository.take() {
                                let data = r.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Repository(data));
                            }
                        }
                        b"object" => {
                            if let Some(m) = current_media.take() {
                                let data = m.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Media(data));
                            }
                        }
                        b"note" => {
                            if let Some(n) = current_note.take() {
                                let data = n.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Note(data));
                            }
                        }
                        b"tag" => {
                            if let Some(t) = current_tag.take() {
                                let data = t.into_data();
                                let handle = data.handle.clone();
                                self.nodes.insert(handle, Node::Tag(data));
                            }
                        }
                        b"eventtype" => in_eventtype = false,
                        b"name" => in_name = false,
                        _ => {}
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

    /// Build the final graph from the parsed data.
    fn build(self) -> Result<Graph, String> {
        let mut graph = Graph::new();

        // Add all nodes
        for (handle, node) in self.nodes {
            graph
                .add_node(handle.clone(), node)
                .map_err(|e| format!("failed to add node {}: {}", handle, e))?;
        }

        // Mark inferred handles
        for h in &self.inferred_handles {
            graph.record_inferred_handle(h.clone());
        }

        // Add all edges
        for edge in self.edges {
            let (source, target) = edge_source_target_helper(&edge);
            // If the target doesn't exist, record it as inferred and skip the edge
            // (it's a dangling reference from the original file)
            if !graph.contains_node(&target) && graph.contains_node(&source) {
                // Dangling hlink ref: treat as not counting for connectivity
                continue;
            }
            graph
                .add_edge(edge)
                .map_err(|e| format!("failed to add edge: {}", e))?;
        }

        Ok(graph)
    }
}

// ---------------------------------------------------------------------------
// Intermediate builders for each primary type
// ---------------------------------------------------------------------------

/// Intermediate builder for Person nodes during parsing.
#[derive(Default)]
struct PersonBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    given_name: String,
    surname: String,
    gender: Option<i32>,
}

impl PersonBuilder {
    fn into_data(self) -> PersonData {
        let primary_name = Name {
            first_name: if self.given_name.is_empty() {
                None
            } else {
                Some(self.given_name)
            },
            surname_list: if self.surname.is_empty() {
                vec![]
            } else {
                vec![Surname {
                    surname: Some(self.surname),
                    ..Surname::default()
                }]
            },
            ..Name::default()
        };
        PersonData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            gender: self.gender,
            primary_name,
            ..PersonData::default()
        }
    }
}

/// Intermediate builder for Family nodes during parsing.
#[derive(Default)]
struct FamilyBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    type_field: Option<FamilyRelType>,
}

impl FamilyBuilder {
    fn into_data(self) -> FamilyData {
        FamilyData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            type_field: self.type_field,
            ..FamilyData::default()
        }
    }
}

/// Intermediate builder for Event nodes during parsing.
#[derive(Default)]
struct EventBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    event_type_text: String,
    description: Option<String>,
    date: Option<DateValue>,
    place_handle: Option<Handle>,
}

impl EventBuilder {
    fn into_data(self) -> EventData {
        // Parse event type from text
        let event_type = match parse_event_type(&self.event_type_text) {
            ParseEventTypeResult::Known(t) => Some(t),
            ParseEventTypeResult::Unknown => None,
        };
        EventData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            event_type: if self.event_type_text.is_empty() {
                None
            } else {
                event_type
            },
            type_field: None,
            description: self.description,
            date: self.date,
            place_handle: self.place_handle,
            ..EventData::default()
        }
    }
}

/// Intermediate builder for Place nodes during parsing.
#[derive(Default)]
struct PlaceBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    title: Option<String>,
    code: Option<String>,
    lat: Option<String>,
    long: Option<String>,
    name: Location,
}

impl PlaceBuilder {
    fn into_data(self) -> PlaceData {
        PlaceData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            title: self.title,
            code: self.code,
            lat: self.lat,
            long: self.long,
            name: self.name,
            ..PlaceData::default()
        }
    }
}

/// Intermediate builder for Source nodes during parsing.
#[derive(Default)]
struct SourceBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    title: String,
    abbrev: Option<String>,
    author: Option<String>,
    pubinfo: Option<String>,
}

impl SourceBuilder {
    fn into_data(self) -> SourceData {
        SourceData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            title: self.title,
            abbrev: self.abbrev,
            author: self.author,
            pubinfo: self.pubinfo,
            ..SourceData::default()
        }
    }
}

/// Intermediate builder for Citation nodes during parsing.
#[derive(Default)]
struct CitationBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    page: Option<String>,
    confidence: Option<i32>,
    source_handle: Option<Handle>,
}

impl CitationBuilder {
    fn into_data(self) -> CitationData {
        CitationData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            page: self.page,
            confidence: self.confidence,
            source_handle: self.source_handle,
            ..CitationData::default()
        }
    }
}

/// Intermediate builder for Repository nodes during parsing.
#[derive(Default)]
struct RepositoryBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    name: Option<String>,
}

impl RepositoryBuilder {
    fn into_data(self) -> RepositoryData {
        RepositoryData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            name: self.name,
            ..RepositoryData::default()
        }
    }
}

/// Intermediate builder for Media objects during parsing.
#[derive(Default)]
struct MediaBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    desc: Option<String>,
    path: Option<String>,
    mime: Option<String>,
    checksum: Option<String>,
}

impl MediaBuilder {
    fn into_data(self) -> MediaData {
        MediaData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            desc: self.desc,
            path: self.path,
            mime: self.mime,
            checksum: self.checksum,
            ..MediaData::default()
        }
    }
}

/// Intermediate builder for Note nodes during parsing.
#[derive(Default)]
struct NoteBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    text: String,
}

impl NoteBuilder {
    fn into_data(self) -> NoteData {
        NoteData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            text: self.text,
            ..NoteData::default()
        }
    }
}

/// Intermediate builder for Tag nodes during parsing.
#[derive(Default)]
struct TagBuilder {
    handle: Handle,
    gramps_id: Option<String>,
    color: Option<String>,
}

impl TagBuilder {
    fn into_data(self) -> TagData {
        TagData {
            handle: self.handle,
            gramps_id: self.gramps_id,
            color: self.color,
            name: String::new(),
            ..TagData::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Date parsing helpers
// ---------------------------------------------------------------------------

/// Read the `val` attribute from a `<dateval>` element.
#[allow(dead_code)]
fn read_dateval_val(e: &quick_xml::events::BytesStart) -> String {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"val" || key.ends_with(b":val") {
            return String::from_utf8_lossy(&attr.value).to_string();
        }
    }
    String::new()
}

/// Parse the year from a `<dateval>` element's `val` attribute.
fn parse_year_from_val(e: &quick_xml::events::BytesStart) -> i64 {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"val" || key.ends_with(b":val") {
            let val = String::from_utf8_lossy(&attr.value);
            // Format is typically "YYYY-MM-DD" or "YYYY-MM-DD (optional parts)"
            if let Some(year_str) = val.split('-').next() {
                if let Ok(year) = year_str.parse::<i64>() {
                    return year;
                }
            }
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Event type parsing
// ---------------------------------------------------------------------------

/// Result of parsing an event type string.
enum ParseEventTypeResult {
    Known(EventType),
    Unknown,
}

/// Parse a Gramps event type string into an `EventType` enum variant.
///
/// Maps human-readable Gramps XML type text (e.g. "Birth", "Death",
/// "Annulment") to the corresponding [`EventType`] variant. The merged
/// schema (5.1 + 5.2) contains both PascalCase and UPPER_SNAKE_CASE
/// variants; this function uses PascalCase where available, uppercase
/// otherwise.
fn parse_event_type(text: &str) -> ParseEventTypeResult {
    use EventType::*;
    let t = text.to_lowercase().replace([' ', '_', '-'], "_");
    let variant = match t.as_str() {
        "adopt" => ADOPT,
        "adult_christen" | "adultchristen" => ADULTCHRISTEN,
        "annulment" => ANNULMENT,
        "adoption" => Adoption,
        "baptism" => Baptism,
        "bar_mitzvah" => BarMitzvah,
        "bat_mitzvah" | "bas_mitzvah" => BatMitzvah,
        "birth" => Birth,
        "bless" => BLESS,
        "burial" => Burial,
        "cause_death" | "cause_of_death" => CAUSEDEATH,
        "census" => Census,
        "christen" | "christening" => CHRISTEN,
        "confirmation" => Confirmation,
        "correspondence" => Correspondence,
        "creates" => Creates,
        "cremation" => CREMATION,
        "death" => Death,
        "degree" => DEGREE,
        "divorce" => Divorce,
        "div_filing" | "divfiling" => DIVFILING,
        "education" => Education,
        "elected" => ELECTED,
        "emigration" => Emigration,
        "engagement" => ENGAGEMENT,
        "first_commun" | "firstcommun" => FIRSTCOMMUN,
        "funeral" => Funeral,
        "graduation" => Graduation,
        "immigration" => Immigration,
        "marriage" => Marriage,
        "marriage_alt" | "marr_alt" | "marriage_alternate" => MARRALT,
        "marriage_banns" | "marr_banns" => MARRBANNS,
        "marriage_contract" | "marr_contr" => MARRCONTR,
        "marriage_license" | "marr_lic" => MARRLIC,
        "marriage_settlement" | "marr_settl" => MARRSETTL,
        "medical_info" | "med_info" | "medinfo" => MEDINFO,
        "military_service" | "military_serv" => MilitaryService,
        "naturalization" => Naturalization,
        "nobility_title" | "nob_title" | "nobtitle" | "nobility" => NOBTITLE,
        "number_of_marriages" | "num_marriages" | "nummarriages" => NUMMARRIAGES,
        "occupation" => Occupation,
        "ordination" => ORDINATION,
        "other" => Other,
        "probate" => Probate,
        "property" => PROPERTY,
        "religion" => Religion,
        "residence" => Residence,
        "retirement" => Retirement,
        "title" => Title,
        "unknown" => UNKNOWN,
        "will" => Will,
        _ => return ParseEventTypeResult::Unknown,
    };
    ParseEventTypeResult::Known(variant)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal Gramps 5.2 XML with a single person.
    fn minimal_52_xml() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
  </header>
  <people>
    <person handle="p0001" id="I0001">
      <gender>1</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <birth>
        <dateval val="1850-07-13"/>
      </birth>
      <death>
        <dateval val="1920-03-01"/>
      </death>
    </person>
  </people>
</database>"#
            .to_string()
    }

    #[test]
    fn parse_minimal_person_roundtrip() {
        let xml = minimal_52_xml();
        let (graph, namespace) = parse_gramps_xml(&xml).unwrap();
        assert_eq!(graph.node_count(), 1);
        assert_eq!(namespace, "http://gramps-project.org/xml/1.7.2/");

        // Verify the person
        let node = graph.get_node(&"p0001".to_string()).unwrap();
        if let Node::Person(ref data) = node {
            assert_eq!(data.handle, "p0001");
            assert_eq!(data.primary_name.first_name.as_deref(), Some("John"));
            assert_eq!(
                data.primary_name
                    .surname_list
                    .first()
                    .and_then(|s| s.surname.as_deref()),
                Some("Smith")
            );
        } else {
            panic!("Expected Person node");
        }
    }

    #[test]
    fn parse_minimal_person_roundtrip_write() {
        let xml = minimal_52_xml();
        let (graph, _ns) = parse_gramps_xml(&xml).unwrap();

        // Write back to XML
        let mut output = Vec::new();
        let serialization_map = output::SerializationMap::new();
        let writer = output::GraphXmlWriter::new(serialization_map, "5.2");
        writer.write(&graph, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Re-parse and verify semantic equivalence
        let (graph2, _ns2) = parse_gramps_xml(&output_str).unwrap();
        assert_eq!(graph2.node_count(), graph.node_count());
        assert_eq!(graph2.edge_count(), graph.edge_count());
    }

    #[test]
    fn parse_family_roundtrip() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
  </header>
  <people>
    <person handle="p0001" id="I0001">
      <gender>1</gender>
      <name><first>John</first><surname>Smith</surname></name>
    </person>
    <person handle="p0002" id="I0002">
      <gender>2</gender>
      <name><first>Jane</first><surname>Smith</surname></name>
    </person>
  </people>
  <families>
    <family handle="f0001">
      <father hlink="p0001"/>
      <mother hlink="p0002"/>
    </family>
  </families>
</database>"#
            .to_string();
        let (graph, _) = parse_gramps_xml(&xml).unwrap();

        assert_eq!(graph.node_count(), 3);
        // 2 people + 1 family = 3 nodes
        // edges: father + mother = 2 edges
        assert_eq!(graph.edge_count(), 2);

        // Verify edges
        let edges: Vec<_> = graph.iter_edges().collect();
        assert!(edges
            .iter()
            .any(|e| matches!(e, Edge::FamilyFather { source, .. } if source == "f0001")));
        assert!(edges
            .iter()
            .any(|e| matches!(e, Edge::FamilyMother { source, .. } if source == "f0001")));
    }

    #[test]
    fn parse_all_types_empty_roundtrip() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
  </header>
  <people>
    <person handle="p0001" id="I0001">
      <gender>1</gender>
      <name><first>John</first></name>
    </person>
  </people>
  <events>
    <event handle="e0001">
      <eventtype><type>Birth</type></eventtype>
    </event>
  </events>
  <places>
    <place handle="pl0001">
      <title>New York</title>
    </place>
  </places>
  <sources>
    <source handle="s0001">
      <title>Census Record</title>
      <author>National Archives</author>
    </source>
  </sources>
  <citations>
    <citation handle="c0001">
      <page>p. 42</page>
    </citation>
  </citations>
  <repositories>
    <repository handle="r0001">
      <rname>National Archives</rname>
    </repository>
  </repositories>
  <objects>
    <object handle="m0001">
      <path>/photos/photo1.jpg</path>
    </object>
  </objects>
  <notes>
    <note handle="n0001">
      <text>Test note content</text>
    </note>
  </notes>
  <tags>
    <tag handle="t0001">
      <color>#ff0000</color>
    </tag>
  </tags>
</database>"#
            .to_string();
        let (graph, _) = parse_gramps_xml(&xml).unwrap();

        assert_eq!(graph.node_count(), 9);
        assert_eq!(graph.edge_count(), 0);

        // Verify each type exists
        assert!(graph.get_node(&"p0001".to_string()).is_some());
        assert!(graph.get_node(&"e0001".to_string()).is_some());
        assert!(graph.get_node(&"pl0001".to_string()).is_some());
        assert!(graph.get_node(&"s0001".to_string()).is_some());
        assert!(graph.get_node(&"c0001".to_string()).is_some());
        assert!(graph.get_node(&"r0001".to_string()).is_some());
        assert!(graph.get_node(&"m0001".to_string()).is_some());
        assert!(graph.get_node(&"n0001".to_string()).is_some());
        assert!(graph.get_node(&"t0001".to_string()).is_some());
    }

    #[test]
    fn parse_citation_with_sourceref() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>"#.to_string()
            + r#"
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
  </header>
  <sources>
    <source handle="s0001" id="S0001">
      <title>Census Record</title>
    </source>
  </sources>
  <citations>
    <citation handle="c0001" id="C0001">
      <sourceref hlink="s0001"/>
      <page>p. 42</page>
    </citation>
  </citations>
</database>"#;
        let (graph, _) = parse_gramps_xml(&xml).unwrap();

        // Should have 2 nodes: source + citation
        assert_eq!(graph.node_count(), 2);

        // Should have 1 edge: CitationSource
        assert_eq!(graph.edge_count(), 1);
        let edges: Vec<_> = graph.iter_edges().collect();
        assert!(edges.iter().any(|e| {
            matches!(e, Edge::CitationSource { source, target }
                if source == "c0001" && target == "s0001")
        }));

        // Verify the citation's source_handle was set
        if let Some(Node::Citation(data)) = graph.get_node(&"c0001".to_string()) {
            assert_eq!(
                data.source_handle.as_deref(),
                Some("s0001"),
                "source_handle should be set on citation data"
            );
        } else {
            panic!("Expected Citation node");
        }
    }

    #[test]
    fn parse_event_with_place_hlink() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>"#.to_string()
            + r#"
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
  </header>
  <events>
    <event handle="e0001" id="E0001">
      <eventtype><type>Birth</type></eventtype>
      <place hlink="pl0001"/>
      <description>Birth of John</description>
    </event>
  </events>
  <places>
    <place handle="pl0001" id="P0001">
      <title>New York</title>
    </place>
  </places>
</database>"#;
        let (graph, _) = parse_gramps_xml(&xml).unwrap();

        // Should have 2 nodes: event + place
        assert_eq!(graph.node_count(), 2);

        // Should have 1 edge: EventPlace
        assert_eq!(graph.edge_count(), 1);
        let edges: Vec<_> = graph.iter_edges().collect();
        assert!(edges.iter().any(|e| {
            matches!(e, Edge::EventPlace { source, target }
                if source == "e0001" && target == "pl0001")
        }));

        // Verify the event's place_handle was set
        if let Some(Node::Event(data)) = graph.get_node(&"e0001".to_string()) {
            assert_eq!(
                data.place_handle.as_deref(),
                Some("pl0001"),
                "place_handle should be set on event data"
            );
        } else {
            panic!("Expected Event node");
        }
    }

    #[test]
    fn parse_event_5_1_flat_format() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1"/>
  </header>
  <events>
    <event handle="e0001">
      <type>Birth</type>
      <description>Birth of John Smith</description>
    </event>
  </events>
  <people>
    <person handle="p0001" id="I0001">
      <gender>1</gender>
      <name><first>John</first><surname>Smith</surname></name>
    </person>
  </people>
</database>"#
            .to_string();
        let (graph, _) = parse_gramps_xml(&xml).unwrap();

        assert_eq!(graph.node_count(), 2);
        assert!(graph.get_node(&"e0001".to_string()).is_some());
        assert!(graph.get_node(&"p0001".to_string()).is_some());
    }

    #[test]
    fn parse_event_5_2_nested_format() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-15" version="5.2"/>
  </header>
  <events>
    <event handle="e0001">
      <eventtype><type>Birth</type></eventtype>
      <description>Birth of John Smith</description>
    </event>
  </events>
  <people>
    <person handle="p0001" id="I0001">
      <gender>1</gender>
      <name><first>John</first><surname>Smith</surname></name>
    </person>
  </people>
</database>"#
            .to_string();
        let (graph, _) = parse_gramps_xml(&xml).unwrap();

        assert_eq!(graph.node_count(), 2);
        assert!(graph.get_node(&"e0001".to_string()).is_some());
        assert!(graph.get_node(&"p0001".to_string()).is_some());
    }
}
