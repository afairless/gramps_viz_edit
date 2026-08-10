use std::collections::HashMap;

/// Maps Graph types to their XML element and attribute names.
///
/// This follows the Gramps XML RelaxNG schema. Person → `"person"` element,
/// family → `"family"` element, etc. Hand-coded initially; the design
/// (Decision 5) describes extracting this from the RelaxNG schema at
/// build time in a future iteration.
#[derive(Clone, Debug, PartialEq)]
pub struct SerializationMap {
    /// Maps primary type name to its XML element info.
    pub type_map: HashMap<String, XmlTypeInfo>,
    /// Maps edge variant name to the XML nesting and attributes.
    pub edge_map: HashMap<String, XmlEdgeInfo>,
    /// Order in which type sections appear in the XML output.
    pub section_order: Vec<String>,
}

/// Information about how a primary type maps to XML.
#[derive(Clone, Debug, PartialEq)]
pub struct XmlTypeInfo {
    /// The XML element name (e.g., "person").
    pub element_name: String,
    /// The section name in the XML output (e.g., "people").
    pub section_name: String,
    /// Attributes to write on the element.
    pub attributes: Vec<XmlAttribute>,
    /// Child elements to nest inside the element.
    pub children: Vec<XmlChild>,
}

/// Information about how an edge maps to XML.
#[derive(Clone, Debug, PartialEq)]
pub struct XmlEdgeInfo {
    /// The parent XML element this edge nests inside.
    pub parent_element: String,
    /// The XML element name for this edge (e.g., "eventref").
    pub element_name: String,
    /// Attribute mappings: (field name, XML attribute name).
    pub attributes: Vec<(String, String)>,
}

/// An attribute mapping from a data struct field to an XML attribute.
#[derive(Clone, Debug, PartialEq)]
pub struct XmlAttribute {
    /// Field name in the data struct.
    pub field: String,
    /// XML attribute name (e.g., "hlink" for handle refs).
    pub attr_name: String,
}

/// A child element nested inside a type's XML element.
#[derive(Clone, Debug, PartialEq)]
pub struct XmlChild {
    /// The XML element name for this child.
    pub element_name: String,
    /// Where the child data comes from.
    pub source: XmlChildSource,
}

/// Describes the source of data for a child XML element.
#[derive(Clone, Debug, PartialEq)]
pub enum XmlChildSource {
    /// Data comes from an inline struct field (e.g., primary_name → Name).
    InlineStruct(String),
    /// Data comes from an array field (e.g., alternate_names).
    Array(String),
    /// Data comes from an edge variant (e.g., PersonEventRef).
    Edge(String),
}

