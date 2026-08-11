#!/usr/bin/env python3
"""Gramps-gen delete backend — headless Gramps DB import/delete/export.

Delegates XML I/O for the delete command to Gramps' own libraries.
The Rust cascade engine computes the deletion set; this script handles
all Gramps database operations and XML I/O.

Usage:
    python3 delete_backend.py \\
        --input in.gramps \\
        --manifest plan.json \\
        --output out.gramps

This script requires Gramps 5.1/5.2 Python libraries to be importable.
"""

from __future__ import annotations

import argparse
import gzip
import json
import os
import re
import shutil
import sys
import tempfile
from typing import Any, Dict, List, Optional

# GTK suppression — must come before any gramps import.
# The import/export plugin machinery pulls in gramps.gui, which triggers
# PyGIWarning if Gtk isn't version-locked. This script never opens a window.
import gi

gi.require_version("Gtk", "3.0")

# ---------------------------------------------------------------------------
# UUID v4 regex for handle validation
# ---------------------------------------------------------------------------
UUID_V4_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    re.IGNORECASE,
)


def detect_version() -> str:
    """Return the installed Gramps version string (e.g. '5.1.6')."""
    import gramps.gen.const as const

    return const.VERSION


def select_backend_plugin_id() -> str:
    """Return the Gramps DB backend plugin id for the installed version.

    Gramps 5.1/5.2 use 'bsddb' (Berkeley DB). Gramps 6.x moved away
    from bsddb. This function detects the installed version and returns
    the appropriate backend id.
    """
    version = detect_version()
    major_minor = ".".join(version.split(".")[:2])
    if major_minor in ("5.1", "5.2"):
        return "bsddb"
    else:
        # 6.x and later — the backend changed; we don't yet support it.
        sys.stderr.write(
            f"ERROR: Gramps version {version} ({major_minor}) is not supported. "
            f"This script requires Gramps 5.1 or 5.2.\n"
        )
        sys.exit(1)
        return ""  # unreachable


def create_temp_db() -> Any:
    """Create an empty, initialized Gramps database in a temporary directory.

    Returns the database object. The caller is responsible for cleaning
    up the temporary directory.

    The init sequence (validated against Gramps 5.1.6):
    1. make_database(backend_plugin_id) → uninitialized DB
    2. db.load(tmpdir)  — creates new DB files if person.db doesn't exist,
       or opens an existing DB. This is the standard Gramps init path.
    """
    from gramps.gen.db.utils import make_database

    backend_id = select_backend_plugin_id()

    db = make_database(backend_id)
    if db is None:
        sys.stderr.write(
            f"ERROR: Failed to create database with backend '{backend_id}'\n"
        )
        sys.exit(1)

    # Create a temporary directory for the Berkeley DB files.
    tmpdir = tempfile.mkdtemp(prefix="gramps_delete_")

    try:
        db.load(tmpdir)
    except Exception as exc:
        shutil.rmtree(tmpdir, ignore_errors=True)
        sys.stderr.write(f"ERROR: Failed to initialise database: {exc}\n")
        sys.exit(1)

    # Store the temp dir path on the db object for cleanup.
    db._temp_dir = tmpdir
    return db


def is_gzip_file(path: str) -> bool:
    """Check if a file has gzip magic bytes (\\x1f\\x8b)."""
    try:
        with open(path, "rb") as f:
            magic = f.read(2)
        return magic == b"\x1f\x8b"
    except OSError:
        return False


def import_xml(db: Any, path: str) -> None:
    """Import a .gramps file into a Gramps database.

    Handles both plain XML and gzip-compressed files. Gramps' importxml
    module handles gzip natively when the filename ends in '.gz', so
    if the input is gzip-compressed we rename a copy.
    """
    from gramps.cli.user import User
    from gramps.plugins.importer import importxml

    if is_gzip_file(path):
        # Gramps importxml handles .gz files natively.
        # Copy to a temp file ending in .gz so the importer detects it.
        tmp_gz = path + ".gramps-import.gz"
        shutil.copy2(path, tmp_gz)
        try:
            importxml.importData(db, tmp_gz, User())
        finally:
            if os.path.exists(tmp_gz) and tmp_gz != path:
                os.unlink(tmp_gz)
    else:
        importxml.importData(db, path, User())


