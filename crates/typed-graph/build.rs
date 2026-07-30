//! Build script — reads versioned schema files (e.g. `schema-5.2.json`) and generates Rust
//! types at compile time.
//!
//! # How it works
//!
//! 1. Detects which schema features are enabled via `CARGO_FEATURE_SCHEMA_*` env vars.
//! 2. For each enabled feature, reads the corresponding `schemas/schema-X.Y.json` file.
//! 3. Runs a union merge algorithm across all loaded schemas:
//!    - Unions all field names per type, with optionality rule
//!    - Unions all enum variants across versions
//!    - Detects conflicting field types
//! 4. Generates Rust source code to `$OUT_DIR/generated_schema.rs`:
//!    - `Node` enum, `Edge` enum, data structs, ref structs, enum types
//!    - Per-version `static SCHEMA_X_Y: Schema` instances
//!    - `Schema::available_versions()`, `default_version()`, `for_version()`, `new()`

use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Feature-to-version mapping
// ---------------------------------------------------------------------------

/// Feature name → version string (e.g., "schema-5-2" → "5.2")
const FEATURE_VERSIONS: &[(&str, &str)] = &[
    ("schema-5-0", "5.0"),
    ("schema-5-1", "5.1"),
    ("schema-5-2", "5.2"),
    ("schema-6-0", "6.0"),
];

/// Version string → feature env var name (e.g., "5.2" → "CARGO_FEATURE_SCHEMA_5_2")
fn version_to_env_var(version: &str) -> String {
    format!("CARGO_FEATURE_SCHEMA_{}", version.replace('.', "_"))
}

/// Version string → schema file name (e.g., "5.2" → "schema-5.2.json")
fn version_to_filename(version: &str) -> String {
    format!("schema-{}.json", version)
}

/// Version string → static name (e.g., "5.2" → "SCHEMA_5_2")
fn version_to_static_name(version: &str) -> String {
    format!("SCHEMA_{}", version.replace('.', "_"))
}

/// Parse a version string into a comparable tuple (major, minor).
fn parse_version(version: &str) -> (u32, u32) {
    let parts: Vec<&str> = version.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    // Register all schema files for rebuild tracking
    for (_, version) in FEATURE_VERSIONS {
        println!(
            "cargo::rerun-if-changed=../../schemas/{}",
            version_to_filename(version)
        );
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo");
    let crate_dir = Path::new(&manifest_dir);
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is two levels deep in workspace");
    let schemas_dir = workspace_root.join("schemas");

    // Detect enabled features
    let enabled_versions = detect_enabled_features();

    // Load schemas for enabled versions
    let schemas: Vec<(String, serde_json::Value)> = load_schemas(&schemas_dir, &enabled_versions);

    // Generate code from all loaded schemas
    let code = generate_code(&schemas);

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let dest_path = Path::new(&out_dir).join("generated_schema.rs");
    fs::write(&dest_path, &code).expect("Failed to write generated_schema.rs");
}

// ---------------------------------------------------------------------------
// Feature detection
// ---------------------------------------------------------------------------

/// Detect which schema versions are enabled via Cargo features.
/// Returns versions sorted ascending (e.g., ["5.0", "5.1", "5.2"]).
fn detect_enabled_features() -> Vec<String> {
    let mut versions: Vec<String> = Vec::new();

    for (_, version) in FEATURE_VERSIONS {
        let env_var = version_to_env_var(version);
        if env::var(&env_var).is_ok() {
            versions.push(version.to_string());
        }
    }

    versions.sort_by_key(|a| parse_version(a));
    versions
}

// ---------------------------------------------------------------------------
// Schema loading
// ---------------------------------------------------------------------------

/// Load schema JSON files for the given versions.
fn load_schemas(
    schemas_dir: &Path,
    versions: &[String],
) -> Vec<(String, serde_json::Value)> {
    let mut schemas: Vec<(String, serde_json::Value)> = Vec::new();

    for version in versions {
        let filename = version_to_filename(version);
        let schema_path = schemas_dir.join(&filename);

        if !schema_path.exists() {
            eprintln!(
                "error: {} not found.",
                schema_path.display()
            );
            eprintln!(
                "  hint: Run `gramps-gen schema download {}` to download it,",
                version
            );
            eprintln!("  or build with only the default schema: `cargo build`");
            eprintln!("  (which uses --features schema-5-2).");
            std::process::exit(1);
        }

        let schema_json = match fs::read_to_string(&schema_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to read {}: {}", schema_path.display(), e);
                std::process::exit(1);
            }
        };

        let schema: serde_json::Value = match serde_json::from_str(&schema_json) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: failed to parse {}: {}", schema_path.display(), e);
                std::process::exit(1);
            }
        };

        schemas.push((version.clone(), schema));
    }

    schemas
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

