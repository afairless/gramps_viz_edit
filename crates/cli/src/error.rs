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
    fn cli_error_source_chain() {
        use std::error::Error;
        let err = CliError::GenerationFailed(
            typed_graph::generate::GenerationError::InvalidConfig("bad".to_string()),
        );
        assert!(err.source().is_some());
    }
}