def cleanup_db(db: Any) -> None:
    """Remove the temporary Berkeley DB directory.

    Uses shutil.rmtree with ignore_errors=True because Berkeley DB may
    hold file locks on some platforms, preventing clean removal.
    """
    temp_dir: Optional[str] = getattr(db, "_temp_dir", None)
    if temp_dir and os.path.isdir(temp_dir):
        try:
            shutil.rmtree(temp_dir, ignore_errors=True)
        except Exception:
            # Berkeley DB may hold file locks; suppress cleanup errors
            # so they don't mask the actual operation result.
            pass


# ---------------------------------------------------------------------------
# Deletion engine
# ---------------------------------------------------------------------------

# Deletion order respects the dependency chain. People must go first because
# delete_person_from_database handles family ref cleanup internally. All other
# types are independent of each other but must follow people.
_DELETION_ORDER: List[str] = [
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
]

# Map manifest type keys to Gramps DB existence-check and delete methods.
_TYPE_OPS: Dict[str, Dict[str, str]] = {
    "people":       {"has": "has_person_handle",     "delete": "delete_person_from_database", "get": "get_person_from_handle"},
    "families":     {"has": "has_family_handle",     "delete": "remove_family"},
    "events":       {"has": "has_event_handle",      "delete": "remove_event"},
    "notes":        {"has": "has_note_handle",       "delete": "remove_note"},
    "places":       {"has": "has_place_handle",      "delete": "remove_place"},
    "sources":      {"has": "has_source_handle",     "delete": "remove_source"},
    "citations":    {"has": "has_citation_handle",   "delete": "remove_citation"},
    "repositories": {"has": "has_repository_handle", "delete": "remove_repository"},
    "media":        {"has": "has_media_handle",      "delete": "remove_media"},
    "tags":         {"has": "has_tag_handle",        "delete": "remove_tag"},
}


def _extract_handle(entry: Any) -> str:
    """Extract a handle string from a v1 (string) or v2 (dict) manifest entry.

    v1 format: "handle-string"
    v2 format: {"handle": "handle-string", "status": "..."}
    """
    if isinstance(entry, str):
        return entry
    if isinstance(entry, dict):
        return entry.get("handle", "")
    return str(entry)


def _validate_handles(
    db: Any,
    manifest: Dict[str, Any],
) -> tuple[Dict[str, List[str]], List[str]]:
    """Validate all handles in the manifest.

    Returns (valid_handles, rejected):
    - valid_handles: {type_key: [handle, ...]} for handles that pass UUID
      validation AND exist in the database.
    - rejected: handles that don't match UUID v4 format.

    Raises ValueError if any valid-format handle is absent from the DB.
    This abort-before-any-deletion rule prevents partial writes.
    """
    plan: Dict[str, Any] = manifest.get("plan", {})
    rejected: List[str] = []
    valid: Dict[str, List[str]] = {}
    missing: List[str] = []

    for type_key in _DELETION_ORDER:
        type_plan = plan.get(type_key)
        if type_plan is None:
            continue
        to_delete: List[Any] = type_plan.get("to_delete", [])
        if not to_delete:
            continue

        type_valid: List[str] = []
        ops = _TYPE_OPS.get(type_key)
        if ops is None:
            continue
        has_fn = getattr(db, ops["has"])

        for entry in to_delete:
            handle = _extract_handle(entry)
            if not UUID_V4_RE.match(handle):
                rejected.append(handle)
                continue
            if not has_fn(handle):
                missing.append(handle)
                continue
            type_valid.append(handle)

        if type_valid:
            valid[type_key] = type_valid

    if missing:
        raise ValueError(
            f"{len(missing)} handle(s) in manifest not found in database "
            f"(first 5): {missing[:5]}"
        )

    return valid, rejected


