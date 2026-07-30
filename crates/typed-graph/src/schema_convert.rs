//! Schema format conversion — JSON Schema format → custom flat format.
//!
//! This module converts Gramps schema files that are in JSON Schema format
//! (the native output of `cls.get_schema()` in Gramps 5.1) to the custom
//! flat format used by `schema-5.2.json`.
//!
//! # Public API
//!
//! - `is_json_schema_format(schema) -> bool` — detect whether a schema is
//!   in JSON Schema format by checking for `"properties"` inside `fields`.
//! - `convert(schema, version, enum_constants) -> Result<Value, String>` —
//!   convert a JSON Schema format schema to the custom flat format.
//! - `validate_flat_format(schema) -> Result<(), Vec<String>>` — validate
//!   the structural correctness of a flat-format schema.

use serde_json::{Map, Value};

/// Detect whether a schema is in JSON Schema format.
///
/// The heuristic: if any primary or secondary type has a `fields` object
/// containing a `"properties"` key (the JSON Schema hallmark), it's JSON
/// Schema format. If all types lack `properties`, it's custom flat format.
pub fn is_json_schema_format(schema: &Value) -> bool {
    // Check primary types
    if let Some(primary) = schema.get("primary_types").and_then(|v| v.as_object()) {
        for info in primary.values() {
            if let Some(fields) = info.get("fields").and_then(|v| v.as_object()) {
                if fields.contains_key("properties") {
                    return true;
                }
            }
        }
    }

    // Check secondary types
    if let Some(secondary) = schema.get("secondary_types").and_then(|v| v.as_object()) {
        for info in secondary.values() {
            if let Some(fields) = info.get("fields").and_then(|v| v.as_object()) {
                if fields.contains_key("properties") {
                    return true;
                }
            }
        }
    }

    false
}

/// Convert a schema from JSON Schema format to the custom flat format.
///
/// The `enum_constants` parameter is a parsed `enum_constants_5_1.json`
/// file mapping enum type names to `{integer_string: name}` pairs.
pub fn convert(
    schema: Value,
    _version: &str,
    enum_constants: &Value,
) -> Result<Value, String> {
    let mut result = Map::new();

    // Copy top-level metadata (version, etc.)
    if let Some(version_val) = schema.get("version") {
        result.insert("version".to_string(), version_val.clone());
    }

    // Convert primary_types
    if let Some(primary_types) = schema.get("primary_types").and_then(|v| v.as_object()) {
        let mut converted_primary = Map::new();
        for (type_name, type_info) in primary_types {
            match convert_type_fields(type_info, enum_constants) {
                Ok(converted) => {
                    converted_primary.insert(type_name.clone(), converted);
                }
                Err(e) => return Err(format!("primary_type {}: {}", type_name, e)),
            }
        }
        result.insert("primary_types".to_string(), Value::Object(converted_primary));
    } else {
        result.insert(
            "primary_types".to_string(),
            Value::Object(Map::new()),
        );
    }

    // Convert secondary_types
    if let Some(secondary_types) = schema.get("secondary_types").and_then(|v| v.as_object()) {
        let mut converted_secondary = Map::new();
        for (type_name, type_info) in secondary_types {
            // Skip Tag if it appears as a secondary type (it's a primary type)
            if type_name == "Tag" {
                eprintln!("cargo::warning=Tag secondary type skipped (it is a primary type)");
                continue;
            }
            match convert_type_fields(type_info, enum_constants) {
                Ok(converted) => {
                    converted_secondary.insert(type_name.clone(), converted);
                }
                Err(e) => return Err(format!("secondary_type {}: {}", type_name, e)),
            }
        }
        result.insert(
            "secondary_types".to_string(),
            Value::Object(converted_secondary),
        );
    } else {
        result.insert(
            "secondary_types".to_string(),
            Value::Object(Map::new()),
        );
    }

    // Convert enum_types: map integer values → string names
    if let Some(enum_types) = schema.get("enum_types").and_then(|v| v.as_object()) {
        let mut converted_enums = Map::new();
        for (enum_name, enum_info) in enum_types {
            let lookup_table = enum_constants.get(enum_name);

            let converted = convert_enum_type(enum_info, enum_name, lookup_table);
            converted_enums.insert(enum_name.clone(), converted);
        }
        result.insert("enum_types".to_string(), Value::Object(converted_enums));
    } else {
        result.insert("enum_types".to_string(), Value::Object(Map::new()));
    }

    // Add synthetic secondary types that are defined inline in the JSON Schema
    // format but need to exist as proper secondary types in the flat format.
    if let Some(st) = result
        .get_mut("secondary_types")
        .and_then(|v| v.as_object_mut())
    {
        if !st.contains_key("DateValue") {
            st.insert("DateValue".to_string(), build_date_value_secondary_type());
        }
        if !st.contains_key("PlaceName") {
            st.insert("PlaceName".to_string(), build_place_name_secondary_type());
        }
        if !st.contains_key("StyledText") {
            st.insert(
                "StyledText".to_string(),
                build_styled_text_secondary_type(),
            );
        }
    }

    Ok(Value::Object(result))
}

