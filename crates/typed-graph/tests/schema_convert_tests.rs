//! Integration tests for schema_convert module.
//!
//! These tests include the module via `#[path]` to verify the conversion
//! logic works correctly in the build.rs context, where the module is
//! also included via `#[path]`.

#[path = "../src/schema_convert.rs"]
mod schema_convert;

use serde_json::json;

#[test]
fn test_build_path_detection() {
    // Verify that the schema_convert module can detect JSON Schema format
    // when included via #[path] (simulating the build.rs context)
    let json_schema = json!({
        "primary_types": {
            "Person": {
                "fields": {
                    "type": "object",
                    "title": "Person",
                    "properties": {
                        "handle": {"type": "string", "maxLength": 50}
                    }
                }
            }
        }
    });
    assert!(schema_convert::is_json_schema_format(&json_schema));

    let flat_schema = json!({
        "primary_types": {
            "Person": {
                "fields": {
                    "handle": {"type": "string", "kind": "handle", "required": true}
                }
            }
        }
    });
    assert!(!schema_convert::is_json_schema_format(&flat_schema));
}

#[test]
fn test_build_path_conversion_roundtrip() {
    // Simulate the full conversion pipeline that build.rs will use
    let schema = json!({
        "version": "5.1",
        "primary_types": {
            "Person": {
                "fields": {
                    "type": "object",
                    "title": "Person",
                    "properties": {
                        "handle": {"type": "string", "maxLength": 50, "title": "Handle"},
                        "gramps_id": {"type": "string", "title": "Gramps ID"},
                        "gender": {"type": "integer", "minimum": 0, "maximum": 2, "title": "Gender"},
                        "primary_name": {
                            "type": "object",
                            "title": "Name",
                            "properties": {
                                "_class": {"enum": ["Name"]},
                                "first_name": {"type": "string", "title": "Given name"},
                                "surname_list": {
                                    "type": "array",
                                    "items": {"type": "object", "title": "Surname"},
                                    "title": "Surnames"
                                }
                            }
                        },
                        "family_list": {
                            "type": "array",
                            "items": {"type": "string", "maxLength": 50},
                            "title": "Families"
                        },
                        "event_ref_list": {
                            "type": "array",
                            "items": {"type": "object", "title": "EventRef"},
                            "title": "Events"
                        },
                        "birth_ref_index": {"type": "integer", "title": "Birth reference index"},
                        "death_ref_index": {"type": "integer", "title": "Death reference index"},
                        "citation_list": {
                            "type": "array",
                            "items": {"type": "string", "maxLength": 50},
                            "title": "Citations"
                        },
                        "note_list": {
                            "type": "array",
                            "items": {"type": "string", "maxLength": 50},
                            "title": "Notes"
                        }
                    }
                },
                "inherit_mixins": ["CitationBase", "NoteBase", "MediaBase", "AttributeBase", "AddressBase", "UrlBase", "LdsOrdBase", "TagBase"]
            }
        },
        "secondary_types": {
            "Name": {
                "fields": {
                    "type": "object",
                    "title": "Name",
                    "properties": {
                        "_class": {"enum": ["Name"]},
                        "first_name": {"type": "string", "title": "Given name"},
                        "surname_list": {
                            "type": "array",
                            "items": {"type": "object", "title": "Surname"},
                            "title": "Surnames"
                        }
                    }
                }
            }
        },
        "enum_types": {
            "EventType": {"values": [0, 1, 2]}
        }
    });

    let enum_constants = json!({
        "EventType": {
            "0": "POS_VALUE",
            "1": "POS_STRING",
            "2": "MARR_SETTL"
        }
    });

    let result = schema_convert::convert(schema, "5.1", &enum_constants)
        .expect("schema conversion should succeed");

    // After conversion, it should NOT be in JSON Schema format
    assert!(!schema_convert::is_json_schema_format(&result));

    // Validate the flat format
    assert!(
        schema_convert::validate_flat_format(&result).is_ok(),
        "converted schema should pass flat-format validation"
    );

    // Verify key fields were converted correctly
    let person = result
        .get("primary_types")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get("Person"))
        .and_then(|v| v.as_object())
        .expect("Person should exist");

    let fields = person
        .get("fields")
        .and_then(|v| v.as_object())
        .expect("Person should have fields");

    // handle should be kind: handle
    let handle = fields.get("handle").expect("handle field should exist");
    assert_eq!(handle.get("kind").and_then(|v| v.as_str()), Some("handle"));

    // gender should be enum_ref
    let gender = fields.get("gender").expect("gender field should exist");
    assert_eq!(
        gender.get("type").and_then(|v| v.as_str()),
        Some("enum_ref")
    );

    // primary_name should be embedded Name
    let name = fields
        .get("primary_name")
        .expect("primary_name field should exist");
    assert_eq!(name.get("type").and_then(|v| v.as_str()), Some("embedded"));
    assert_eq!(name.get("schema").and_then(|v| v.as_str()), Some("Name"));

    // family_list should be array of handle_ref
    let family = fields
        .get("family_list")
        .expect("family_list field should exist");
    assert_eq!(family.get("type").and_then(|v| v.as_str()), Some("array"));
    let items = family.get("items").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        items.get("kind").and_then(|v| v.as_str()),
        Some("handle_ref")
    );

    // event_ref_list should be array of embedded refs
    let events = fields
        .get("event_ref_list")
        .expect("event_ref_list field should exist");
    assert_eq!(events.get("type").and_then(|v| v.as_str()), Some("array"));
    let event_items = events.get("items").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        event_items.get("embedded").and_then(|v| v.as_str()),
        Some("EventRef")
    );

    // birth_ref_index and death_ref_index should be preserved
    let birth = fields
        .get("birth_ref_index")
        .expect("birth_ref_index should exist");
    assert_eq!(birth.get("type").and_then(|v| v.as_str()), Some("integer"));

    // _class should be stripped
    assert!(!fields.contains_key("_class"), "_class should be stripped");

    // Enum values should be converted to strings
    let enum_types = result
        .get("enum_types")
        .and_then(|v| v.as_object())
        .unwrap();
    let event_type = enum_types.get("EventType").unwrap();
    let values = event_type.get("values").and_then(|v| v.as_array()).unwrap();
    let strings: Vec<&str> = values.iter().filter_map(|v| v.as_str()).collect();
    assert!(strings.contains(&"POS_VALUE"));
    assert!(strings.contains(&"POS_STRING"));
    assert!(strings.contains(&"MARR_SETTL"));

    // DateValue should be present in secondary_types
    let secondary = result
        .get("secondary_types")
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(
        secondary.contains_key("DateValue"),
        "DateValue should be present"
    );
}

