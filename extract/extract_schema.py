#!/usr/bin/env python3
"""Extract Gramps schema from the Python data model and emit schema-{version}.json.

Usage:
    PYTHONPATH=/path/to/gramps/repo python extract_schema.py

This imports gramps.gen.lib from a local Gramps source clone and introspects
the primary data classes (Person, Family, Event, Place, etc.) to produce a
schema-{version}.json artifact describing the data model structure.

⚠ WARNING: Only point PYTHONPATH at a trusted Gramps source checkout.
  The extractor imports and executes Python code from that path.

If Gramps is not available, run with --mock to use built-in mock classes
for testing or development.

Options:
    --version VERSION   Override the schema version string (default: auto-detect
                        from Gramps source, or 5.2 if not available)
    --output PATH       Write output to PATH (default: schemas/schema-{version}.json)
    --enum-names        Extract enum integer-to-name mappings instead of the full schema.
    --gramps-path PATH  Path to Gramps source checkout (added to PYTHONPATH).
"""

import argparse
import enum
import importlib
import inspect
import json
import os
import sys
import types
from pathlib import Path
from collections import OrderedDict
import re
from typing import Any, Dict, List, Optional, Tuple, Union

# ---- Primary types to extract ----
PRIMARY_TYPES = [
    "Person",
    "Family",
    "Event",
    "Place",
    "Source",
    "Citation",
    "Repository",
    "Media",
    "Note",
    "Tag",
]

# Known mixin base names (suffixes used by Gramps)
MIXIN_SUFFIXES = [
    "CitationBase",
    "NoteBase",
    "MediaBase",
    "AttributeBase",
    "AddressBase",
    "UrlBase",
    "LdsOrdBase",
    "TagBase",
]

# Known secondary/embedded types
SECONDARY_TYPES = [
    "EventRef",
    "ChildRef",
    "FamilyRel",
    "PersonRef",
    "PlaceRef",
    "LdsOrd",
    "Address",
    "Attribute",
    "Url",
    "Name",
    "NameOrigin",
    "Surname",
    "Location",
    "MediaRef",
    "Tag",
    "RepoRef",
    "SourceRef",
]

# Known enum types
ENUM_TYPES = [
    "EventType",
    "EventRoleType",
    "ChildRefType",
    "Gender",
    "NameType",
    "NameFormat",
    "AttributeType",
    "UrlType",
    "NoteType",
    "MediaPathType",
    "SourceMediaType",
    "RepositoryType",
    "PlaceType",
    "LdsOrdType",
    "FamilyRelType",
    "DateQuality",
    "DateModifier",
]


def is_handle_ref(field_name: str, field_type: Any) -> bool:
    """Heuristic: check if a field looks like a handle reference."""
    if field_name.endswith("_handle"):
        return True
    if field_name.endswith("_list") and hasattr(field_type, "__origin__"):
        # Check if list element type is a Handle type
        args = getattr(field_type, "__args__", [])
        if args and str(args[0]).endswith("Handle"):
            return True
    return False


def resolve_target_type(field_name: str, field_type: Any) -> Optional[str]:
    """Given a handle-ref field, determine the target Gramps primary type.

    Heuristic: strip '_handle' or '_list' suffix and map to primary type name.
    """
    name = field_name
    if name.endswith("_handle"):
        stem = name[:-7]  # remove '_handle'
    elif name.endswith("_list"):
        stem = name[:-5]  # remove '_list'
    else:
        return None

    # Map common stems to Gramps primary types
    mapping = {
        "handle": None,  # self-referential handle, not a cross-ref
        "father": "Person",
        "mother": "Person",
        "child": "Person",
        "person": "Person",
        "family": "Family",
        "event": "Event",
        "place": "Place",
        "source": "Source",
        "citation": "Citation",
        "repository": "Repository",
        "media": "Media",
        "note": "Note",
        "tag": "Tag",
        "object": "Media",  # Gramps uses 'object' for media references
    }
    return mapping.get(stem)