/// Convert fields and mixins for a single type from JSON Schema → flat format.
fn convert_type_fields(
    type_info: &Value,
    enum_constants: &Value,
) -> Result<Value, String> {
    let mut result = Map::new();
    let mut converted_fields = Map::new();

    // Extract the `properties` object from the JSON Schema format
    let properties = type_info
        .get("fields")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get("properties"))
        .and_then(|v| v.as_object());

    let properties = match properties {
        Some(p) => p,
        None => {
            // Not JSON Schema format or no fields; return as-is
            return Ok(type_info.clone());
        }
    };

    // Track seen field names to avoid duplicates from normalization
    let mut seen_fields: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (field_name, field_schema) in properties {
        // Skip internal Gramps bookkeeping fields
        if field_name == "_class" || field_name == "private" || field_name == "change" {
            continue;
        }

        // Normalize field name: urls → url_list
        let normalized_name = if field_name == "urls" {
            "url_list".to_string()
        } else {
            field_name.clone()
        };

        // Skip duplicates (e.g., after normalization)
        if !seen_fields.insert(normalized_name.clone()) {
            continue;
        }

        let flat_field = convert_field(field_schema, field_name, enum_constants);
        converted_fields.insert(normalized_name, flat_field);
    }

    result.insert("fields".to_string(), Value::Object(converted_fields));

    // Preserve inherit_mixins
    if let Some(mixins) = type_info.get("inherit_mixins") {
        result.insert("inherit_mixins".to_string(), mixins.clone());
    }

    Ok(Value::Object(result))
}

