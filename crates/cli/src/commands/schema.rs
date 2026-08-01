//! Schema subcommand — list and download Gramps schemas.
//!
//! This module implements the `schema` subcommand group with:
//! - `schema list` — show local and available remote schemas
//! - `schema download [VERSION|--all]` — download a schema from Gramps GitHub

use clap::Args;
use clap::Subcommand;
use std::path::PathBuf;

/// Arguments for the `schema` subcommand group.
#[derive(Subcommand)]
pub enum SchemaCommand {
    /// List available schemas (local + remote)
    List(SchemaListArgs),
    /// Download a schema from Gramps GitHub
    Download(SchemaDownloadArgs),
}

/// Arguments for `schema list`.
#[derive(Args, Default)]
pub struct SchemaListArgs {}

/// Arguments for `schema download`.
#[derive(Args)]
pub struct SchemaDownloadArgs {
    /// Specific version to download (e.g., "5.1")
    #[arg()]
    pub version: Option<String>,

    /// Download all available versions
    #[arg(long)]
    pub all: bool,
}

/// Known version → latest tag mapping for Gramps releases.
/// Populated lazily from GitHub API, with a static fallback.
const KNOWN_TAGS: &[(&str, &str)] = &[
    ("5.0", "v5.0.2"),
    ("5.1", "v5.1.6"),
    ("5.2", "v5.2.4"),
    ("6.0", "v6.0.8"),
];

/// Run the schema command.
pub fn run(command: SchemaCommand) -> Result<(), crate::CliError> {
    match command {
        SchemaCommand::List(_) => cmd_list(),
        SchemaCommand::Download(args) => cmd_download(args),
    }
}

// ---------------------------------------------------------------------------
// schema list
// ---------------------------------------------------------------------------

/// Scan local schemas from the `schemas/` directory.
fn scan_local_schemas() -> Vec<String> {
    let mut versions = Vec::new();
    let schemas_dir = workspace_root().join("schemas");
    if let Ok(entries) = std::fs::read_dir(&schemas_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(rest) = name.strip_prefix("schema-") {
                if let Some(ver) = rest.strip_suffix(".json") {
                    versions.push(ver.to_string());
                }
            }
        }
    }
    versions.sort();
    versions
}

/// Find the workspace root by walking up from the manifest dir.
fn workspace_root() -> PathBuf {
    // In the binary, CARGO_MANIFEST_DIR isn't set. Determine from binary path or cwd.
    // We look for the `schemas/` directory relative to cwd.
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut candidate = cwd.clone();
    loop {
        if candidate.join("schemas").join("schema-5.2.json").exists() {
            return candidate;
        }
        if !candidate.pop() {
            break;
        }
    }
    // Fallback: use CWD
    cwd
}

/// Display local and remote schemas.
fn cmd_list() -> Result<(), crate::CliError> {
    let local = scan_local_schemas();

    println!("Local schemas:");
    for version in KNOWN_TAGS {
        let (ver, tag) = *version;
        let is_local = local.contains(&ver.to_string());
        let marker = if is_local { "✓" } else { " " };
        println!(
            "  {} {} (latest: {}){}",
            marker,
            ver,
            tag,
            if is_local { "" } else { " — not downloaded" }
        );
    }

    // Try to fetch remote tags from GitHub API
    println!();
    match fetch_remote_tags() {
        Ok(remote_tags) => {
            println!("Available from GitHub (gramps-project/gramps):");
            for (ver, tag) in &remote_tags {
                let is_local = local.contains(ver);
                let marker = if is_local { "✓" } else { "•" };
                println!(
                    "  {} {} (latest: {}){}",
                    marker,
                    ver,
                    tag,
                    if is_local { "" } else { " — not downloaded" }
                );
            }
        }
        Err(e) => {
            eprintln!("Note: Cannot reach GitHub API. Showing local schemas only.");
            eprintln!("  ({})", e);
        }
    }

    println!();
    println!("Run `gramps-gen schema download <version>` to download.");
    println!("Run `gramps-gen schema download --all` to download all available.");

    Ok(())
}

// ---------------------------------------------------------------------------
// schema download
// ---------------------------------------------------------------------------

/// Download a schema version.
fn cmd_download(args: SchemaDownloadArgs) -> Result<(), crate::CliError> {
    let versions_to_download: Vec<String> = if args.all {
        KNOWN_TAGS.iter().map(|(v, _)| v.to_string()).collect()
    } else if let Some(version) = &args.version {
        vec![version.clone()]
    } else {
        return Err(crate::CliError::ConfigError(
            "Please specify a version to download, or use --all".to_string(),
        ));
    };

    for version in &versions_to_download {
        download_single_version(version)?;
    }

    Ok(())
}

