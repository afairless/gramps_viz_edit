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
# Smoke test (runs when invoked as __main__ with --smoke-test)
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

    # Stub: full orchestration comes in Step 3.
    # For now, validate the import path exists.
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

    # Validate manifest exists
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

    # For Step 1, just verify the import works.
    db = None
    try:
        db = create_temp_db()
        import_xml(db, args.input)
        # Smoke: export step deferred to Step 3
        result = {
            "status": "ok",
            "output": None,
            "deleted": 0,
            "message": "Import succeeded; deletions not yet implemented",
            "rejected": [],
        }
        json.dump(result, sys.stdout)
        sys.stdout.flush()
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