def resolve_embedded_target_type(field_name: str) -> Optional[List[Dict[str, str]]]:
    """Given an embedded ref field name, determine the edge targets.

    Returns a list of {link, target} dicts, or None if not an embedded ref.
    """
    # Known embedded ref fields and their edge targets
    known_edges = {
        "event_ref_list": [{"link": "EventRef.ref", "target": "Event"}],
        "child_ref_list": [{"link": "ChildRef.ref", "target": "Person"}],
        "person_ref_list": [{"link": "PersonRef.ref", "target": "Person"}],
        "place_ref_list": [{"link": "PlaceRef.ref", "target": "Place"}],
        "media_list": [{"link": "MediaRef.ref", "target": "Media"}],
        "repo_ref_list": [{"link": "RepoRef.ref", "target": "Repository"}],
        "source_ref_list": [{"link": "SourceRef.ref", "target": "Source"}],
        "citation_ref_list": [{"link": "CitationRef.ref", "target": "Citation"}],
        "lds_ord_list": [],  # LDS ordinances are embedded, not cross-refs
        "attribute_list": [],  # Attributes are embedded, not cross-refs
        "address_list": [],  # Addresses are embedded, not cross-refs
        "url_list": [],  # URLs are embedded, not cross-refs
    }
    return known_edges.get(field_name)


def get_schema_via_introspection(cls: type) -> Dict[str, Any]:
    """Fallback schema extraction via class introspection.

    Calls get_schema() if available on the class, otherwise introspects
    the __init__ signature and type hints.
    """
    # Try get_schema() method first
    if hasattr(cls, "get_schema"):
        try:
            result = cls.get_schema()
            if isinstance(result, dict):
                return result
        except Exception:
            pass

    # Fallback: introspect __init__ params and type hints
    schema = {}
    try:
        hints = get_type_hints_safe(cls)
        for field_name, field_type in hints.items():
            schema[field_name] = describe_field(field_name, field_type, cls)
    except Exception:
        pass

    return schema


def get_type_hints_safe(cls: type) -> Dict[str, Any]:
    """Safely get type hints, returning empty dict on failure."""
    try:
        return typing_get_type_hints(cls)
    except Exception:
        return {}


def typing_get_type_hints(cls: type) -> Dict[str, Any]:
    """Wrapper around typing.get_type_hints that handles edge cases."""
    try:
        import typing

        return typing.get_type_hints(cls)
    except Exception:
        return {}


def describe_field(
    field_name: str, field_type: Any, cls: type
) -> Dict[str, Any]:
    """Describe a single field's type, required-ness, and cardinality."""
    desc: Dict[str, Any] = {"type": str(field_type) if field_type else "unknown"}

    # Determine if field is required (heuristic: not Optional, not has default)
    desc["required"] = not is_optional(field_type)

    # Handle array types
    origin = getattr(field_type, "__origin__", None)
    if origin is list or origin is List:
        desc["type"] = "array"
        args = getattr(field_type, "__args__", [])
        if args:
            desc["items"] = str(args[0])
        desc["cardinality"] = {"min": 0, "max": None}

        # Check if this is an embedded ref field
        edges = resolve_embedded_target_type(field_name)
        if edges is not None:
            desc["items"] = {
                "embedded": resolve_embedded_type_name(field_name),
                "edges": edges,
            }

    # Handle handle_ref fields
    if is_handle_ref(field_name, field_type):
        target = resolve_target_type(field_name, field_type)
        if target:
            desc["kind"] = "handle_ref"
            desc["target"] = target
        else:
            desc["kind"] = "handle"

    # Handle embedded types (single, not list)
    if field_name.endswith("_ref") and not field_name.endswith("_list"):
        embedded_name = resolve_embedded_type_name(field_name)
        if embedded_name:
            desc["type"] = "embedded"
            desc["schema"] = embedded_name

    return desc


def is_optional(field_type: Any) -> bool:
    """Check if a type is Optional[T]."""
    origin = getattr(field_type, "__origin__", None)
    if origin is Union:
        args = getattr(field_type, "__args__", [])
        return type(None) in args
    return False


