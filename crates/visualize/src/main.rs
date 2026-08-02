//! Tauri application entry point — `gramps-gen-visualize`.
//!
//! This binary is only compiled when the `visualize` feature is enabled
//! (see `required-features` in Cargo.toml). It opens a native webview
//! window that renders the family-group force-directed graph.

fn main() {
    #[cfg(feature = "visualize")]
    {
        tauri::Builder::default()
            .run(tauri::generate_context!())
            .expect("error while running gramps-gen-visualize");
    }
}
