//! Validate command — validate a .gramps file's structure.
//!
//! This module implements the `validate` subcommand, which checks
//! that a .gramps file is well-formed XML and has the expected
//! document structure.
//!
//! # Minimal validation
//!
//! Full `.gramps` file parsing (reconstructing a `Graph` from Gramps XML)
//! is a significant effort beyond Phase 7's scope. The initial `validate`
//! command implements a minimal XML structure check:
//!
//! - Verify the file is well-formed XML
//! - Verify it has a `<database>` root element with the expected namespace
//! - Verify it has a `<header>` section
//! - Report structural issues (missing required sections, malformed XML)

use clap::Args;

/// Arguments for the `validate` subcommand.
#[derive(Args, Clone, Debug)]
pub struct ValidateArgs {
    /// Path to a .gramps file to validate
    pub file: String,

    /// Promote plausibility warnings to errors
    #[arg(long)]
    pub strict: bool,
}

/// Run the validate command.
pub fn run(args: ValidateArgs) -> Result<(), crate::error::CliError> {
    let file_path = &args.file;

    // Read the file
    let content = gramps_reader::read_gramps_file(file_path)?;

    // Validate XML structure
    match validate_gramps_xml_structure(&content) {
        Ok(()) => {
            eprintln!("{}: valid", file_path);
            Ok(())
        }
        Err(errors) if args.strict || errors.iter().any(|e| e.severity == "error") => {
            for error in &errors {
                eprintln!("{}: {}", file_path, error.message);
            }
            Err(crate::error::CliError::ValidationFailed(vec![]))
        }
        Err(errors) => {
            // Non-strict mode with only warnings
            if errors.is_empty() {
                eprintln!("{}: valid", file_path);
                Ok(())
            } else {
                eprintln!("{}: valid with {} warnings", file_path, errors.len());
                for error in &errors {
                    eprintln!("Warning: {}", error.message);
                }
                Ok(())
            }
        }
    }
}

/// A validation finding.
#[derive(Debug)]
struct ValidationFinding {
    severity: &'static str,
    message: String,
}

/// Minimal XML structure validation for .gramps files.
///
/// Checks well-formedness and expected elements without full
/// graph reconstruction.
fn validate_gramps_xml_structure(content: &str) -> Result<(), Vec<ValidationFinding>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut has_database = false;
    let mut has_header = false;
    let mut errors: Vec<ValidationFinding> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "database" => {
                        has_database = true;
                        // Check for namespace
                        let has_namespace = e.attributes().filter_map(|a| a.ok()).any(|attr| {
                            let key = String::from_utf8_lossy(attr.key.as_ref());
                            let val = String::from_utf8_lossy(&attr.value);
                            key.as_ref() == "xmlns"
                                && val.as_ref() == "http://gramps-project.org/xml/1.7.2/"
                        });
                        if !has_namespace {
                            errors.push(ValidationFinding {
                                    severity: "warning",
                                    message: "<database> element missing expected namespace 'http://gramps-project.org/xml/1.7.2/'".to_string(),
                            });
                        }
                    }
                    "header" => {
                        has_header = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                errors.push(ValidationFinding {
                    severity: "error",
                    message: format!("XML parse error: {}", e),
                });
                break;
            }
            _ => {}
        }
    }

    if !has_database {
        errors.push(ValidationFinding {
            severity: "error",
            message: "Missing <database> root element".to_string(),
        });
    }

    if !has_header {
        errors.push(ValidationFinding {
            severity: "warning",
            message: "Missing <header> section in .gramps file".to_string(),
        });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_command_valid_file() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-01" version="5.2"/>
    <researcher><resname>Test</resname></researcher>
  </header>
</database>"#;
        let result = validate_gramps_xml_structure(xml);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_command_missing_database() {
        let xml = r#"<?xml version="1.0"?>
<root></root>"#;
        let result = validate_gramps_xml_structure(xml);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("database")));
    }

    #[test]
    fn validate_command_not_xml() {
        let content = "this is not XML at all";
        let result = validate_gramps_xml_structure(content);
        assert!(result.is_err());
    }

    #[test]
    fn validate_command_file_not_found() {
        let args = ValidateArgs {
            file: "/nonexistent/file.gramps".to_string(),
            strict: false,
        };
        let result = run(args);
        match result {
            Err(crate::error::CliError::Io { .. }) => {} // Expected
            _ => panic!("Expected Io error, got: {:?}", result),
        }
    }

    #[test]
    fn validate_command_gzip_compressed_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // A minimal valid Gramps XML document with the expected namespace
        // and header section.
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header>
    <created date="2025-01-01" version="5.2"/>
    <researcher><resname>Test</resname></researcher>
  </header>
</database>"#;

        // Gzip-compress the XML.
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(xml.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&compressed).unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let args = ValidateArgs {
            file: path,
            strict: false,
        };
        let result = run(args);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    }

    #[test]
    fn validate_command_missing_namespace() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <header>
    <created date="2025-01-01" version="5.2"/>
  </header>
</database>"#;
        let result = validate_gramps_xml_structure(xml);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("namespace")));
    }

    #[test]
    fn validate_command_with_header() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
</database>"#;
        let result = validate_gramps_xml_structure(xml);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("header")));
    }
}