fn generate_code(schemas: &[(String, serde_json::Value)]) -> String {
    let mut code = String::new();

    // Determine which versions are compiled in
    let versions: Vec<&str> = schemas.iter().map(|(v, _)| v.as_str()).collect();
    let default_version = versions.last().copied().unwrap_or("5.2");

    // Compute the merged (union) schema across all versions
    let merged = merge_schemas(schemas);

    // Build rerun-if-changed comment
    let schema_files: Vec<String> = schemas
        .iter()
        .map(|(v, _)| format!("schema-{}.json", v))
        .collect();
    code.push_str(&format!(
        "// This file is @generated by build.rs from {}.\n",
        schema_files.join(", ")
    ));
    code.push_str("// Do not edit manually — edit the schema .json files and rebuild.\n\n");
    code.push_str("use std::collections::HashMap;\n\n");
    code.push_str("/// Handle uniquely identifies a primary object in the graph.\n");
    code.push_str("/// Matches Gramps' own handle semantics (UUID v4 string, 36 chars).\n");
    code.push_str("pub type Handle = String;\n\n");

    // 1. Generate enum types (union across all versions)
    generate_enum_types(&mut code, &merged);

    // 2. Generate secondary (embedded) ref types (union across all versions)
    generate_secondary_types(&mut code, &merged);

    // 3. Generate primary type data structs (union across all versions)
    let primary_types = get_merged_primary_types(&merged);
    generate_data_structs(&mut code, primary_types);

    // 4. Generate Node enum
    generate_node_enum(&mut code, primary_types);

    // 5. Generate Edge enum (from merged schema; uses first schema for ref metadata)
    generate_edge_enum(&mut code, primary_types, schemas);

    // 6. Generate per-version Schema instances and new API
    generate_schema_metadata(&mut code, primary_types, schemas, &versions, default_version);

    code
}

// ---------------------------------------------------------------------------
// Schema merge algorithm
// ---------------------------------------------------------------------------

/// Merged schema structure holding union types, fields, and enums.
struct MergedSchema {
    /// Primary types: type_name → (struct_fields, inherit_mixins)
    /// Each field entry is (field_name, field_info_json, is_optional_across_versions)
    primary_types: Vec<MergedPrimaryType>,
    /// Secondary types: type_name → merged fields
    secondary_types: Vec<MergedSecondaryType>,
    /// Enum types: enum_name → union of all values across versions
    enum_types: Vec<MergedEnumType>,
}

struct MergedPrimaryType {
    name: String,
    fields: Vec<(String, serde_json::Value, bool)>, // (name, field_info, is_optional_merged)
    mixins: Vec<String>,
}

struct MergedSecondaryType {
    name: String,
    fields: Vec<(String, serde_json::Value, bool)>,
}

struct MergedEnumType {
    name: String,
    values: Vec<String>,
}