impl SerializationMap {
    /// Build a hand-coded `SerializationMap` for all 10 primary types.
    ///
    /// This is the initial implementation. The design describes extracting
    /// this mapping from the Gramps RelaxNG schema at build time in a
    /// future iteration.
    pub fn new() -> Self {
        let mut type_map = HashMap::new();
        let mut edge_map = HashMap::new();

        // -----------------------------------------------------------------------
        // Tags
        // -----------------------------------------------------------------------
        type_map.insert(
            "Tag".to_string(),
            XmlTypeInfo {
                element_name: "tag".to_string(),
                section_name: "tags".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "name".to_string(),
                        source: XmlChildSource::InlineStruct("name".to_string()),
                    },
                    XmlChild {
                        element_name: "color".to_string(),
                        source: XmlChildSource::InlineStruct("color".to_string()),
                    },
                    XmlChild {
                        element_name: "priority".to_string(),
                        source: XmlChildSource::InlineStruct("priority".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Events
        // -----------------------------------------------------------------------
        type_map.insert(
            "Event".to_string(),
            XmlTypeInfo {
                element_name: "event".to_string(),
                section_name: "events".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "eventtype".to_string(),
                        source: XmlChildSource::InlineStruct("event_type".to_string()),
                    },
                    XmlChild {
                        element_name: "dateval".to_string(),
                        source: XmlChildSource::InlineStruct("date".to_string()),
                    },
                    XmlChild {
                        element_name: "description".to_string(),
                        source: XmlChildSource::InlineStruct("description".to_string()),
                    },
                    XmlChild {
                        element_name: "place".to_string(),
                        source: XmlChildSource::Edge("EventPlace".to_string()),
                    },
                    XmlChild {
                        element_name: "citationref".to_string(),
                        source: XmlChildSource::Edge("EventCitation".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("EventNote".to_string()),
                    },
                    XmlChild {
                        element_name: "mediaref".to_string(),
                        source: XmlChildSource::Edge("EventMediaRef".to_string()),
                    },
                    XmlChild {
                        element_name: "tagref".to_string(),
                        source: XmlChildSource::Edge("EventTag".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // People
        // -----------------------------------------------------------------------
        type_map.insert(
            "Person".to_string(),
            XmlTypeInfo {
                element_name: "person".to_string(),
                section_name: "people".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "gender".to_string(),
                        source: XmlChildSource::InlineStruct("gender".to_string()),
                    },
                    XmlChild {
                        element_name: "name".to_string(),
                        source: XmlChildSource::InlineStruct("primary_name".to_string()),
                    },
                    XmlChild {
                        element_name: "name".to_string(),
                        source: XmlChildSource::Array("alternate_names".to_string()),
                    },
                    XmlChild {
                        element_name: "eventref".to_string(),
                        source: XmlChildSource::Edge("PersonEventRef".to_string()),
                    },
                    XmlChild {
                        element_name: "personref".to_string(),
                        source: XmlChildSource::Edge("PersonPersonRef".to_string()),
                    },
                    XmlChild {
                        element_name: "parentin".to_string(),
                        source: XmlChildSource::Edge("PersonParentFamily".to_string()),
                    },
                    XmlChild {
                        element_name: "childin".to_string(),
                        source: XmlChildSource::Edge("PersonFamily".to_string()),
                    },
                    XmlChild {
                        element_name: "citationref".to_string(),
                        source: XmlChildSource::Edge("PersonCitation".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("PersonNote".to_string()),
                    },
                    XmlChild {
                        element_name: "mediaref".to_string(),
                        source: XmlChildSource::Edge("PersonMediaRef".to_string()),
                    },
                    XmlChild {
                        element_name: "tagref".to_string(),
                        source: XmlChildSource::Edge("PersonTag".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Families
        // -----------------------------------------------------------------------
        type_map.insert(
            "Family".to_string(),
            XmlTypeInfo {
                element_name: "family".to_string(),
                section_name: "families".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "rel".to_string(),
                        source: XmlChildSource::Edge("FamilyRelation".to_string()),
                    },
                    XmlChild {
                        element_name: "father".to_string(),
                        source: XmlChildSource::Edge("FamilyFather".to_string()),
                    },
                    XmlChild {
                        element_name: "mother".to_string(),
                        source: XmlChildSource::Edge("FamilyMother".to_string()),
                    },
                    XmlChild {
                        element_name: "childref".to_string(),
                        source: XmlChildSource::Edge("FamilyChildRef".to_string()),
                    },
                    XmlChild {
                        element_name: "eventref".to_string(),
                        source: XmlChildSource::Edge("FamilyEventRef".to_string()),
                    },
                    XmlChild {
                        element_name: "citationref".to_string(),
                        source: XmlChildSource::Edge("FamilyCitation".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("FamilyNote".to_string()),
                    },
                    XmlChild {
                        element_name: "mediaref".to_string(),
                        source: XmlChildSource::Edge("FamilyMediaRef".to_string()),
                    },
                    XmlChild {
                        element_name: "tagref".to_string(),
                        source: XmlChildSource::Edge("FamilyTag".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Citations
        // -----------------------------------------------------------------------
        type_map.insert(
            "Citation".to_string(),
            XmlTypeInfo {
                element_name: "citation".to_string(),
                section_name: "citations".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "confidence".to_string(),
                        source: XmlChildSource::InlineStruct("confidence".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("CitationNote".to_string()),
                    },
                    XmlChild {
                        element_name: "mediaref".to_string(),
                        source: XmlChildSource::Edge("CitationMediaRef".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Sources
        // -----------------------------------------------------------------------
        type_map.insert(
            "Source".to_string(),
            XmlTypeInfo {
                element_name: "source".to_string(),
                section_name: "sources".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "stitle".to_string(),
                        source: XmlChildSource::InlineStruct("title".to_string()),
                    },
                    XmlChild {
                        element_name: "sabbrev".to_string(),
                        source: XmlChildSource::InlineStruct("abbrev".to_string()),
                    },
                    XmlChild {
                        element_name: "reporef".to_string(),
                        source: XmlChildSource::Edge("SourceRepoRef".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("SourceNote".to_string()),
                    },
                    XmlChild {
                        element_name: "mediaref".to_string(),
                        source: XmlChildSource::Edge("SourceMediaRef".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Places
        // -----------------------------------------------------------------------
        type_map.insert(
            "Place".to_string(),
            XmlTypeInfo {
                element_name: "placeobj".to_string(),
                section_name: "places".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                    XmlAttribute {
                        field: "type".to_string(),
                        attr_name: "type".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "ptitle".to_string(),
                        source: XmlChildSource::InlineStruct("title".to_string()),
                    },
                    XmlChild {
                        element_name: "placeref".to_string(),
                        source: XmlChildSource::Edge("PlacePlaceRef".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("PlaceNote".to_string()),
                    },
                    XmlChild {
                        element_name: "mediaref".to_string(),
                        source: XmlChildSource::Edge("PlaceMediaRef".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Objects (Media)
        // -----------------------------------------------------------------------
        type_map.insert(
            "Media".to_string(),
            XmlTypeInfo {
                element_name: "object".to_string(),
                section_name: "objects".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "file".to_string(),
                        source: XmlChildSource::InlineStruct("file".to_string()),
                    },
                    XmlChild {
                        element_name: "description".to_string(),
                        source: XmlChildSource::InlineStruct("description".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("MediaNote".to_string()),
                    },
                    XmlChild {
                        element_name: "tagref".to_string(),
                        source: XmlChildSource::Edge("MediaTag".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Repositories
        // -----------------------------------------------------------------------
        type_map.insert(
            "Repository".to_string(),
            XmlTypeInfo {
                element_name: "repository".to_string(),
                section_name: "repositories".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "rname".to_string(),
                        source: XmlChildSource::InlineStruct("name".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("RepositoryNote".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Notes
        // -----------------------------------------------------------------------
        type_map.insert(
            "Note".to_string(),
            XmlTypeInfo {
                element_name: "note".to_string(),
                section_name: "notes".to_string(),
                attributes: vec![
                    XmlAttribute {
                        field: "handle".to_string(),
                        attr_name: "handle".to_string(),
                    },
                    XmlAttribute {
                        field: "gramps_id".to_string(),
                        attr_name: "id".to_string(),
                    },
                    XmlAttribute {
                        field: "type".to_string(),
                        attr_name: "type".to_string(),
                    },
                    XmlAttribute {
                        field: "format".to_string(),
                        attr_name: "format".to_string(),
                    },
                ],
                children: vec![
                    XmlChild {
                        element_name: "text".to_string(),
                        source: XmlChildSource::InlineStruct("text".to_string()),
                    },
                    XmlChild {
                        element_name: "noteref".to_string(),
                        source: XmlChildSource::Edge("NoteNoteRef".to_string()),
                    },
                    XmlChild {
                        element_name: "tagref".to_string(),
                        source: XmlChildSource::Edge("NoteTag".to_string()),
                    },
                ],
            },
        );

        // -----------------------------------------------------------------------
        // Edge map — embedded refs with metadata
        // -----------------------------------------------------------------------
        edge_map.insert(
            "PersonEventRef".to_string(),
            XmlEdgeInfo {
                parent_element: "person".to_string(),
                element_name: "eventref".to_string(),
                attributes: vec![
                    ("hlink".to_string(), "hlink".to_string()),
                    ("role".to_string(), "role".to_string()),
                ],
            },
        );
        edge_map.insert(
            "FamilyChildRef".to_string(),
            XmlEdgeInfo {
                parent_element: "family".to_string(),
                element_name: "childref".to_string(),
                attributes: vec![
                    ("hlink".to_string(), "hlink".to_string()),
                    ("relation".to_string(), "rel".to_string()),
                ],
            },
        );
        edge_map.insert(
            "FamilyEventRef".to_string(),
            XmlEdgeInfo {
                parent_element: "family".to_string(),
                element_name: "eventref".to_string(),
                attributes: vec![
                    ("hlink".to_string(), "hlink".to_string()),
                    ("role".to_string(), "role".to_string()),
                ],
            },
        );
        edge_map.insert(
            "PersonPersonRef".to_string(),
            XmlEdgeInfo {
                parent_element: "person".to_string(),
                element_name: "personref".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );
        edge_map.insert(
            "SourceRepoRef".to_string(),
            XmlEdgeInfo {
                parent_element: "source".to_string(),
                element_name: "reporef".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );

        // -----------------------------------------------------------------------
        // Edge map — mixin refs (citation, note, media, tag)
        // -----------------------------------------------------------------------
        for (edge_name, parent) in &[
            ("PersonCitation", "person"),
            ("EventCitation", "event"),
            ("FamilyCitation", "family"),
        ] {
            edge_map.insert(
                edge_name.to_string(),
                XmlEdgeInfo {
                    parent_element: parent.to_string(),
                    element_name: "citationref".to_string(),
                    attributes: vec![("hlink".to_string(), "hlink".to_string())],
                },
            );
        }
        for (edge_name, parent) in &[
            ("PersonNote", "person"),
            ("EventNote", "event"),
            ("FamilyNote", "family"),
            ("CitationNote", "citation"),
            ("SourceNote", "source"),
            ("PlaceNote", "place"),
            ("MediaNote", "object"),
            ("RepositoryNote", "repository"),
            ("NoteNoteRef", "note"),
        ] {
            edge_map.insert(
                edge_name.to_string(),
                XmlEdgeInfo {
                    parent_element: parent.to_string(),
                    element_name: "noteref".to_string(),
                    attributes: vec![("hlink".to_string(), "hlink".to_string())],
                },
            );
        }
        for (edge_name, parent) in &[
            ("PersonMediaRef", "person"),
            ("EventMediaRef", "event"),
            ("FamilyMediaRef", "family"),
            ("CitationMediaRef", "citation"),
            ("SourceMediaRef", "source"),
            ("PlaceMediaRef", "place"),
        ] {
            edge_map.insert(
                edge_name.to_string(),
                XmlEdgeInfo {
                    parent_element: parent.to_string(),
                    element_name: "mediaref".to_string(),
                    attributes: vec![("hlink".to_string(), "hlink".to_string())],
                },
            );
        }
        for (edge_name, parent) in &[
            ("PersonTag", "person"),
            ("EventTag", "event"),
            ("FamilyTag", "family"),
            ("MediaTag", "object"),
            ("NoteTag", "note"),
        ] {
            edge_map.insert(
                edge_name.to_string(),
                XmlEdgeInfo {
                    parent_element: parent.to_string(),
                    element_name: "tagref".to_string(),
                    attributes: vec![("hlink".to_string(), "hlink".to_string())],
                },
            );
        }

        // -----------------------------------------------------------------------
        // Edge map — direct handle refs
        // -----------------------------------------------------------------------
        edge_map.insert(
            "FamilyFather".to_string(),
            XmlEdgeInfo {
                parent_element: "family".to_string(),
                element_name: "father".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );
        edge_map.insert(
            "FamilyMother".to_string(),
            XmlEdgeInfo {
                parent_element: "family".to_string(),
                element_name: "mother".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );
        edge_map.insert(
            "PersonFamily".to_string(),
            XmlEdgeInfo {
                parent_element: "person".to_string(),
                element_name: "childin".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );
        edge_map.insert(
            "PersonParentFamily".to_string(),
            XmlEdgeInfo {
                parent_element: "person".to_string(),
                element_name: "parentin".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );
        edge_map.insert(
            "EventPlace".to_string(),
            XmlEdgeInfo {
                parent_element: "event".to_string(),
                element_name: "place".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );
        edge_map.insert(
            "CitationSource".to_string(),
            XmlEdgeInfo {
                parent_element: "citation".to_string(),
                element_name: "sourceref".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );
        edge_map.insert(
            "PlacePlaceRef".to_string(),
            XmlEdgeInfo {
                parent_element: "placeobj".to_string(),
                element_name: "placeref".to_string(),
                attributes: vec![("hlink".to_string(), "hlink".to_string())],
            },
        );

        // Section order per Gramps XML schema
        let section_order = vec![
            "tags".to_string(),
            "events".to_string(),
            "people".to_string(),
            "families".to_string(),
            "citations".to_string(),
            "sources".to_string(),
            "places".to_string(),
            "objects".to_string(),
            "repositories".to_string(),
            "notes".to_string(),
        ];

        SerializationMap {
            type_map,
            edge_map,
            section_order,
        }
    }
}

impl Default for SerializationMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_map_new_has_all_primary_types() {
        let map = SerializationMap::new();
        let expected_types = [
            "Tag",
            "Event",
            "Person",
            "Family",
            "Citation",
            "Source",
            "Place",
            "Media",
            "Repository",
            "Note",
        ];
        for type_name in &expected_types {
            assert!(
                map.type_map.contains_key(*type_name),
                "Missing type: {}",
                type_name
            );
        }
        assert_eq!(map.type_map.len(), 10);
    }

    #[test]
    fn serialization_map_person_mapping() {
        let map = SerializationMap::new();
        let person = map.type_map.get("Person").unwrap();
        assert_eq!(person.element_name, "person");
        assert_eq!(person.section_name, "people");

        // Check handle and gramps_id attributes
        let handle_attr = person.attributes.iter().find(|a| a.field == "handle");
        assert!(handle_attr.is_some());
        assert_eq!(handle_attr.unwrap().attr_name, "handle");

        let gramps_id_attr = person.attributes.iter().find(|a| a.field == "gramps_id");
        assert!(gramps_id_attr.is_some());
        assert_eq!(gramps_id_attr.unwrap().attr_name, "id");
    }

    #[test]
    fn serialization_map_section_order() {
        let map = SerializationMap::new();
        let expected = vec![
            "tags",
            "events",
            "people",
            "families",
            "citations",
            "sources",
            "places",
            "objects",
            "repositories",
            "notes",
        ];
        assert_eq!(map.section_order, expected);
    }

    #[test]
    fn serialization_map_edge_exists() {
        let map = SerializationMap::new();
        assert!(map.edge_map.contains_key("PersonFamily"));
        assert!(map.edge_map.contains_key("PersonEventRef"));
        assert!(map.edge_map.contains_key("FamilyChildRef"));
        assert!(map.edge_map.contains_key("FamilyFather"));
        assert!(map.edge_map.contains_key("FamilyMother"));
        assert!(map.edge_map.contains_key("PersonCitation"));
        assert!(map.edge_map.contains_key("PersonNote"));
        assert!(map.edge_map.contains_key("PersonMediaRef"));
        assert!(map.edge_map.contains_key("PersonTag"));
        assert!(map.edge_map.contains_key("EventPlace"));
        assert!(map.edge_map.contains_key("CitationSource"));
        assert!(map.edge_map.contains_key("EventCitation"));
        assert!(map.edge_map.contains_key("EventNote"));
        assert!(map.edge_map.contains_key("EventMediaRef"));
        assert!(map.edge_map.contains_key("EventTag"));
    }

    #[test]
    fn serialization_map_edge_parent_element() {
        let map = SerializationMap::new();
        let person_event_ref = map.edge_map.get("PersonEventRef").unwrap();
        assert_eq!(person_event_ref.parent_element, "person");
        assert_eq!(person_event_ref.element_name, "eventref");
    }

    #[test]
    fn serialization_map_default() {
        let map = SerializationMap::default();
        let map2 = SerializationMap::new();
        assert_eq!(map, map2);
    }
}
