//! Unified error types for the gramps-reader crate.

use std::fmt;

/// Unified error type covering all failure modes of gramps-reader.
#[derive(Debug)]
pub enum Error {
    /// XML parse error with descriptive message.
    XmlParseError {
        /// Human-readable error message with byte position.
        message: String,
    },
    /// File I/O error (file not found, permission denied, etc.).
    IoError {
        /// Path of the file that could not be read.
        path: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Gzip decompression error (corrupted or truncated gzip data).
    GzipError {
        /// Path of the file that could not be decompressed.
        path: String,
        /// Underlying decompression error.
        source: std::io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::XmlParseError { message } => {
                write!(f, "XML parse error: {}", message)
            }
            Error::IoError { path, source } => {
                write!(f, "I/O error for '{}': {}", path, source)
            }
            Error::GzipError { path, source } => {
                write!(f, "gzip decompression error for '{}': {}", path, source)
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::IoError { source, .. } | Error::GzipError { source, .. } => Some(source),
            Error::XmlParseError { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_parse_error_display() {
        let err = Error::XmlParseError {
            message: "Unexpected end of input at byte 42".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("XML parse error"));
        assert!(display.contains("byte 42"));
    }

    #[test]
    fn xml_parse_error_source_is_none() {
        use std::error::Error as _;
        let err = Error::XmlParseError {
            message: "test error".to_string(),
        };
        assert!(err.source().is_none());
    }

    #[test]
    fn io_error_source_returns_some() {
        use std::error::Error as _;
        let source = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::IoError {
            path: "missing.gramps".to_string(),
            source,
        };
        assert!(err.source().is_some());
    }

    #[test]
    fn gzip_error_source_returns_some() {
        use std::error::Error as _;
        let source = std::io::Error::new(std::io::ErrorKind::InvalidData, "corrupt gzip");
        let err = Error::GzipError {
            path: "corrupt.gramps".to_string(),
            source,
        };
        assert!(err.source().is_some());
    }

    #[test]
    fn io_error_display() {
        let err = Error::IoError {
            path: "missing.gramps".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let display = format!("{}", err);
        assert!(display.contains("I/O error"));
        assert!(display.contains("missing.gramps"));
        assert!(display.contains("file not found"));
    }

    #[test]
    fn gzip_error_display() {
        let err = Error::GzipError {
            path: "corrupt.gramps".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "corrupt gzip"),
        };
        let display = format!("{}", err);
        assert!(display.contains("gzip decompression error"));
        assert!(display.contains("corrupt.gramps"));
        assert!(display.contains("corrupt gzip"));
    }
}
