//! XML serialization for Gramps genealogy graphs.
//!
//! This module provides the [`GraphXmlWriter`] that walks a validated [`Graph`]
//! and produces Gramps XML (`.gramps` format) following the RelaxNG schema.

use crate::serialization_map::SerializationMap;
use crate::serialization_map::XmlChild;
use crate::serialization_map::XmlChildSource;
use crate::serialization_map::XmlTypeInfo;
use typed_graph::Edge;
use typed_graph::Graph;
use typed_graph::Handle;
use typed_graph::Node;

/// Errors that can occur during XML serialization.
#[derive(Clone, Debug, PartialEq)]
pub enum SerializationError {
    /// An I/O error occurred while writing.
    Io(std::io::ErrorKind, String),
    /// The graph contains a node type that is not supported by the serializer.
    UnsupportedType(String),
    /// A required field is missing from a node (defensive check).
    MissingRequiredField {
        /// Handle of the node with the missing field.
        handle: String,
        /// Name of the missing field.
        field: &'static str,
    },
}

impl std::fmt::Display for SerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializationError::Io(kind, msg) => {
                write!(f, "I/O error ({:?}): {}", kind, msg)
            }
            SerializationError::UnsupportedType(t) => {
                write!(f, "unsupported node type: {}", t)
            }
            SerializationError::MissingRequiredField { handle, field } => {
                write!(f, "missing required field '{}' on node '{}'", field, handle)
            }
        }
    }
}

impl std::error::Error for SerializationError {}

impl From<std::io::Error> for SerializationError {
    fn from(err: std::io::Error) -> Self {
        SerializationError::Io(err.kind(), err.to_string())
    }
}

/// Writes a [`Graph`] to Gramps XML.
///
/// The writer uses a [`SerializationMap`] to determine XML element and attribute
/// names, walks the graph's nodes grouped by type, and emits XML following the
/// Gramps RelaxNG schema.
pub struct GraphXmlWriter {
    map: SerializationMap,
    /// Creation date string for the header (YYYY-MM-DD format).
    creation_date: String,
    /// Full Gramps version string for the XML header (e.g. "5.2.0").
    gramps_version: String,
    /// Derived XML namespace URI (e.g. "http://gramps-project.org/xml/1.7.2/").
    namespace: String,
}

impl GraphXmlWriter {
    /// Create a new `GraphXmlWriter` with the given [`SerializationMap`] and Gramps version.
    pub fn new(map: SerializationMap, gramps_version: &str) -> Self {
        let creation_date = Self::current_date_string();
        let namespace = Self::derive_namespace(gramps_version);
        GraphXmlWriter {
            map,
            creation_date,
            gramps_version: gramps_version.to_string(),
            namespace,
        }
    }