def delete_items(db: Any, manifest: Dict[str, Any]) -> tuple[int, List[str]]:
    """Delete items from the database per the manifest, in dependency order.

    All deletions run inside a single DbTxn for atomicity. Handles are
    validated before any deletion begins.

    Returns (deleted_count, rejected):
    - deleted_count: total number of handles successfully deleted.
    - rejected: handles with invalid UUID v4 format (skipped).
    """
    from gramps.gen.db import DbTxn

    # Pre-deletion handle validation.
    valid, rejected = _validate_handles(db, manifest)

    if not valid:
        return 0, rejected

    deleted_count = 0

    with DbTxn("gramps-gen delete", db) as trans:
        for type_key in _DELETION_ORDER:
            handles = valid.get(type_key)
            if not handles:
                continue

            ops = _TYPE_OPS.get(type_key)
            if ops is None:
                continue

            delete_fn = getattr(db, ops["delete"])

            for handle in handles:
                if type_key == "people":
                    # delete_person_from_database takes a Person object,
                    # not a handle.
                    get_fn = getattr(db, ops["get"])
                    person = get_fn(handle)
                    delete_fn(person, trans)
                else:
                    delete_fn(handle, trans)

                deleted_count += 1

    return deleted_count, rejected


# ---------------------------------------------------------------------------
# XML namespace detection
# ---------------------------------------------------------------------------

# Known Gramps XML namespaces and their versions.
_KNOWN_XMLNS: Dict[str, str] = {
    "http://gramps-project.org/xml/1.7.1/": "5.1",
    "http://gramps-project.org/xml/1.7.2/": "5.2",
}


def read_xmlns_from_input(input_path: str) -> Optional[str]:
    """Read the xmlns from the input XML header.

    Returns the Gramps version string (e.g. '5.1') or None if
    the namespace is unrecognized.

    Uses a streaming approach: reads the first KB and looks for
    the xmlns attribute on the <database> element.
    """
    try:
        opener = gzip.open if is_gzip_file(input_path) else open
        with opener(input_path, "rt", encoding="utf-8") as f:
            chunk = f.read(4096)
    except OSError:
        return None

    match = re.search(r'xmlns="([^"]+)"', chunk)
    if match is None:
        return None
    xmlns = match.group(1)
    return _KNOWN_XMLNS.get(xmlns)


# ---------------------------------------------------------------------------
# Export
# ---------------------------------------------------------------------------

def export_xml(db: Any, path: str) -> None:
    """Export the database to a Gramps XML file.

    Gzip-compresses the output if the filename ends in '.gz'.
    Uses Gramps' own XmlWriter for the highest fidelity output.
    """
    from gramps.cli.user import User
    from gramps.plugins.export.exportxml import XmlWriter

    should_compress = path.endswith(".gz")
    writer = XmlWriter(db, User(), strip_photos=0, compress=1 if should_compress else 0)
    writer.write(path)