/// Convert a single field from JSON Schema format to flat format.
fn convert_field(
    field_schema: &Value,
    field_name: &str,
    enum_constants: &Value,
) -> Value {
    let mut flat = Map::new();

    // Map JSON Schema type
    let js_type = field_schema
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("string");

    // Handle oneOf [null, ...] pattern for optional fields
    if let Some(one_of) = field_schema.get("oneOf").and_then(|v| v.as_array()) {
        // Find the non-null type in oneOf
        let non_null = one_of.iter().find(|alt| {
            alt.get("type")
                .and_then(|v| v.as_str())
                .map(|t| t != "null")
                .unwrap_or(true)
        });
        if let Some(actual_schema) = non_null {
            // Recursively convert the non-null alternative
            let mut inner = convert_field(actual_schema, field_name, enum_constants);
            // Replace the required field (default true) with false for optional
            if let Some(obj) = inner.as_object_mut() { obj.insert("required".to_string(), Value::Bool(false)); }
            return inner;
        }
    }

    match js_type {
        "string" => {
            flat.insert("type".to_string(), Value::String("string".to_string()));

            // Determine kind (handle, handle_ref, or plain string)
            if field_name == "handle" {
                flat.insert("kind".to_string(), Value::String("handle".to_string()));
                flat.insert("required".to_string(), Value::Bool(true));
            } else if field_name.ends_with("_handle") {
                let target = resolve_handle_target(field_name);
                if let Some(t) = target {
                    flat.insert(
                        "kind".to_string(),
                        Value::String("handle_ref".to_string()),
                    );
                    flat.insert("target".to_string(), Value::String(t));
                }
                // Handle ref fields are optional by default (father_handle, mother_handle, etc.)
                flat.insert("required".to_string(), Value::Bool(false));
            } else {
                // Plain string — required unless it's gramps_id
                let is_required = field_name != "gramps_id"
                    && !field_name.ends_with("_handle")
                    && !field_name.ends_with("_ref");
                flat.insert("required".to_string(), Value::Bool(is_required));
            }
        }
        "integer" => {
            flat.insert("type".to_string(), Value::String("integer".to_string()));

            // Detect enum_ref fields by name (modifier, quality, gender, type)
            if let Some(enum_ref_target) = detect_enum_ref(field_name) {
                flat.insert(
                    "type".to_string(),
                    Value::String("enum_ref".to_string()),
                );
                flat.insert(
                    "target".to_string(),
                    Value::String(enum_ref_target),
                );
            }

            // Integer fields are optional unless they're handle refs (never required)
            flat.insert("required".to_string(), Value::Bool(false));
        }
        "boolean" => {
            flat.insert("type".to_string(), Value::String("boolean".to_string()));
            flat.insert("required".to_string(), Value::Bool(false));
        }
        "array" => {
            flat.insert("type".to_string(), Value::String("array".to_string()));
            flat.insert("required".to_string(), Value::Bool(false));
            flat.insert(
                "cardinality".to_string(),
                Value::Object(
                    vec![
                        ("min".to_string(), Value::Number(serde_json::Number::from(0))),
                        ("max".to_string(), Value::Null),
                    ]
                    .into_iter()
                    .collect(),
                ),
            );

            // Convert items
            if let Some(items) = field_schema.get("items") {
                let converted_items = convert_array_items(items, field_name, enum_constants);
                flat.insert("items".to_string(), converted_items);
            }
        }
        "object" => {
            // Object fields are embedded types (or Date, or Type enum references)
            let title = field_schema
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if title == "Date" {
                // Convert Date object → DateValue embedded reference
                flat.insert(
                    "type".to_string(),
                    Value::String("embedded".to_string()),
                );
                flat.insert(
                    "schema".to_string(),
                    Value::String("DateValue".to_string()),
                );
                flat.insert("required".to_string(), Value::Bool(false));
            } else if title == "Type" {
                // Fields like frel/mrel with title "Type" are actually enum refs
                // to ChildRefType. Detect via _class.enum in properties.
                let enum_target = field_schema
                    .get("properties")
                    .and_then(|p| p.get("_class"))
                    .and_then(|c| c.get("enum"))
                    .and_then(|e| e.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .map(|class_name| {
                        // Map class name to enum type name
                        match class_name {
                            "ChildRefType" => "ChildRefType",
                            "EventRoleType" => "EventRoleType",
                            "AttributeType" => "AttributeType",
                            "NameType" => "NameType",
                            "NameOriginType" => "NameOriginType",
                            "NoteType" => "NoteType",
                            _ => class_name,
                        }
                    });
                if let Some(target) = enum_target {
                    flat.insert(
                        "type".to_string(),
                        Value::String("enum_ref".to_string()),
                    );
                    flat.insert("target".to_string(), Value::String(target.to_string()));
                    flat.insert("required".to_string(), Value::Bool(true));
                } else {
                    flat.insert(
                        "type".to_string(),
                        Value::String("embedded".to_string()),
                    );
                    flat.insert(
                        "schema".to_string(),
                        Value::String(normalize_type_title(title)),
                    );
                    flat.insert("required".to_string(), Value::Bool(true));
                }
            } else {
                // Use normalized title for the schema name
                flat.insert(
                    "type".to_string(),
                    Value::String("embedded".to_string()),
                );
                flat.insert(
                    "schema".to_string(),
                    Value::String(normalize_type_title(title)),
                );
                // Embedded objects are required unless they have oneOf null wrapper
                // (handled above in oneOf branch)
                flat.insert("required".to_string(), Value::Bool(true));
            }
        }
        _ => {
            flat.insert("type".to_string(), Value::String("string".to_string()));
            flat.insert("required".to_string(), Value::Bool(false));
        }
    }

    Value::Object(flat)
}

/// Resolve the target primary type for a `_handle` field.
fn resolve_handle_target(field_name: &str) -> Option<String> {
    let stem = field_name.strip_suffix("_handle")?;
    match stem {
        "father" | "mother" => Some("Person".to_string()),
        "place" => Some("Place".to_string()),
        "source" => Some("Source".to_string()),
        "repository" | "repo" => Some("Repository".to_string()),
        "media" | "object" => Some("Media".to_string()),
        "family" => Some("Family".to_string()),
        "event" => Some("Event".to_string()),
        "citation" => Some("Citation".to_string()),
        "note" => Some("Note".to_string()),
        "tag" => Some("Tag".to_string()),
        "person" => Some("Person".to_string()),
        "child" => Some("Person".to_string()),
        _ => None,
    }
}

/// Detect whether a field name suggests an enum_ref, and return the target
/// enum type name.
fn detect_enum_ref(field_name: &str) -> Option<String> {
    match field_name {
        "modifier" => Some("DateModifier".to_string()),
        "quality" => Some("DateQuality".to_string()),
        "gender" => Some("Gender".to_string()),
        "type" | "type_field" => {
            // Type is ambiguous; return None and let the caller handle it
            None
        }
        _ => None,
    }
}

/// Normalize a type title from JSON Schema format to the proper PascalCase
/// type name used in the custom flat format.
///
/// JSON Schema titles often contain spaces (e.g., "Place Name", "Event reference")
/// while the flat format uses compact PascalCase (e.g., "PlaceName", "EventRef").
fn normalize_type_title(title: &str) -> String {
    match title {
        "Event reference" | "Event Reference" => "EventRef".to_string(),
        "Child Reference" => "ChildRef".to_string(),
        "Person ref" | "Person Ref" => "PersonRef".to_string(),
        "Place ref" | "Place Ref" => "PlaceRef".to_string(),
        "Media ref" | "Media Ref" => "MediaRef".to_string(),
        "Repository ref" | "Repository Ref" => "RepoRef".to_string(),
        "Source ref" | "Source Ref" => "SourceRef".to_string(),
        "LDS Ordinance" => "LdsOrd".to_string(),
        "Place Name" => "PlaceName".to_string(),
        "Styled Text" => "StyledText".to_string(),
        _ => {
            // General normalization: strip separators (spaces, hyphens, underscores)
            // and capitalize each part
            if title.contains(|c: char| c.is_whitespace() || c == '-' || c == '_') {
                title
                    .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
                    .filter(|s| !s.is_empty())
                    .map(|part| {
                        let mut chars = part.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(f) => {
                                f.to_uppercase().to_string() + chars.as_str()
                            }
                        }
                    })
                    .collect()
            } else {
                title.to_string()
            }
        }
    }
}