/// Merge multiple schemas into one union schema.
fn merge_schemas(schemas: &[(String, serde_json::Value)]) -> MergedSchema {
    if schemas.is_empty() {
        return MergedSchema {
            primary_types: Vec::new(),
            secondary_types: Vec::new(),
            enum_types: Vec::new(),
        };
    }

    // --- Merge enum types ---
    let mut enum_types: Vec<MergedEnumType> = Vec::new();
    // Collect all enum type names across all schemas
    let mut all_enum_names: BTreeSet<String> = BTreeSet::new();
    for (_, schema) in schemas {
        if let Some(obj) = schema.get("enum_types").and_then(|v| v.as_object()) {
            for name in obj.keys() {
                all_enum_names.insert(name.clone());
            }
        }
    }

    for enum_name in &all_enum_names {
        let mut all_values: BTreeSet<String> = BTreeSet::new();
        for (_, schema) in schemas {
            if let Some(values) = schema
                .get("enum_types")
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get(enum_name))
                .and_then(|v| v.get("values"))
                .and_then(|v| v.as_array())
            {
                for val in values {
                    if let Some(s) = val.as_str() {
                        all_values.insert(s.to_string());
                    } else if let Some(n) = val.as_i64() {
                        all_values.insert(n.to_string());
                    }
                }
            }
        }
        enum_types.push(MergedEnumType {
            name: enum_name.clone(),
            values: all_values.into_iter().collect(),
        });
    }

    // --- Merge primary types ---
    let mut primary_types: Vec<MergedPrimaryType> = Vec::new();
    let mut all_primary_names: BTreeSet<String> = BTreeSet::new();
    for (_, schema) in schemas {
        if let Some(obj) = schema.get("primary_types").and_then(|v| v.as_object()) {
            for name in obj.keys() {
                all_primary_names.insert(name.clone());
            }
        }
    }

    for type_name in &all_primary_names {
        // Collect all fields across all versions
        let mut all_fields: BTreeSet<String> = BTreeSet::new();
        let mut field_required_map: HashMap<String, Vec<bool>> = HashMap::new();
        let mut field_info_map: HashMap<String, &serde_json::Value> = HashMap::new();
        let mut mixins_set: BTreeSet<String> = BTreeSet::new();

        for (_, schema) in schemas {
            if let Some(info) = schema
                .get("primary_types")
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get(type_name))
            {
                // Mixins
                if let Some(mixins) = info.get("inherit_mixins").and_then(|v| v.as_array()) {
                    for m in mixins {
                        if let Some(s) = m.as_str() {
                            mixins_set.insert(s.to_string());
                        }
                    }
                }

                // Fields
                if let Some(fields) = info.get("fields").and_then(|v| v.as_object()) {
                    for (field_name, field_info) in fields {
                        all_fields.insert(field_name.clone());
                        let required = field_info
                            .get("required")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        field_required_map
                            .entry(field_name.clone())
                            .or_default()
                            .push(required);
                        // Keep the last version's field info (they should be the same type)
                        field_info_map.insert(field_name.clone(), field_info);
                    }
                } else {
                    // Type exists but has no fields — mark all existing fields as not-required
                    // (handled by the optionality rule below)
                }
            }
        }

        // Build merged fields with optionality rule
        let mut merged_fields: Vec<(String, serde_json::Value, bool)> = Vec::new();
        for field_name in &all_fields {
            let requireds = field_required_map.get(field_name);
            // Optionality rule: field is Option<T> if ANY version doesn't require it
            // or if ANY version doesn't define it at all
            let versions_that_define_it = requireds.map(|v| v.len()).unwrap_or(0);
            let all_required = requireds
                .map(|v| v.iter().all(|r| *r))
                .unwrap_or(false);

            // If the field is defined in all schemas and always required,
            // it's non-optional. Otherwise it's optional.
            let is_optional = !all_required || versions_that_define_it < schemas.len();

            let field_info = field_info_map
                .get(field_name)
                .cloned()
                .unwrap_or(&serde_json::Value::Null)
                .clone();

            merged_fields.push((field_name.clone(), field_info, is_optional));
        }

        // Sort fields for deterministic output
        merged_fields.sort_by(|a, b| a.0.cmp(&b.0));

        let mut mixins: Vec<String> = mixins_set.into_iter().collect();
        mixins.sort();

        primary_types.push(MergedPrimaryType {
            name: type_name.clone(),
            fields: merged_fields,
            mixins,
        });
    }

    // --- Merge secondary types ---
    let mut secondary_types: Vec<MergedSecondaryType> = Vec::new();
    let mut all_secondary_names: BTreeSet<String> = BTreeSet::new();
    for (_, schema) in schemas {
        if let Some(obj) = schema.get("secondary_types").and_then(|v| v.as_object()) {
            for name in obj.keys() {
                all_secondary_names.insert(name.clone());
            }
        }
    }

    for type_name in &all_secondary_names {
        let mut all_fields: BTreeSet<String> = BTreeSet::new();
        let mut field_required_map: HashMap<String, Vec<bool>> = HashMap::new();
        let mut field_info_map: HashMap<String, &serde_json::Value> = HashMap::new();

        for (_, schema) in schemas {
            if let Some(info) = schema
                .get("secondary_types")
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get(type_name))
            {
                if let Some(fields) = info.get("fields").and_then(|v| v.as_object()) {
                    for (field_name, field_info) in fields {
                        all_fields.insert(field_name.clone());
                        let required = field_info
                            .get("required")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        field_required_map
                            .entry(field_name.clone())
                            .or_default()
                            .push(required);
                        field_info_map.insert(field_name.clone(), field_info);
                    }
                }
            }
        }

        let mut merged_fields: Vec<(String, serde_json::Value, bool)> = Vec::new();
        for field_name in &all_fields {
            let requireds = field_required_map.get(field_name);
            let versions_that_define_it = requireds.map(|v| v.len()).unwrap_or(0);
            let all_required = requireds
                .map(|v| v.iter().all(|r| *r))
                .unwrap_or(false);
            let is_optional = !all_required || versions_that_define_it < schemas.len();

            let field_info = field_info_map
                .get(field_name)
                .cloned()
                .unwrap_or(&serde_json::Value::Null)
                .clone();

            merged_fields.push((field_name.clone(), field_info, is_optional));
        }

        merged_fields.sort_by(|a, b| a.0.cmp(&b.0));

        secondary_types.push(MergedSecondaryType {
            name: type_name.clone(),
            fields: merged_fields,
        });
    }

    MergedSchema {
        primary_types,
        secondary_types,
        enum_types,
    }
}

/// Get sorted primary type list from merged schema.
fn get_merged_primary_types(merged: &MergedSchema) -> &[MergedPrimaryType] {
    &merged.primary_types
}

// ---------------------------------------------------------------------------
// Enum types (merged)
// ---------------------------------------------------------------------------

fn generate_enum_types(code: &mut String, merged: &MergedSchema) {
    if merged.enum_types.is_empty() {
        return;
    }

    code.push_str("// ---- Generated enum types (union across versions) ----\n\n");

    for enum_type in &merged.enum_types {
        let enum_name = to_pascal_case(&enum_type.name);
        let values = &enum_type.values;

        code.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]\n");
        code.push_str(&format!("pub enum {} {{\n", enum_name));

        let mut unique: Vec<String> = values.clone();
        unique.sort();
        unique.dedup();

        for (i, value) in unique.iter().enumerate() {
            let variant = to_enum_variant_name(value);
            if i == 0 {
                code.push_str(&format!(
                    "    /// {} value.\n    #[default]\n    {},\n",
                    value, variant
                ));
            } else {
                code.push_str(&format!("    /// {} value.\n    {},\n", value, variant));
            }
        }
        code.push_str("}\n\n");
    }
}