def resolve_embedded_type_name(field_name: str) -> Optional[str]:
    """Map a field name to its embedded secondary type name."""
    # Strip trailing '_list' or '_ref' suffix
    name = field_name
    if name.endswith("_list"):
        name = name[:-5]
    elif name.endswith("_ref"):
        name = name[:-4]

    # Common mappings
    known = {
        "event_ref": "EventRef",
        "child_ref": "ChildRef",
        "person_ref": "PersonRef",
        "place_ref": "PlaceRef",
        "media_ref": "MediaRef",
        "repo_ref": "RepoRef",
        "source_ref": "SourceRef",
        "citation_ref": "CitationRef",
        "lds_ord": "LdsOrd",
        "attribute": "Attribute",
        "address": "Address",
        "url": "Url",
        "primary_name": "Name",
        "alternate_name": "Name",
        "name": "Name",
        "surname_list": "Surname",
        "location": "Location",
        "tag": "Tag",
        "family": "Family",
    }
    return known.get(name) or known.get(field_name)


def discover_mixins(cls: type) -> List[str]:
    """Discover mixin base classes of a Gramps primary type."""
    mixins = []
    for base in getattr(cls, "__mro__", []):
        base_name = getattr(base, "__name__", "")
        for suffix in MIXIN_SUFFIXES:
            if base_name == suffix:
                mixins.append(base_name)
    return mixins


def discover_enum_values(enum_cls: type) -> List[Any]:
    """Discover all values of a Gramps enum class.

    Handles both standard Python enums and Gramps-style integer-constant classes.
    """
    values = []

    # Standard Python Enum
    if isinstance(enum_cls, type) and issubclass(enum_cls, enum.Enum):
        for member in enum_cls:
            try:
                values.append(member.value)
            except Exception:
                values.append(member.name)
        return values

    # Gramps-style class with uppercase constants (e.g., EventType.BIRTH = 'Birth')
    for attr_name in dir(enum_cls):
        if attr_name.isupper() and not attr_name.startswith("_"):
            try:
                val = getattr(enum_cls, attr_name)
                if not callable(val) and not isinstance(val, (staticmethod, classmethod, property)):
                    values.append(val)
            except Exception:
                pass

    return values


def extract_primary_type(
    cls: type, cls_name: str
) -> Dict[str, Any]:
    """Extract schema for a single primary type class."""
    schema = get_schema_via_introspection(cls)
    mixins = discover_mixins(cls)

    result: Dict[str, Any] = {
        "fields": schema,
        "inherit_mixins": mixins,
    }
    return result


def extract_secondary_type(
    cls: type, cls_name: str
) -> Dict[str, Any]:
    """Extract schema for a secondary/embedded type class."""
    schema = get_schema_via_introspection(cls)

    result: Dict[str, Any] = {
        "fields": schema,
    }
    return result


def extract_enum_type(cls: type, cls_name: str) -> Dict[str, Any]:
    """Extract values for an enum type class."""
    values = discover_enum_values(cls)
    return {"values": values}


def try_import_gramps() -> Optional[types.ModuleType]:
    """Try to import gramps.gen.lib. Returns None if not available."""
    try:
        import gramps.gen.lib as gramps_lib

        return gramps_lib
    except ImportError:
        return None


def _is_json_schema_format(schema: Dict[str, Any]) -> bool:
    """Detect if the extracted schema is in JSON Schema format.

    JSON Schema format (Gramps 5.1 style) has fields like:
        "fields": { "type": "object", "title": "Person", "properties": {...} }

    Custom flat format (Gramps 5.2 style) has fields like:
        "fields": { "handle": {"type": "string", "kind": "handle"}, ... }

    The key indicator is whether a "properties" key exists inside the
    fields object of any primary or secondary type.
    """
    for type_category in ("primary_types", "secondary_types"):
        for type_name, type_info in schema.get(type_category, {}).items():
            if isinstance(type_info, dict):
                fields = type_info.get("fields", {})
                if isinstance(fields, dict) and "properties" in fields:
                    return True
    return False


