//! Streaming XML element filter for removing orphaned elements from Gramps XML.
//!
//! Reads a `.gramps` (or `.gramps.gz`) file, removes elements (e.g. `<event>`
//! or `<note>`) whose `handle` attribute matches a given set, and writes the
//! result to a new file.
//!
//! Uses `quick-xml` Reader + Writer for a single-pass, memory-efficient streaming
//! approach. Gzip decompression is transparent based on file header bytes.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;

use flate2::read::GzDecoder;
use gramps_reader::xml::{read_handle_attr, strip_prefix};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

/// Statistics from an element cleaning run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanStats {
    /// Number of elements removed from the XML.
    pub elements_removed: usize,
    /// Number of handles requested for removal but not found in the XML.
    pub elements_not_found: usize,
}

/// Errors from element cleaning.
#[derive(Debug)]
pub enum CleanError {
    /// I/O error with file path context.
    Io {
        path: String,
        source: std::io::Error,
    },
    /// XML parse error with descriptive message.
    XmlParse {
        message: String,
    },
}

impl std::fmt::Display for CleanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CleanError::Io { path, source } => {
                write!(f, "I/O error for '{}': {}", path, source)
            }
            CleanError::XmlParse { message } => {
                write!(f, "XML parse error: {}", message)
            }
        }
    }
}

