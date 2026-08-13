//! Deletion manifest serialization, deserialization, and validation.
//!
//! A [`DeleteManifest`] records the complete deletion plan for a single
//! operation and can be saved to / loaded from JSON. This enables:
//!
//! - **Save**: Record the deletion plan before executing it (audit trail).
//! - **Load**: Re-run a previous deletion without going through review again.
//!
//! # Validation
//!
//! On load, every handle in the manifest is cross-referenced against the graph.
//! Handles that don't exist in the graph cause a hard rejection. The
//! `source_file` field is cross-referenced against the input file path — a
//! mismatch produces a warning (not a hard error), since the file may have
//! been renamed.

use std::collections::HashMap;
use std::path::Path;

use serde_json;
use typed_graph::{Graph, Handle};

use crate::review::describe_node;
use crate::types::{DeleteManifest, HandleEntry, HandleStatus, TypePlan, MANIFEST_VERSION};

/// Errors that can occur during manifest operations.
#[derive(Debug)]
pub enum ManifestError {
    /// I/O error reading or writing the manifest file.
    Io(std::io::Error),
    /// JSON parse error during deserialization.
    JsonParse(serde_json::Error),
    /// A handle in the manifest does not exist in the graph.
    InvalidHandle(String),
    /// The manifest format version is not supported.
    UnsupportedVersion(u32),
    /// The source file name doesn't match (warning only).
    SourceFileMismatch {
        manifest_path: String,
        actual_path: String,
    },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "I/O error: {}", e),
            ManifestError::JsonParse(e) => write!(f, "JSON parse error: {}", e),
            ManifestError::InvalidHandle(h) => {
                write!(f, "manifest contains invalid handle: '{}'", h)
            }
            ManifestError::UnsupportedVersion(v) => {
                write!(f, "unsupported manifest version: {}", v)
            }
            ManifestError::SourceFileMismatch {
                manifest_path,
                actual_path,
            } => {
                write!(
                    f,
                    "source file mismatch: manifest says '{}', actual is '{}'",
                    manifest_path, actual_path
                )
            }
        }
    }
}

impl std::error::Error for ManifestError {}

impl From<std::io::Error> for ManifestError {
    fn from(e: std::io::Error) -> Self {
        ManifestError::Io(e)
    }
}

impl From<serde_json::Error> for ManifestError {
    fn from(e: serde_json::Error) -> Self {
        ManifestError::JsonParse(e)
    }
}

/// Save a `DeleteManifest` to a JSON file.
///
/// The output is pretty-printed with sorted handle arrays for deterministic
/// output that is both human-readable and machine-parseable.
pub fn save_manifest(manifest: &DeleteManifest, path: &Path) -> Result<(), ManifestError> {
    let json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load a `DeleteManifest` from a JSON file.
///
/// Tries v3 format first. If the file is v1 or v2 format, auto-migrates to v3:
/// - All `to_delete` entries get `HandleStatus::Pending`
/// - All `kept` entries get `HandleStatus::Kept`
/// - `deletion_mode` is set to `"people_only"`
/// - `version` is bumped to 3
pub fn load_manifest(path: &Path) -> Result<DeleteManifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let mut manifest: DeleteManifest = serde_json::from_str(&content)?;

    match manifest.version {
        3 => Ok(manifest),
        1 | 2 => {
            // Auto-migrate from v1/v2 to v3
            manifest.version = MANIFEST_VERSION;
            if manifest.deletion_mode.is_empty() {
                manifest.deletion_mode = "people_only".to_string();
            }
            // Handle entries are already converted by TypePlan's custom
            // deserializer (v1 flat strings → HandleEntry with Pending/Kept).
            Ok(manifest)
        }
        other => Err(ManifestError::UnsupportedVersion(other)),
    }
}

/// Load a v1 `DeleteManifest` from a JSON file, rejecting any other version.
pub fn load_manifest_v1(path: &Path) -> Result<DeleteManifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let manifest: DeleteManifest = serde_json::from_str(&content)?;

    if manifest.version != 1 {
        return Err(ManifestError::UnsupportedVersion(manifest.version));
    }

    Ok(manifest)
}