def detect_gramps_version() -> Optional[str]:
    """Detect the Gramps version from the installed gramps package.

    Tries in order:
    1. import gramps.version and read VERSION or VERSION tuple
    2. Parse gramps/version.py if available

    Returns a major.minor string (e.g. "5.2") or None if detection fails.
    """
    try:
        import gramps.version as gv

        # Gramps has VERSION tuple like (5, 2, 0)
        if hasattr(gv, "VERSION"):
            v = gv.VERSION
            if isinstance(v, (tuple, list)) and len(v) >= 2:
                return f"{v[0]}.{v[1]}"
        # Some versions have VERSION as a string
        if hasattr(gv, "VERSION"):
            v = str(gv.VERSION)
            match = re.match(r"(\d+)\.(\d+)", v)
            if match:
                return f"{match.group(1)}.{match.group(2)}"
    except (ImportError, AttributeError):
        pass

    # Try reading gramps/version.py directly
    try:
        import importlib.util

        spec = importlib.util.find_spec("gramps.version")
        if spec and spec.origin:
            with open(spec.origin) as f:
                content = f.read()
            match = re.search(r"VERSION\s*=\s*\(?(\d+),\s*(\d+)", content)
            if match:
                return f"{match.group(1)}.{match.group(2)}"
    except Exception:
        pass

    return None


def extract_schema(gramps_lib: Any) -> Dict[str, Any]:
    """Extract schema from Gramps classes.

    If gramps_lib is None, returns an empty structure (caller should provide
    a mock or fallback).
    """
    schema: Dict[str, Any] = {
        "version": "5.2",
        "primary_types": {},
        "secondary_types": {},
        "enum_types": {},
    }

    if gramps_lib is None:
        return schema

    # Extract primary types
    for cls_name in PRIMARY_TYPES:
        cls = getattr(gramps_lib, cls_name, None)
        if cls is None or not isinstance(cls, type):
            continue
        schema["primary_types"][cls_name] = extract_primary_type(cls, cls_name)

    # Extract secondary types
    for cls_name in SECONDARY_TYPES:
        cls = getattr(gramps_lib, cls_name, None)
        if cls is None or not isinstance(cls, type):
            continue
        schema["secondary_types"][cls_name] = extract_secondary_type(cls, cls_name)

    # Extract enum types
    for cls_name in ENUM_TYPES:
        cls = getattr(gramps_lib, cls_name, None)
        if cls is None or not isinstance(cls, type):
            continue
        schema["enum_types"][cls_name] = extract_enum_type(cls, cls_name)

    return schema


def extract_enum_names(gramps_lib: Any) -> Dict[str, Dict[str, str]]:
    """Extract enum integer-to-name mappings from Gramps lib.

    Returns a dict like:
    {
        "EventType": {"0": "Custom", "1": "Marriage", ...},
        "EventRoleType": {"0": "Primary", ...},
        ...
    }
    """
    result: Dict[str, Dict[str, str]] = {}

    for cls_name in ENUM_TYPES:
        cls = getattr(gramps_lib, cls_name, None)
        if cls is None or not isinstance(cls, type):
            continue

        mapping: Dict[str, str] = {}

        # Standard Python Enum
        if issubclass(cls, enum.Enum):
            for member in cls:
                try:
                    val = member.value
                    name = member.name
                    mapping[str(val)] = name
                except Exception:
                    pass
        else:
            # Gramps-style class with uppercase constants
            for attr_name in dir(cls):
                if attr_name.isupper() and not attr_name.startswith("_"):
                    try:
                        val = getattr(cls, attr_name)
                        if not callable(val) and not isinstance(
                            val, (staticmethod, classmethod, property)
                        ):
                            mapping[str(val)] = attr_name
                    except Exception:
                        pass

        if mapping:
            result[cls_name] = mapping

    return result


