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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::XmlParseError { message } => {
                write!(f, "XML parse error: {}", message)
            }
        }
    }
}

impl std::error::Error for Error {}

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
}
