//! XML serialization for Gramps genealogy graphs.
//!
//! This module provides the [`GraphXmlWriter`] that walks a validated [`Graph`]
//! and produces Gramps XML (`.gramps` format) following the RelaxNG schema.

use crate::serialization_map::SerializationMap;

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
    _map: SerializationMap,
}

impl GraphXmlWriter {
    /// Create a new `GraphXmlWriter` with the given [`SerializationMap`].
    pub fn new(map: SerializationMap) -> Self {
        GraphXmlWriter { _map: map }
    }

    /// Serialize the graph to the given writer.
    ///
    /// Returns an error if the graph contains unsupported types or if
    /// writing to the output fails.
    pub fn write(
        &self,
        _graph: &typed_graph::Graph,
        writer: &mut impl std::io::Write,
    ) -> Result<(), SerializationError> {
        // Placeholder: write a minimal XML declaration
        writeln!(writer, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
        writeln!(
            writer,
            r#"<database xmlns="http://gramps-project.org/xml/1.7.2/">"#
        )?;
        writeln!(writer, "  <!-- output crate scaffold -->")?;
        writeln!(writer, "</database>")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typed_graph::graph::Graph;

    #[test]
    fn serialization_error_display_and_error_traits() {
        let err = SerializationError::UnsupportedType("Foo".to_string());
        let display = format!("{}", err);
        assert!(display.contains("Foo"));

        let err = SerializationError::MissingRequiredField {
            handle: "h1".to_string(),
            field: "name",
        };
        let display = format!("{}", err);
        assert!(display.contains("h1"));
        assert!(display.contains("name"));

        let err = SerializationError::Io(std::io::ErrorKind::NotFound, "file not found".to_string());
        let display = format!("{}", err);
        assert!(display.contains("NotFound"));
    }

    #[test]
    fn serialization_error_io_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err: SerializationError = io_err.into();
        match err {
            SerializationError::Io(std::io::ErrorKind::PermissionDenied, _) => {}
            _ => panic!("Expected Io(PermissionDenied, _)"),
        }
    }

    #[test]
    fn xml_writer_new() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map);
        // Just verify it constructs without panicking
        let _ = writer;
    }

    #[test]
    fn xml_writer_empty_graph() {
        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map);
        let graph = Graph::new();
        let mut output = Vec::new();
        let result = writer.write(&graph, &mut output);
        assert!(result.is_ok());
        let xml = String::from_utf8(output).unwrap();
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(xml.contains("<database"));
        assert!(xml.contains("</database>"));
    }
}