/// Convert array `items` from JSON Schema format to flat format.
fn convert_array_items(
    items: &Value,
    field_name: &str,
    _enum_constants: &Value,
) -> Value {
    let mut result = Map::new();

    if let Some(items_obj) = items.as_object() {
        let item_type = items_obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("string");

        match item_type {
            "string" => {
                // Array of strings with maxLength: 50 → handle_ref list
                let target = resolve_list_target(field_name);
                if let Some(t) = target {
                    result.insert(
                        "kind".to_string(),
                        Value::String("handle_ref".to_string()),
                    );
                    result.insert("target".to_string(), Value::String(t));
                } else {
                    result.insert("kind".to_string(), Value::String("handle_ref".to_string()));
                    result.insert("target".to_string(), Value::String("Unknown".to_string()));
                }
            }
            "object" => {
                let title = items_obj
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Normalize the title to a proper type name
                let normalized_title = normalize_type_title(title);

                // Check if this is a Ref type (embedded ref with edge metadata)
                if normalized_title.ends_with("Ref") && !normalized_title.ends_with("AttributeRef")
                {
                    result.insert(
                        "embedded".to_string(),
                        Value::String(normalized_title.clone()),
                    );

                    // Add edges
                    let edges = build_edges_for_ref(&normalized_title, field_name);
                    result.insert("edges".to_string(), Value::Array(edges));
                } else {
                    // Plain embedded type (e.g., Attribute, Address, Url, LdsOrd, PlaceName)
                    result.insert("embedded".to_string(), Value::String(normalized_title));
                    // These don't have edge metadata (they are embedded values)
                    result.insert("edges".to_string(), Value::Array(vec![]));
                }
            }
            _ => {
                result.insert("type".to_string(), Value::String(item_type.to_string()));
            }
        }
    } else if let Some(item_str) = items.as_str() {
        result.insert("embedded".to_string(), Value::String(item_str.to_string()));
    }

    Value::Object(result)
}

/// Resolve the target primary type for a `_list` handle_ref array.
fn resolve_list_target(field_name: &str) -> Option<String> {
    let stem = field_name.strip_suffix("_list")?;
    // Remove trailing `_` if present (e.g., "citation_list" → "citation")
    let singular = stem.trim_end_matches('s');
    match singular {
        "family" => Some("Family".to_string()),
        "citation" => Some("Citation".to_string()),
        "note" => Some("Note".to_string()),
        "media" | "object" => Some("Media".to_string()),
        "tag" => Some("Tag".to_string()),
        "source" | "repository" | "repo" => {
            // citation_list → handle_ref to Citation (not based on stem)
            // These are handled by mixins, so this is a fallback
            None
        }
        _ => None,
    }
}

/// Build edge metadata for an embedded ref type.
fn build_edges_for_ref(embedded_name: &str, _field_name: &str) -> Vec<Value> {
    let target = match embedded_name {
        "EventRef" => "Event",
        "ChildRef" => "Person",
        "PersonRef" => "Person",
        "PlaceRef" => "Place",
        "MediaRef" => "Media",
        "RepoRef" => "Repository",
        "SourceRef" => "Source",
        "CitationRef" => "Citation",
        _ => return vec![],
    };

    vec![Value::Object(
        vec![
            (
                "link".to_string(),
                Value::String(format!("{}.ref", embedded_name)),
            ),
            ("target".to_string(), Value::String(target.to_string())),
        ]
        .into_iter()
        .collect(),
    )]
}

/// Convert an enum type from integer values to string names.
fn convert_enum_type(
    enum_info: &Value,
    enum_name: &str,
    lookup_table: Option<&Value>,
) -> Value {
    let values = enum_info
        .get("values")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|val| {
                    if let Some(n) = val.as_i64() {
                        // Integer value — look up by string key
                        let key = n.to_string();
                        if let Some(table) = lookup_table {
                            if let Some(name) = table.get(&key).and_then(|v| v.as_str()) {
                                return Value::String(name.to_string());
                            }
                        }
                        // Not found — emit warning and use fallback
                        eprintln!(
                            "cargo::warning=Unknown {} integer value {}; using fallback",
                            enum_name, n
                        );
                        Value::String(format!("Unknown({})", n))
                    } else if let Some(s) = val.as_str() {
                        // Already a string, pass through
                        Value::String(s.to_string())
                    } else {
                        Value::String("Unknown".to_string())
                    }
                })
                .collect::<Vec<Value>>()
        })
        .unwrap_or_default();

    let mut result = Map::new();
    result.insert("values".to_string(), Value::Array(values));
    Value::Object(result)
}