#[test]
fn test_convert_with_date_field() {
    // Test that Date fields with oneOf [null, object] are converted correctly
    let schema = json!({
        "version": "5.1",
        "primary_types": {
            "Citation": {
                "fields": {
                    "type": "object",
                    "title": "Citation",
                    "properties": {
                        "handle": {"type": "string", "maxLength": 50, "title": "Handle"},
                        "date": {
                            "oneOf": [
                                {"type": "null"},
                                {
                                    "type": "object",
                                    "title": "Date",
                                    "properties": {
                                        "dateval": {"type": "array"},
                                        "modifier": {"type": "integer"},
                                        "quality": {"type": "integer"},
                                        "text": {"type": "string"}
                                    }
                                }
                            ],
                            "title": "Date"
                        }
                    }
                },
                "inherit_mixins": []
            }
        },
        "secondary_types": {},
        "enum_types": {}
    });

    let result =
        schema_convert::convert(schema, "5.1", &json!(null)).expect("conversion should succeed");

    let citation = result
        .get("primary_types")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get("Citation"))
        .and_then(|v| v.as_object())
        .expect("Citation should exist");

    let fields = citation.get("fields").and_then(|v| v.as_object()).unwrap();
    let date = fields.get("date").expect("date field should exist");
    assert_eq!(date.get("type").and_then(|v| v.as_str()), Some("embedded"));
    assert_eq!(
        date.get("schema").and_then(|v| v.as_str()),
        Some("DateValue")
    );
    assert_eq!(date.get("required").and_then(|v| v.as_bool()), Some(false));
}

#[test]
fn test_validate_flat_format_ok() {
    let schema = json!({
        "primary_types": {
            "Person": {
                "fields": {
                    "handle": {"type": "string", "kind": "handle", "required": true},
                    "family_list": {
                        "type": "array",
                        "items": {"kind": "handle_ref", "target": "Family"},
                        "required": false
                    },
                    "primary_name": {
                        "type": "embedded",
                        "schema": "Name",
                        "required": true
                    }
                }
            }
        }
    });
    assert!(schema_convert::validate_flat_format(&schema).is_ok());
}
