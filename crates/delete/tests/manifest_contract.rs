//! Manifest data-contract tests.
//!
//! Serialize and deserialize the shared manifest fixture to pin the JSON
//! shape Rust emits and Python consumes. A format change on either side
//! fails this test loudly.
//!
//! See the `data-contracts` skill and
//! `docs/research/gramps-python-delete-backend.md`.

use delete::types::{DeleteManifest, TypePlan};
use std::collections::HashMap;

/// The canonical fixture both Rust and Python tests consume.
const CONTRACT_FIXTURE: &str = include_str!("fixtures/manifest-contract-v1.json");

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

    let people = manifest
        .plan
        .get("people")
        .expect("people must be present");
    assert_eq!(people.to_delete.len(), 1);
    assert_eq!(
        people.to_delete[0].handle,
        "a5f0c1a2-4000-4b3d-8000-000000000001"
    );
    assert_eq!(people.to_delete[0].status, delete::types::HandleStatus::Pending);
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
