//! Manifest data-contract tests.
//!
//! Serialize and deserialize the shared manifest fixture to pin the JSON
//! shape Rust emits and Python consumes. A format change on either side
//! fails this test loudly.
//!
//! See the `data-contracts` skill and
//! `docs/research/gramps-python-delete-backend.md`.

use delete::manifest::load_manifest;
use delete::types::{DeleteManifest, HandleStatus, TypePlan};
use std::collections::HashMap;

/// The canonical fixture both Rust and Python tests consume.
const CONTRACT_FIXTURE: &str = include_str!("fixtures/manifest-contract-v1.json");

/// The canonical v2 fixture with status-tracked entries.
const CONTRACT_FIXTURE_V2: &str = include_str!("fixtures/manifest-contract-v2.json");

#[test]
fn contract_v1_deserializes_to_delete_manifest() {
    let manifest: DeleteManifest =
        serde_json::from_str(CONTRACT_FIXTURE).expect("contract fixture must deserialize");

    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.source_file, "test.gramps");
    assert_eq!(
        manifest.selections_file,
        Some("selections.json".to_string())
    );
    assert_eq!(
        manifest.seed_people,
        vec!["a5f0c1a2-4000-4b3d-8000-000000000001"]
    );
    assert_eq!(manifest.plan.len(), 10, "all 10 types must be present");
}

#[test]
fn contract_v1_has_correct_type_keys() {
    let manifest: DeleteManifest =
        serde_json::from_str(CONTRACT_FIXTURE).expect("contract fixture must deserialize");

    let expected_keys: Vec<&str> = vec![
        "people",
        "families",
        "events",
        "notes",
        "places",
        "sources",
        "citations",
        "repositories",
        "media",
        "tags",
    ];

    for key in &expected_keys {
        assert!(
            manifest.plan.contains_key(*key),
            "missing expected key: {key}"
        );
    }
}

#[test]
fn contract_v1_roundtrip_is_stable() {
    // Round-trip: deserialize → serialize → deserialize again → equal.
    let manifest1: DeleteManifest =
        serde_json::from_str(CONTRACT_FIXTURE).expect("contract fixture must deserialize");

    let json = serde_json::to_string_pretty(&manifest1).unwrap();
    let manifest2: DeleteManifest =
        serde_json::from_str(&json).expect("serialized must deserialize back");

    assert_eq!(manifest1, manifest2);
}

#[test]
fn contract_v1_people_has_to_delete_and_kept() {
    let manifest: DeleteManifest =
        serde_json::from_str(CONTRACT_FIXTURE).expect("contract fixture must deserialize");

    let people = manifest.plan.get("people").expect("people must be present");
    assert_eq!(people.to_delete.len(), 1);
    assert_eq!(
        people.to_delete[0].handle,
        "a5f0c1a2-4000-4b3d-8000-000000000001"
    );
    assert_eq!(
        people.to_delete[0].status,
        delete::types::HandleStatus::Pending
    );
    assert_eq!(people.kept.len(), 1);
    assert_eq!(
        people.kept[0].handle,
        "a5f0c1a2-4000-4b3d-8000-000000000002"
    );
    assert_eq!(people.kept[0].status, delete::types::HandleStatus::Kept);
}

#[test]
fn contract_v1_serialize_from_rust_matches_fixture_shape() {
    // Build the same manifest in Rust, serialize it, and verify the JSON
    // has the same top-level keys.
    let mut plan = HashMap::new();
    for key in &[
        "people",
        "families",
        "events",
        "notes",
        "places",
        "sources",
        "citations",
        "repositories",
        "media",
        "tags",
    ] {
        plan.insert(
            key.to_string(),
            TypePlan {
                to_delete: vec![],
                kept: vec![],
            },
        );
    }

    let manifest = DeleteManifest {
        version: 1,
        source_file: "test.gramps".to_string(),
        selections_file: Some("selections.json".to_string()),
        created_at: "2025-01-15T10:30:00Z".to_string(),
        seed_people: vec!["a5f0c1a2-4000-4b3d-8000-000000000001".to_string()],
        deletion_mode: "people_only".to_string(),
        plan,
    };

    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("serialized must be valid JSON");

    // Assert top-level keys
    assert!(parsed["version"].is_number());
    assert!(parsed["source_file"].is_string());
    assert!(parsed["selections_file"].is_string());
    assert!(parsed["created_at"].is_string());
    assert!(parsed["seed_people"].is_array());
    assert!(parsed["deletion_mode"].is_string());
    assert!(parsed["plan"].is_object());

    // Assert plan keys are snake_case
    let plan_obj = parsed["plan"].as_object().expect("plan must be object");
    for key in plan_obj.keys() {
        assert!(
            key.chars().all(|c| c.is_lowercase() || c == '_'),
            "plan key '{key}' must be snake_case"
        );
    }
}