/// Build the DateValue secondary type definition (matching 5.2 format).
fn build_date_value_secondary_type() -> Value {
    let mut fields = Map::new();
    let mut year = Map::new();
    year.insert("type".to_string(), Value::String("integer".to_string()));
    year.insert("required".to_string(), Value::Bool(true));
    fields.insert("year".to_string(), Value::Object(year));

    let mut month = Map::new();
    month.insert("type".to_string(), Value::String("integer".to_string()));
    month.insert("required".to_string(), Value::Bool(false));
    fields.insert("month".to_string(), Value::Object(month));

    let mut day = Map::new();
    day.insert("type".to_string(), Value::String("integer".to_string()));
    day.insert("required".to_string(), Value::Bool(false));
    fields.insert("day".to_string(), Value::Object(day));

    let mut modifier = Map::new();
    modifier.insert(
        "type".to_string(),
        Value::String("enum_ref".to_string()),
    );
    modifier.insert("target".to_string(), Value::String("DateModifier".to_string()));
    modifier.insert("required".to_string(), Value::Bool(true));
    fields.insert("modifier".to_string(), Value::Object(modifier));

    let mut quality = Map::new();
    quality.insert(
        "type".to_string(),
        Value::String("enum_ref".to_string()),
    );
    quality.insert("target".to_string(), Value::String("DateQuality".to_string()));
    quality.insert("required".to_string(), Value::Bool(true));
    fields.insert("quality".to_string(), Value::Object(quality));

    let mut text = Map::new();
    text.insert("type".to_string(), Value::String("string".to_string()));
    text.insert("required".to_string(), Value::Bool(false));
    fields.insert("text".to_string(), Value::Object(text));

    let mut result = Map::new();
    result.insert("fields".to_string(), Value::Object(fields));
    Value::Object(result)
}

/// Build the PlaceName secondary type definition (from 5.1 schema).
fn build_place_name_secondary_type() -> Value {
    let mut fields = Map::new();

    let mut value = Map::new();
    value.insert("type".to_string(), Value::String("string".to_string()));
    value.insert("required".to_string(), Value::Bool(false));
    fields.insert("value".to_string(), Value::Object(value));

    let mut date = Map::new();
    date.insert(
        "type".to_string(),
        Value::String("embedded".to_string()),
    );
    date.insert("schema".to_string(), Value::String("DateValue".to_string()));
    date.insert("required".to_string(), Value::Bool(false));
    fields.insert("date".to_string(), Value::Object(date));

    let mut result = Map::new();
    result.insert("fields".to_string(), Value::Object(fields));
    Value::Object(result)
}

/// Build the StyledText secondary type definition (from 5.1 schema).
fn build_styled_text_secondary_type() -> Value {
    let mut fields = Map::new();

    let mut string = Map::new();
    string.insert("type".to_string(), Value::String("string".to_string()));
    string.insert("required".to_string(), Value::Bool(false));
    fields.insert("string".to_string(), Value::Object(string));

    let mut format = Map::new();
    format.insert("type".to_string(), Value::String("integer".to_string()));
    format.insert("required".to_string(), Value::Bool(false));
    fields.insert("format".to_string(), Value::Object(format));

    let mut result = Map::new();
    result.insert("fields".to_string(), Value::Object(fields));
    Value::Object(result)
}