def main():

    output_path = Path.cwd() / "schemas"
    output_path.mkdir(exist_ok=True, parents=True)
    output_filename = "schema-5.2.json"
    output_filepath = output_path / output_filename

    parser = argparse.ArgumentParser(
        description="Extract Gramps schema and emit schema-{version}.json"
    )
    parser.add_argument(
        "--version",
        default=None,
        help="Schema version string to embed in the output (default: auto-detect from Gramps source)",
    )
    parser.add_argument(
        "--mock",
        action="store_true",
        help="Use built-in mock classes instead of importing Gramps",
    )
    parser.add_argument(
        "--output",
        "-o",
        default=output_filepath,
        help="Output path for schema-{version}.json (default: schemas/schema-5.2.json)",
    )
    parser.add_argument(
        "--enum-names",
        action="store_true",
        help="Extract enum integer-to-name mappings instead of the full schema",
    )
    parser.add_argument(
        "--gramps-path",
        default=None,
        help="Path to Gramps source checkout (added to PYTHONPATH)",
    )
    args = parser.parse_args()

    # Add Gramps source to path if provided
    if args.gramps_path:
        sys.path.insert(0, os.path.abspath(args.gramps_path))

    # ---- Enum names mode ----
    if args.enum_names:
        if args.mock:
            print("Error: --enum-names requires a real Gramps import, not --mock", file=sys.stderr)
            sys.exit(1)

        gramps_lib = try_import_gramps()
        if gramps_lib is None:
            print(
                "Warning: gramps.gen.lib not found. "
                "Use --gramps-path to point to a Gramps source checkout.",
                file=sys.stderr,
            )
            sys.exit(1)

        enum_names = extract_enum_names(gramps_lib)

        output_path = args.output
        try:
            with open(output_path, "w") as f:
                json.dump(enum_names, f, indent=2, default=str)
        except (IOError, OSError) as e:
            print(f"Error: Cannot write to {output_path}: {e}", file=sys.stderr)
            sys.exit(1)

        print(f"Enum names written to {output_path}", file=sys.stderr)
        print(f"  Enum types: {len(enum_names)}", file=sys.stderr)
        for enum_name, mapping in sorted(enum_names.items()):
            print(f"    {enum_name}: {len(mapping)} values", file=sys.stderr)
        return

    # ---- Full schema extraction mode ----
    gramps_lib = None
    if not args.mock:
        gramps_lib = try_import_gramps()
        if gramps_lib is None:
            print(
                "Warning: gramps.gen.lib not found. "
                "Use --mock for built-in mock classes, or set PYTHONPATH.",
                file=sys.stderr,
            )
            sys.exit(1)
    else:
        # Import mock classes from the test module
        try:
            from test_extractor import MockGrampsLib

            gramps_lib = MockGrampsLib()
        except ImportError:
            print(
                "Error: --mock mode requires test_extractor.py with MockGrampsLib",
                file=sys.stderr,
            )
            sys.exit(1)

    schema = extract_schema(gramps_lib)

    # Resolve version: auto-detect from Gramps source, or use --version flag
    version = args.version
    if version is None:
        version = detect_gramps_version()
    if version:
        schema["version"] = version

    output_path = args.output
    try:
        with open(output_path, "w") as f:
            json.dump(schema, f, indent=2, default=str)
    except (IOError, OSError) as e:
        print(f"Error: Cannot write to {output_path}: {e}", file=sys.stderr)
        sys.exit(1)

    print(f"Schema written to {output_path}", file=sys.stderr)
    print(
        f"  Primary types: {len(schema.get('primary_types', {}))}",
        file=sys.stderr,
    )
    print(
        f"  Secondary types: {len(schema.get('secondary_types', {}))}",
        file=sys.stderr,
    )
    print(f"  Enum types: {len(schema.get('enum_types', {}))}", file=sys.stderr)

    # Detect JSON Schema format and warn users
    if _is_json_schema_format(schema):
        print(
            "\n⚠ Warning: This schema is in JSON Schema format (Gramps 5.1 style).",
            file=sys.stderr,
        )
        print(
            "  The build system (typed-graph/build.rs) automatically converts",
            file=sys.stderr,
        )
        print(
            "  JSON Schema format to the custom flat format at compile time.",
            file=sys.stderr,
        )
        print(
            "  No manual conversion is needed — the schema file can be used as-is.",
            file=sys.stderr,
        )


if __name__ == "__main__":
    main()