// ---------------------------------------------------------------------------
// v2 contract tests
// ---------------------------------------------------------------------------

#[test]
fn contract_v2_deserializes_to_delete_manifest() {
    let manifest: DeleteManifest =
        serde_json::from_str(CONTRACT_FIXTURE_V2).expect("v2 contract fixture must deserialize");

    assert_eq!(manifest.version, 2);
    assert_eq!(manifest.source_file, "test.gramps");
    assert_eq!(
        manifest.selections_file,
        Some("selections.json".to_string())
    );
    assert_eq!(
        manifest.seed_people,
        vec!["a5f0c1a2-4000-4b3d-8000-000000000001"]
    );
    assert_eq!(manifest.deletion_mode, "people_only");
    assert_eq!(manifest.plan.len(), 10, "all 10 types must be present");
}

#[test]
fn contract_v2_roundtrip_is_stable() {
    let manifest1: DeleteManifest =
        serde_json::from_str(CONTRACT_FIXTURE_V2).expect("v2 fixture must deserialize");

    let json = serde_json::to_string_pretty(&manifest1).unwrap();
    let manifest2: DeleteManifest =
        serde_json::from_str(&json).expect("serialized v2 must deserialize back");

    assert_eq!(manifest1, manifest2);
}

#[test]
fn contract_v2_people_entries_have_statuses() {
    let manifest: DeleteManifest =
        serde_json::from_str(CONTRACT_FIXTURE_V2).expect("v2 fixture must deserialize");

    let people = manifest.plan.get("people").expect("people must be present");
    assert_eq!(people.to_delete.len(), 2);
    assert_eq!(
        people.to_delete[0].handle,
        "a5f0c1a2-4000-4b3d-8000-000000000001"
    );
    assert_eq!(people.to_delete[0].status, HandleStatus::Pending);
    assert_eq!(
        people.to_delete[1].handle,
        "a5f0c1a2-4000-4b3d-8000-000000000002"
    );
    assert_eq!(people.to_delete[1].status, HandleStatus::Deleted);
    assert_eq!(people.kept.len(), 1);
    assert_eq!(
        people.kept[0].handle,
        "a5f0c1a2-4000-4b3d-8000-000000000003"
    );
    assert_eq!(people.kept[0].status, HandleStatus::Kept);
}

#[test]
fn contract_v1_fixture_load_migrates_to_v2() {
    // Write the v1 fixture to a temp file and load it through load_manifest
    // (which auto-migrates v1 → v2).
    let dir = std::env::temp_dir();
    let path = dir.join("contract-v1-load-test.json");
    std::fs::write(&path, CONTRACT_FIXTURE).expect("write v1 fixture");

    let manifest = load_manifest(&path).expect("v1 fixture must load and migrate");
    let _ = std::fs::remove_file(&path);

    assert_eq!(manifest.version, 2, "version must be bumped to 2");
    assert_eq!(manifest.deletion_mode, "people_only");

    let people = manifest.plan.get("people").expect("people must be present");
    assert_eq!(people.to_delete.len(), 1);
    assert_eq!(people.to_delete[0].status, HandleStatus::Pending);
    assert_eq!(people.kept[0].status, HandleStatus::Kept);
}

#[test]
fn contract_v2_serialize_from_rust_matches_fixture_shape() {
    // Build a v2 manifest in Rust, serialize it, and verify entries are
    // objects with handle + status keys (not bare strings).
    let mut plan = HashMap::new();
    plan.insert(
        "people".to_string(),
        TypePlan {
            to_delete: vec![delete::types::HandleEntry {
                handle: "a5f0c1a2-4000-4b3d-8000-000000000001".to_string(),
                status: HandleStatus::Pending,
            }],
            kept: vec![],
        },
    );

    let manifest = DeleteManifest {
        version: 2,
        source_file: "test.gramps".to_string(),
        selections_file: Some("selections.json".to_string()),
        created_at: "2025-01-15T10:30:00Z".to_string(),
        seed_people: vec!["a5f0c1a2-4000-4b3d-8000-000000000001".to_string()],
        deletion_mode: "people_only".to_string(),
        plan,
    };

    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("serialized v2 must be valid JSON");

    assert_eq!(parsed["version"], 2);
    assert_eq!(parsed["deletion_mode"], "people_only");

    // to_delete entries must be objects with handle + status
    let to_delete = &parsed["plan"]["people"]["to_delete"];
    assert!(to_delete.is_array());
    let first = to_delete[0].as_object().expect("entry must be an object");
    assert!(
        first.contains_key("handle"),
        "v2 entries must have a handle key"
    );
    assert!(
        first.contains_key("status"),
        "v2 entries must have a status key"
    );
}
