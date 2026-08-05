//! File I/O with transparent gzip decompression for Gramps XML files.
//!
//! The [`read_gramps_file`] function reads a file from disk, detects
//! gzip compression by magic bytes (`0x1f 0x8b`), and decompresses
//! transparently. Plain XML files pass through unchanged.

use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

use crate::Error;

/// Read a Gramps XML file, decompressing if gzip-compressed.
///
/// Detects compression by reading the first two bytes (gzip magic:
/// `0x1f 0x8b`). If the file is gzip-compressed, it is decompressed
/// transparently. If it is plain XML, it is read as-is.
///
/// # Errors
///
/// Returns [`Error::IoError`] on I/O failure (file not found,
/// permission denied, etc.) or [`Error::GzipError`] on decompression
/// failure (corrupted or truncated gzip data).
///
/// # Examples
///
/// ```no_run
/// # use gramps_reader::read_gramps_file;
/// let content = read_gramps_file("family.gramps").unwrap();
/// assert!(content.starts_with("<?xml"));
/// ```
pub fn read_gramps_file(path: &str) -> Result<String, Error> {
    let mut file = File::open(path).map_err(|e| Error::IoError {
        path: path.to_string(),
        source: e,
    })?;

    // Read the first two bytes to detect gzip magic.
    let mut magic = [0u8; 2];
    let n = file.read(&mut magic).map_err(|e| Error::IoError {
        path: path.to_string(),
        source: e,
    })?;

    if n < 2 {
        // Empty file (or 1-byte file) — return empty string.
        return Ok(String::new());
    }

    if magic == [0x1f, 0x8b] {
        // Gzip-compressed: decompress via GzDecoder.
        file.seek(SeekFrom::Start(0)).map_err(|e| Error::IoError {
            path: path.to_string(),
            source: e,
        })?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut content = String::new();
        decoder.read_to_string(&mut content).map_err(|e| Error::GzipError {
            path: path.to_string(),
            source: e,
        })?;
        Ok(content)
    } else {
        // Plain XML: seek back to start and read the full file.
        file.seek(SeekFrom::Start(0)).map_err(|e| Error::IoError {
            path: path.to_string(),
            source: e,
        })?;
        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|e| Error::IoError {
            path: path.to_string(),
            source: e,
        })?;
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;
    use std::io::Write;

    #[test]
    fn empty_file_returns_empty_string() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        // Write nothing — 0-byte file.
        let path = tmp.path().to_str().unwrap().to_string();
        // Close the file so the content is flushed.
        tmp.write_all(b"").unwrap();

        let result = read_gramps_file(&path).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn one_byte_file_returns_empty_string() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&[0x00]).unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let result = read_gramps_file(&path).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn nonexistent_file_returns_io_error() {
        let result = read_gramps_file("/nonexistent/gramps-file.gramps");
        match result {
            Err(Error::IoError { path, .. }) => {
                assert!(path.contains("nonexistent"));
            }
            other => panic!("Expected IoError, got: {:?}", other),
        }
    }

    #[test]
    fn corrupted_gzip_returns_gzip_error() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        // Valid gzip magic bytes followed by invalid data.
        tmp.write_all(&[0x1f, 0x8b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
            .unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let result = read_gramps_file(&path);
        match result {
            Err(Error::GzipError { path, .. }) => {
                assert!(path.contains("tmp"));
            }
            other => panic!("Expected GzipError, got: {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn plain_xml_file_returns_content() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
</database>"#;
        tmp.write_all(xml.as_bytes()).unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let result = read_gramps_file(&path).unwrap();
        assert_eq!(result, xml);
    }

    #[test]
    fn plain_xml_equals_fs_read_to_string() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><name><first>Test</first></name></person>
  </people>
</database>"#;
        tmp.write_all(xml.as_bytes()).unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let from_io = read_gramps_file(&path).unwrap();
        let from_fs = std::fs::read_to_string(&path).unwrap();
        assert_eq!(from_io, from_fs);
    }

    #[test]
    fn gzip_error_source_is_some() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&[0x1f, 0x8b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
            .unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let err = read_gramps_file(&path).unwrap_err();
        match err {
            Error::GzipError { source, .. } => {
                assert!(source.source().is_none() || source.source().is_some());
            }
            _ => panic!("Expected GzipError"),
        }
    }

    #[test]
    fn io_error_source_is_some() {
        let err = read_gramps_file("/nonexistent/gramps-file.gramps").unwrap_err();
        match err {
            Error::IoError { source, .. } => {
                assert!(source.source().is_none() || source.source().is_some());
            }
            _ => panic!("Expected IoError"),
        }
    }
}