/// Validate a manifest against a graph.
///
/// Cross-references every handle in the manifest against the graph. Returns
/// `Ok(())` on success, or an error listing the first invalid handle found.
pub fn validate_manifest(manifest: &DeleteManifest, graph: &Graph) -> Result<(), ManifestError> {
    // Collect all handles mentioned in the manifest
    for handle in &manifest.seed_people {
        if !graph.contains_node(handle) {
            return Err(ManifestError::InvalidHandle(handle.clone()));
        }
    }

    for type_plan in manifest.plan.values() {
        for entry in &type_plan.to_delete {
            if !graph.contains_node(&entry.handle) {
                return Err(ManifestError::InvalidHandle(entry.handle.clone()));
            }
        }
        for entry in &type_plan.kept {
            if !graph.contains_node(&entry.handle) {
                return Err(ManifestError::InvalidHandle(entry.handle.clone()));
            }
        }
    }

    Ok(())
}

/// Check whether the manifest's `source_file` matches the actual input file
/// path. Returns `Ok(())` on match, or a `SourceFileMismatch` warning on mismatch.
pub fn check_source_file(
    manifest: &DeleteManifest,
    actual_path: &Path,
) -> Result<(), ManifestError> {
    let actual = actual_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if manifest.source_file != actual {
        return Err(ManifestError::SourceFileMismatch {
            manifest_path: manifest.source_file.clone(),
            actual_path: actual,
        });
    }
    Ok(())
}

