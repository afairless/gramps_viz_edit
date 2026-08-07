//! Unified error types for the CLI.

use std::fmt;

/// Unified CLI error type covering all failure modes.
#[derive(Debug)]
pub enum CliError {
    /// I/O error with context.
    Io {
        path: String,
        source: std::io::Error,
    },
    /// Configuration parsing error (CLI args or YAML scenario).
    ConfigError(String),
    /// Generation failed (exhausted constraints, invalid config).
    GenerationFailed(typed_graph::generate::GenerationError),
    /// Validation found errors.
    ValidationFailed(Vec<typed_graph::ValidationError>),
    /// Serialization failure.
    SerializationFailed(output::SerializationError),
    /// Scenario file parse error.
    ScenarioError(crate::scenario::ScenarioError),
    /// GitHub API unreachable or rate-limited.
    GitHubApiError(String),
    /// Git clone failed.
    GitCloneFailed(String),
    /// Schema extraction (Python) failed.
    SchemaExtractionFailed(String),
    /// Schema version not found in tag map.
    SchemaDownloadError(String),
    /// Python 3 not available.
    PythonNotFound,

    /// Gzip decompression error (corrupted or truncated gzip data).
    GzipError {
        /// Path of the file that could not be decompressed.
        path: String,
        /// Underlying decompression error.
        source: std::io::Error,
    },
    /// XML parse error with descriptive message.
    XmlParseError {
        /// Human-readable error message with byte position.
        message: String,
    },
    /// Diff analysis failed.
    DiffFailed(String),
    /// Diff-viz integration failed.
    IntegrateFailed(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Io { path, source } => {
                write!(f, "I/O error for '{}': {}", path, source)
            }
            CliError::ConfigError(msg) => {
                write!(f, "configuration error: {}", msg)
            }
            CliError::GenerationFailed(e) => {
                write!(f, "generation failed: {}", e)
            }
            CliError::ValidationFailed(errors) => {
                write!(f, "validation failed with {} error(s)", errors.len())
            }
            CliError::SerializationFailed(e) => {
                write!(f, "serialization failed: {}", e)
            }
            CliError::ScenarioError(e) => {
                write!(f, "scenario error: {}", e)
            }
            CliError::GitHubApiError(msg) => {
                write!(f, "GitHub API error: {}", msg)
            }
            CliError::GitCloneFailed(msg) => {
                write!(f, "git clone failed: {}", msg)
            }
            CliError::SchemaExtractionFailed(msg) => {
                write!(f, "schema extraction failed: {}", msg)
            }
            CliError::SchemaDownloadError(msg) => {
                write!(f, "schema download error: {}", msg)
            }
            CliError::PythonNotFound => {
                write!(
                    f,
                    "Python 3 not found. Install Python 3 or manually place a schema file."
                )
            }
            CliError::GzipError { path, source } => {
                write!(f, "gzip decompression error for '{}': {}", path, source)
            }
            CliError::XmlParseError { message } => {
                write!(f, "XML parse error: {}", message)
            }
            CliError::DiffFailed(msg) => {
                write!(f, "diff failed: {}", msg)
            }
            CliError::IntegrateFailed(msg) => {
                write!(f, "integration failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Io { source, .. } => Some(source),
            CliError::GenerationFailed(e) => Some(e),
            CliError::SerializationFailed(e) => Some(e),
            CliError::ScenarioError(e) => Some(e),
            CliError::GzipError { source, .. } => Some(source),
            CliError::XmlParseError { .. } => None,
            _ => None,
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        CliError::Io {
            path: String::new(),
            source: err,
        }
    }
}

impl From<typed_graph::generate::GenerationError> for CliError {
    fn from(err: typed_graph::generate::GenerationError) -> Self {
        CliError::GenerationFailed(err)
    }
}

impl From<output::SerializationError> for CliError {
    fn from(err: output::SerializationError) -> Self {
        CliError::SerializationFailed(err)
    }
}

impl From<crate::scenario::ScenarioError> for CliError {
    fn from(err: crate::scenario::ScenarioError) -> Self {
        CliError::ScenarioError(err)
    }
}

impl From<gramps_reader::Error> for CliError {
    fn from(err: gramps_reader::Error) -> Self {
        match err {
            gramps_reader::Error::IoError { path, source } => CliError::Io { path, source },
            gramps_reader::Error::GzipError { path, source } => {
                CliError::GzipError { path, source }
            }
            gramps_reader::Error::XmlParseError { message } => CliError::XmlParseError { message },
            gramps_reader::Error::UnsupportedSchema {
                version,
                schema_version,
            } => CliError::ConfigError(format!(
                "unsupported schema version '{}' (file reports {}; not compiled in). \
                     hint: run `gramps-gen schema download {}` to add support",
                schema_version, version, schema_version
            )),
        }
    }
}

impl From<integrate::IntegrateError> for CliError {
    fn from(err: integrate::IntegrateError) -> Self {
        CliError::IntegrateFailed(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_io_display() {
        let err = CliError::Io {
            path: "test.txt".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let display = format!("{}", err);
        assert!(display.contains("test.txt"));
        assert!(display.contains("file not found"));
    }

    #[test]
    fn cli_error_config_display() {
        let err = CliError::ConfigError("invalid count".to_string());
        let display = format!("{}", err);
        assert!(display.contains("invalid count"));
    }

    #[test]
    fn cli_error_validation_display() {
        let err = CliError::ValidationFailed(vec![]);
        let display = format!("{}", err);
        assert!(display.contains("0 error"));
    }

    #[test]
    fn cli_error_generation_display() {
        let err = CliError::GenerationFailed(
            typed_graph::generate::GenerationError::InvalidConfig("bad config".to_string()),
        );
        let display = format!("{}", err);
        assert!(display.contains("bad config"));
    }

    #[test]
    fn cli_error_serialization_display() {
        let err = CliError::SerializationFailed(output::SerializationError::UnsupportedType(
            "Foo".to_string(),
        ));
        let display = format!("{}", err);
        assert!(display.contains("Foo"));
    }

    #[test]
    fn cli_error_scenario_display() {
        let err = CliError::ScenarioError(crate::scenario::ScenarioError::Io {
            path: "test.yaml".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        });
        let display = format!("{}", err);
        assert!(display.contains("test.yaml"));
    }

    #[test]
    fn cli_error_io_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err: CliError = io_err.into();
        match err {
            CliError::Io { .. } => {}
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn cli_error_xml_parse_display() {
        let err = CliError::XmlParseError {
            message: "Unexpected end of input at byte 42".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("XML parse error"));
        assert!(display.contains("byte 42"));
    }

    #[test]
    fn cli_error_xml_parse_source_is_none() {
        use std::error::Error;
        let err = CliError::XmlParseError {
            message: "test error".to_string(),
        };
        assert!(err.source().is_none());
    }

    #[test]
    fn cli_error_source_chain() {
        use std::error::Error;
        let err = CliError::GenerationFailed(
            typed_graph::generate::GenerationError::InvalidConfig("bad".to_string()),
        );
        assert!(err.source().is_some());
    }

    #[test]
    fn cli_error_gzip_error_display() {
        let err = CliError::GzipError {
            path: "corrupt.gramps".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "corrupt gzip data"),
        };
        let display = format!("{}", err);
        assert!(display.contains("gzip decompression error"));
        assert!(display.contains("corrupt.gramps"));
        assert!(display.contains("corrupt gzip data"));
    }

    #[test]
    fn cli_error_gzip_error_source_returns_some() {
        use std::error::Error;
        let err = CliError::GzipError {
            path: "corrupt.gramps".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "bad"),
        };
        assert!(err.source().is_some());
    }

    #[test]
    fn cli_error_from_gramps_reader_io() {
        let reader_err = gramps_reader::Error::IoError {
            path: "missing.gramps".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        let cli_err: CliError = reader_err.into();
        match cli_err {
            CliError::Io { path, .. } => assert_eq!(path, "missing.gramps"),
            other => panic!("Expected Io variant, got: {:?}", other),
        }
    }

    #[test]
    fn cli_error_from_gramps_reader_gzip() {
        let reader_err = gramps_reader::Error::GzipError {
            path: "corrupt.gramps".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "bad gzip"),
        };
        let cli_err: CliError = reader_err.into();
        match cli_err {
            CliError::GzipError { path, .. } => assert_eq!(path, "corrupt.gramps"),
            other => panic!("Expected GzipError variant, got: {:?}", other),
        }
    }

    #[test]
    fn cli_error_from_gramps_reader_xml_parse() {
        let reader_err = gramps_reader::Error::XmlParseError {
            message: "parse error".to_string(),
        };
        let cli_err: CliError = reader_err.into();
        match cli_err {
            CliError::XmlParseError { message } => assert_eq!(message, "parse error"),
            other => panic!("Expected XmlParseError variant, got: {:?}", other),
        }
    }

    #[test]
    fn cli_error_integrate_failed_display() {
        let err = CliError::IntegrateFailed("bad csv".to_string());
        let display = format!("{}", err);
        assert!(display.contains("integration failed"));
        assert!(display.contains("bad csv"));
    }

    #[test]
    fn cli_error_from_integrate_error() {
        let integrate_err = integrate::IntegrateError::DiffReadError("bad file".to_string());
        let cli_err: CliError = integrate_err.into();
        match cli_err {
            CliError::IntegrateFailed(msg) => assert!(msg.contains("bad file")),
            other => panic!("Expected IntegrateFailed variant, got: {:?}", other),
        }
    }
}
