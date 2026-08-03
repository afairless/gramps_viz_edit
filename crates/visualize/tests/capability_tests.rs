//! Tests for the Tauri v2 capability file.
//!
//! Validates that `capabilities/default.json` exists, is valid JSON, and
//! has the expected structure. This is a file-format check — it does not
//! test the Tauri IPC runtime behavior.

use std::fs;

#[test]
fn capability_file_exists() {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    assert!(path.exists(), "capabilities/default.json not found");
}

#[test]
fn capability_file_is_valid_json() {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let content = fs::read_to_string(&path).expect("Failed to read capabilities/default.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("capabilities/default.json is not valid JSON");

    assert_eq!(
        parsed["identifier"], "default",
        "identifier must be 'default'"
    );
    assert!(
        parsed["description"]
            .as_str()
            .map_or(false, |s| !s.is_empty()),
        "description must be a non-empty string"
    );

    // Verify the window list contains "main"
    let windows = parsed["windows"]
        .as_array()
        .expect("windows must be an array");
    assert!(
        windows.contains(&serde_json::json!("main")),
        "windows must include 'main'"
    );

    // Verify the required permissions are present
    let permissions = parsed["permissions"]
        .as_array()
        .expect("permissions must be an array");
    assert!(
        permissions.contains(&serde_json::json!("core:default")),
        "permissions must include core:default"
    );
    assert!(
        permissions.contains(&serde_json::json!("dialog:allow-open")),
        "permissions must include dialog:allow-open"
    );
    assert!(
        permissions.contains(&serde_json::json!("dialog:allow-save")),
        "permissions must include dialog:allow-save"
    );
}
