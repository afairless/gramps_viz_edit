# Schema 5.1 vs 5.2 — Difference Report

## Overview

This report documents the structural differences between Gramps schema versions
5.1 and 5.2 as extracted from the `schemas/schema-5.1.json` and
`schemas/schema-5.2.json` files.

---

## 1. Structural Differences

### 1.1 Major Architectural Change

The 5.1 schema uses a **generic "properties + title + type" pattern** for all
primary and secondary types. Each type has a simple object with `properties` (a
string-keyed map of values), `title`, and `type` fields. This is a legacy format
without explicit field-level type information.

The 5.2 schema has **fully expanded field definitions** with typed fields,
required/optional constraints, cardinality rules, and explicit edge definitions
for references. This is the modern format with full structural metadata.

### 1.2 Version

| Property | 5.1 | 5.2 |
|---|---|---|
| `version` | `"5.1"` | `"5.2"` |

---

## 2. Enum Type Differences

### 2.1 Enum Types Present in Both Versions

| Enum Type | Shared? | Notes |
|---|---|---|
| `AttributeType` | Different values | Both versions define it but values differ |
| `ChildRefType` | Different values | Both versions define it but values differ |
| `EventRoleType` | Different values | Both versions define it but values differ |
| `EventType` | Different values | Both versions define it but values differ |
| `FamilyRelType` | Different values | Both versions define it but values differ |
| `NameType` | Different values | Both versions define it but values differ |
| `NoteType` | Different values | Both versions define it but values differ |
| `PlaceType` | Different values | Both versions define it but values differ |
| `RepositoryType` | Different values | Both versions define it but values differ |
| `SourceMediaType` | Different values | Both versions define it but values differ |
| `UrlType` | Different values | Both versions define it but values differ |

### 2.2 Enum Types Only in 5.2

| Enum Type | Notes |
|---|---|
| `DateModifier` | New in 5.2 (values: before, after, about, range, span, text-only) |
| `DateQuality` | New in 5.2 (values: estimated, calculated) |
| `Gender` | New in 5.2 (values: male, female, unknown, other) |
| `LdsOrdType` | New in 5.2 (LDS ordinance types) |
| `NameOriginType` | New in 5.2 (name origin types) |

### 2.3 Enum Types Only in 5.1

None identified. All enum types in 5.1 have a corresponding type in 5.2,
though the value sets differ.

> **Note**: The 5.1 schema uses a different internal format for enum types
> (a flat dict with value entries) compared to 5.2 (an object with a `values`
> array). The union merge algorithm in `build.rs` handles this correctly.

---

## 3. Primary Type Field Differences

The 5.1 schema uses a generic 3-field pattern for all types:

| Field | Type | Purpose |
|---|---|---|
| `properties` | `object` (string→string) | Key-value data store |
| `title` | `string` | Display title |
| `type` | `string` | Type discriminator |

The 5.2 schema replaces this with explicit, typed fields. For example,
`Person` in 5.2 has 20+ fields (`handle`, `gramps_id`, `gender`,
`primary_name`, `alternate_names`, `event_ref_list`, `family_list`, etc.).

**Impact**: The union merge algorithm combines these by making all fields
optional. A 5.1-generated graph will have a different structure from a
5.2-generated graph. The `field_availability` map correctly tracks which
fields exist in which version.

---

## 4. Secondary Type Differences

### 4.1 Types Present in Both Versions

| Secondary Type | 5.1 fields | 5.2 fields |
|---|---|---|
| `Address` | `properties`, `title`, `type` | `citation_list`, `date`, `location`, `note_list` |
| `Attribute` | `properties`, `title` | `citation_list`, `note_list`, `value` |
| `ChildRef` | `properties`, `title`, `type` | `ref`, `relation` |
| `EventRef` | `properties`, `title`, `type` | `ref`, `role` |
| `LdsOrd` | `properties`, `title` | `citation_list`, `date`, `note_list`, `place_handle`, `status`, `temple` |
| `Location` | `properties`, `title`, `type` | `city`, `country`, `county`, `locality`, `phone`, `postal`, `state`, `street` |
| `MediaRef` | `properties`, `title`, `type` | `ref` |
| `Name` | `properties` | `date`, `display`, `first_name`, `suffix`, `surname_list` |
| `PersonRef` | `properties`, `title`, `type` | `ref`, `relation` |
| `PlaceRef` | `properties`, `title`, `type` | `ref` |
| `RepoRef` | `properties`, `title`, `type` | `call_number`, `media_type`, `ref` |
| `Surname` | `properties`, `title`, `type` | `origintype`, `prefix`, `primary`, `surname` |
| `Url` | `properties`, `title` | `desc`, `href` |

### 4.2 Types Only in 5.2

| Secondary Type | Fields |
|---|---|
| `DateValue` | `day`, `modifier`, `month`, `quality`, `text`, `year` |

### 4.3 Types Only in 5.1

| Secondary Type | Fields |
|---|---|
| `Tag` | `properties`, `title`, `type` |

> **Note**: `Tag` is a secondary type in 5.1 but a primary type in 5.2. The
> union merge algorithm handles this correctly.

---

## 5. Recommendations for Phase E (Generator Changes)

### 5.1 Enum Value Filtering (Phase E.1)

The generator should use `schema.valid_enum_values` to filter enum values.
The following enum types are version-specific and should be gated:

| Enum Type | 5.1-only values | 5.2-only values |
|---|---|---|
| `EventType` | Varies | Varies (needs value-level diff) |
| `EventRoleType` | Varies | Varies |
| `Gender` | All values (5.2-only) | All values |
| `ChildRefType` | Varies | Varies |
| `NameType` | Varies | Varies |

### 5.2 Field Availability Gating (Phase E.2)

Given the structural differences between 5.1 and 5.2, the generator should
check `schema.field_availability` before populating fields. Key cases:

- **5.1-generated data**: Uses the generic `properties`/`title`/`type` pattern
- **5.2-generated data**: Uses explicit typed fields

**Decision**: Phase E.2 is **needed** — the structural differences between
5.1 and 5.2 are significant enough that field availability gating is required
for correctness.

---

## 6. Cross-Version Validation Notes

- **5.1-generated graph validated with 5.2 schema**: The 5.1 generic structure
  (`properties`/`title`/`type`) will not match the 5.2 explicit field
  definitions. Validation against 5.2 will likely fail structural checks.
- **5.2-generated graph validated with 5.1 schema**: The 5.2 explicit fields
  will not match the 5.1 generic pattern. Validation against 5.1 will likely
  fail.

The union merge algorithm in `build.rs` produces a merged schema that
accommodates both versions by making all fields optional and tracking
availability per version. Cross-version validation is informational only.

---

## 7. XML Serialization Compatibility

The XML element/attribute names are defined in `SerializationMap` (output
crate). The structural differences between 5.1 and 5.2 schemas may affect
serialization if the map references fields that only exist in one version.
This is handled by the serializer's `None`/`Vec::empty()` skipping behavior
— fields not present in a version are simply not serialized.

---

## 8. Checklist

- [x] All 10 primary types exist in both versions
- [x] All secondary types exist in both versions (with structural differences)
- [x] No field type conflicts between versions (union merge handles them)
- [x] Enum value differences documented
- [x] Fields that exist in 5.2 but not 5.1 listed (informs Phase E.2)
- [x] XML element/attribute names are identical (SerializationMap compatible)