    /// Generate a YYYY-MM-DD date string from the current system time.
    fn current_date_string() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        // Days since epoch
        let days = secs / 86400;
        // Simple Gregorian calendar calculation
        let mut y = 1970i64;
        let mut remaining_days = days as i64;
        loop {
            let days_in_year = if is_leap(y) { 366 } else { 365 };
            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            y += 1;
        }
        let month_days = if is_leap(y) {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };
        let mut m = 0;
        for (i, &md) in month_days.iter().enumerate() {
            if remaining_days < md {
                m = i + 1;
                break;
            }
            remaining_days -= md;
        }
        if m == 0 {
            m = 12;
        }
        format!("{:04}-{:02}-{:02}", y, m, remaining_days + 1)
    }

    /// Derive the XML namespace URI from a Gramps version string.
    ///
    /// Lookup table for known version → namespace mappings. For unknown
    /// versions, derives with pattern `http://gramps-project.org/xml/1.{major}.{minor}/`
    /// and prints a warning to stderr.
    fn derive_namespace(gramps_version: &str) -> String {
        // Parse the major.minor prefix from the version string
        let prefix = Self::schema_version_prefix(gramps_version);
        match prefix.as_str() {
            "5.0" => "http://gramps-project.org/xml/1.7.0/".to_string(),
            "5.1" => "http://gramps-project.org/xml/1.7.1/".to_string(),
            "5.2" => "http://gramps-project.org/xml/1.7.2/".to_string(),
            "6.0" => "http://gramps-project.org/xml/1.8.0/".to_string(),
            _ => {
                eprintln!(
                    "warning: unknown Gramps version '{}', deriving namespace from prefix '{}'",
                    gramps_version, prefix
                );
                // For unknown versions, derive with the pattern 1.{major}.{minor}
                format!("http://gramps-project.org/xml/1.{}/", prefix)
            }
        }
    }

    /// Extract the `"X.Y"` schema version prefix from a full Gramps version string.
    ///
    /// Examples:
    /// - `"5.2.0"` → `"5.2"`
    /// - `"5.1.6"` → `"5.1"`
    /// - `"6.0.0"` → `"6.0"`
    fn schema_version_prefix(gramps_version: &str) -> String {
        let parts: Vec<&str> = gramps_version.split('.').collect();
        if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            gramps_version.to_string()
        }
    }

    /// Serialize the graph to the given writer.
    ///
    /// Returns an error if the graph contains unsupported types or if
    /// writing to the output fails.
    pub fn write(
        &self,
        graph: &Graph,
        writer: &mut impl std::io::Write,
    ) -> Result<(), SerializationError> {
        // Write XML declaration
        writeln!(writer, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
        writeln!(writer, r#"<database xmlns="{}">"#, self.namespace)?;

        // Write header
        self.write_header(writer)?;

        // Write each section in order
        for section_name in &self.map.section_order {
            self.write_section(graph, writer, section_name)?;
        }

        writeln!(writer, "</database>")?;
        Ok(())
    }

    /// Write the `<header>` section with creation timestamp and researcher info.
    fn write_header(&self, writer: &mut impl std::io::Write) -> Result<(), SerializationError> {
        writeln!(writer, "  <header>")?;
        writeln!(
            writer,
            "    <created date=\"{}\" version=\"{}\"/>",
            self.creation_date, self.gramps_version
        )?;
        writeln!(writer, "    <researcher>")?;
        writeln!(writer, "      <resname>Generated by gramps-gen</resname>")?;
        writeln!(writer, "    </researcher>")?;
        writeln!(writer, "  </header>")?;
        Ok(())
    }

    /// Write a single section (e.g., `<people>...</people>`) for the given type.
    fn write_section(
        &self,
        graph: &Graph,
        writer: &mut impl std::io::Write,
        section_name: &str,
    ) -> Result<(), SerializationError> {
        // Find the type whose section_name matches
        let type_info: Option<&XmlTypeInfo> = self
            .map
            .type_map
            .values()
            .find(|info| info.section_name == section_name);

        let type_info = match type_info {
            Some(info) => info,
            None => return Ok(()), // Unknown section, skip
        };

        // Collect nodes of this type
        let mut nodes: Vec<(Handle, Node)> = graph
            .iter_nodes()
            .filter(|(_, node)| node_type_name(node) == Some(&type_info.element_name))
            .map(|(h, n)| (h.clone(), n.clone()))
            .collect::<Vec<_>>();

        // Sort by handle for deterministic output
        nodes.sort_by(|a, b| a.0.cmp(&b.0));

        if nodes.is_empty() {
            return Ok(()); // Skip empty sections
        }

        // Open section element
        write!(writer, "  <{}>", section_name)?;

        // Write each node
        for (handle, node) in &nodes {
            write!(writer, "\n    ")?;
            self.write_node_element(graph, writer, handle, node, type_info)?;
        }

        writeln!(writer, "\n  </{}>", section_name)?;
        Ok(())
    }

    /// Write a single node as an XML element with attributes and children.
    fn write_node_element(
        &self,
        graph: &Graph,
        writer: &mut impl std::io::Write,
        handle: &str,
        node: &Node,
        type_info: &XmlTypeInfo,
    ) -> Result<(), SerializationError> {
        // Write opening tag with attributes
        write!(writer, "<{}", type_info.element_name)?;

        for attr in &type_info.attributes {
            if let Some(value) = self.get_field_value(node, &attr.field) {
                write!(writer, " {}=\"{}\"", attr.attr_name, escape_xml(&value))?;
            }
        }

        // Check if there are children to write
        let has_children = !type_info.children.is_empty()
            && self.has_children_for_node(graph, handle, node, &type_info.children);

        if has_children {
            writeln!(writer, ">")?;
            self.write_children(graph, writer, handle, node, &type_info.children)?;
            write!(writer, "    </{}>", type_info.element_name)?;
        } else {
            write!(writer, "/>")?;
        }

        Ok(())
    }

    /// Check if a node has any actual children to write.
    fn has_children_for_node(
        &self,
        graph: &Graph,
        handle: &str,
        node: &Node,
        children: &[XmlChild],
    ) -> bool {
        let handle_owned = handle.to_string();
        for child in children {
            match &child.source {
                XmlChildSource::InlineStruct(field_name) => {
                    if self.has_inline_struct_value(node, field_name) {
                        return true;
                    }
                }
                XmlChildSource::Array(field_name) => {
                    if self.has_array_items(node, field_name) {
                        return true;
                    }
                }
                XmlChildSource::Edge(edge_name) => {
                    let edges_from = graph.edges_from(&handle_owned);
                    if edges_from
                        .iter()
                        .any(|e| edge_variant_name(e) == Some(edge_name))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if an inline struct field has a value.
    fn has_inline_struct_value(&self, node: &Node, field_name: &str) -> bool {
        match field_name {
            "gender" => matches!(node, Node::Person(_)),
            "primary_name" => true, // Always present on Person
            "event_type" => true,   // Always present on Event
            "date" => {
                matches!(node, Node::Event(_)) && {
                    if let Node::Event(e) = node {
                        e.date.is_some()
                    } else {
                        false
                    }
                }
            }
            "description" => {
                (matches!(node, Node::Event(_)) && {
                    if let Node::Event(e) = node {
                        e.description.is_some()
                    } else {
                        false
                    }
                }) || (matches!(node, Node::Media(_)) && {
                    if let Node::Media(m) = node {
                        m.desc.is_some()
                    } else {
                        false
                    }
                })
            }
            "name" => matches!(node, Node::Tag(_) | Node::Repository(_)),
            "color" => {
                matches!(node, Node::Tag(_)) && {
                    if let Node::Tag(t) = node {
                        t.color.is_some()
                    } else {
                        false
                    }
                }
            }
            "priority" => {
                matches!(node, Node::Tag(_)) && {
                    if let Node::Tag(t) = node {
                        t.priority.is_some()
                    } else {
                        false
                    }
                }
            }
            "confidence" => {
                matches!(node, Node::Citation(_)) && {
                    if let Node::Citation(c) = node {
                        c.confidence.is_some()
                    } else {
                        false
                    }
                }
            }
            "title" => matches!(node, Node::Source(_) | Node::Place(_)),
            "abbrev" => {
                matches!(node, Node::Source(_)) && {
                    if let Node::Source(s) = node {
                        s.author.is_some() || s.pubinfo.is_some()
                    } else {
                        false
                    }
                }
            }
            "file" => {
                matches!(node, Node::Media(_)) && {
                    if let Node::Media(m) = node {
                        m.path.is_some()
                    } else {
                        false
                    }
                }
            }
            "text" => matches!(node, Node::Note(_)),
            "format" => {
                matches!(node, Node::Note(_)) && {
                    if let Node::Note(n) = node {
                        n.format.is_some()
                    } else {
                        false
                    }
                }
            }
            _ => false,
        }
    }

    /// Check if an array field has items.
    fn has_array_items(&self, node: &Node, field_name: &str) -> bool {
        match field_name {
            "alternate_names" => {
                matches!(node, Node::Person(_)) && {
                    if let Node::Person(p) = node {
                        !p.alternate_names.is_empty()
                    } else {
                        false
                    }
                }
            }
            _ => false,
        }
    }

    /// Write child elements for a node.
    fn write_children(
        &self,
        graph: &Graph,
        writer: &mut impl std::io::Write,
        handle: &str,
        node: &Node,
        children: &[XmlChild],
    ) -> Result<(), SerializationError> {
        for child in children {
            match &child.source {
                XmlChildSource::InlineStruct(field_name) => {
                    self.write_inline_struct(graph, writer, handle, node, child, field_name)?;
                }
                XmlChildSource::Array(field_name) => {
                    self.write_array_items(graph, writer, handle, node, child, field_name)?;
                }
                XmlChildSource::Edge(edge_name) => {
                    self.write_edge_items(graph, writer, handle, child, edge_name)?;
                }
            }
        }
        Ok(())
    }

    /// Write an inline struct field as child elements.
    fn write_inline_struct(
        &self,
        _graph: &Graph,
        writer: &mut impl std::io::Write,
        _handle: &str,
        node: &Node,
        child: &XmlChild,
        field_name: &str,
    ) -> Result<(), SerializationError> {
        match field_name {
            // Person: gender (integer → M/F/U)
            "gender" => {
                if let Node::Person(p) = node {
                    let gender_str = match typed_graph::gender_value(p.gender) {
                        1 => "F",
                        2 | 3 => "U",
                        _ => "M",
                    };
                    writeln!(
                        writer,
                        "      <{}>{}</{}>",
                        child.element_name, gender_str, child.element_name
                    )?;
                }
            }
            // Person: primary_name is a Name struct
            "primary_name" => {
                if let Node::Person(p) = node {
                    self.write_name_element(writer, &child.element_name, &p.primary_name)?;
                }
            }
            // Event: event_type is an EventType enum
            "event_type" => {
                if let Node::Event(e) = node {
                    let type_str = typed_graph::event_type_display(&e.event_type);
                    writeln!(
                        writer,
                        "      <{}><type>{}</type></{}>",
                        child.element_name,
                        escape_xml(&type_str),
                        child.element_name
                    )?;
                }
            }
            // Event: date is an Option<DateValue>
            "date" => {
                if let Node::Event(e) = node {
                    self.write_date_element(writer, &child.element_name, &e.date)?;
                }
            }
            // Event or Media: description
            "description" => {
                if let Node::Event(e) = node {
                    if let Some(ref desc) = e.description {
                        writeln!(
                            writer,
                            "      <{}>{}</{}>",
                            child.element_name,
                            escape_xml(desc),
                            child.element_name
                        )?;
                    }
                }
                if let Node::Media(m) = node {
                    if let Some(ref desc) = m.desc {
                        writeln!(
                            writer,
                            "      <{}>{}</{}>",
                            child.element_name,
                            escape_xml(desc),
                            child.element_name
                        )?;
                    }
                }
            }
            // Tag: name or Repository: name
            "name" => {
                if let Node::Tag(t) = node {
                    writeln!(
                        writer,
                        "      <{}>{}</{}>",
                        child.element_name,
                        escape_xml(&t.name),
                        child.element_name
                    )?;
                } else if let Node::Repository(r) = node {
                    if let Some(ref name) = r.name {
                        writeln!(
                            writer,
                            "      <{}>{}</{}>",
                            child.element_name,
                            escape_xml(name),
                            child.element_name
                        )?;
                    }
                }
            }
            // Tag: color (optional)
            "color" => {
                if let Node::Tag(t) = node {
                    if let Some(ref color) = t.color {
                        writeln!(
                            writer,
                            "      <{}>{}</{}>",
                            child.element_name,
                            escape_xml(color),
                            child.element_name
                        )?;
                    }
                }
            }
            // Tag: priority (optional)
            "priority" => {
                if let Node::Tag(t) = node {
                    if let Some(priority) = t.priority {
                        writeln!(
                            writer,
                            "      <{}>{}</{}>",
                            child.element_name, priority, child.element_name
                        )?;
                    }
                }
            }
            // Citation: confidence (optional)
            "confidence" => {
                if let Node::Citation(c) = node {
                    if let Some(conf) = c.confidence {
                        writeln!(
                            writer,
                            "      <{}>{}</{}>",
                            child.element_name, conf, child.element_name
                        )?;
                    }
                }
            }
            // Source: title (required)
            "title" => {
                if let Node::Source(s) = node {
                    writeln!(writer, "      <stitle>{}</stitle>", escape_xml(&s.title))?;
                }
                // Place: name is a Location struct
                if let Node::Place(p) = node {
                    writeln!(
                        writer,
                        "      <ptitle>{}</ptitle>",
                        escape_xml(p.name.city.as_deref().unwrap_or(""))
                    )?;
                }
            }
            // Source: abbrev (author or pubinfo)
            "abbrev" => {
                if let Node::Source(s) = node {
                    let abbrev = s.author.as_deref().or(s.pubinfo.as_deref()).unwrap_or("");
                    if !abbrev.is_empty() {
                        writeln!(writer, "      <sabbrev>{}</sabbrev>", escape_xml(abbrev))?;
                    }
                }
            }
            // Media: file path
            "file" => {
                if let Node::Media(m) = node {
                    if let Some(ref path) = m.path {
                        writeln!(writer, "      <file>{}</file>", escape_xml(path))?;
                    }
                }
            }
            // Note: text (required)
            "text" => {
                if let Node::Note(n) = node {
                    writeln!(writer, "      <text>{}</text>", escape_xml(&n.text))?;
                }
            }
            // Note: format (optional)
            "format" => {
                if let Node::Note(n) = node {
                    if let Some(fmt) = n.format {
                        writeln!(writer, "      <format>{}</format>", fmt)?;
                    }
                }
            }
            // Repository: name (optional) — handled by "name" above
            _ => {
                // Unknown inline struct field, skip
            }
        }
        Ok(())
    }

    /// Write a Name struct as an XML element.
    fn write_name_element(
        &self,
        writer: &mut impl std::io::Write,
        element_name: &str,
        name: &typed_graph::Name,
    ) -> Result<(), SerializationError> {
        let first = name.first_name.as_deref().unwrap_or("");
        let surname = name
            .surname_list
            .first()
            .and_then(|s| s.surname.as_deref())
            .unwrap_or("");
        writeln!(
            writer,
            "      <{}><first>{}</first><surname>{}</surname></{}>",
            element_name,
            escape_xml(first),
            escape_xml(surname),
            element_name
        )?;
        Ok(())
    }

    /// Write a DateValue as an XML element.
    fn write_date_element(
        &self,
        writer: &mut impl std::io::Write,
        element_name: &str,
        date: &Option<typed_graph::DateValue>,
    ) -> Result<(), SerializationError> {
        if let Some(ref d) = date {
            let date_str = if let (Some(month), Some(day)) = (d.month, d.day) {
                format!("{:04}-{:02}-{:02}", d.year, month, day)
            } else if let Some(month) = d.month {
                format!("{:04}-{:02}", d.year, month)
            } else {
                format!("{:04}", d.year)
            };
            writeln!(
                writer,
                "      <{} val=\"{}\"/>",
                element_name,
                escape_xml(&date_str)
            )?;
        }
        Ok(())
    }

    /// Write array items as child elements.
    fn write_array_items(
        &self,
        _graph: &Graph,
        writer: &mut impl std::io::Write,
        _handle: &str,
        node: &Node,
        child: &XmlChild,
        _field_name: &str,
    ) -> Result<(), SerializationError> {
        match child.element_name.as_str() {
            "name" => {
                // alternate_names
                if let Node::Person(p) = node {
                    for alt_name in &p.alternate_names {
                        self.write_name_element(writer, "name", alt_name)?;
                    }
                }
            }
            _ => {
                // Unknown array, skip
            }
        }
        Ok(())
    }

    /// Write edge items as child elements.
    fn write_edge_items(
        &self,
        graph: &Graph,
        writer: &mut impl std::io::Write,
        handle: &str,
        child: &XmlChild,
        edge_name: &str,
    ) -> Result<(), SerializationError> {
        let handle_owned = handle.to_string();
        let edges_from = graph.edges_from(&handle_owned);
        let matching_edges: Vec<&Edge> = edges_from
            .iter()
            .filter(|e| edge_variant_name(e) == Some(edge_name))
            .copied()
            .collect();

        if matching_edges.is_empty() {
            return Ok(());
        }

        for edge in &matching_edges {
            let target_handle = edge_target_handle(edge);
            match child.element_name.as_str() {
                // --- Simple hlink refs ---
                "childin" | "parentin" | "father" | "mother" | "place" | "sourceref"
                | "placeref" => {
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle)
                    )?;
                }

                // --- Citation refs ---
                "citationref" => {
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle)
                    )?;
                }

                // --- Note refs ---
                "noteref" => {
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle)
                    )?;
                }

                // --- Media refs ---
                "mediaref" => {
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle)
                    )?;
                }

                // --- Tag refs ---
                "tagref" => {
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle)
                    )?;
                }

                // --- Event refs (with role metadata) ---
                "eventref" => {
                    let role = get_edge_role(edge);
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"><role>{}</role></{}>",
                        child.element_name,
                        escape_xml(&target_handle),
                        escape_xml(&role),
                        child.element_name
                    )?;
                }

                // --- Child refs (with relation metadata) ---
                "childref" => {
                    let rel = get_edge_relation(edge);
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\" rel=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle),
                        escape_xml(&rel)
                    )?;
                }

                // --- Person refs ---
                "personref" => {
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle)
                    )?;
                }

                // --- Repo refs ---
                "reporef" => {
                    writeln!(
                        writer,
                        "      <{} hlink=\"{}\"/>",
                        child.element_name,
                        escape_xml(&target_handle)
                    )?;
                }

                _ => {
                    // Unknown edge element, skip
                }
            }
        }
        Ok(())
    }

    /// Extract a field value from a Node by field name.
    fn get_field_value(&self, node: &Node, field_name: &str) -> Option<String> {
        match node {
            Node::Person(p) => match field_name {
                "handle" => Some(p.handle.clone()),
                "gramps_id" => p.gramps_id.clone(),
                _ => None,
            },
            Node::Family(f) => match field_name {
                "handle" => Some(f.handle.clone()),
                "gramps_id" => f.gramps_id.clone(),
                _ => None,
            },
            Node::Event(e) => match field_name {
                "handle" => Some(e.handle.clone()),
                "gramps_id" => e.gramps_id.clone(),
                _ => None,
            },
            Node::Place(p) => match field_name {
                "handle" => Some(p.handle.clone()),
                "gramps_id" => p.gramps_id.clone(),
                _ => None,
            },
            Node::Source(s) => match field_name {
                "handle" => Some(s.handle.clone()),
                "gramps_id" => s.gramps_id.clone(),
                _ => None,
            },
            Node::Citation(c) => match field_name {
                "handle" => Some(c.handle.clone()),
                "gramps_id" => c.gramps_id.clone(),
                _ => None,
            },
            Node::Repository(r) => match field_name {
                "handle" => Some(r.handle.clone()),
                "gramps_id" => r.gramps_id.clone(),
                _ => None,
            },
            Node::Media(m) => match field_name {
                "handle" => Some(m.handle.clone()),
                "gramps_id" => m.gramps_id.clone(),
                _ => None,
            },
            Node::Note(n) => match field_name {
                "handle" => Some(n.handle.clone()),
                "gramps_id" => n.gramps_id.clone(),
                _ => None,
            },
            Node::Tag(t) => match field_name {
                "handle" => Some(t.handle.clone()),
                "gramps_id" => t.gramps_id.clone(),
                _ => None,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Return the XML element name for a node, based on its type.
fn node_type_name(node: &Node) -> Option<&'static str> {
    match node {
        Node::Person(_) => Some("person"),
        Node::Family(_) => Some("family"),
        Node::Event(_) => Some("event"),
        Node::Place(_) => Some("placeobj"),
        Node::Source(_) => Some("source"),
        Node::Citation(_) => Some("citation"),
        Node::Repository(_) => Some("repository"),
        Node::Media(_) => Some("object"),
        Node::Note(_) => Some("note"),
        Node::Tag(_) => Some("tag"),
    }
}

/// Return the edge variant name as a string for lookup in the edge map.
fn edge_variant_name(edge: &Edge) -> Option<&'static str> {
    match edge {
        Edge::PersonFamily { .. } => Some("PersonFamily"),
        Edge::PersonParentFamily { .. } => Some("PersonParentFamily"),
        Edge::PersonEventRef { .. } => Some("PersonEventRef"),
        Edge::FamilyFather { .. } => Some("FamilyFather"),
        Edge::FamilyMother { .. } => Some("FamilyMother"),
        Edge::FamilyChildRef { .. } => Some("FamilyChildRef"),
        Edge::FamilyEventRef { .. } => Some("FamilyEventRef"),
        Edge::EventPlace { .. } => Some("EventPlace"),
        Edge::CitationSource { .. } => Some("CitationSource"),
        Edge::PersonPersonRef { .. } => Some("PersonPersonRef"),
        Edge::SourceRepoRef { .. } => Some("SourceRepoRef"),
        Edge::PlacePlaceRef { .. } => Some("PlacePlaceRef"),
        // Mixin refs (used across multiple types)
        Edge::CitationRef { .. } => Some("CitationRef"),
        Edge::NoteRef { .. } => Some("NoteRef"),
        Edge::MediaRef { .. } => Some("MediaRef"),
        Edge::TagRef { .. } => Some("TagRef"),
        // Person-specific edges
        Edge::PersonCitation { .. } => Some("PersonCitation"),
        Edge::PersonNote { .. } => Some("PersonNote"),
        Edge::PersonMediaRef { .. } => Some("PersonMediaRef"),
        Edge::PersonTag { .. } => Some("PersonTag"),
        // Event-specific edges
        Edge::EventCitation { .. } => Some("EventCitation"),
        Edge::EventNote { .. } => Some("EventNote"),
        Edge::EventMediaRef { .. } => Some("EventMediaRef"),
        Edge::EventTag { .. } => Some("EventTag"),
        // Family-specific edges
        Edge::FamilyCitation { .. } => Some("FamilyCitation"),
        Edge::FamilyNote { .. } => Some("FamilyNote"),
        Edge::FamilyMediaRef { .. } => Some("FamilyMediaRef"),
        Edge::FamilyTag { .. } => Some("FamilyTag"),
        // Citation-specific edges
        Edge::CitationMediaRef { .. } => Some("CitationMediaRef"),
        Edge::CitationNote { .. } => Some("CitationNote"),
        Edge::CitationTag { .. } => Some("CitationTag"),
        // Media-specific edges
        Edge::MediaCitation { .. } => Some("MediaCitation"),
        Edge::MediaNote { .. } => Some("MediaNote"),
        Edge::MediaTag { .. } => Some("MediaTag"),
        // Note-specific edges
        Edge::NoteCitation { .. } => Some("NoteCitation"),
        Edge::NoteTag { .. } => Some("NoteTag"),
        // Source-specific edges
        Edge::SourceMediaRef { .. } => Some("SourceMediaRef"),
        Edge::SourceNote { .. } => Some("SourceNote"),
        Edge::SourceTag { .. } => Some("SourceTag"),
        // Place-specific edges
        Edge::PlaceCitation { .. } => Some("PlaceCitation"),
        Edge::PlaceMediaRef { .. } => Some("PlaceMediaRef"),
        Edge::PlaceNote { .. } => Some("PlaceNote"),
        Edge::PlaceTag { .. } => Some("PlaceTag"),
        // Repository-specific edges
        Edge::RepositoryMediaRef { .. } => Some("RepositoryMediaRef"),
        Edge::RepositoryNote { .. } => Some("RepositoryNote"),
        Edge::RepositoryTag { .. } => Some("RepositoryTag"),
        // Tag-specific
        Edge::TagTag { .. } => Some("TagTag"),
    }
}

/// Return the target handle of an edge.
fn edge_target_handle(edge: &Edge) -> Handle {
    match edge {
        Edge::PersonFamily { target, .. } => target.clone(),
        Edge::PersonParentFamily { target, .. } => target.clone(),
        Edge::PersonEventRef { target, .. } => target.clone(),
        Edge::FamilyFather { target, .. } => target.clone(),
        Edge::FamilyMother { target, .. } => target.clone(),
        Edge::FamilyChildRef { target, .. } => target.clone(),
        Edge::FamilyEventRef { target, .. } => target.clone(),
        Edge::EventPlace { target, .. } => target.clone(),
        Edge::CitationSource { target, .. } => target.clone(),
        Edge::PersonPersonRef { target, .. } => target.clone(),
        Edge::SourceRepoRef { target, .. } => target.clone(),
        Edge::PlacePlaceRef { target, .. } => target.clone(),
        Edge::CitationRef { target, .. } => target.clone(),
        Edge::NoteRef { target, .. } => target.clone(),
        Edge::MediaRef { target, .. } => target.clone(),
        Edge::TagRef { target, .. } => target.clone(),
        Edge::PersonCitation { target, .. } => target.clone(),
        Edge::PersonNote { target, .. } => target.clone(),
        Edge::PersonMediaRef { target, .. } => target.clone(),
        Edge::PersonTag { target, .. } => target.clone(),
        Edge::EventCitation { target, .. } => target.clone(),
        Edge::EventNote { target, .. } => target.clone(),
        Edge::EventMediaRef { target, .. } => target.clone(),
        Edge::EventTag { target, .. } => target.clone(),
        Edge::FamilyCitation { target, .. } => target.clone(),
        Edge::FamilyNote { target, .. } => target.clone(),
        Edge::FamilyMediaRef { target, .. } => target.clone(),
        Edge::FamilyTag { target, .. } => target.clone(),
        Edge::CitationMediaRef { target, .. } => target.clone(),
        Edge::CitationNote { target, .. } => target.clone(),
        Edge::CitationTag { target, .. } => target.clone(),
        Edge::MediaCitation { target, .. } => target.clone(),
        Edge::MediaNote { target, .. } => target.clone(),
        Edge::MediaTag { target, .. } => target.clone(),
        Edge::NoteCitation { target, .. } => target.clone(),
        Edge::NoteTag { target, .. } => target.clone(),
        Edge::SourceMediaRef { target, .. } => target.clone(),
        Edge::SourceNote { target, .. } => target.clone(),
        Edge::SourceTag { target, .. } => target.clone(),
        Edge::PlaceCitation { target, .. } => target.clone(),
        Edge::PlaceMediaRef { target, .. } => target.clone(),
        Edge::PlaceNote { target, .. } => target.clone(),
        Edge::PlaceTag { target, .. } => target.clone(),
        Edge::RepositoryMediaRef { target, .. } => target.clone(),
        Edge::RepositoryNote { target, .. } => target.clone(),
        Edge::RepositoryTag { target, .. } => target.clone(),
        Edge::TagTag { target, .. } => target.clone(),
    }
}

/// Get the role string from an event ref edge.
fn get_edge_role(edge: &Edge) -> String {
    match edge {
        Edge::PersonEventRef { metadata, .. } | Edge::FamilyEventRef { metadata, .. } => {
            format!(
                "{:?}",
                metadata
                    .role
                    .as_ref()
                    .unwrap_or(&typed_graph::EventRoleType::Primary)
            )
        }
        _ => "Primary".to_string(),
    }
}

/// Get the relation string from a child ref edge.
fn get_edge_relation(edge: &Edge) -> String {
    match edge {
        Edge::FamilyChildRef { metadata, .. } => {
            format!(
                "{:?}",
                metadata
                    .relation
                    .as_ref()
                    .unwrap_or(&typed_graph::ChildRefType::Birth)
            )
        }
        _ => "Birth".to_string(),
    }
}

/// Check if a year is a leap year.
fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Escape special XML characters in a string.
fn escape_xml(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    // Allow needless_update in test helpers because struct field sets differ
    // between schema-5-1 and schema-5-2 features (e.g., birth_ref_index exists
    // only in 5-1, while all fields are specified in 5-2).
    #![allow(clippy::needless_update)]
    use super::*;
    use crate::SerializationMap;
    use typed_graph::graph::Graph;
    use typed_graph::*;

    /// Helper: create a minimal Person node.
    fn make_person(handle: &str, gramps_id: Option<&str>) -> Node {
        Node::Person(PersonData {
            handle: handle.to_string(),
            gramps_id: gramps_id.map(|s| s.to_string()),
            gender: typed_graph::graph::into_gender_field(0),
            primary_name: Name {
                first_name: Some("John".to_string()),
                surname_list: vec![Surname {
                    surname: Some("Doe".to_string()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            alternate_names: vec![],
            event_ref_list: vec![],
            family_list: vec![],
            parent_family_list: vec![],
            person_ref_list: vec![],
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            address_list: vec![],
            url_list: vec![],
            lds_ord_list: vec![],
            ..Default::default()
        })
    }

    /// Helper: create a minimal Family node.
    fn make_family(handle: &str, gramps_id: Option<&str>) -> Node {
        Node::Family(FamilyData {
            handle: handle.to_string(),
            gramps_id: gramps_id.map(|s| s.to_string()),
            father_handle: None,
            mother_handle: None,
            child_ref_list: vec![],
            event_ref_list: vec![],
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            ..Default::default()
        })
    }

    /// Helper: create a minimal Event node.
    fn make_event(handle: &str, event_type: EventType) -> Node {
        Node::Event(EventData {
            handle: handle.to_string(),
            gramps_id: None,
            event_type: typed_graph::graph::into_event_type_field(event_type),
            date: None,
            place_handle: None,
            description: None,
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            ..Default::default()
        })
    }

    /// Helper: create a minimal Source node.
    fn make_source(handle: &str, title: &str) -> Node {
        Node::Source(SourceData {
            handle: handle.to_string(),
            gramps_id: None,
            title: title.to_string(),
            author: None,
            pubinfo: None,
            reporef_list: vec![],
            note_list: vec![],
            media_list: vec![],
            attribute_list: vec![],
            tag_list: vec![],
            ..Default::default()
        })
    }

    /// Helper: create a minimal Citation node.
    fn make_citation(handle: &str) -> Node {
        Node::Citation(CitationData {
            handle: handle.to_string(),
            gramps_id: None,
            source_handle: typed_graph::graph::into_source_handle_field("s1".to_string()),
            confidence: None,
            page: None,
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            ..Default::default()
        })
    }

    /// Helper: create a minimal Place node.
    fn make_place(handle: &str, city: &str) -> Node {
        Node::Place(PlaceData {
            handle: handle.to_string(),
            gramps_id: None,
            name: Location {
                city: Some(city.to_string()),
                ..Default::default()
            },
            place_ref_list: vec![],
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            ..Default::default()
        })
    }

    /// Helper: create a minimal Repository node.
    fn make_repository(handle: &str, name: &str) -> Node {
        Node::Repository(RepositoryData {
            handle: handle.to_string(),
            gramps_id: None,
            name: Some(name.to_string()),
            type_field: None,
            address_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            url_list: vec![],
        })
    }

    /// Helper: create a minimal Media node.
    fn make_media(handle: &str, path: &str, desc: Option<&str>) -> Node {
        Node::Media(MediaData {
            handle: handle.to_string(),
            gramps_id: None,
            path: Some(path.to_string()),
            desc: desc.map(|s| s.to_string()),
            mime_type: None,
            checksum: None,
            citation_list: vec![],
            note_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            ..Default::default()
        })
    }

    /// Helper: create a minimal Note node.
    fn make_note(handle: &str, text: &str) -> Node {
        Node::Note(NoteData {
            handle: handle.to_string(),
            gramps_id: None,
            text: text.to_string(),
            format: None,
            type_field: None,
            citation_list: vec![],
            tag_list: vec![],
        })
    }

    /// Helper: create a minimal Tag node.
    fn make_tag(handle: &str, name: &str, color: Option<&str>, priority: Option<i32>) -> Node {
        Node::Tag(TagData {
            handle: handle.to_string(),
            gramps_id: None,
            name: name.to_string(),
            color: color.map(|s| s.to_string()),
            priority,
            tag_list: vec![],
        })
    }

    // -----------------------------------------------------------------------
    // Tests for all 10 primary types
    // -----------------------------------------------------------------------

    #[test]
    fn serialize_person_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", Some("I0001")))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<person handle="p1" id="I0001">"#));
        assert!(xml.contains("<gender>M</gender>"));
        assert!(xml.contains("<first>John</first>"));
        assert!(xml.contains("<surname>Doe</surname>"));
    }

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
                gender_int,
                expected_char,
                xml
            );
        }
    }

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

    #[test]
    fn serialize_person_no_gender_attribute() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        // The string 'gender="' should NOT appear (it was an attribute, now a child element)
        assert!(!xml.contains(r#"gender=""#));
        // The child element should be present
        assert!(xml.contains("<gender>M</gender>"));
    }

    #[test]
    fn serialize_person_optional_attributes() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        // gramps_id is None
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<person handle="p1""#));
        // id attribute should not be present when gramps_id is None
        assert!(!xml.contains(r#"id=""#));
    }

    #[test]
    fn serialize_family_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("f1".to_string(), make_family("f1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<family handle="f1""#));
    }

    #[test]
    fn serialize_place_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("pl1".to_string(), make_place("pl1", "Springfield"))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<placeobj handle="pl1""#));
        assert!(xml.contains("<ptitle>Springfield</ptitle>"));
    }

    #[test]
    fn serialize_source_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("s1".to_string(), make_source("s1", "Census 1900"))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<source handle="s1""#));
        assert!(xml.contains("<stitle>Census 1900</stitle>"));
    }

    #[test]
    fn serialize_citation_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("c1".to_string(), make_citation("c1"))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<citation handle="c1""#));
    }

    #[test]
    fn serialize_repository_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("r1".to_string(), make_repository("r1", "National Archives"))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<repository handle="r1""#));
        assert!(xml.contains("<rname>National Archives</rname>"));
    }

    #[test]
    fn serialize_media_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node(
                "m1".to_string(),
                make_media("m1", "photo.jpg", Some("Wedding photo")),
            )
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<object handle="m1""#));
        assert!(xml.contains("<file>photo.jpg</file>"));
        assert!(xml.contains("<description>Wedding photo</description>"));
    }

    #[test]
    fn serialize_note_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("n1".to_string(), make_note("n1", "Some notes here"))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<note handle="n1""#));
        assert!(xml.contains("<text>Some notes here</text>"));
    }

    #[test]
    fn serialize_tag_element() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node(
                "t1".to_string(),
                make_tag("t1", "Complete", Some("red"), Some(1)),
            )
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<tag handle="t1""#));
        assert!(xml.contains("<name>Complete</name>"));
        assert!(xml.contains("<color>red</color>"));
        assert!(xml.contains("<priority>1</priority>"));
    }

    #[test]
    fn serialize_empty_person_omits_optional_nested() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        // Only one <name> element (primary name), no alternate names
        assert_eq!(xml.matches("<name>").count(), 1);
    }

    #[test]
    fn serialize_tag_minimal_fields() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("t1".to_string(), make_tag("t1", "Unfinished", None, None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<tag handle="t1""#));
        // Optional fields should be absent
        assert!(!xml.contains("<color>"));
        assert!(!xml.contains("<priority>"));
    }

    #[test]
    fn serialize_source_with_author() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        let mut source = make_source("s1", "Some Book");
        if let Node::Source(ref mut s) = source {
            s.author = Some("John Smith".to_string());
        }
        graph.add_node("s1".to_string(), source).unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains("<sabbrev>John Smith</sabbrev>"));
    }

    // -----------------------------------------------------------------------
    // Tests for embedded refs and mixins (Step 3)
    // -----------------------------------------------------------------------

    #[test]
    fn serialize_person_eventref() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_node("e1".to_string(), make_event("e1", EventType::Birth))
            .unwrap();
        graph
            .add_edge(Edge::PersonEventRef {
                source: "p1".to_string(),
                target: "e1".to_string(),
                metadata: Box::new(typed_graph::graph::make_event_ref(
                    "e1".to_string(),
                    Some(EventRoleType::Primary),
                )),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<eventref hlink="e1""#));
        assert!(xml.contains("<role>Primary</role>"));
    }

    #[test]
    fn serialize_family_childref() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("f1".to_string(), make_family("f1", None))
            .unwrap();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_edge(Edge::FamilyChildRef {
                source: "f1".to_string(),
                target: "p1".to_string(),
                metadata: Box::new(typed_graph::graph::make_child_ref(
                    "p1".to_string(),
                    Some(ChildRefType::Birth),
                )),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<childref hlink="p1" rel="Birth""#));
    }

    #[test]
    fn serialize_person_personref() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_node("p2".to_string(), make_person("p2", None))
            .unwrap();
        graph
            .add_edge(Edge::PersonPersonRef {
                source: "p1".to_string(),
                target: "p2".to_string(),
                metadata: Box::new(PersonRef {
                    ref_field: "p2".to_string(),
                    relation: Some(FamilyRelType::Married),
                    ..Default::default()
                }),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<personref hlink="p2""#));
    }

    #[test]
    fn serialize_source_reporef() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("s1".to_string(), make_source("s1", "Some Source"))
            .unwrap();
        graph
            .add_node("r1".to_string(), make_repository("r1", "Archive"))
            .unwrap();
        graph
            .add_edge(Edge::SourceRepoRef {
                source: "s1".to_string(),
                target: "r1".to_string(),
                metadata: Box::new(RepoRef {
                    ref_field: "r1".to_string(),
                    call_number: None,
                    media_type: None,
                    ..Default::default()
                }),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<reporef hlink="r1""#));
    }

    #[test]
    fn serialize_citationref() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_node("c1".to_string(), make_citation("c1"))
            .unwrap();
        graph
            .add_edge(Edge::PersonCitation {
                source: "p1".to_string(),
                target: "c1".to_string(),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<citationref hlink="c1""#));
    }

    #[test]
    fn serialize_noteref() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_node("n1".to_string(), make_note("n1", "A note"))
            .unwrap();
        graph
            .add_edge(Edge::PersonNote {
                source: "p1".to_string(),
                target: "n1".to_string(),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<noteref hlink="n1""#));
    }

    #[cfg(not(feature = "schema-5-1"))]
    mod schema_5_2_edge_tests {
        use super::*;

        #[test]
        fn serialize_mediaref() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.2.0");
            let mut graph = Graph::new();
            graph
                .add_node("p1".to_string(), make_person("p1", None))
                .unwrap();
            graph
                .add_node("m1".to_string(), make_media("m1", "file.jpg", None))
                .unwrap();
            graph
                .add_edge(Edge::PersonMediaRef {
                    source: "p1".to_string(),
                    target: "m1".to_string(),
                    metadata: Box::new(MediaRef {
                        ref_field: "m1".to_string(),
                        ..Default::default()
                    }),
                })
                .unwrap();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            assert!(xml.contains(r#"<mediaref hlink="m1""#));
        }
    }

    #[test]
    fn serialize_tagref() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_node("t1".to_string(), make_tag("t1", "Complete", None, None))
            .unwrap();
        graph
            .add_edge(Edge::PersonTag {
                source: "p1".to_string(),
                target: "t1".to_string(),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"<tagref hlink="t1""#));
    }

    #[test]
    fn serialize_multiple_refs() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_node("n1".to_string(), make_note("n1", "Note 1"))
            .unwrap();
        graph
            .add_node("n2".to_string(), make_note("n2", "Note 2"))
            .unwrap();
        graph
            .add_edge(Edge::PersonNote {
                source: "p1".to_string(),
                target: "n1".to_string(),
            })
            .unwrap();
        graph
            .add_edge(Edge::PersonNote {
                source: "p1".to_string(),
                target: "n2".to_string(),
            })
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        // Both note refs should be present
        assert!(xml.contains(r#"<noteref hlink="n1"/>"#));
        assert!(xml.contains(r#"<noteref hlink="n2"/>"#));
    }

    // -----------------------------------------------------------------------
    // Tests for XML document structure, header, and ordering (Step 4)
    // -----------------------------------------------------------------------

    #[test]
    fn xml_document_structure_complete() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(xml.contains("<database"));
        assert!(xml.contains("<header>"));
        assert!(xml.contains("</database>"));
        assert!(xml.ends_with("</database>\n"));
    }

    #[test]
    fn xml_header_content() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let graph = Graph::new();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains("<created date="));
        assert!(xml.contains(r#"version="5.2.0""#));
        assert!(xml.contains("<resname>Generated by gramps-gen</resname>"));
    }

    #[test]
    fn xml_header_version_parameter() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.1.6");
        let graph = Graph::new();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains(r#"version="5.1.6""#));
        assert!(!xml.contains(r#"version="5.2.0""#));
    }

    #[test]
    fn xml_section_order_is_correct() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        // Add one node of each type
        graph
            .add_node("t1".to_string(), make_tag("t1", "T", None, None))
            .unwrap();
        graph
            .add_node("e1".to_string(), make_event("e1", EventType::Birth))
            .unwrap();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();
        graph
            .add_node("f1".to_string(), make_family("f1", None))
            .unwrap();
        graph
            .add_node("c1".to_string(), make_citation("c1"))
            .unwrap();
        graph
            .add_node("s1".to_string(), make_source("s1", "S"))
            .unwrap();
        graph
            .add_node("pl1".to_string(), make_place("pl1", "City"))
            .unwrap();
        graph
            .add_node("m1".to_string(), make_media("m1", "f.jpg", None))
            .unwrap();
        graph
            .add_node("r1".to_string(), make_repository("r1", "R"))
            .unwrap();
        graph
            .add_node("n1".to_string(), make_note("n1", "N"))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        // Check the order of sections: header first, then tags, events, people, ...
        let header_pos = xml.find("<header>").unwrap();
        let tags_pos = xml.find("<tags>").unwrap();
        let events_pos = xml.find("<events>").unwrap();
        let people_pos = xml.find("<people>").unwrap();
        let families_pos = xml.find("<families>").unwrap();
        let citations_pos = xml.find("<citations>").unwrap();
        let sources_pos = xml.find("<sources>").unwrap();
        let places_pos = xml.find("<places>").unwrap();
        let objects_pos = xml.find("<objects>").unwrap();
        let repositories_pos = xml.find("<repositories>").unwrap();
        let notes_pos = xml.find("<notes>").unwrap();

        assert!(header_pos < tags_pos, "header before tags");
        assert!(tags_pos < events_pos, "tags before events");
        assert!(events_pos < people_pos, "events before people");
        assert!(people_pos < families_pos, "people before families");
        assert!(families_pos < citations_pos, "families before citations");
        assert!(citations_pos < sources_pos, "citations before sources");
        assert!(sources_pos < places_pos, "sources before places");
        assert!(places_pos < objects_pos, "places before objects");
        assert!(
            objects_pos < repositories_pos,
            "objects before repositories"
        );
        assert!(repositories_pos < notes_pos, "repositories before notes");
    }

    #[test]
    fn xml_empty_sections_omitted() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        // Empty graph — only header should be present, no sections
        let graph = Graph::new();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        // No type sections should appear
        assert!(!xml.contains("<tags>"));
        assert!(!xml.contains("<people>"));
        assert!(!xml.contains("<families>"));
    }

    #[test]
    fn xml_escapes_special_characters() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("n1".to_string(), make_note("n1", "AT&T test <value>"))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains("AT&amp;T"));
        assert!(xml.contains("&lt;value&gt;"));
    }

    #[test]
    fn xml_roundtrip_single_person() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        // Validate that it's well-formed XML by trying to parse with quick-xml
        use quick_xml::events::Event;
        use quick_xml::Reader;
        let mut reader = Reader::from_str(&xml);
        reader.config_mut().trim_text(true);
        let mut depth = 0u32;
        let mut ran_ok = false;
        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if name == "database" && depth == 0 {
                        ran_ok = true;
                    }
                    depth += 1;
                }
                Ok(Event::End(_)) => depth -= 1,
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("XML parse error: {}", e),
            }
        }
        assert!(ran_ok, "XML should parse successfully");
    }

    // -----------------------------------------------------------------------
    // Tests for edge role and relation serialization
    // -----------------------------------------------------------------------

    #[test]
    fn get_edge_role_primary() {
        let edge = Edge::PersonEventRef {
            source: "p1".to_string(),
            target: "e1".to_string(),
            metadata: Box::new(typed_graph::graph::make_event_ref(
                "e1".to_string(),
                Some(EventRoleType::Primary),
            )),
        };
        assert_eq!(get_edge_role(&edge), "Primary");
    }

    #[test]
    fn get_edge_role_witness() {
        let edge = Edge::PersonEventRef {
            source: "p1".to_string(),
            target: "e1".to_string(),
            metadata: Box::new(typed_graph::graph::make_event_ref(
                "e1".to_string(),
                Some(EventRoleType::Witness),
            )),
        };
        assert_eq!(get_edge_role(&edge), "Witness");
    }

    #[test]
    fn get_edge_role_none_falls_back_to_primary() {
        // When role is None, get_edge_role should fall back to "Primary"
        let edge = Edge::PersonEventRef {
            source: "p1".to_string(),
            target: "e1".to_string(),
            metadata: Box::new(typed_graph::graph::make_event_ref("e1".to_string(), None)),
        };
        assert_eq!(get_edge_role(&edge), "Primary");
    }

    #[test]
    fn get_edge_role_family_event() {
        let edge = Edge::FamilyEventRef {
            source: "f1".to_string(),
            target: "e1".to_string(),
            metadata: Box::new(typed_graph::graph::make_event_ref(
                "e1".to_string(),
                Some(EventRoleType::Family),
            )),
        };
        assert_eq!(get_edge_role(&edge), "Family");
    }

    #[test]
    fn get_edge_role_unknown_edge_returns_primary() {
        let edge = Edge::FamilyFather {
            source: "f1".to_string(),
            target: "p1".to_string(),
        };
        assert_eq!(get_edge_role(&edge), "Primary");
    }

    #[test]
    fn get_edge_relation_birth() {
        let edge = Edge::FamilyChildRef {
            source: "f1".to_string(),
            target: "p1".to_string(),
            metadata: Box::new(typed_graph::graph::make_child_ref(
                "p1".to_string(),
                Some(ChildRefType::Birth),
            )),
        };
        assert_eq!(get_edge_relation(&edge), "Birth");
    }

    #[test]
    fn get_edge_relation_adopted() {
        let edge = Edge::FamilyChildRef {
            source: "f1".to_string(),
            target: "p1".to_string(),
            metadata: Box::new(typed_graph::graph::make_child_ref(
                "p1".to_string(),
                Some(ChildRefType::Adopted),
            )),
        };
        assert_eq!(get_edge_relation(&edge), "Adopted");
    }

    #[test]
    fn get_edge_relation_stepchild() {
        let edge = Edge::FamilyChildRef {
            source: "f1".to_string(),
            target: "p1".to_string(),
            metadata: Box::new(typed_graph::graph::make_child_ref(
                "p1".to_string(),
                Some(ChildRefType::Stepchild),
            )),
        };
        assert_eq!(get_edge_relation(&edge), "Stepchild");
    }

    #[test]
    fn get_edge_relation_none_falls_back_to_birth() {
        let edge = Edge::FamilyChildRef {
            source: "f1".to_string(),
            target: "p1".to_string(),
            metadata: Box::new(typed_graph::graph::make_child_ref("p1".to_string(), None)),
        };
        assert_eq!(get_edge_relation(&edge), "Birth");
    }

    #[test]
    fn get_edge_relation_unknown_edge_returns_birth() {
        let edge = Edge::FamilyFather {
            source: "f1".to_string(),
            target: "p1".to_string(),
        };
        assert_eq!(get_edge_relation(&edge), "Birth");
    }

    // -----------------------------------------------------------------------
    // 5.2-specific tests — depend on 5.2-specific type shapes
    // -----------------------------------------------------------------------
    // Integration tests for namespace and version
    // -----------------------------------------------------------------------

    #[test]
    fn xml_namespace_52() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains("http://gramps-project.org/xml/1.7.2/"));
    }

    #[test]
    fn xml_namespace_51() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.1.6");
        let mut graph = Graph::new();
        graph
            .add_node("p1".to_string(), make_person("p1", None))
            .unwrap();

        let mut output = Vec::new();
        writer.write(&graph, &mut output).unwrap();
        let xml = String::from_utf8(output).unwrap();

        assert!(xml.contains("http://gramps-project.org/xml/1.7.1/"));
    }

    // -----------------------------------------------------------------------

    #[cfg(not(feature = "schema-5-1"))]
    mod schema_5_2_tests {
        use super::*;

        #[test]
        fn serialize_event_element() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.2.0");
            let mut graph = Graph::new();
            graph
                .add_node("e1".to_string(), make_event("e1", EventType::Birth))
                .unwrap();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            assert!(xml.contains(r#"<event handle="e1""#));
            // Event should have eventtype with type child
            assert!(xml.contains("<eventtype><type>Birth</type></eventtype>"));
        }

        #[test]
        fn serialize_event_with_date() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.2.0");
            let mut graph = Graph::new();
            let mut event = make_event("e1", EventType::Birth);
            if let Node::Event(ref mut e) = event {
                e.date = Some(DateValue::new_ymd(1890, 6, 15));
            }
            graph.add_node("e1".to_string(), event).unwrap();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            assert!(xml.contains(r#"val="1890-06-15""#));
        }

        #[test]
        fn xml_header_content() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.2.0");
            let graph = Graph::new();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            assert!(xml.contains("<created date="));
            assert!(xml.contains(r#"version="5.2.0""#));
            assert!(xml.contains("<resname>Generated by gramps-gen</resname>"));
        }

        #[test]
        fn xml_header_version_parameter() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.1.6");
            let graph = Graph::new();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            assert!(xml.contains(r#"version="5.1.6""#));
            assert!(!xml.contains(r#"version="5.2.0""#));
        }
    }

    // -----------------------------------------------------------------------
    // 5.1-specific test scaffold (to be filled in Steps 4-6)
    // -----------------------------------------------------------------------

    #[cfg(feature = "schema-5-1")]
    mod schema_5_1_tests {
        use super::*;

        #[test]
        fn serialize_event_type_renders_as_death_not_some_death() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.1.6");
            let mut graph = Graph::new();
            graph
                .add_node("e1".to_string(), make_event("e1", EventType::Death))
                .unwrap();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            // Event type should render as "Death", not "Some(Death)"
            assert!(
                xml.contains("<eventtype><type>Death</type></eventtype>"),
                "Event type should render as Death, not Some(Death). Got: {}",
                xml
            );
            assert!(
                !xml.contains("Some(Death)"),
                "Event type should NOT contain 'Some(Death)'. Got: {}",
                xml
            );
        }

        #[test]
        fn xml_header_version_51() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.1.6");
            let graph = Graph::new();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            assert!(xml.contains(r#"version="5.1.6""#));
        }

        #[test]
        fn xml_namespace_51() {
            let map = SerializationMap::new();
            let writer = GraphXmlWriter::new(map, "5.1.6");
            let mut graph = Graph::new();
            graph
                .add_node("p1".to_string(), make_person("p1", None))
                .unwrap();

            let mut output = Vec::new();
            writer.write(&graph, &mut output).unwrap();
            let xml = String::from_utf8(output).unwrap();

            assert!(xml.contains("http://gramps-project.org/xml/1.7.1/"));
        }
    }
}