/// Download a single schema version.
fn download_single_version(version: &str) -> Result<(), crate::CliError> {
    // Resolve version to git tag
    let tag = KNOWN_TAGS
        .iter()
        .find(|(v, _)| *v == version)
        .map(|(_, tag)| *tag)
        .ok_or_else(|| {
            crate::CliError::SchemaDownloadError(format!(
            "No tag found for version {}. Run `gramps-gen schema list` to see available versions.",
            version
        ))
        })?;

    // Check that Python is available
    let python_check = std::process::Command::new("python3")
        .arg("--version")
        .output();
    if python_check.is_err() {
        return Err(crate::CliError::PythonNotFound);
    }

    let schemas_dir = workspace_root().join("schemas");
    let output_path = schemas_dir.join(format!("schema-{}.json", version));

    // Warn about code execution
    eprintln!("WARNING: This will download and execute Python code from");
    eprintln!("  gramps-project/gramps at tag {}", tag);
    eprintln!("Continuing in 3 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Create temp dir for clone
    let temp_dir = std::env::temp_dir().join(format!("gramps-schema-{}", version));
    let _ = std::fs::remove_dir_all(&temp_dir);

    eprintln!("Cloning gramps-project/gramps at tag {}...", tag);
    let clone_result = std::process::Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            tag,
            "https://github.com/gramps-project/gramps.git",
        ])
        .arg(temp_dir.to_str().unwrap())
        .output()
        .map_err(|e| {
            crate::CliError::GitCloneFailed(format!(
                "Failed to run git clone: {}. Is git installed?",
                e
            ))
        })?;

    if !clone_result.status.success() {
        let stderr = String::from_utf8_lossy(&clone_result.stderr);
        return Err(crate::CliError::GitCloneFailed(format!(
            "Git clone failed for tag {}: {}",
            tag,
            stderr.trim()
        )));
    }

    // Run the extractor
    eprintln!("Running schema extractor...");
    let extractor_path = workspace_root().join("extract/extract_schema.py");
    let extractor_output = std::process::Command::new("python3")
        .args([
            extractor_path.to_str().unwrap(),
            "--version",
            version,
            "--output",
            output_path.to_str().unwrap(),
        ])
        .env("PYTHONPATH", temp_dir.to_str().unwrap())
        .output()
        .map_err(|e| {
            crate::CliError::SchemaExtractionFailed(format!(
                "Failed to run Python extractor: {}",
                e
            ))
        })?;

    if !extractor_output.status.success() {
        let stderr = String::from_utf8_lossy(&extractor_output.stderr);
        let _ = std::fs::remove_file(&output_path);
        return Err(crate::CliError::SchemaExtractionFailed(format!(
            "Extractor failed: {}",
            stderr.trim()
        )));
    }

    // Validate the output
    let output_content = std::fs::read_to_string(&output_path).map_err(|e| {
        crate::CliError::SchemaExtractionFailed(format!("Cannot read output file: {}", e))
    })?;
    let parsed: serde_json::Value = serde_json::from_str(&output_content).map_err(|e| {
        crate::CliError::SchemaExtractionFailed(format!("Output is not valid JSON: {}", e))
    })?;

    // Verify required fields
    let has_primary = parsed
        .get("primary_types")
        .and_then(|v| v.as_object())
        .is_some();
    let has_secondary = parsed
        .get("secondary_types")
        .and_then(|v| v.as_object())
        .is_some();
    let has_enum = parsed
        .get("enum_types")
        .and_then(|v| v.as_object())
        .is_some();
    let output_version = parsed.get("version").and_then(|v| v.as_str()).unwrap_or("");

    if !has_primary || !has_secondary || !has_enum {
        let _ = std::fs::remove_file(&output_path);
        return Err(crate::CliError::SchemaExtractionFailed(
            "Extracted JSON is missing required sections (primary_types, secondary_types, enum_types)".to_string(),
        ));
    }

    if output_version != version {
        eprintln!(
            "Note: extracted version is {} (expected {})",
            output_version, version
        );
    }

    // Clean up temp dir
    let _ = std::fs::remove_dir_all(&temp_dir);

    let primary_count = parsed["primary_types"]
        .as_object()
        .map(|o| o.len())
        .unwrap_or(0);
    let secondary_count = parsed["secondary_types"]
        .as_object()
        .map(|o| o.len())
        .unwrap_or(0);
    let enum_count = parsed["enum_types"]
        .as_object()
        .map(|o| o.len())
        .unwrap_or(0);

    eprintln!(
        "Schema written to {} ({} primary types, {} secondary types, {} enum types)",
        output_path.display(),
        primary_count,
        secondary_count,
        enum_count
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// GitHub API
// ---------------------------------------------------------------------------

/// Fetch available Gramps tags from GitHub API.
fn fetch_remote_tags() -> Result<Vec<(String, String)>, String> {
    // Try cache first
    let cache_dir = dirs_cache_dir();
    let cache_file = cache_dir.join("tags.json");
    if let Some(cached) = read_cached_tags(&cache_file) {
        return Ok(cached);
    }

    // Fetch from GitHub using ureq
    let url = "https://api.github.com/repos/gramps-project/gramps/tags?per_page=100";
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let tags: Vec<serde_json::Value> =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    // Process tags: extract major.minor from vX.Y.Z, keep latest patch per major.minor
    let mut version_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for tag in &tags {
        let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
        // Parse tag like "v5.1.6"
        let stripped = name.strip_prefix('v').unwrap_or(name);
        let parts: Vec<&str> = stripped.split('.').collect();
        if parts.len() == 3 {
            let major: u32 = parts[0].parse().unwrap_or(0);
            let minor: u32 = parts[1].parse().unwrap_or(0);
            let patch: u32 = parts[2].parse().unwrap_or(0);
            let key = format!("{}.{}", major, minor);
            let latest_patch = version_map
                .get(&key)
                .map(|existing| {
                    let existing_stripped = existing.strip_prefix('v').unwrap_or(existing);
                    let existing_parts: Vec<&str> = existing_stripped.split('.').collect();
                    if existing_parts.len() == 3 {
                        existing_parts[2].parse().unwrap_or(0)
                    } else {
                        0
                    }
                })
                .unwrap_or(0);
            if patch > latest_patch {
                version_map.insert(key, name.to_string());
            }
        }
    }

    // Filter to versions >= 5.0, exclude pre-release
    let mut result: Vec<(String, String)> = version_map
        .into_iter()
        .filter(|(ver, _)| {
            if let Some((major,)) = parse_two_part(ver) {
                major >= 5
            } else {
                false
            }
        })
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));

    // Cache the result
    if let Some(parent) = cache_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&result) {
        let _ = std::fs::write(&cache_file, &json);
    }

    Ok(result)
}

