//! Library root for the visualize crate.
//!
//! This crate builds the graph data model and date imputation algorithms
//! that power the Tauri-based family-group visualization. When the
//! `visualize` feature is enabled, the `gramps-gen-visualize` binary
//! provides a Tauri desktop shell.
//!
//! Without the `visualize` feature this crate is a plain library —
//! no WebKit2GTK / WebView2 system dependencies are needed.