impl std::error::Error for CleanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CleanError::Io { source, .. } => Some(source),
            CleanError::XmlParse { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Gzip detection
// ---------------------------------------------------------------------------

/// Detect whether a file is gzip-compressed by reading the first two bytes.
///
/// Gzip magic bytes are `0x1f 0x8b`. Returns `true` if the file starts with
/// these bytes.
fn is_gzip(path: &Path) -> Result<bool, CleanError> {
    let mut file = File::open(path).map_err(|e| CleanError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut buf = [0u8; 2];
    file.read_exact(&mut buf).map_err(|e| CleanError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(buf == [0x1f, 0x8b])
}

// ---------------------------------------------------------------------------
// Core cleaning function
// ---------------------------------------------------------------------------

/// Convert a `quick_xml::Error` into a `CleanError` with file path context.
fn map_xml_error(output: &Path, err: quick_xml::Error) -> CleanError {
    match err {
        quick_xml::Error::Io(io_err) => {
            // quick_xml::Error::Io wraps an Arc<std::io::Error>
            let msg = io_err.to_string();
            CleanError::Io {
                path: output.display().to_string(),
                source: std::io::Error::other(msg),
            }
        }
        other => CleanError::XmlParse {
            message: other.to_string(),
        },
    }
}

/// Check if a handle from an XML attribute matches any handle in the deletion
/// set, and remove it if it does.
///
/// Gramps re-exports handles with a leading `_` prefix (e.g. `_e0001` in XML
/// vs `e0001` in the manifest). We check both the literal handle and the
/// underscore-prefixed/stripped variant.
fn remove_matching_handle(remaining: &mut HashSet<String>, handle: &str) -> bool {
    if remaining.remove(handle) {
        return true;
    }
    // If the XML handle has a leading `_`, also check without it
    if let Some(stripped) = handle.strip_prefix('_') {
        if remaining.remove(stripped) {
            return true;
        }
    }
    // If the XML handle has no leading `_`, also check with it
    let prefixed = format!("_{}", handle);
    if remaining.remove(&prefixed) {
        return true;
    }
    false
}

/// Remove elements matching the given handles from a Gramps XML file.
///
/// Reads the input file (with transparent gzip decompression), streams through
/// the XML, removes elements (e.g. `<event>`, `<note>`) whose XML tag name
/// matches `element_name` and whose `handle` attribute is in `handles`, and
/// writes the result to `output`.
///
/// The output is always written as plain `.gramps` (no gzip compression).
/// Writing is atomic: data is written to a temporary file first, then renamed
/// to the final path on success.
///
/// # Returns
///
/// `CleanStats` with the count of removed elements and the count of requested
/// handles that were not found in the XML.
pub fn clean_elements_xml(
    input: &Path,
    output: &Path,
    element_name: &str,
    handles: &HashSet<String>,
) -> Result<CleanStats, CleanError> {
    if !input.exists() {
        return Err(CleanError::Io {
            path: input.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "input file not found"),
        });
    }

    // Open input with transparent gzip decompression
    let reader: Box<dyn BufRead> = if is_gzip(input)? {
        let file = File::open(input).map_err(|e| CleanError::Io {
            path: input.display().to_string(),
            source: e,
        })?;
        let decoder = GzDecoder::new(BufReader::new(file));
        Box::new(BufReader::new(decoder))
    } else {
        let file = File::open(input).map_err(|e| CleanError::Io {
            path: input.display().to_string(),
            source: e,
        })?;
        Box::new(BufReader::new(file))
    };

    // Write to a temp file first, then rename on success
    let temp_output = output.with_extension("tmp.gramps");
    let out_file = File::create(&temp_output).map_err(|e| CleanError::Io {
        path: temp_output.display().to_string(),
        source: e,
    })?;

    let mut xml_reader = Reader::from_reader(reader);
    xml_reader.config_mut().trim_text(true);
    let mut xml_writer = Writer::new_with_indent(out_file, b' ', 2);
    let mut buf = Vec::new();

    // Track which handles we're looking for
    let mut remaining: HashSet<String> = handles.iter().cloned().collect();
    let mut elements_removed: usize = 0;

    // State machine for skipping elements
    let mut skip_depth: usize = 0;

    // Process XML events
    loop {
        buf.clear();
        let event = xml_reader.read_event_into(&mut buf).map_err(|e| CleanError::XmlParse {
            message: format!("XML parse error at byte {}: {}", xml_reader.buffer_position(), e),
        })?;

        match event {
            Event::Start(ref e) => {
                if skip_depth > 0 {
                    // We're already inside an element being skipped — increment depth
                    skip_depth += 1;
                    continue;
                }

                let name = e.name();
                let local_name = strip_prefix(name.as_ref());
                if local_name == element_name.as_bytes() {
                    if let Some(handle) = read_handle_attr(e) {
                        if remove_matching_handle(&mut remaining, &handle) {
                            // Start skipping this element
                            skip_depth = 1;
                            elements_removed += 1;
                            continue;
                        }
                    }
                }

                // Not a matching element, or not in the deletion set - write through
                xml_writer
                    .write_event(Event::Start(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::Empty(ref e) => {
                if skip_depth > 0 {
                    // Shouldn't happen, but defensive
                    continue;
                }

                let name = e.name();
                let local_name = strip_prefix(name.as_ref());
                if local_name == element_name.as_bytes() {
                    if let Some(handle) = read_handle_attr(e) {
                        if remove_matching_handle(&mut remaining, &handle) {
                            // Skip this self-closing element
                            elements_removed += 1;
                            continue;
                        }
                    }
                }

                // Not a matching element to remove — write through
                xml_writer
                    .write_event(Event::Empty(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::End(ref e) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    if skip_depth == 0 {
                        // Done skipping this element — resume writing
                    }
                    continue;
                }

                xml_writer
                    .write_event(Event::End(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::Decl(ref e) => {
                // Write XML declaration through
                xml_writer
                    .write_event(Event::Decl(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::Text(ref e) => {
                if skip_depth > 0 {
                    continue;
                }
                xml_writer
                    .write_event(Event::Text(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::CData(ref e) => {
                if skip_depth > 0 {
                    continue;
                }
                xml_writer
                    .write_event(Event::CData(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::Comment(ref e) => {
                if skip_depth > 0 {
                    continue;
                }
                xml_writer
                    .write_event(Event::Comment(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::PI(ref e) => {
                if skip_depth > 0 {
                    continue;
                }
                xml_writer
                    .write_event(Event::PI(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::DocType(ref e) => {
                // Write DOCTYPE through
                xml_writer
                    .write_event(Event::DocType(e.to_owned()))
                    .map_err(|e| map_xml_error(output, e))?;
            }

            Event::Eof => {
                break;
            }
        }
    }

    // Flush and finalize the writer
    let mut out_writer = xml_writer.into_inner();
    out_writer.flush().map_err(|e| CleanError::Io {
        path: output.display().to_string(),
        source: e,
    })?;

    // Rename temp file to final output (atomic on same filesystem)
    std::fs::rename(&temp_output, output).map_err(|e| CleanError::Io {
        path: output.display().to_string(),
        source: e,
    })?;

    let elements_not_found = remaining.len();

    Ok(CleanStats {
        elements_removed,
        elements_not_found,
    })
}

/// Remove events matching the given handles from a Gramps XML file.
///
/// Convenience wrapper around [`clean_elements_xml`] with `element_name = "event"`.
pub fn clean_events_xml(
    input: &Path,
    output: &Path,
    event_handles: &HashSet<String>,
) -> Result<CleanStats, CleanError> {
    clean_elements_xml(input, output, "event", event_handles)
}

/// Remove notes matching the given handles from a Gramps XML file.
///
/// Convenience wrapper around [`clean_elements_xml`] with `element_name = "note"`.
pub fn clean_notes_xml(
    input: &Path,
    output: &Path,
    note_handles: &HashSet<String>,
) -> Result<CleanStats, CleanError> {
    clean_elements_xml(input, output, "note", note_handles)
}

/// Remove places matching the given handles from a Gramps XML file.
///
/// Convenience wrapper around [`clean_elements_xml`] with `element_name = "place"`.
pub fn clean_places_xml(
    input: &Path,
    output: &Path,
    place_handles: &HashSet<String>,
) -> Result<CleanStats, CleanError> {
    clean_elements_xml(input, output, "place", place_handles)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: write a string to a temp file, return the path.
    fn write_temp(content: &str) -> (std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.gramps");
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        (path, dir)
    }

    /// Helper: write gzip-compressed content to a temp file.
    fn write_gzip_temp(content: &str) -> (std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.gramps.gz");
        let file = File::create(&path).unwrap();
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder.write_all(content.as_bytes()).unwrap();
        encoder.finish().unwrap();
        (path, dir)
    }

    /// Helper: run clean_events_xml and return the output content.
    fn run_clean(
        input: &Path,
        handles: &[&str],
        output_dir: &std::path::Path,
    ) -> (CleanStats, String) {
        let output = output_dir.join("output.gramps");
        let handle_set: HashSet<String> = handles.iter().map(|s| s.to_string()).collect();
        let stats = clean_events_xml(input, &output, &handle_set).unwrap();
        let content = std::fs::read_to_string(&output).unwrap();
        (stats, content)
    }

    // -----------------------------------------------------------------------
    // remove_single_event
    // -----------------------------------------------------------------------

    #[test]
    fn remove_single_event() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <header><created date="2024-01-01" version="5.2"/></header>
  <people>
    <person handle="p0001"><gender>M</gender><name><first>John</first></name></person>
  </people>
  <events>
    <event handle="e0001"><type>Birth</type><dateval val="1980-01-15"/></event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "Event handle should be removed");
        assert!(
            content.contains("<event"),
            "Should not have <event> elements"
        );
        assert!(
            content.contains("<person handle=\"p0001\">"),
            "Person should remain"
        );
        assert!(
            content.contains("<header>"),
            "Header should remain"
        );
        assert!(
            content.contains("<?xml"),
            "XML declaration should remain"
        );
    }

    // -----------------------------------------------------------------------
    // remove_self_closing_event
    // -----------------------------------------------------------------------

    #[test]
    fn remove_self_closing_event() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001"/>
    <event handle="e0002"/>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "e0001 should be removed");
        assert!(content.contains("e0002"), "e0002 should remain");
    }

    // -----------------------------------------------------------------------
    // keep_unrelated_event
    // -----------------------------------------------------------------------

    #[test]
    fn keep_unrelated_event() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001"><type>Birth</type></event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e9999"], output_dir.path());

        assert_eq!(stats.elements_removed, 0);
        assert_eq!(stats.elements_not_found, 1);
        assert!(content.contains("e0001"), "Event should remain");
    }

    // -----------------------------------------------------------------------
    // no_events_to_remove
    // -----------------------------------------------------------------------

    #[test]
    fn no_events_to_remove() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001"><type>Birth</type></event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &[], output_dir.path());

        assert_eq!(stats.elements_removed, 0);
        assert_eq!(stats.elements_not_found, 0);
        assert!(content.contains("e0001"), "Event should remain");
        // Output should be identical to input (modulo quick-xml formatting)
        assert!(content.contains("Birth"), "Content should be preserved");
    }

    // -----------------------------------------------------------------------
    // handle_not_found
    // -----------------------------------------------------------------------

    #[test]
    fn handle_not_found() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001"><type>Birth</type></event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, _content) = run_clean(&input, &["e0001", "e9999"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 1);
    }

    // -----------------------------------------------------------------------
    // namespace_prefixed_event
    // -----------------------------------------------------------------------

    #[test]
    fn namespace_prefixed_event() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <ns:event ns:handle="e0001" xmlns:ns="http://example.com/ns">
      <ns:type>Birth</ns:type>
    </ns:event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "Event should be removed");
    }

    // -----------------------------------------------------------------------
    // nested_eventtype
    // -----------------------------------------------------------------------

    #[test]
    fn nested_eventtype() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001">
      <eventtype>
        <type>Birth</type>
      </eventtype>
      <dateval val="1980-01-15"/>
    </event>
    <event handle="e0002">
      <eventtype>
        <type>Death</type>
      </eventtype>
      <dateval val="2050-06-01"/>
    </event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "e0001 should be removed");
        assert!(content.contains("e0002"), "e0002 should remain");
        assert!(
            content.contains("Death"),
            "Death type should remain in kept event"
        );
    }

    // -----------------------------------------------------------------------
    // flat_type
    // -----------------------------------------------------------------------

    #[test]
    fn flat_type() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001">
      <type>Birth</type>
      <dateval val="1980-01-15"/>
    </event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "Event should be removed");
        assert!(!content.contains("Birth"), "Event body should be removed");
    }

    // -----------------------------------------------------------------------
    // multiple_events_mixed
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_events_mixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001"><type>Birth</type></event>
    <event handle="e0002"><type>Death</type></event>
    <event handle="e0003"><type>Marriage</type></event>
    <event handle="e0004"><type>Divorce</type></event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001", "e0003"], output_dir.path());

        assert_eq!(stats.elements_removed, 2);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "e0001 should be removed");
        assert!(content.contains("e0002"), "e0002 should remain");
        assert!(!content.contains("e0003"), "e0003 should be removed");
        assert!(content.contains("e0004"), "e0004 should remain");
    }

    // -----------------------------------------------------------------------
    // non_event_xml_preserved
    // -----------------------------------------------------------------------

    #[test]
    fn non_event_xml_preserved() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.2//EN" "http://gramps-project.org/xml/1.7.2/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2024-01-01" version="5.2"/>
    <researcher><resname>Test</resname></researcher>
  </header>
  <tags/>
  <events>
    <event handle="e0001"><type>Birth</type></event>
  </events>
  <people>
    <person handle="p0001">
      <gender>M</gender>
      <name><first>John</first><surname>Smith</surname></name>
      <eventref hlink="e0001" role="Primary"/>
    </person>
  </people>
  <families/>
  <citations/>
  <sources/>
  <places/>
  <objects/>
  <repositories/>
  <notes/>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);

        // All non-event structures should be preserved
        assert!(content.contains("<?xml"), "XML declaration");
        assert!(content.contains("DOCTYPE"), "DOCTYPE");
        assert!(content.contains("<header>"), "Header");
        assert!(content.contains("<resname>Test</resname>"), "Researcher");
        assert!(content.contains("<tags/>"), "Tags");
        assert!(content.contains("<person handle=\"p0001\">"), "Person");
        assert!(content.contains("John"), "Person name");
        assert!(content.contains("Smith"), "Person surname");
        assert!(content.contains("<eventref hlink=\"e0001\""), "Event ref");
        assert!(content.contains("<families/>"), "Families");
        assert!(content.contains("<citations/>"), "Citations");
        assert!(content.contains("<sources/>"), "Sources");
        assert!(content.contains("<places/>"), "Places");
        assert!(content.contains("<objects/>"), "Objects");
        assert!(content.contains("<repositories/>"), "Repositories");
        assert!(content.contains("<notes/>"), "Notes");
    }

    // -----------------------------------------------------------------------
    // gzip_input
    // -----------------------------------------------------------------------

    #[test]
    fn gzip_input() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001"><type>Birth</type></event>
    <event handle="e0002"><type>Death</type></event>
  </events>
</database>"#;
        let (input, _dir) = write_gzip_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let output = output_dir.path().join("output.gramps");
        let handle_set: HashSet<String> = vec!["e0001".to_string()].into_iter().collect();
        let stats = clean_events_xml(&input, &output, &handle_set).unwrap();
        let content = std::fs::read_to_string(&output).unwrap();

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "e0001 should be removed");
        assert!(content.contains("e0002"), "e0002 should remain");
        // Output should be plain .gramps, not gzip
        let meta = std::fs::metadata(&output).unwrap();
        assert!(meta.len() > 0, "Output should be non-empty");
    }

    // -----------------------------------------------------------------------
    // input_file_not_found
    // -----------------------------------------------------------------------

    #[test]
    fn input_file_not_found() {
        let input = Path::new("/nonexistent/file.gramps");
        let output = Path::new("/tmp/out.gramps");
        let handle_set: HashSet<String> = HashSet::new();
        let result = clean_events_xml(input, output, &handle_set);
        match result {
            Err(CleanError::Io { path, .. }) => {
                assert!(path.contains("nonexistent"), "Path should be in error");
            }
            other => panic!("Expected Io error, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // CleanError display and error traits
    // -----------------------------------------------------------------------

    #[test]
    fn clean_error_display_io() {
        let err = CleanError::Io {
            path: "test.gramps".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let display = err.to_string();
        assert!(display.contains("test.gramps"));
        assert!(display.contains("file not found"));
    }

    #[test]
    fn clean_error_display_xml() {
        let err = CleanError::XmlParse {
            message: "unexpected token at byte 42".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("XML parse error"));
        assert!(display.contains("byte 42"));
    }

    #[test]
    fn clean_error_source_io() {
        use std::error::Error;
        let inner = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = CleanError::Io {
            path: "test.gramps".to_string(),
            source: inner,
        };
        assert!(err.source().is_some());
    }

    #[test]
    fn clean_error_source_xml_is_none() {
        use std::error::Error;
        let err = CleanError::XmlParse {
            message: "parse error".to_string(),
        };
        assert!(err.source().is_none());
    }

    // -----------------------------------------------------------------------
    // CleanStats
    // -----------------------------------------------------------------------

    #[test]
    fn clean_stats_default() {
        let stats = CleanStats {
            elements_removed: 0,
            elements_not_found: 0,
        };
        assert_eq!(stats.elements_removed, 0);
        assert_eq!(stats.elements_not_found, 0);
    }

    #[test]
    fn clean_stats_clone_and_eq() {
        let a = CleanStats {
            elements_removed: 5,
            elements_not_found: 2,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // -----------------------------------------------------------------------
    // remove_matching_handle
    // -----------------------------------------------------------------------

    #[test]
    fn remove_matching_handle_exact_match() {
        let mut set: HashSet<String> = vec!["e0001".to_string()].into_iter().collect();
        assert!(remove_matching_handle(&mut set, "e0001"));
        assert!(set.is_empty());
    }

    #[test]
    fn remove_matching_handle_unprefixed_in_xml_but_prefixed_in_set() {
        // XML has "e0001", set has "_e0001"
        let mut set: HashSet<String> = vec!["_e0001".to_string()].into_iter().collect();
        assert!(remove_matching_handle(&mut set, "e0001"));
        assert!(set.is_empty());
    }

    #[test]
    fn remove_matching_handle_prefixed_in_xml_but_unprefixed_in_set() {
        // XML has "_e0001", set has "e0001"
        let mut set: HashSet<String> = vec!["e0001".to_string()].into_iter().collect();
        assert!(remove_matching_handle(&mut set, "_e0001"));
        assert!(set.is_empty());
    }

    #[test]
    fn remove_matching_handle_no_match() {
        let mut set: HashSet<String> = vec!["e0001".to_string()].into_iter().collect();
        assert!(!remove_matching_handle(&mut set, "e9999"));
        assert!(!set.is_empty());
    }

    // -----------------------------------------------------------------------
    // underscore_prefix_in_xml
    // -----------------------------------------------------------------------

    #[test]
    fn underscore_prefix_in_xml() {
        // Test that an event with Gramps underscore-prefixed handle is matched
        // against the manifest handle (without underscore).
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="_e0001"><type>Birth</type></event>
  </events>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean(&input, &["e0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("e0001"), "Event should be removed");
    }

    // -----------------------------------------------------------------------
    // clean_notes_xml tests
    // -----------------------------------------------------------------------

    /// Helper: run clean_notes_xml and return the output content.
    fn run_clean_notes(
        input: &Path,
        handles: &[&str],
        output_dir: &std::path::Path,
    ) -> (CleanStats, String) {
        let output = output_dir.join("output.gramps");
        let handle_set: HashSet<String> = handles.iter().map(|s| s.to_string()).collect();
        let stats = clean_notes_xml(input, &output, &handle_set).unwrap();
        let content = std::fs::read_to_string(&output).unwrap();
        (stats, content)
    }

    // -----------------------------------------------------------------------
    // remove_single_note
    // -----------------------------------------------------------------------

    #[test]
    fn remove_single_note() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001"><text>Test note</text></note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("n0001"), "Note handle should be removed");
        assert!(
            content.contains("<?xml"),
            "XML declaration should remain"
        );
    }

    // -----------------------------------------------------------------------
    // remove_self_closing_note
    // -----------------------------------------------------------------------

    #[test]
    fn remove_self_closing_note() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001"/>
    <note handle="n0002"/>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("n0001"), "n0001 should be removed");
        assert!(content.contains("n0002"), "n0002 should remain");
    }

    // -----------------------------------------------------------------------
    // keep_unrelated_note
    // -----------------------------------------------------------------------

    #[test]
    fn keep_unrelated_note() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001"><text>Test</text></note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n9999"], output_dir.path());

        assert_eq!(stats.elements_removed, 0);
        assert_eq!(stats.elements_not_found, 1);
        assert!(content.contains("n0001"), "Note should remain");
    }

    // -----------------------------------------------------------------------
    // no_notes_to_remove
    // -----------------------------------------------------------------------

    #[test]
    fn no_notes_to_remove() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001"><text>Test</text></note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &[], output_dir.path());

        assert_eq!(stats.elements_removed, 0);
        assert_eq!(stats.elements_not_found, 0);
        assert!(content.contains("n0001"), "Note should remain");
        assert!(content.contains("Test"), "Content should be preserved");
    }

    // -----------------------------------------------------------------------
    // note_with_text_body
    // -----------------------------------------------------------------------

    #[test]
    fn note_with_text_body() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001"><text>This is the note text. Can be multi-paragraph.</text></note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("n0001"), "Note should be removed");
        assert!(!content.contains("multi-paragraph"), "Body should be removed");
    }

    // -----------------------------------------------------------------------
    // note_with_style
    // -----------------------------------------------------------------------

    #[test]
    fn note_with_style() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001" type="Research">
      <text>Styled note</text>
      <style name="bold">
        <range start="0" end="4"/>
      </style>
    </note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("n0001"), "Note with style should be removed");
    }

    // -----------------------------------------------------------------------
    // note_with_type_attr
    // -----------------------------------------------------------------------

    #[test]
    fn note_with_type_attr() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001" type="Research" format="0">
      <text>Research note</text>
    </note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("n0001"), "Note with type attr should be removed");
    }

    // -----------------------------------------------------------------------
    // notes_not_in_xml
    // -----------------------------------------------------------------------

    #[test]
    fn notes_not_in_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001"><text>Test</text></note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, _content) = run_clean_notes(&input, &["n0001", "n9999"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 1);
    }

    // -----------------------------------------------------------------------
    // namespace_prefixed_note
    // -----------------------------------------------------------------------

    #[test]
    fn namespace_prefixed_note() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <notes>
    <ns:note ns:handle="n0001" xmlns:ns="http://example.com/ns">
      <ns:text>Namespaced note</ns:text>
    </ns:note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("n0001"), "Namespaced note should be removed");
    }

    // -----------------------------------------------------------------------
    // multiple_notes_mixed
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_notes_mixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <notes>
    <note handle="n0001"><text>First</text></note>
    <note handle="n0002"><text>Second</text></note>
    <note handle="n0003"><text>Third</text></note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001", "n0003"], output_dir.path());

        assert_eq!(stats.elements_removed, 2);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("n0001"), "n0001 should be removed");
        assert!(content.contains("n0002"), "n0002 should remain");
        assert!(!content.contains("n0003"), "n0003 should be removed");
    }

    // -----------------------------------------------------------------------
    // notes_section_preserved
    // -----------------------------------------------------------------------

    #[test]
    fn notes_section_preserved() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <header><created date="2024-01-01" version="5.2"/></header>
  <notes>
    <note handle="n0001"><text>Only note</text></note>
  </notes>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_notes(&input, &["n0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        // <notes> section tag should still be present even though all notes removed
        assert!(content.contains("<notes>"), "<notes> section should be preserved");
        assert!(content.contains("</notes>"), "</notes> section should be preserved");
        assert!(content.contains("<header>"), "Header should remain");
    }

    // -----------------------------------------------------------------------
    // clean_places_xml tests
    // -----------------------------------------------------------------------

    /// Helper: run clean_places_xml and return the output content.
    fn run_clean_places(
        input: &Path,
        handles: &[&str],
        output_dir: &std::path::Path,
    ) -> (CleanStats, String) {
        let output = output_dir.join("output.gramps");
        let handle_set: HashSet<String> = handles.iter().map(|s| s.to_string()).collect();
        let stats = clean_places_xml(input, &output, &handle_set).unwrap();
        let content = std::fs::read_to_string(&output).unwrap();
        (stats, content)
    }

    // -----------------------------------------------------------------------
    // remove_single_place
    // -----------------------------------------------------------------------

    #[test]
    fn remove_single_place() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <places>
    <place handle="pl0001">
      <ptitle>New York City</ptitle>
    </place>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_places(&input, &["pl0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("pl0001"), "Place handle should be removed");
        assert!(
            !content.contains("New York City"),
            "Place body should be removed"
        );
        assert!(
            content.contains("<?xml"),
            "XML declaration should remain"
        );
    }

    // -----------------------------------------------------------------------
    // remove_self_closing_place
    // -----------------------------------------------------------------------

    #[test]
    fn remove_self_closing_place() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <places>
    <place handle="pl0001"/>
    <place handle="pl0002"/>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_places(&input, &["pl0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("pl0001"), "pl0001 should be removed");
        assert!(content.contains("pl0002"), "pl0002 should remain");
    }

    // -----------------------------------------------------------------------
    // keep_unrelated_place
    // -----------------------------------------------------------------------

    #[test]
    fn keep_unrelated_place() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <places>
    <place handle="pl0001">
      <ptitle>New York City</ptitle>
    </place>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_places(&input, &["pl9999"], output_dir.path());

        assert_eq!(stats.elements_removed, 0);
        assert_eq!(stats.elements_not_found, 1);
        assert!(content.contains("pl0001"), "Place should remain");
    }

    // -----------------------------------------------------------------------
    // place_with_body_and_children
    // -----------------------------------------------------------------------

    #[test]
    fn place_with_body_and_children() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <places>
    <place handle="pl0001">
      <ptitle>New York City</ptitle>
      <coord lat="40.7128" long="-74.0060"/>
      <placeref hlink="pl0002"/>
    </place>
    <place handle="pl0002">
      <ptitle>New York</ptitle>
    </place>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_places(&input, &["pl0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(
            !content.contains("pl0001"),
            "Place handle should be removed"
        );
        assert!(
            !content.contains("40.7128"),
            "Nested <coord> should be removed with the place"
        );
        assert!(
            !content.contains("<placeref hlink=\"pl0002\"/>"),
            "Nested <placeref> should be removed with the place"
        );
        assert!(
            content.contains("pl0002"),
            "Sibling place pl0002 should remain"
        );
    }

    // -----------------------------------------------------------------------
    // place_hlink_in_event_not_removed
    // -----------------------------------------------------------------------

    #[test]
    fn place_hlink_in_event_not_removed() {
        // An event's <place hlink="..."/> reference has no `handle` attribute,
        // so it is NOT a place definition and must pass through untouched.
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <events>
    <event handle="e0001">
      <type>Birth</type>
      <place hlink="pl0001"/>
    </event>
  </events>
  <places>
    <place handle="pl0001">
      <ptitle>New York City</ptitle>
    </place>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        // Remove the actual place definition only
        let (stats, content) = run_clean_places(&input, &["pl0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        // The event's place hlink reference must survive
        assert!(
            content.contains("<place hlink=\"pl0001\"/>"),
            "Event place hlink reference should not be removed"
        );
        assert!(
            !content.contains("New York City"),
            "Place definition body should be removed"
        );
    }

    // -----------------------------------------------------------------------
    // multiple_places_mixed
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_places_mixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <places>
    <place handle="pl0001"><ptitle>New York</ptitle></place>
    <place handle="pl0002"><ptitle>Boston</ptitle></place>
    <place handle="pl0003"><ptitle>Chicago</ptitle></place>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_places(&input, &["pl0001", "pl0003"], output_dir.path());

        assert_eq!(stats.elements_removed, 2);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("New York"), "pl0001 should be removed");
        assert!(content.contains("Boston"), "pl0002 should remain");
        assert!(!content.contains("Chicago"), "pl0003 should be removed");
    }

    // -----------------------------------------------------------------------
    // namespace_prefixed_place
    // -----------------------------------------------------------------------

    #[test]
    fn namespace_prefixed_place() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <places>
    <ns:place ns:handle="pl0001" xmlns:ns="http://example.com/ns">
      <ns:ptitle>Namespaced place</ns:ptitle>
    </ns:place>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_places(&input, &["pl0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        assert!(!content.contains("pl0001"), "Namespaced place should be removed");
    }

    // -----------------------------------------------------------------------
    // places_section_preserved
    // -----------------------------------------------------------------------

    #[test]
    fn places_section_preserved() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <header><created date="2024-01-01" version="5.2"/></header>
  <places>
    <place handle="pl0001"><ptitle>Only place</ptitle></place>
  </places>
</database>"#;
        let (input, _dir) = write_temp(xml);
        let output_dir = tempfile::tempdir().unwrap();
        let (stats, content) = run_clean_places(&input, &["pl0001"], output_dir.path());

        assert_eq!(stats.elements_removed, 1);
        assert_eq!(stats.elements_not_found, 0);
        // <places> section tag should still be present even though all places removed
        assert!(content.contains("<places>"), "<places> section should be preserved");
        assert!(content.contains("</places>"), "</places> section should be preserved");
        assert!(content.contains("<header>"), "Header should remain");
    }
}
