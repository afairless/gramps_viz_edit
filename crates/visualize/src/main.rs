//! Tauri application entry point — `gramps-gen-visualize`.
//!
//! This binary is only compiled when the `visualize` feature is enabled
//! (see `required-features` in Cargo.toml). It opens a native webview
//! window that renders the family-group force-directed graph.
//!
//! # Argument parsing
//!
//! A single positional file path is accepted. Flags:
//! - `--no-impute`: Skip date imputation for undated persons.
//! - `--generation-gap <N>`: Years per generation (default 25).

use tauri::Manager;

fn main() {
    #[cfg(feature = "visualize")]
    {
        // Single-pass argument extraction: positional path + flags.
        let cli_args = {
            let args: Vec<String> = std::env::args().skip(1).collect();
            visualize::parse_cli_args(&args)
        };

        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .setup(move |app| {
                if let Some(window) = app.get_webview_window("main") {
                    let path_json = serde_json::to_string(&cli_args.path)
                        .unwrap_or_else(|_| "null".into());
                    let no_impute_json = serde_json::to_string(&cli_args.no_impute)
                        .unwrap_or_else(|_| "false".into());
                    let gap_json = serde_json::to_string(&cli_args.generation_gap)
                        .unwrap_or_else(|_| "25".into());
                    window.eval(&format!(
                        "window.__GRAMPS_FILE__ = {}; window.__NO_IMPUTE__ = {}; window.__GENERATION_GAP__ = {};",
                        path_json, no_impute_json, gap_json,
                    ))?;
                }
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![load_graph, export_selections])
            .run(tauri::generate_context!())
            .expect("error while running gramps-gen-visualize");
    }
}

/// Tauri IPC command: load a `.gramps` file and return the graph data.
#[cfg(feature = "visualize")]
#[tauri::command]
fn load_graph(
    path: &str,
    no_impute: bool,
    generation_gap: u32,
) -> Result<visualize::GraphData, String> {
    visualize::load_graph_data(path, no_impute, generation_gap)
}

/// Tauri IPC command: export selected persons to a JSON file.
/// Writes the serialized selection to `path` and returns the path on success.
#[cfg(feature = "visualize")]
#[tauri::command]
fn export_selections(
    path: String,
    selections: Vec<visualize::SelectedPerson>,
) -> Result<String, String> {
    let export = serde_json::to_string_pretty(&selections)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, &export).map_err(|e| format!("Cannot write to '{}': {}", path, e))?;
    Ok(path)
}