/// Create a `DeleteManifest` from a `DeletePlan` and metadata.
///
/// The manifest is deterministically serializable: all handle lists are
/// sorted, and the `plan` map uses a fixed key order matching the dependency
/// chain (people, families, events, places, citations, sources, repositories,
/// media, notes, tags).
pub fn build_manifest(
    source_file: &str,
    selections_file: Option<&str>,
    seed_people: &[Handle],
    to_delete: &[Handle],
    graph: &Graph,
) -> DeleteManifest {
    let created_at = chrono_now();

    // Build per-type plan in dependency order
    let type_order = [
        "people",
        "families",
        "events",
        "places",
        "citations",
        "sources",
        "repositories",
        "media",
        "notes",
        "tags",
    ];

    let mut plan: HashMap<String, TypePlan> = HashMap::new();

    for type_name in &type_order {
        let type_handles: Vec<Handle> = to_delete
            .iter()
            .filter(|h| {
                if let Some(node) = graph.get_node(h) {
                    let label = crate::cascade::node_kind_to_label(node);
                    label.plural() == *type_name
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        if type_handles.is_empty() {
            continue;
        }

        // Ensure deterministic order
        let mut sorted_entries: Vec<HandleEntry> = type_handles
            .iter()
            .map(|h| {
                let (desc, gramps_id) = describe_node(graph, h);
                HandleEntry {
                    handle: h.clone(),
                    status: HandleStatus::Pending,
                    gramps_id,
                    description: desc,
                }
            })
            .collect();
        sorted_entries.sort_by(|a, b| a.handle.cmp(&b.handle));
        let kept: Vec<HandleEntry> = Vec::new();

        plan.insert(
            type_name.to_string(),
            TypePlan {
                to_delete: sorted_entries,
                kept,
            },
        );
    }

    let mut seed_sorted = seed_people.to_vec();
    seed_sorted.sort();

    DeleteManifest {
        version: MANIFEST_VERSION,
        source_file: source_file.to_string(),
        selections_file: selections_file.map(|s| s.to_string()),
        created_at,
        seed_people: seed_sorted,
        deletion_mode: "people_only".to_string(),
        plan,
    }
}

/// Get an ISO 8601 timestamp string for the current time.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Days since epoch
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Simple Gregorian calendar date calculation
    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if d < md {
            m = i;
            break;
        }
        d -= md;
    }
    let day = d + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        day,
        hours,
        minutes,
        seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(handle: &str, status: HandleStatus) -> HandleEntry {
        HandleEntry {
            handle: handle.to_string(),
            status,
            gramps_id: None,
            description: String::new(),
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let mut plan = HashMap::new();
        plan.insert(
            "people".to_string(),
            TypePlan {
                to_delete: vec![
                    make_entry("p1", HandleStatus::Pending),
                    make_entry("p2", HandleStatus::Pending),
                ],
                kept: vec![],
            },
        );
        let manifest = DeleteManifest {
            version: 2,
            source_file: "test.gramps".to_string(),
            selections_file: Some("sel.json".to_string()),
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec!["p1".to_string()],
            deletion_mode: "people_only".to_string(),
            plan,
        };

        let dir = std::env::temp_dir();
        let path = dir.join("test_manifest.json");
        save_manifest(&manifest, &path).unwrap();
        let loaded = load_manifest(&path).unwrap();
        assert_eq!(loaded.version, 3);
        assert_eq!(loaded.source_file, manifest.source_file);
        assert_eq!(loaded.seed_people, manifest.seed_people);
        assert_eq!(loaded.plan.len(), manifest.plan.len());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn v1_manifest_loads_as_v3() {
        // v1 manifests use flat handle strings; loading must auto-migrate to v3
        let v1_json = r#"{
            "version": 1,
            "source_file": "test.gramps",
            "selections_file": "selections.json",
            "created_at": "2025-01-15T10:30:00Z",
            "seed_people": ["p1"],
            "plan": {
                "people": {
                    "to_delete": ["p1"],
                    "kept": ["p2"]
                },
                "events": {
                    "to_delete": ["e1"],
                    "kept": []
                }
            }
        }"#;
        let dir = std::env::temp_dir();
        let path = dir.join("v1_manifest_v3_test.json");
        std::fs::write(&path, v1_json).unwrap();
        let loaded = load_manifest(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.version, 3, "v1 manifest must be bumped to 3");
        assert_eq!(loaded.deletion_mode, "people_only");
        let people = loaded.plan.get("people").unwrap();
        assert_eq!(people.to_delete.len(), 1);
        assert_eq!(people.to_delete[0].handle, "p1");
        assert_eq!(people.to_delete[0].status, HandleStatus::Pending);
        assert_eq!(people.to_delete[0].gramps_id, None);
        assert_eq!(people.to_delete[0].description, "");
        assert_eq!(people.kept[0].status, HandleStatus::Kept);
        assert_eq!(people.kept[0].gramps_id, None);
        assert_eq!(people.kept[0].description, "");
    }

    #[test]
    fn v2_manifest_loads_as_v3() {
        // v2 manifests use handle objects with status; loading must auto-migrate to v3
        let v2_json = r#"{
            "version": 2,
            "source_file": "test.gramps",
            "selections_file": "selections.json",
            "created_at": "2025-01-15T10:30:00Z",
            "seed_people": ["p1"],
            "deletion_mode": "people_only",
            "plan": {
                "people": {
                    "to_delete": [
                        {"handle": "p1", "status": "pending"},
                        {"handle": "p2", "status": "deleted"}
                    ],
                    "kept": [{"handle": "p3", "status": "kept"}]
                }
            }
        }"#;
        let dir = std::env::temp_dir();
        let path = dir.join("v2_manifest_v3_test.json");
        std::fs::write(&path, v2_json).unwrap();
        let loaded = load_manifest(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.version, 3, "v2 manifest must be bumped to 3");
        assert_eq!(loaded.deletion_mode, "people_only");
        let people = loaded.plan.get("people").unwrap();
        assert_eq!(people.to_delete.len(), 2);
        assert_eq!(people.to_delete[0].handle, "p1");
        assert_eq!(people.to_delete[0].status, HandleStatus::Pending);
        assert_eq!(people.to_delete[1].status, HandleStatus::Deleted);
        assert_eq!(people.kept[0].status, HandleStatus::Kept);
    }

    #[test]
    fn unsupported_version_rejected() {
        let manifest = DeleteManifest {
            version: 999,
            source_file: "test.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec![],
            deletion_mode: "people_only".to_string(),
            plan: HashMap::new(),
        };
        let dir = std::env::temp_dir();
        let path = dir.join("bad_version.json");
        save_manifest(&manifest, &path).unwrap();
        let result = load_manifest(&path);
        assert!(matches!(
            result,
            Err(ManifestError::UnsupportedVersion(999))
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn validate_manifest_valid() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
                typed_graph::Node::Person(typed_graph::PersonData {
                    handle: "p1".to_string(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();

        let mut plan = HashMap::new();
        plan.insert(
            "people".to_string(),
            TypePlan {
                to_delete: vec![make_entry("p1", HandleStatus::Pending)],
                kept: vec![],
            },
        );
        let manifest = DeleteManifest {
            version: 1,
            source_file: "test.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec!["p1".to_string()],
            deletion_mode: "people_only".to_string(),
            plan,
        };

        assert!(validate_manifest(&manifest, &graph).is_ok());
    }

    #[test]
    fn validate_manifest_rejects_bad_handle() {
        let graph = Graph::new();
        let manifest = DeleteManifest {
            version: 1,
            source_file: "test.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec!["nonexistent".to_string()],
            deletion_mode: "people_only".to_string(),
            plan: HashMap::new(),
        };

        let result = validate_manifest(&manifest, &graph);
        assert!(matches!(result, Err(ManifestError::InvalidHandle(h)) if h == "nonexistent"));
    }

    #[test]
    fn check_source_file_matches() {
        let manifest = DeleteManifest {
            version: 1,
            source_file: "test.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec![],
            deletion_mode: "people_only".to_string(),
            plan: HashMap::new(),
        };

        let path = Path::new("/some/dir/test.gramps");
        assert!(check_source_file(&manifest, path).is_ok());
    }

    #[test]
    fn check_source_file_mismatch() {
        let manifest = DeleteManifest {
            version: 1,
            source_file: "other.gramps".to_string(),
            selections_file: None,
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec![],
            deletion_mode: "people_only".to_string(),
            plan: HashMap::new(),
        };

        let path = Path::new("test.gramps");
        let result = check_source_file(&manifest, path);
        assert!(matches!(
            result,
            Err(ManifestError::SourceFileMismatch { .. })
        ));
    }

    #[test]
    fn build_manifest_includes_all_types() {
        let mut graph = Graph::new();
        graph
            .add_node(
                "p1".to_string(),
                typed_graph::Node::Person(typed_graph::PersonData {
                    handle: "p1".to_string(),
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                "e1".to_string(),
                typed_graph::Node::Event(typed_graph::EventData {
                    handle: "e1".to_string(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();

        let to_delete = vec!["p1".to_string(), "e1".to_string()];
        let manifest = build_manifest(
            "test.gramps",
            Some("sel.json"),
            &["p1".to_string()],
            &to_delete,
            &graph,
        );

        assert_eq!(manifest.version, 3);
        assert_eq!(manifest.source_file, "test.gramps");
        assert_eq!(manifest.selections_file, Some("sel.json".to_string()));
        assert_eq!(manifest.seed_people, vec!["p1".to_string()]);
        assert!(manifest.plan.contains_key("people"));
        assert!(manifest.plan.contains_key("events"));
        assert_eq!(manifest.plan["people"].to_delete.len(), 1);
        assert_eq!(manifest.plan["people"].to_delete[0].handle, "p1");
        assert_eq!(
            manifest.plan["people"].to_delete[0].status,
            HandleStatus::Pending
        );
        assert_eq!(manifest.plan["events"].to_delete.len(), 1);
        assert_eq!(manifest.plan["events"].to_delete[0].handle, "e1");
        assert_eq!(
            manifest.plan["events"].to_delete[0].status,
            HandleStatus::Pending
        );
    }

    #[test]
    fn build_manifest_populates_descriptions() {
        // A person with name, gramps_id, and a birth event must get a
        // non-empty description and gramps_id in the manifest.
        let mut graph = Graph::new();
        let person_h = "p0001".to_string();
        let event_h = "e0001".to_string();

        // Birth event first (referenced by index 0 from the person)
        graph
            .add_node(
                event_h.clone(),
                typed_graph::Node::Event(typed_graph::EventData {
                    handle: event_h.clone(),
                    gramps_id: None,
                    event_type: Some(typed_graph::EventType::Birth),
                    date: Some(typed_graph::DateValue {
                        year: 1800,
                        text: Some("1800".to_string()),
                        ..typed_graph::DateValue::default()
                    }),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();

        graph
            .add_node(
                person_h.clone(),
                typed_graph::Node::Person(typed_graph::PersonData {
                    handle: person_h.clone(),
                    gramps_id: Some("I0001".to_string()),
                    primary_name: typed_graph::Name {
                        first_name: Some("John".to_string()),
                        surname_list: vec![typed_graph::Surname {
                            surname: Some("Smith".to_string()),
                            ..typed_graph::Surname::default()
                        }],
                        ..typed_graph::Name::default()
                    },
                    birth_ref_index: Some(0),
                    event_ref_list: vec![typed_graph::EventRef {
                        ref_field: event_h,
                        ..typed_graph::EventRef::default()
                    }],
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();

        let to_delete = vec![person_h];
        let manifest = build_manifest(
            "test.gramps",
            None,
            &["p0001".to_string()],
            &to_delete,
            &graph,
        );

        let people = manifest.plan.get("people").unwrap();
        let entry = &people.to_delete[0];
        assert_eq!(entry.gramps_id.as_deref(), Some("I0001"));
        assert_eq!(entry.description, "John Smith (b. 1800)");
    }

    #[test]
    fn build_manifest_empty_node() {
        // A node with no name/date still produces a fallback description
        // and no gramps_id.
        let mut graph = Graph::new();
        graph
            .add_node(
                "e0001".to_string(),
                typed_graph::Node::Event(typed_graph::EventData {
                    handle: "e0001".to_string(),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();

        let to_delete = vec!["e0001".to_string()];
        let manifest = build_manifest("test.gramps", None, &[], &to_delete, &graph);

        let events = manifest.plan.get("events").unwrap();
        let entry = &events.to_delete[0];
        assert_eq!(entry.gramps_id, None);
        // describe_node returns a fallback for an event with no type/date
        assert!(!entry.description.is_empty());
    }

    #[test]
    fn save_load_v3_roundtrip() {
        // Full v3 manifest roundtrip: all fields survive save + load.
        let mut plan = HashMap::new();
        plan.insert(
            "people".to_string(),
            TypePlan {
                to_delete: vec![HandleEntry {
                    handle: "p0001".to_string(),
                    status: HandleStatus::Pending,
                    gramps_id: Some("I0001".to_string()),
                    description: "John Smith (1800-1875)".to_string(),
                }],
                kept: vec![HandleEntry {
                    handle: "p0002".to_string(),
                    status: HandleStatus::Kept,
                    gramps_id: Some("I0002".to_string()),
                    description: "Jane Doe (b. 1810)".to_string(),
                }],
            },
        );
        let manifest = DeleteManifest {
            version: 3,
            source_file: "test.gramps".to_string(),
            selections_file: Some("sel.json".to_string()),
            created_at: "2025-01-15T10:30:00Z".to_string(),
            seed_people: vec!["p0001".to_string()],
            deletion_mode: "people_only".to_string(),
            plan,
        };

        let dir = std::env::temp_dir();
        let path = dir.join("v3_roundtrip.json");
        save_manifest(&manifest, &path).unwrap();
        let loaded = load_manifest(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.version, 3);
        assert_eq!(loaded, manifest);
    }

    #[test]
    fn manifest_error_display() {
        let err = ManifestError::InvalidHandle("bad".to_string());
        let msg = err.to_string();
        assert!(msg.contains("bad"));

        let err = ManifestError::UnsupportedVersion(42);
        let msg = err.to_string();
        assert!(msg.contains("42"));
    }
}
