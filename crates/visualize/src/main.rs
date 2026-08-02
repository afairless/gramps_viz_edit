//! Tauri application entry point — `gramps-gen-visualize`.
//!
//! This binary is only compiled when the `visualize` feature is enabled
//! (see `required-features` in Cargo.toml). It opens a native webview
//! window that renders the family-group force-directed graph.

fn main() {
    #[cfg(feature = "visualize")]
    {
        tauri::Builder::default()
            .invoke_handler(tauri::generate_handler![load_graph])
            .run(tauri::generate_context!())
            .expect("error while running gramps-gen-visualize");
    }
}

/// Tauri IPC command: load a `.gramps` file and return the graph data.
#[cfg(feature = "visualize")]
#[tauri::command]
fn load_graph(path: &str, no_impute: bool, generation_gap: u32) -> Result<visualize::GraphData, String> {
    visualize::load_graph_data(path, no_impute, generation_gap)
}