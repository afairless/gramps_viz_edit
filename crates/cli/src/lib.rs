//! CLI library — gramps-gen command-line interface.
//!
//! This library provides the command modules, error types, progress
//! reporting, and scenario file parsing for the gramps-gen binary.

#![deny(deprecated)]

pub mod commands;
pub mod error;
pub mod progress;
pub mod scenario;

/// Re-export commonly used types.
pub use error::CliError;