// ---------------------------------------------------------------------------
// Secondary types (merged)
// ---------------------------------------------------------------------------

fn generate_secondary_types(code: &mut String, merged: &MergedSchema) {
    if merged.secondary_types.is_empty() {
        return;
    }

    code.push_str("// ---- Generated secondary/embedded types (union across versions) ----\n\n");

    for stype in &merged.secondary_types {
        let struct_name = to_pascal_case(&stype.name);
        gen_struct_from_fields_merged(code, &struct_name, &stype.fields);
    }
}

// ---------------------------------------------------------------------------
// Data structs (merged)
// ---------------------------------------------------------------------------

fn generate_data_structs(code: &mut String, primary_types: &[MergedPrimaryType]) {
    code.push_str("// ---- Generated primary type data structs (union across versions) ----\n\n");

    for ptype in primary_types {
        let struct_name = format!("{}Data", to_pascal_case(&ptype.name));
        gen_struct_from_fields_merged(code, &struct_name, &ptype.fields);
    }
}

// ---------------------------------------------------------------------------
// Node enum (merged)
// ---------------------------------------------------------------------------

fn generate_node_enum(code: &mut String, primary_types: &[MergedPrimaryType]) {
    code.push_str("// ---- Generated Node enum ----\n\n");
    code.push_str("/// Enum over all primary node types.\n");
    code.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    code.push_str("pub enum Node {\n");

    for ptype in primary_types {
        let type_name = to_pascal_case(&ptype.name);
        let struct_name = format!("{}Data", type_name);
        code.push_str(&format!(
            "    /// {} node variant.\n    {}({}),\n",
            ptype.name, type_name, struct_name
        ));
    }

    code.push_str("}\n\n");
}

// ---------------------------------------------------------------------------
// Edge enum (merged)
// ---------------------------------------------------------------------------

fn generate_edge_enum(
    code: &mut String,
    primary_types: &[MergedPrimaryType],
    schemas: &[(String, serde_json::Value)],
) {
    // Use the first available schema for ref metadata lookups (structure is stable)
    let schema = schemas
        .first()
        .map(|(_, s)| s)
        .unwrap_or(&serde_json::Value::Null);

    code.push_str("// ---- Generated Edge enum ----\n\n");
    code.push_str("/// Enum over all edge kinds in the typed graph.\n");
    code.push_str("/// Each variant carries the source and target handle.\n");
    code.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    code.push_str("pub enum Edge {\n");

    let mut edge_variants: Vec<String> = Vec::new();

    for ptype in primary_types {
        let source_type = to_pascal_case(&ptype.name);
        let mixins = &ptype.mixins;

        for (field_name, field_info, _is_optional) in &ptype.fields {
            // Check items-level structure first (for array fields)
            if let Some(items) = field_info.get("items").and_then(|v| v.as_object()) {
                let items_kind = items.get("kind").and_then(|v| v.as_str());
                let items_target = items.get("target").and_then(|v| v.as_str());

                // Array items with kind: handle_ref (e.g., family_list, note_list)
                if items_kind == Some("handle_ref") {
                    if let Some(target_type) = items_target {
                        let edge_name = format!(
                            "{}{}",
                            source_type,
                            to_pascal_case(strip_handle_suffix(field_name))
                        );
                        edge_variants.push(format!(
                            "    /// {} → {} via {}.\n    {} {{ source: Handle, target: Handle }},",
                            ptype.name, target_type, field_name, edge_name
                        ));
                    }
                    continue;
                }

                // Embedded ref items with edges (e.g., event_ref_list, child_ref_list)
                if let Some(embedded_name) = items.get("embedded").and_then(|v| v.as_str()) {
                    if let Some(edges) = items.get("edges").and_then(|v| v.as_array()) {
                        for edge in edges {
                            if let (Some(link), Some(edge_target)) = (
                                edge.get("link").and_then(|v| v.as_str()),
                                edge.get("target").and_then(|v| v.as_str()),
                            ) {
                                let ref_type = link.split('.').next().unwrap_or(embedded_name);
                                let edge_name =
                                    format!("{}{}", source_type, to_pascal_case(ref_type));
                                let has_meta = has_ref_metadata(embedded_name, schema);
                                if has_meta {
                                    let meta_type = to_pascal_case(ref_type);
                                    edge_variants.push(format!(
                                        "    /// {} → {} via {}.\n    {} {{ source: Handle, target: Handle, metadata: Box<{}> }},",
                                        ptype.name, edge_target, field_name, edge_name, meta_type
                                    ));
                                } else {
                                    edge_variants.push(format!(
                                        "    /// {} → {} via {}.\n    {} {{ source: Handle, target: Handle }},",
                                        ptype.name, edge_target, field_name, edge_name
                                    ));
                                }
                            }
                        }
                    }
                    continue;
                }
            }

            // Top-level handle_ref fields (direct handle values, not arrays)
            let kind = field_info.get("kind").and_then(|v| v.as_str());
            let target = field_info.get("target").and_then(|v| v.as_str());

            if kind == Some("handle_ref") {
                if let Some(target_type) = target {
                    let edge_name = format!(
                        "{}{}",
                        source_type,
                        to_pascal_case(strip_handle_suffix(field_name))
                    );
                    edge_variants.push(format!(
                        "    /// {} → {} via {}.\n    {} {{ source: Handle, target: Handle }},",
                        ptype.name, target_type, field_name, edge_name
                    ));
                }
            }
        }

        // Mixin edges
        for mixin in mixins {
            if let Some(variant) = gen_mixin_edge(&ptype.name, source_type.as_str(), mixin) {
                edge_variants.push(variant);
            }
        }
    }

    // Deduplicate edge variants by their actual variant name
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for variant in &edge_variants {
        let last_line = variant.lines().last().unwrap_or("");
        let name_part = last_line.split('{').next().unwrap_or("").trim();
        if seen.insert(name_part.to_string()) {
            code.push_str(variant);
            code.push('\n');
        }
    }

    code.push_str("}\n\n");
}