/// Parse "X.Y" into (X, Y).
fn parse_two_part(ver: &str) -> Option<(u32,)> {
    let parts: Vec<&str> = ver.split('.').collect();
    if parts.len() >= 2 {
        parts[0].parse().ok().map(|m| (m,))
    } else {
        None
    }
}

/// Get the cache directory for gramps-gen.
fn dirs_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".cache/gramps-gen")
}

/// Read cached tags from the cache file if it's fresh enough (TTL: 1 hour).
fn read_cached_tags(cache_file: &PathBuf) -> Option<Vec<(String, String)>> {
    let metadata = std::fs::metadata(cache_file).ok()?;
    let modified = metadata.modified().ok()?;
    let now = std::time::SystemTime::now();
    let age = now.duration_since(modified).ok()?;
    if age.as_secs() > 3600 {
        return None; // Cache expired (1 hour)
    }
    let content = std::fs::read_to_string(cache_file).ok()?;
    serde_json::from_str(&content).ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_two_part() {
        assert_eq!(parse_two_part("5.2"), Some((5,)));
        assert_eq!(parse_two_part("5.0"), Some((5,)));
        assert_eq!(parse_two_part("6.0"), Some((6,)));
        assert_eq!(parse_two_part("invalid"), None);
    }

    #[test]
    fn test_scan_local_schemas() {
        // We can't easily test this without real files, but we can verify it doesn't crash
        let _ = scan_local_schemas();
    }

    #[test]
    fn test_known_tags_valid() {
        for (ver, tag) in KNOWN_TAGS {
            assert!(tag.starts_with('v'), "Tag {} should start with 'v'", tag);
            let parts: Vec<&str> = ver.split('.').collect();
            assert_eq!(parts.len(), 2, "Version {} should be major.minor", ver);
        }
    }
}
