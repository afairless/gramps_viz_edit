//! Schema version detection from Gramps XML headers.
//!
//! Scans the XML header element `<created version="..."/>` to detect
//! which Gramps schema version a file was produced with. The caller
//! uses this to select the compiled-in [`Schema`] for parsing.

use crate::error::Error;
use crate::xml::strip_prefix;
use quick_xml::events::Event;
use quick_xml::Reader;

/// Detect the schema version from a Gramps XML document's header.
///
/// Scans the `<header><created version="X.Y"/>` element and validates
/// that the version is compiled into the current binary via
/// `typed-graph::Schema::available_versions()`.
///
/// # Errors
///
/// - [`Error::XmlParseError`] if the XML is malformed.
/// - [`Error::UnsupportedSchema`] if the version is not compiled in.
/// - Returns `Error::XmlParseError` if the `<header>` or `<created>`
///   element is missing (the document is not a valid Gramps XML file).
pub fn detect_schema_version(content: &str) -> Result<String, Error> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut in_header = false;
    let mut version: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"header" => in_header = true,
                    b"created" if in_header => {
                        // Read the version attribute from <created version="X.Y"/>
                        for attr in e.attributes().flatten() {
                            let key = attr.key.as_ref();
                            if key == b"version" || key.ends_with(b":version") {
                                version = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                        // <created> is self-closing (<created version="5.2"/>)
                        // so we don't need to wait for End.
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                if name == b"header" {
                    break;
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

    match version {
        Some(v) => {
            // Extract the major.minor prefix (e.g., "5.2.0" → "5.2").
            let schema_version = v.split('.').take(2).collect::<Vec<_>>().join(".");

            // Check it's compiled in.
            let available = typed_graph::Schema::available_versions();
            if available.contains(&schema_version.as_str()) {
                Ok(schema_version)
            } else {
                Err(Error::UnsupportedSchema {
                    version: v,
                    schema_version,
                })
            }
        }
        None => Err(Error::XmlParseError {
            message: "missing <header><created version=\"...\"/> element".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;

    /// Helper: wrap XML in a minimal database envelope.
    fn with_database(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
{body}
</database>"#,
        )
    }

    // -----------------------------------------------------------------------
    // Valid 5.2 header
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_5_2() {
        let xml = with_database("  <header><created version=\"5.2\"/></header>");
        let version = detect_schema_version(&xml).unwrap();
        assert_eq!(version, "5.2");
    }

    // -----------------------------------------------------------------------
    // Full-version header (e.g., "5.2.0") normalizes to schema prefix
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_full_5_2_normalized() {
        let xml = with_database("  <header><created version=\"5.2.0\"/></header>");
        let version = detect_schema_version(&xml).unwrap();
        assert_eq!(version, "5.2");
    }

    // -----------------------------------------------------------------------
    // Valid 5.1 header (if compiled in)
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_5_1() {
        let xml = with_database("  <header><created version=\"5.1\"/></header>");
        let result = detect_schema_version(&xml);
        // With both schemas as default features, 5.1 is now compiled in.
        assert_eq!(result.unwrap(), "5.1");
    }

    // -----------------------------------------------------------------------
    // Unknown version
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_unknown() {
        let xml = with_database("  <header><created version=\"99.99\"/></header>");
        let result = detect_schema_version(&xml);
        match result {
            Err(Error::UnsupportedSchema {
                version,
                schema_version,
            }) => {
                assert_eq!(version, "99.99");
                assert_eq!(schema_version, "99.99");
            }
            other => panic!("Expected UnsupportedSchema, got: {:?}", other.map(|_| ())),
        }
    }

    // -----------------------------------------------------------------------
    // Missing <header> element
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_missing_header() {
        let xml = with_database("  <people><person handle=\"p1\"/></people>");
        let result = detect_schema_version(&xml);
        match result {
            Err(Error::XmlParseError { message }) => {
                assert!(message.contains("missing"));
            }
            other => panic!("Expected XmlParseError, got: {:?}", other.map(|_| ())),
        }
    }

    // -----------------------------------------------------------------------
    // Malformed XML
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_malformed_xml() {
        let result = detect_schema_version("<database><header><created version=");
        match result {
            Err(Error::XmlParseError { .. }) => {}
            other => panic!("Expected XmlParseError, got: {:?}", other.map(|_| ())),
        }
    }

    // -----------------------------------------------------------------------
    // Empty content
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_empty_content() {
        let result = detect_schema_version("");
        match result {
            Err(Error::XmlParseError { .. }) => {}
            other => panic!("Expected XmlParseError, got: {:?}", other.map(|_| ())),
        }
    }

    // -----------------------------------------------------------------------
    // Display for UnsupportedSchema
    // -----------------------------------------------------------------------

    #[test]
    fn unsupported_schema_display_from_header() {
        let err = Error::UnsupportedSchema {
            version: "9.9.9".to_string(),
            schema_version: "9.9".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("unsupported schema version"));
        assert!(display.contains("9.9"));
        assert!(display.contains("9.9.9"));
        assert!(display.contains("hint:"));
    }

    #[test]
    fn unsupported_schema_source_is_none_from_header() {
        let err = Error::UnsupportedSchema {
            version: "9.9.9".to_string(),
            schema_version: "9.9".to_string(),
        };
        assert!(err.source().is_none());
    }

    // -----------------------------------------------------------------------
    // Namespace-prefixed header
    // -----------------------------------------------------------------------

    #[test]
    fn detect_schema_version_namespace_prefixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:header><ns:created ns:version="5.2"/></ns:header>
</ns:database>"#;
        let version = detect_schema_version(xml).unwrap();
        assert_eq!(version, "5.2");
    }
}
