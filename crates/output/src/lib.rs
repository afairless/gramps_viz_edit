//! Gramps XML output for generated genealogy graphs.
//!
//! This crate walks a validated [`Graph`] and produces Gramps XML
//! (`.gramps` format) following the RelaxNG schema.
//!
//! # Architecture
//!
//! - [`SerializationMap`] maps Graph types to XML element and attribute names.
//! - [`GraphXmlWriter`] walks the graph and emits XML via a `Write`-based API.
//! - [`SerializationError`] covers I/O errors, unsupported types, and
//!   missing required fields.

pub mod serialization_map;
pub mod xml;

pub use serialization_map::SerializationMap;
pub use xml::GraphXmlWriter;
pub use xml::SerializationError;