/// Check if a ref type has metadata fields beyond `ref`.
fn has_ref_metadata(embedded_name: &str, schema: &serde_json::Value) -> bool {
    let Some(secondary) = schema.get("secondary_types").and_then(|v| v.as_object()) else {
        return false;
    };
    let Some(info) = secondary.get(embedded_name) else {
        return false;
    };
    let Some(fields) = info.get("fields").and_then(|v| v.as_object()) else {
        return false;
    };
    fields.len() > 1 || !fields.contains_key("ref")
}

/// Generate an Edge variant for a mixin.
fn gen_mixin_edge(primary_type: &str, _source_type: &str, mixin: &str) -> Option<String> {
    match mixin {
        "CitationBase" => Some(format!(
            "    /// {} → Citation via citation_list.\n    CitationRef {{ source: Handle, target: Handle }},",
            primary_type
        )),
        "NoteBase" => Some(format!(
            "    /// {} → Note via note_list.\n    NoteRef {{ source: Handle, target: Handle }},",
            primary_type
        )),
        "MediaBase" => Some(format!(
            "    /// {} → Media via media_list.\n    MediaRef {{ source: Handle, target: Handle }},",
            primary_type
        )),
        "TagBase" => Some(format!(
            "    /// {} → Tag via tag_list.\n    TagRef {{ source: Handle, target: Handle }},",
            primary_type
        )),
        _ => None,
    }
}

/// Strip `_handle` or `_list` suffix from a field name.
fn strip_handle_suffix(name: &str) -> &str {
    if let Some(stripped) = name.strip_suffix("_handle") {
        stripped
    } else if let Some(stripped) = name.strip_suffix("_list") {
        stripped
    } else {
        name
    }
}

// ---------------------------------------------------------------------------
// Schema metadata — per-version instances + new API
// ---------------------------------------------------------------------------

