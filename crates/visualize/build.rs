//! Build script for the visualize crate.
//!
//! `tauri-build` processes `tauri.conf.json` and bundles frontend assets
//! from `frontend/dist/`. It is only needed when the `visualize` feature
//! (the Tauri desktop shell) is enabled; otherwise this script is a no-op
//! so the crate builds as a plain library without system webview deps.

fn main() {
    #[cfg(feature = "visualize")]
    tauri_build::build();
}