# ---------------------------------------------------------------------------
# Smoke test
# ---------------------------------------------------------------------------
def _smoke_test() -> None:
    """Minimal smoke test: create DB, import empty XML, clean up."""
    import time

    print("Smoke test: creating temp DB...", file=sys.stderr)
    db = create_temp_db()
    try:
        print(f"  DB dir: {db._temp_dir}", file=sys.stderr)
        print(f"  Version: {detect_version()}", file=sys.stderr)

        # Create a minimal valid Gramps XML for import testing
        minimal_xml = (
            '<?xml version="1.0" encoding="UTF-8"?>\n'
            '<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"\n'
            '"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">\n'
            '<database xmlns="http://gramps-project.org/xml/1.7.1/">\n'
            "  <header>\n"
            "    <created date=\"2025-01-15\" version=\"5.1.6\"/>\n"
            "    <researcher/>\n"
            "  </header>\n"
            "  <people/>\n"
            "  <families/>\n"
            "  <events/>\n"
            "  <places/>\n"
            "  <sources/>\n"
            "  <citations/>\n"
            "  <repositories/>\n"
            "  <media/>\n"
            "  <notes/>\n"
            "  <tags/>\n"
            "</database>\n"
        )

        with tempfile.NamedTemporaryFile(
            mode="w",
            suffix=".gramps",
            delete=False,
            encoding="utf-8",
        ) as tmp:
            tmp.write(minimal_xml)
            tmp_path = tmp.name

        try:
            print("Smoke test: importing minimal XML...", file=sys.stderr)
            import_xml(db, tmp_path)
            print("Smoke test: PASSED", file=sys.stderr)
        finally:
            os.unlink(tmp_path)
    finally:
        cleanup_db(db)


def main() -> None:
    """Parse args, orchestrate import/delete/export, print JSON result."""
    parser = argparse.ArgumentParser(
        description="Gramps-gen delete backend",
    )
    parser.add_argument(
        "--input",
        required=True,
        help="Input .gramps file (plain or .gz)",
    )
    parser.add_argument(
        "--manifest",
        required=True,
        help="JSON manifest with deletion plan",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Output .gramps file",
    )

    args = parser.parse_args()

    # Validate input files exist.
    if not os.path.exists(args.input):
        result = {
            "status": "error",
            "message": f"Input file not found: {args.input}",
            "output": None,
            "deleted": None,
            "rejected": [],
        }
        json.dump(result, sys.stdout)
        sys.stdout.flush()
        sys.exit(1)

    if not os.path.exists(args.manifest):
        result = {
            "status": "error",
            "message": f"Manifest file not found: {args.manifest}",
            "output": None,
            "deleted": None,
            "rejected": [],
        }
        json.dump(result, sys.stdout)
        sys.stdout.flush()
        sys.exit(1)

    # Read the manifest.
    try:
        with open(args.manifest, "r", encoding="utf-8") as f:
            manifest = json.load(f)
    except (OSError, json.JSONDecodeError) as exc:
        result = {
            "status": "error",
            "message": f"Failed to read manifest: {exc}",
            "output": None,
            "deleted": None,
            "rejected": [],
        }
        json.dump(result, sys.stdout)
        sys.stdout.flush()
        sys.exit(1)

    db = None
    try:
        # 1. Create temp DB and import the input file.
        db = create_temp_db()
        import_xml(db, args.input)

        # 2. Delete items per the manifest.
        deleted_count, rejected = delete_items(db, manifest)

        # 3. Export to a temp file, then atomically rename.
        #    (Deletion is already atomic via DbTxn; export is
        #    separate, so we use os.replace for atomic output.)
        tmp_output = args.output + ".tmp"
        try:
            export_xml(db, tmp_output)
            os.replace(tmp_output, args.output)
        finally:
            if os.path.exists(tmp_output):
                os.unlink(tmp_output)

        result = {
            "status": "ok",
            "output": args.output,
            "deleted": deleted_count,
            "message": None,
            "rejected": rejected,
        }
        json.dump(result, sys.stdout)
        sys.stdout.flush()

    except ValueError as exc:
        # Handle validation errors (missing handles, etc.)
        result = {
            "status": "error",
            "message": str(exc),
            "output": None,
            "deleted": None,
            "rejected": [],
        }
        json.dump(result, sys.stdout)
        sys.stdout.flush()
        sys.exit(1)
    except Exception as exc:
        result = {
            "status": "error",
            "message": str(exc),
            "output": None,
            "deleted": None,
            "rejected": [],
        }
        json.dump(result, sys.stdout)
        sys.stdout.flush()
        sys.exit(1)
    finally:
        if db is not None:
            cleanup_db(db)


if __name__ == "__main__":
    main()