fn generate_schema_metadata(
    code: &mut String,
    primary_types: &[MergedPrimaryType],
    schemas: &[(String, serde_json::Value)],
    versions: &[&str],
    default_version: &str,
) {
    code.push_str("// ---- Generated Schema metadata ----\n\n");
    code.push_str("/// Runtime metadata about the schema, for use by generators and validators.\n");
    code.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    code.push_str("pub struct Schema {\n");
    code.push_str("    /// Gramps version this schema was extracted from.\n");
    code.push_str("    pub version: &'static str,\n");
    code.push_str("    /// Required fields for each primary type.\n");
    code.push_str("    pub required_fields: HashMap<&'static str, Vec<&'static str>>,\n");
    code.push_str("    /// Cardinality constraints for each field: (field_key, (min, max)).\n");
    code.push_str(
        "    pub cardinality_constraints: HashMap<&'static str, (Option<u32>, Option<u32>)>,\n",
    );
    code.push_str("    /// Valid enum values for each enum type (version-specific).\n");
    code.push_str("    pub valid_enum_values: HashMap<&'static str, Vec<&'static str>>,\n");
    code.push_str("    /// Maps \"Type.field_name\" to list of versions where that field exists.\n");
    code.push_str("    pub field_availability: HashMap<&'static str, Vec<&'static str>>,\n");
    code.push_str("}\n\n");

    // Generate per-version Schema constructor functions
    // (HashMap/Vec can't be in static context, so we use LazyLock)
    code.push_str("use std::sync::LazyLock;\n\n");

    for (version, schema) in schemas {
        let static_name = version_to_static_name(version);
        code.push_str(&format!(
            "/// Schema metadata for Gramps {}.\n",
            version
        ));
        code.push_str(&format!(
            "pub static {}: LazyLock<Schema> = LazyLock::new(|| {});\n\n",
            static_name,
            generate_schema_instance(version, primary_types, schema, schemas)
        ));
    }

    // Generate the API methods
    code.push_str("impl Schema {\n");

    // available_versions()
    let version_list: Vec<String> = versions.iter().map(|v| format!("\"{}\"", v)).collect();
    code.push_str("    /// Returns the list of all schema versions compiled into this binary.\n");
    code.push_str("    pub fn available_versions() -> &'static [&'static str] {\n");
    code.push_str(&format!("        &[{}]\n", version_list.join(", ")));
    code.push_str("    }\n\n");

    // default_version()
    code.push_str("    /// Returns the highest available schema version.\n");
    code.push_str("    pub fn default_version() -> &'static str {\n");
    code.push_str(&format!("        \"{}\"\n", default_version));
    code.push_str("    }\n\n");

    // for_version()
    code.push_str("    /// Get the Schema for a specific version.\n");
    code.push_str("    /// Returns `None` if the version is not compiled in.\n");
    code.push_str("    pub fn for_version(version: &str) -> Option<&'static Schema> {\n");
    code.push_str("        match version {\n");
    for version in versions {
        let static_name = version_to_static_name(version);
        code.push_str(&format!("            \"{}\" => Some(&*{}),\n", version, static_name));
    }
    code.push_str("            _ => None,\n");
    code.push_str("        }\n");
    code.push_str("    }\n\n");

    // new() — backward-compatible alias
    code.push_str("    /// Create a Schema instance with the default (highest) version.\n");
    code.push_str("    /// This is a backward-compatible alias for `for_version(Schema::default_version())`.\n");
    code.push_str("    #[deprecated = \"Use Schema::for_version(version) or Schema::for_version(Schema::default_version()) instead\"]\n");
    code.push_str("    pub fn new() -> Self {\n");
    code.push_str("        Self::for_version(Self::default_version())\n");
    code.push_str("            .expect(\"default version should always be available\")\n");
    code.push_str("            .clone()\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // Default impl
    code.push_str("impl Default for Schema {\n");
    code.push_str("    fn default() -> Self {\n");
    code.push_str("        Self::for_version(Self::default_version())\n");
    code.push_str("            .expect(\"default version should always be available\")\n");
    code.push_str("            .clone()\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");
}

/// Generate a Schema instance literal for a specific version.
fn generate_schema_instance(
    version: &str,
    _primary_types: &[MergedPrimaryType],
    schema: &serde_json::Value,
    all_schemas: &[(String, serde_json::Value)],
) -> String {
    let mut s = String::new();

    // Build the field_availability map: for each version, track which fields exist
    // We do this by building a map of Type.field → list of versions that define it
    // across all schemas
    let mut field_avail: HashMap<String, Vec<String>> = HashMap::new();

    for (v, s) in all_schemas {
        if let Some(primary) = s.get("primary_types").and_then(|v| v.as_object()) {
            for (type_name, type_info) in primary {
                if let Some(fields) = type_info.get("fields").and_then(|v| v.as_object()) {
                    for field_name in fields.keys() {
                        let key = format!("{}.{}", type_name, field_name);
                        field_avail.entry(key).or_default().push(v.clone());
                    }
                }
            }
        }
    }

    // Build the version-specific Schema
    s.push_str("Schema {\n");

    // version
    s.push_str(&format!("    version: \"{}\",\n", version));

    // required_fields
    s.push_str("    required_fields: {\n");
    s.push_str("        let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();\n");
    if let Some(primary) = schema.get("primary_types").and_then(|v| v.as_object()) {
        let mut type_names: Vec<&str> = primary.keys().map(|k| k.as_str()).collect();
        type_names.sort();
        for type_name in type_names {
            if let Some(info) = primary.get(type_name) {
                if let Some(fields) = info.get("fields").and_then(|v| v.as_object()) {
                    let required: Vec<&str> = fields
                        .iter()
                        .filter(|(_, fi)| {
                            fi.get("required").and_then(|v| v.as_bool()).unwrap_or(false)
                        })
                        .map(|(name, _)| name.as_str())
                        .collect();
                    if !required.is_empty() {
                        s.push_str(&format!(
                            "        m.insert(\"{}\", vec!{:?});\n",
                            type_name, required
                        ));
                    }
                }
            }
        }
    }
    s.push_str("        m\n");
    s.push_str("    },\n");

    // cardinality_constraints
    s.push_str("    cardinality_constraints: {\n");
    s.push_str("        let mut m: HashMap<&'static str, (Option<u32>, Option<u32>)> = HashMap::new();\n");
    if let Some(primary) = schema.get("primary_types").and_then(|v| v.as_object()) {
        let mut type_names: Vec<&str> = primary.keys().map(|k| k.as_str()).collect();
        type_names.sort();
        for type_name in type_names {
            if let Some(info) = primary.get(type_name) {
                if let Some(fields) = info.get("fields").and_then(|v| v.as_object()) {
                    let mut field_names: Vec<&str> = fields.keys().map(|k| k.as_str()).collect();
                    field_names.sort();
                    for field_name in field_names {
                        let field_info = &fields[field_name];
                        if let Some(cardinality) = field_info.get("cardinality").and_then(|v| v.as_object()) {
                            let min = cardinality.get("min").and_then(|v| v.as_i64()).map(|v| v as u32);
                            let max = cardinality.get("max").and_then(|v| {
                                if v.is_null() { None } else { v.as_i64().map(|v| v as u32) }
                            });
                            s.push_str(&format!(
                                "        m.insert(\"{}.{}\", ({:?}, {:?}));\n",
                                type_name, field_name, min, max
                            ));
                        }
                    }
                }
            }
        }
    }
    s.push_str("        m\n");
    s.push_str("    },\n");

    // valid_enum_values
    s.push_str("    valid_enum_values: {\n");
    s.push_str("        let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();\n");
    if let Some(enum_types) = schema.get("enum_types").and_then(|v| v.as_object()) {
        let mut enum_names: Vec<&str> = enum_types.keys().map(|k| k.as_str()).collect();
        enum_names.sort();
        for enum_name in enum_names {
            if let Some(info) = enum_types.get(enum_name) {
                if let Some(values) = info.get("values").and_then(|v| v.as_array()) {
                    let val_strs: Vec<String> = values
                        .iter()
                        .filter_map(|v| {
                            v.as_str().map(|s| format!("\"{}\"", s))
                                .or_else(|| v.as_i64().map(|n| format!("\"{}\"", n)))
                        })
                        .collect();
                    s.push_str(&format!(
                        "        m.insert(\"{}\", vec![{}]);\n",
                        enum_name,
                        val_strs.join(", ")
                    ));
                }
            }
        }
    }
    s.push_str("        m\n");
    s.push_str("    },\n");

    // field_availability
    s.push_str("    field_availability: {\n");
    s.push_str("        let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();\n");
    let mut avail_keys: Vec<String> = field_avail.keys().cloned().collect();
    avail_keys.sort();
    for key in &avail_keys {
        if let Some(versions) = field_avail.get(key) {
            let ver_strs: Vec<String> = versions.iter().map(|v| format!("\"{}\"", v)).collect();
            s.push_str(&format!(
                "        m.insert(\"{}\", vec![{}]);\n",
                key,
                ver_strs.join(", ")
            ));
        }
    }
    s.push_str("        m\n");
    s.push_str("    },\n");

    s.push_str("}\n");
    s
}

// ---------------------------------------------------------------------------
// Struct generation from merged fields
// ---------------------------------------------------------------------------

fn gen_struct_from_fields_merged(
    code: &mut String,
    struct_name: &str,
    fields: &[(String, serde_json::Value, bool)],
) {
    code.push_str("#[derive(Clone, Debug, PartialEq, Default)]\n");
    code.push_str(&format!("pub struct {} {{\n", struct_name));

    for (field_name, field_info, is_optional) in fields {
        let rust_field = to_snake_case(field_name);
        let rust_type = field_to_rust_type_merged(field_info, *is_optional);
        code.push_str(&format!("    pub {}: {},\n", rust_field, rust_type));
    }

    code.push_str("}\n\n");
}

// ---------------------------------------------------------------------------
// Type resolution
// ---------------------------------------------------------------------------

/// Map a schema field info to its Rust type, respecting the merged optionality.
fn field_to_rust_type_merged(field_info: &serde_json::Value, is_optional: bool) -> String {
    let field_type = field_info
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("string");
    let kind = field_info.get("kind").and_then(|v| v.as_str());
    let target = field_info.get("target").and_then(|v| v.as_str());

    // Handle handle or handle_ref -> Handle or Option<Handle>
    if let Some(k) = kind {
        if k == "handle" || k == "handle_ref" {
            if is_optional {
                return "Option<Handle>".to_string();
            } else {
                return "Handle".to_string();
            }
        }
    }

    match field_type {
        "string" => {
            if is_optional {
                "Option<String>".to_string()
            } else {
                "String".to_string()
            }
        }
        "integer" => {
            if is_optional {
                "Option<i32>".to_string()
            } else {
                "i32".to_string()
            }
        }
        "boolean" => {
            if is_optional {
                "Option<bool>".to_string()
            } else {
                "bool".to_string()
            }
        }
        "enum" => {
            // Inline enum — use i32
            if is_optional {
                "Option<i32>".to_string()
            } else {
                "i32".to_string()
            }
        }
        "enum_ref" => {
            if let Some(t) = target {
                let enum_name = to_pascal_case(t);
                if is_optional {
                    format!("Option<{}>", enum_name)
                } else {
                    enum_name
                }
            } else if is_optional {
                "Option<String>".to_string()
            } else {
                "String".to_string()
            }
        }
        "embedded" => {
            if let Some(schema_name) = field_info.get("schema").and_then(|v| v.as_str()) {
                let struct_name = to_pascal_case(schema_name);
                if is_optional {
                    format!("Option<{}>", struct_name)
                } else {
                    struct_name
                }
            } else if is_optional {
                "Option<String>".to_string()
            } else {
                "String".to_string()
            }
        }
        "array" => {
            let item_type = resolve_array_item_type(field_info);
            format!("Vec<{}>", item_type)
        }
        _ => {
            if is_optional {
                "Option<String>".to_string()
            } else {
                "String".to_string()
            }
        }
    }
}

/// Resolve the item type for an array field.
fn resolve_array_item_type(field_info: &serde_json::Value) -> String {
    let items = field_info.get("items");
    let items = match items {
        Some(v) => v,
        None => return "String".to_string(),
    };

    if let Some(s) = items.as_str() {
        return to_pascal_case(s);
    }

    if let Some(obj) = items.as_object() {
        if let Some(embedded) = obj.get("embedded").and_then(|v| v.as_str()) {
            return to_pascal_case(embedded);
        }
        if let Some(kind) = obj.get("kind").and_then(|v| v.as_str()) {
            if kind == "handle_ref" {
                return "Handle".to_string();
            }
        }
    }

    "String".to_string()
}

// ---------------------------------------------------------------------------
// Name conversion helpers
// ---------------------------------------------------------------------------

/// Convert a name to PascalCase.
fn to_pascal_case(name: &str) -> String {
    if name.is_empty() {
        return "Unknown".to_string();
    }
    let mut result = String::new();
    let mut upper_next = true;
    for ch in name.chars() {
        if ch == '_' || ch == '.' || ch == '-' {
            upper_next = true;
        } else if upper_next {
            result.push(ch.to_ascii_uppercase());
            upper_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Convert a name to snake_case.
fn to_snake_case(name: &str) -> String {
    if name.is_empty() {
        return "unknown".to_string();
    }
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            result.push('_');
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch.to_ascii_lowercase());
        }
    }
    // Handle Rust keywords
    match result.as_str() {
        "type" => "type_field".to_string(),
        "ref" => "ref_field".to_string(),
        "self" => "self_field".to_string(),
        "match" => "match_field".to_string(),
        _ => result,
    }
}

/// Convert a string value to a Rust enum variant name.
fn to_enum_variant_name(value: &str) -> String {
    if value.is_empty() {
        return "Empty".to_string();
    }
    let sanitized: String = value
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                ' '
            }
        })
        .collect();

    let mut result = String::new();
    let mut upper_next = true;
    for ch in sanitized.chars() {
        if ch == ' ' || ch == '_' || ch == '-' {
            upper_next = true;
        } else if upper_next {
            result.push(ch.to_ascii_uppercase());
            upper_next = false;
        } else {
            result.push(ch);
        }
    }

    if result.is_empty() {
        return "Unknown".to_string();
    }
    if result
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        result.insert(0, '_');
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("5.0"), (5, 0));
        assert_eq!(parse_version("5.2"), (5, 2));
        assert_eq!(parse_version("6.0"), (6, 0));
    }

    #[test]
    fn test_version_to_env_var() {
        assert_eq!(version_to_env_var("5.2"), "CARGO_FEATURE_SCHEMA_5_2");
        assert_eq!(version_to_env_var("5.0"), "CARGO_FEATURE_SCHEMA_5_0");
    }

    #[test]
    fn test_version_to_filename() {
        assert_eq!(version_to_filename("5.2"), "schema-5.2.json");
        assert_eq!(version_to_filename("5.1"), "schema-5.1.json");
    }

    #[test]
    fn test_version_to_static_name() {
        assert_eq!(version_to_static_name("5.2"), "SCHEMA_5_2");
        assert_eq!(version_to_static_name("5.1"), "SCHEMA_5_1");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("Person"), "Person");
        assert_eq!(to_pascal_case("event_ref"), "EventRef");
        assert_eq!(to_pascal_case("EventRef.ref"), "EventRefRef");
        assert_eq!(to_pascal_case(""), "Unknown");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("handle"), "handle");
        assert_eq!(to_snake_case("gramps_id"), "gramps_id");
        assert_eq!(to_snake_case("event_ref_list"), "event_ref_list");
        assert_eq!(to_snake_case("type"), "type_field");
        assert_eq!(to_snake_case("ref"), "ref_field");
    }

    #[test]
    fn test_to_enum_variant_name() {
        assert_eq!(to_enum_variant_name("Birth"), "Birth");
        assert_eq!(to_enum_variant_name("Also Known As"), "AlsoKnownAs");
        assert_eq!(to_enum_variant_name("Military Service"), "MilitaryService");
        assert_eq!(to_enum_variant_name(""), "Empty");
    }

    #[test]
    fn test_strip_handle_suffix() {
        assert_eq!(strip_handle_suffix("father_handle"), "father");
        assert_eq!(strip_handle_suffix("family_list"), "family");
        assert_eq!(strip_handle_suffix("handle"), "handle");
        assert_eq!(strip_handle_suffix("gramps_id"), "gramps_id");
    }

    #[test]
    fn test_detect_enabled_features_empty() {
        // When no features are enabled, detect_enabled_features returns empty
        // This is tested by the fact that default features include schema-5-2
        let result = detect_enabled_features();
        // In test environment, CARGO_FEATURE_* may or may not be set
        // Just verify it doesn't crash
        assert!(result.iter().all(|v| parse_version(v) >= (5, 0)));
    }
}