/// Validate that a flat-format schema has all required keys and valid structure.
pub fn validate_flat_format(schema: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    for category_label in &["primary_types", "secondary_types"] {
        if let Some(obj) = schema
            .get(*category_label)
            .and_then(|v| v.as_object())
        {
            for (type_name, type_info) in obj {
                if let Some(fields) = type_info
                    .get("fields")
                    .and_then(|v| v.as_object())
                {
                    for (field_name, field_info) in fields {
                        // Every field must have a "type" key
                        if field_info.get("type").is_none() {
                            errors.push(format!(
                                "{}.{}: missing 'type'",
                                type_name, field_name
                            ));
                        }

                        // handle_ref must have target
                        if field_info
                            .get("kind")
                            .and_then(|v| v.as_str())
                            == Some("handle_ref")
                            && field_info.get("target").is_none()
                        {
                            errors.push(format!(
                                "{}.{}: handle_ref missing 'target'",
                                type_name, field_name
                            ));
                        }

                        // embedded must have schema
                        if field_info
                            .get("type")
                            .and_then(|v| v.as_str())
                            == Some("embedded")
                            && field_info.get("schema").is_none()
                        {
                            errors.push(format!(
                                "{}.{}: embedded missing 'schema'",
                                type_name, field_name
                            ));
                        }

                        // array must have items
                        if field_info
                            .get("type")
                            .and_then(|v| v.as_str())
                            == Some("array")
                            && field_info.get("items").is_none()
                        {
                            errors.push(format!(
                                "{}.{}: array missing 'items'",
                                type_name, field_name
                            ));
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- is_json_schema_format ----

    #[test]
    fn detect_json_schema_format_true() {
        let schema = json!({
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
        assert!(is_json_schema_format(&schema));
    }

    #[test]
    fn detect_json_schema_format_false() {
        let schema = json!({
            "primary_types": {
                "Person": {
                    "fields": {
                        "handle": {"type": "string", "kind": "handle", "required": true}
                    }
                }
            }
        });
        assert!(!is_json_schema_format(&schema));
    }

    #[test]
    fn detect_json_schema_format_empty() {
        let schema = json!({
            "primary_types": {}
        });
        assert!(!is_json_schema_format(&schema));
    }

    // ---- Convert primary fields ----

    #[test]
    fn convert_primary_string_field() {
        let field = json!({"type": "string", "maxLength": 50, "title": "Gramps ID"});
        let result = convert_field(&field, "gramps_id", &Value::Null);
        assert_eq!(result.get("type").and_then(|v| v.as_str()), Some("string"));
        assert_eq!(
            result.get("required").and_then(|v| v.as_bool()),
            Some(false)
        ); // gramps_id is optional
    }

    #[test]
    fn convert_primary_integer_field() {
        let field = json!({"type": "integer", "minimum": 0, "maximum": 3, "title": "Gender"});
        let result = convert_field(&field, "gender", &Value::Null);
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("enum_ref")
        ); // gender → enum_ref
        assert_eq!(
            result.get("target").and_then(|v| v.as_str()),
            Some("Gender")
        );
    }

    #[test]
    fn convert_primary_boolean_field() {
        let field = json!({"type": "boolean", "title": "Private"});
        let result = convert_field(&field, "private", &Value::Null);
        // private is skipped (line 138-140), but test the type conversion
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("boolean")
        );
    }

    #[test]
    fn convert_primary_embedded_object() {
        let field = json!({
            "type": "object",
            "title": "Name",
            "properties": {
                "_class": {"enum": ["Name"]},
                "surname_list": {"type": "array"}
            }
        });
        let result = convert_field(&field, "primary_name", &Value::Null);
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("embedded")
        );
        assert_eq!(
            result.get("schema").and_then(|v| v.as_str()),
            Some("Name")
        );
        assert_eq!(
            result.get("required").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn convert_primary_handle_ref() {
        let field = json!({"type": "string", "maxLength": 50, "title": "Father"});
        let result = convert_field(&field, "father_handle", &Value::Null);
        assert_eq!(
            result.get("kind").and_then(|v| v.as_str()),
            Some("handle_ref")
        );
        assert_eq!(
            result.get("target").and_then(|v| v.as_str()),
            Some("Person")
        );
    }

    #[test]
    fn convert_primary_array_handle_list() {
        let field = json!({
            "type": "array",
            "items": {"type": "string", "maxLength": 50},
            "title": "Families"
        });
        let result = convert_field(&field, "family_list", &Value::Null);
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("array")
        );
        let items = result.get("items").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            items.get("kind").and_then(|v| v.as_str()),
            Some("handle_ref")
        );
        assert_eq!(
            items.get("target").and_then(|v| v.as_str()),
            Some("Family")
        );
    }

    #[test]
    fn convert_primary_array_embedded_ref() {
        let field = json!({
            "type": "array",
            "items": {
                "type": "object",
                "title": "EventRef",
                "properties": {
                    "ref": {"type": "string"},
                    "role": {"type": "object"}
                }
            },
            "title": "Events"
        });
        let result = convert_field(&field, "event_ref_list", &Value::Null);
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("array")
        );
        let items = result.get("items").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            items.get("embedded").and_then(|v| v.as_str()),
            Some("EventRef")
        );
    }

    #[test]
    fn convert_secondary_strips_internal() {
        // Test that _class, private, change are stripped during full conversion
        let type_info = json!({
            "fields": {
                "type": "object",
                "title": "EventRef",
                "properties": {
                    "_class": {"enum": ["EventRef"]},
                    "private": {"type": "boolean"},
                    "ref": {"type": "string", "maxLength": 50, "title": "Event"},
                    "role": {"type": "object", "title": "EventRoleType"}
                }
            }
        });
        let result = convert_type_fields(&type_info, &Value::Null).unwrap();
        let fields = result.get("fields").and_then(|v| v.as_object()).unwrap();
        assert!(!fields.contains_key("_class"), "_class should be stripped");
        assert!(!fields.contains_key("private"), "private should be stripped");
        assert!(fields.contains_key("ref"), "ref should be preserved");
        assert!(fields.contains_key("role"), "role should be preserved");
    }

    // ---- Enum conversion ----

    #[test]
    fn convert_enum_int_to_name() {
        let enum_info = json!({"values": [0, 1, 2, 11]});
        let enum_constants = json!({
            "EventType": {
                "0": "POS_VALUE",
                "1": "POS_STRING",
                "2": "MARR_SETTL",
                "11": "ADOPT"
            }
        });
        let result = convert_enum_type(
            &enum_info,
            "EventType",
            // Pass the per-enum lookup table (the value at enum_constants["EventType"])
            enum_constants.get("EventType"),
        );
        let values = result.get("values").and_then(|v| v.as_array()).unwrap();
        let strings: Vec<&str> = values.iter().filter_map(|v| v.as_str()).collect();
        assert!(strings.contains(&"POS_VALUE"));
        assert!(strings.contains(&"POS_STRING"));
        assert!(strings.contains(&"MARR_SETTL"));
        assert!(strings.contains(&"ADOPT"));
    }

    #[test]
    fn convert_enum_unknown_int() {
        let enum_info = json!({"values": [0, 99]});
        let enum_constants = json!({
            "EventType": {"0": "POS_VALUE"}
        });
        let result = convert_enum_type(
            &enum_info,
            "EventType",
            // Pass the per-enum lookup table
            enum_constants.get("EventType"),
        );
        let values = result.get("values").and_then(|v| v.as_array()).unwrap();
        let strings: Vec<&str> = values.iter().filter_map(|v| v.as_str()).collect();
        assert!(strings.contains(&"POS_VALUE"));
        assert!(strings.contains(&"Unknown(99)")); // fallback for unknown integer
    }

    // ---- Date handling ----

    #[test]
    fn convert_date_embedded_field() {
        // Date field with oneOf [null, object]
        let field = json!({
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
        });
        let result = convert_field(&field, "date", &Value::Null);
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("embedded")
        );
        assert_eq!(
            result.get("schema").and_then(|v| v.as_str()),
            Some("DateValue")
        );
        assert_eq!(
            result.get("required").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn convert_required_by_structure() {
        // Fields without oneOf null wrapper → required: true
        let field = json!({"type": "string", "maxLength": 50, "title": "Handle"});
        let result = convert_field(&field, "handle", &Value::Null);
        assert_eq!(
            result.get("required").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn convert_optional_by_oneof() {
        // Fields with oneOf [null, …] → required: false
        let field = json!({
            "oneOf": [
                {"type": "null"},
                {"type": "string", "title": "Description"}
            ],
            "title": "Description"
        });
        let result = convert_field(&field, "description", &Value::Null);
        assert_eq!(
            result.get("required").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn convert_enum_ref_by_field_name() {
        // modifier → DateModifier
        let field = json!({"type": "integer", "title": "Modifier"});
        let result = convert_field(&field, "modifier", &Value::Null);
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("enum_ref")
        );
        assert_eq!(
            result.get("target").and_then(|v| v.as_str()),
            Some("DateModifier")
        );

        // quality → DateQuality
        let field_q = json!({"type": "integer", "title": "Quality"});
        let result_q = convert_field(&field_q, "quality", &Value::Null);
        assert_eq!(
            result_q.get("target").and_then(|v| v.as_str()),
            Some("DateQuality")
        );
    }

    #[test]
    fn convert_primary_empty_properties() {
        // Type with only _class/private/change → empty fields
        let type_info = json!({
            "fields": {
                "type": "object",
                "title": "Tag",
                "properties": {
                    "_class": {"enum": ["Tag"]},
                    "private": {"type": "boolean"},
                    "change": {"type": "integer"}
                }
            }
        });
        let result = convert_type_fields(&type_info, &Value::Null).unwrap();
        let fields = result.get("fields").and_then(|v| v.as_object()).unwrap();
        assert!(fields.is_empty());
    }

    #[test]
    fn convert_oneof_more_than_two() {
        // oneOf with 3+ alternatives picks first non-null type
        let field = json!({
            "oneOf": [
                {"type": "null"},
                {"type": "string", "title": "Text A"},
                {"type": "string", "title": "Text B"}
            ],
            "title": "Multi"
        });
        let result = convert_field(&field, "multi", &Value::Null);
        assert_eq!(
            result.get("type").and_then(|v| v.as_str()),
            Some("string")
        );
        assert_eq!(
            result.get("required").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn convert_urls_to_url_list() {
        let type_info = json!({
            "fields": {
                "type": "object",
                "title": "Person",
                "properties": {
                    "urls": {
                        "type": "array",
                        "items": {"type": "object", "title": "Url"},
                        "title": "URLs"
                    },
                    "handle": {"type": "string", "maxLength": 50, "title": "Handle"}
                }
            }
        });
        let result = convert_type_fields(&type_info, &Value::Null).unwrap();
        let fields = result.get("fields").and_then(|v| v.as_object()).unwrap();
        assert!(!fields.contains_key("urls"), "urls should be renamed");
        assert!(
            fields.contains_key("url_list"),
            "url_list should be present"
        );
    }

    // ---- validate_flat_format ----

    #[test]
    fn validate_flat_format_all_required_keys() {
        let schema = json!({
            "primary_types": {
                "Person": {
                    "fields": {
                        "handle": {"type": "string", "kind": "handle", "required": true},
                        "no_type_field": {"kind": "handle"}
                    }
                }
            }
        });
        let result = validate_flat_format(&schema);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("missing 'type'")));
    }

    #[test]
    fn validate_flat_format_handle_ref_missing_target() {
        let schema = json!({
            "primary_types": {
                "Person": {
                    "fields": {
                        "bad_ref": {
                            "type": "string",
                            "kind": "handle_ref"
                        }
                    }
                }
            }
        });
        let result = validate_flat_format(&schema);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.contains("handle_ref missing 'target'")));
    }

    #[test]
    fn validate_flat_format_embedded_missing_schema() {
        let schema = json!({
            "secondary_types": {
                "SomeType": {
                    "fields": {
                        "data": {
                            "type": "embedded"
                        }
                    }
                }
            }
        });
        let result = validate_flat_format(&schema);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("embedded missing 'schema'")));
    }

    #[test]
    fn validate_flat_format_array_missing_items() {
        let schema = json!({
            "primary_types": {
                "Person": {
                    "fields": {
                        "list": {
                            "type": "array"
                        }
                    }
                }
            }
        });
        let result = validate_flat_format(&schema);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("array missing 'items'")));
    }

    // ---- DateValue secondary type ----

    #[test]
    fn build_date_value_type() {
        let dv = build_date_value_secondary_type();
        let fields = dv.get("fields").and_then(|v| v.as_object()).unwrap();
        assert!(fields.contains_key("year"));
        assert!(fields.contains_key("month"));
        assert!(fields.contains_key("day"));
        assert!(fields.contains_key("modifier"));
        assert!(fields.contains_key("quality"));
        assert!(fields.contains_key("text"));

        // year is required
        let year = fields.get("year").unwrap();
        assert_eq!(
            year.get("required").and_then(|v| v.as_bool()),
            Some(true)
        );
        // modifiers are required
        let modifier = fields.get("modifier").unwrap();
        assert_eq!(
            modifier.get("type").and_then(|v| v.as_str()),
            Some("enum_ref")
        );
    }

    // ---- Full conversion round-trip ----

    #[test]
    fn property_convert_idempotent() {
        // Converting an already-flat schema should be a no-op
        let flat = json!({
            "version": "5.2",
            "primary_types": {
                "Person": {
                    "fields": {
                        "handle": {"type": "string", "kind": "handle", "required": true}
                    },
                    "inherit_mixins": ["CitationBase"]
                }
            },
            "enum_types": {
                "EventType": {"values": ["Birth", "Death"]}
            },
            "secondary_types": {}
        });
        // is_json_schema_format should return false for flat format
        assert!(!is_json_schema_format(&flat));

        // convert on a flat schema should return Ok and add DateValue secondary type
        let result = convert(flat.clone(), "5.2", &Value::Null).unwrap();
        // The converter always adds DateValue secondary type (for 5.1-only builds)
        assert_eq!(result.get("version"), flat.get("version"));
        assert_eq!(
            result.get("primary_types"),
            flat.get("primary_types")
        );
        assert_eq!(result.get("enum_types"), flat.get("enum_types"));
        // DateValue should be present
        let secondary = result.get("secondary_types").and_then(|v| v.as_object()).unwrap();
        assert!(
            secondary.contains_key("DateValue"),
            "DateValue should be added"
        );
    }

    #[test]
    fn property_is_json_schema_false_after_convert() {
        // After converting a JSON Schema format, is_json_schema_format should be false
        let schema = json!({
            "version": "5.1",
            "primary_types": {
                "Person": {
                    "fields": {
                        "type": "object",
                        "title": "Person",
                        "properties": {
                            "handle": {"type": "string", "maxLength": 50, "title": "Handle"}
                        }
                    },
                    "inherit_mixins": []
                }
            },
            "enum_types": {},
            "secondary_types": {}
        });
        assert!(is_json_schema_format(&schema));

        let result = convert(schema, "5.1", &Value::Null).unwrap();
        assert!(!is_json_schema_format(&result));
    }
}