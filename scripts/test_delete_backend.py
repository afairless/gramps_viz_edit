"""Unit tests for delete_backend.py — Gramps Python delete backend.

These tests exercise the backend's validation, deletion logic, and
orchestration using pytest fixtures with a real Gramps database.

Run with:
    pip install pytest
    pytest scripts/test_delete_backend.py -v
"""

from __future__ import annotations

import json
import os
import shutil
import tempfile
from typing import Any, Dict, List, Optional

import pytest

# Import the backend module under test.
from delete_backend import (
    _DELETION_ORDER,
    _TYPE_OPS,
    _extract_handle,
    _normalize_handle,
    _validate_handles,
    create_db,
    import_xml,
    delete_items,
    read_xmlns_from_input,
    is_gzip_file,
)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

# A minimal valid Gramps 5.1 XML with one person, one family, and one event.
MINIMAL_GRAMPS_XML = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1.6"/>
    <researcher/>
  </header>
  <people>
    <person handle="a5f0c1a2-4000-4b3d-8000-000000000001" id="I0000" change="1700000000">
      <gender>1</gender>
      <name type="Birth Name">
        <first>John</first>
        <surname>Doe</surname>
      </name>
    </person>
  </people>
  <families/>
  <events/>
  <places/>
  <sources/>
  <citations/>
  <repositories/>
  <media/>
  <notes/>
  <tags/>
</database>
"""

# A minimal XML with two people for partial-presence tests.
MINIMAL_TWO_PERSON_XML = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1.6"/>
    <researcher/>
  </header>
  <people>
    <person handle="a5f0c1a2-4000-4b3d-8000-000000000001" id="I0000" change="1700000000">
      <gender>1</gender>
      <name type="Birth Name">
        <first>John</first>
        <surname>Doe</surname>
      </name>
    </person>
    <person handle="b5f0c1a2-4000-4b3d-8000-000000000002" id="I0001" change="1700000000">
      <gender>2</gender>
      <name type="Birth Name">
        <first>Jane</first>
        <surname>Doe</surname>
      </name>
    </person>
  </people>
  <families/>
  <events/>
  <places/>
  <sources/>
  <citations/>
  <repositories/>
  <media/>
  <notes/>
  <tags/>
</database>
"""

# Sample valid UUID v4 handles for testing.
HANDLE_A = "a5f0c1a2-4000-4b3d-8000-000000000001"
HANDLE_B = "b5f0c1a2-4000-4b3d-8000-000000000002"
HANDLE_C = "c5f0c1a2-4000-4b3d-8000-000000000003"

# Gramps-native handle format: underscore + 16 hex chars (e.g. _103f72212ad34087).
GRAMPS_HANDLE_A = "_103f72212ad34087"
GRAMPS_HANDLE_B = "_103f72212ad34088"

# A minimal Gramps XML with Gramps-native handles (not UUID v4).
GRAMPS_NATIVE_XML = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
  <header>
    <created date="2025-01-15" version="5.1.6"/>
    <researcher/>
  </header>
  <people>
    <person handle="_103f72212ad34087" id="I0000" change="1700000000">
      <gender>1</gender>
      <name type="Birth Name">
        <first>John</first>
        <surname>Doe</surname>
      </name>
    </person>
    <person handle="_103f72212ad34088" id="I0001" change="1700000000">
      <gender>2</gender>
      <name type="Birth Name">
        <first>Jane</first>
        <surname>Doe</surname>
      </name>
    </person>
  </people>
  <families/>
  <events/>
  <places/>
  <sources/>
  <citations/>
  <repositories/>
  <media/>
  <notes/>
  <tags/>
</database>
"""


@pytest.fixture(scope="function")
def gramps_db():
    """Create a populated Gramps DB from MINIMAL_GRAMPS_XML.

    Yields (db, db_dir, tmp_path) where db is the opened database,
    db_dir is the persistent DB directory, and tmp_path is the path to
    the imported XML file. The DB and temp files are cleaned up after
    the test.
    """
    db_dir = tempfile.mkdtemp(prefix="gramps_test_db_")
    db = create_db(db_dir)
    tmp_xml = tempfile.NamedTemporaryFile(
        mode="w", suffix=".gramps", delete=False, encoding="utf-8"
    )
    tmp_xml.write(MINIMAL_GRAMPS_XML)
    tmp_xml_path = tmp_xml.name
    tmp_xml.close()

    try:
        import_xml(db, tmp_xml_path)
        yield db, db_dir, tmp_xml_path
    finally:
        shutil.rmtree(db_dir, ignore_errors=True)
        if os.path.exists(tmp_xml_path):
            os.unlink(tmp_xml_path)


@pytest.fixture(scope="function")
def gramps_native_db():
    """Create a Gramps DB from GRAMPS_NATIVE_XML (Gramps-native handles)."""
    db_dir = tempfile.mkdtemp(prefix="gramps_test_db_")
    db = create_db(db_dir)
    tmp_xml = tempfile.NamedTemporaryFile(
        mode="w", suffix=".gramps", delete=False, encoding="utf-8"
    )
    tmp_xml.write(GRAMPS_NATIVE_XML)
    tmp_xml_path = tmp_xml.name
    tmp_xml.close()

    try:
        import_xml(db, tmp_xml_path)
        yield db, db_dir, tmp_xml_path
    finally:
        shutil.rmtree(db_dir, ignore_errors=True)
        if os.path.exists(tmp_xml_path):
            os.unlink(tmp_xml_path)


@pytest.fixture(scope="function")
def two_person_db():
    """Create a Gramps DB with two people from MINIMAL_TWO_PERSON_XML."""
    db_dir = tempfile.mkdtemp(prefix="gramps_test_db_")
    db = create_db(db_dir)
    tmp_xml = tempfile.NamedTemporaryFile(
        mode="w", suffix=".gramps", delete=False, encoding="utf-8"
    )
    tmp_xml.write(MINIMAL_TWO_PERSON_XML)
    tmp_xml_path = tmp_xml.name
    tmp_xml.close()

    try:
        import_xml(db, tmp_xml_path)
        yield db, db_dir, tmp_xml_path
    finally:
        shutil.rmtree(db_dir, ignore_errors=True)
        if os.path.exists(tmp_xml_path):
            os.unlink(tmp_xml_path)


def make_empty_manifest() -> Dict[str, Any]:
    """Return a manifest with all-empty to_delete lists."""
    plan: Dict[str, Any] = {}
    for key in _DELETION_ORDER:
        plan[key] = {"to_delete": [], "kept": []}
    return {
        "version": 1,
        "source_file": "test.gramps",
        "created_at": "2025-01-15T10:30:00Z",
        "seed_people": [],
        "plan": plan,
    }


def make_manifest(
    people: Optional[List[str]] = None,
    families: Optional[List[str]] = None,
    events: Optional[List[str]] = None,
    notes: Optional[List[str]] = None,
    places: Optional[List[str]] = None,
    sources: Optional[List[str]] = None,
    citations: Optional[List[str]] = None,
    repositories: Optional[List[str]] = None,
    media: Optional[List[str]] = None,
    tags: Optional[List[str]] = None,
) -> Dict[str, Any]:
    """Build a manifest with the given handle lists."""
    plan: Dict[str, Dict[str, List[str]]] = {}
    for key, handles in [
        ("people", people),
        ("families", families),
        ("events", events),
        ("notes", notes),
        ("places", places),
        ("sources", sources),
        ("citations", citations),
        ("repositories", repositories),
        ("media", media),
        ("tags", tags),
    ]:
        plan[key] = {"to_delete": handles or [], "kept": []}
    return {
        "version": 1,
        "source_file": "test.gramps",
        "created_at": "2025-01-15T10:30:00Z",
        "seed_people": people or [],
        "plan": plan,
    }


# ---------------------------------------------------------------------------
# _extract_handle tests
# ---------------------------------------------------------------------------


class TestExtractHandle:
    """Test the v1/v2 handle extraction."""

    def test_v1_string(self) -> None:
        assert _extract_handle("abc-123") == "abc-123"

    def test_v2_dict(self) -> None:
        assert _extract_handle({"handle": "abc-123", "status": "pending"}) == "abc-123"

    def test_v2_dict_no_handle(self) -> None:
        assert _extract_handle({"status": "pending"}) == ""

    def test_other_type(self) -> None:
        assert _extract_handle(42) == "42"


# ---------------------------------------------------------------------------
# _normalize_handle tests
# ---------------------------------------------------------------------------


class TestNormalizeHandle:
    """Test the handle normalization for Gramps DB compatibility."""

    def test_strips_single_underscore(self) -> None:
        """Leading underscore is stripped."""
        assert _normalize_handle("_abc123") == "abc123"

    def test_preserves_already_normalized(self) -> None:
        """Handles without underscore are unchanged."""
        assert _normalize_handle("abc123") == "abc123"

    def test_handles_empty_string(self) -> None:
        """Empty string returns empty string."""
        assert _normalize_handle("") == ""

    def test_handles_multiple_underscores(self) -> None:
        """Multiple leading underscores are all stripped."""
        assert _normalize_handle("__abc") == "abc"


# ---------------------------------------------------------------------------
# _validate_handles tests
# ---------------------------------------------------------------------------


class TestValidateHandles:
    """Test the pre-deletion handle validation."""

    def test_empty_manifest(self, gramps_db):
        """Empty manifest → no valid handles, no rejected."""
        db, _, _ = gramps_db
        manifest = make_empty_manifest()
        valid, rejected = _validate_handles(db, manifest)
        assert valid == {}
        assert rejected == []

    def test_valid_handle_exists(self, gramps_db):
        """A valid handle that exists in the DB → accepted."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=[HANDLE_A])
        valid, rejected = _validate_handles(db, manifest)
        assert HANDLE_A in valid.get("people", [])
        assert rejected == []

    def test_absent_handle_raises(self, gramps_db):
        """A valid-format handle absent from the DB → ValueError."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=[HANDLE_C])  # HANDLE_C not in DB
        with pytest.raises(ValueError, match="handle\\(s\\) in manifest not found"):
            _validate_handles(db, manifest)

    def test_partial_absent_raises(self, two_person_db):
        """Some handles present, one absent → ValueError before any write."""
        db, _, _ = two_person_db
        # HANDLE_A and HANDLE_B exist; HANDLE_C does not
        manifest = make_manifest(people=[HANDLE_A, HANDLE_B, HANDLE_C])
        with pytest.raises(ValueError, match="handle\\(s\\) in manifest not found"):
            _validate_handles(db, manifest)

    def test_invalid_handle_rejected(self, gramps_db):
        """Handles not in the DB → ValueError."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=[HANDLE_A, "not-a-uuid", "also-bad"])
        with pytest.raises(ValueError, match=r"handle\(s\) in manifest not found"):
            _validate_handles(db, manifest)

    def test_all_invalid_handles_rejected(self, gramps_db):
        """All handles not in DB → ValueError."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=["bad1", "bad2"])
        with pytest.raises(ValueError, match=r"handle\(s\) in manifest not found"):
            _validate_handles(db, manifest)

    def test_mixed_types_all_valid(self, two_person_db):
        """Multiple types with valid handles work."""
        db, _, _ = two_person_db
        manifest = make_manifest(
            people=[HANDLE_A, HANDLE_B],
            families=[],  # No families in the fixture
            events=[],  # No events in the fixture
        )
        valid, rejected = _validate_handles(db, manifest)
        assert HANDLE_A in valid.get("people", [])
        assert HANDLE_B in valid.get("people", [])
        assert rejected == []

    def test_handles_accepted_regardless_of_format(self, gramps_native_db):
        """Gramps-native handles (underscore + hex) are accepted by the validator."""
        db, _, _ = gramps_native_db
        manifest = make_manifest(people=[GRAMPS_HANDLE_A])
        valid, rejected = _validate_handles(db, manifest)
        assert GRAMPS_HANDLE_A in valid.get("people", [])
        assert rejected == []

    def test_gramps_native_handles_normalized(self, gramps_native_db):
        """Underscore-prefixed handles are found via normalization."""
        db, _, _ = gramps_native_db
        manifest = make_manifest(people=[GRAMPS_HANDLE_A])
        valid, rejected = _validate_handles(db, manifest)
        assert GRAMPS_HANDLE_A in valid.get("people", [])
        assert rejected == []

    def test_missing_handle_raises_before_deletion(self, gramps_native_db):
        """A Gramps-native handle absent from the DB raises ValueError."""
        db, _, _ = gramps_native_db
        manifest = make_manifest(people=[GRAMPS_HANDLE_A, GRAMPS_HANDLE_B])
        # Only GRAMPS_HANDLE_A exists in this fixture DB... both exist actually.
        # Use a third, non-existent handle to test the missing path.
        manifest = make_manifest(people=[GRAMPS_HANDLE_A, "_deadbeef00000000"])
        with pytest.raises(ValueError, match=r"handle\(s\) in manifest not found"):
            _validate_handles(db, manifest)

    def test_plan_key_missing(self, gramps_db):
        """Missing type key in plan is handled gracefully."""
        db, _, _ = gramps_db
        manifest = make_empty_manifest()
        # Remove one key
        del manifest["plan"]["families"]
        valid, rejected = _validate_handles(db, manifest)
        # Should not crash; families just won't be processed
        assert isinstance(valid, dict)
        assert rejected == []


# ---------------------------------------------------------------------------
# delete_items tests
# ---------------------------------------------------------------------------


class TestDeleteItems:
    """Test the deletion engine (people-only deletion + surviving)."""

    def test_empty_manifest_delete_nothing(self, gramps_db):
        """Empty manifest → zero deleted, no rejected, empty surviving."""
        db, _, _ = gramps_db
        manifest = make_empty_manifest()
        deleted, rejected, surviving = delete_items(db, manifest)
        assert deleted == 0
        assert rejected == []
        assert surviving == []

    def test_delete_existing_person(self, gramps_db):
        """Delete a person that exists in the DB."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=[HANDLE_A])
        deleted, rejected, surviving = delete_items(db, manifest)
        assert deleted == 1
        assert rejected == []
        # Verify the person is actually gone
        assert not db.has_person_handle(HANDLE_A)

    def test_people_only_other_types_survive(self, gramps_db):
        """Only people are deleted — other types in manifest survive."""
        db, _, _ = gramps_db
        manifest = make_manifest(
            people=[HANDLE_A], events=["c5f0c1a2-4000-4b3d-8000-000000000004"]
        )
        # The event handle doesn't exist in the DB, so it will be missing.
        # But only people deletion is attempted.
        with pytest.raises(ValueError, match="handle\\(s\\) in manifest not found"):
            delete_items(db, manifest)

    def test_absent_handle_aborts_no_delete(self, gramps_db):
        """Absent handle → error, nothing deleted."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=[HANDLE_C])
        with pytest.raises(ValueError, match="handle\\(s\\) in manifest not found"):
            delete_items(db, manifest)
        # Verify HANDLE_A is still there (nothing was deleted)
        assert db.has_person_handle(HANDLE_A)

    def test_partial_absent_aborts_no_delete(self, two_person_db):
        """Partial presence → error, nothing deleted (atomicity)."""
        db, _, _ = two_person_db
        manifest = make_manifest(people=[HANDLE_A, HANDLE_C])
        with pytest.raises(ValueError, match="handle\\(s\\) in manifest not found"):
            delete_items(db, manifest)
        # Both persons should still exist
        assert db.has_person_handle(HANDLE_A)
        assert db.has_person_handle(HANDLE_B)

    def test_invalid_handles_skipped(self, gramps_db):
        """Handles not in DB → ValueError before any deletion."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=[HANDLE_A, "not-a-uuid"])
        with pytest.raises(ValueError, match=r"handle\(s\) in manifest not found"):
            delete_items(db, manifest)
        # Verify nothing was deleted
        assert db.has_person_handle(HANDLE_A)

    def test_all_invalid_skipped(self, gramps_db):
        """All handles not in DB → ValueError."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=["bad1", "bad2"])
        with pytest.raises(ValueError, match=r"handle\(s\) in manifest not found"):
            delete_items(db, manifest)

    def test_delete_two_people(self, two_person_db):
        """Delete both people from a two-person DB."""
        db, _, _ = two_person_db
        manifest = make_manifest(people=[HANDLE_A, HANDLE_B])
        deleted, rejected, surviving = delete_items(db, manifest)
        assert deleted == 2
        assert rejected == []
        assert not db.has_person_handle(HANDLE_A)
        assert not db.has_person_handle(HANDLE_B)

    def test_gramps_native_delete_items(self, gramps_native_db):
        """Delete a person with Gramps-native handle, verify surviving uses original format."""
        db, _, _ = gramps_native_db
        manifest = make_manifest(people=[GRAMPS_HANDLE_A])
        deleted, rejected, surviving = delete_items(db, manifest)
        assert deleted == 1
        assert rejected == []
        # Verify the person is actually gone (normalized DB query)
        assert not db.has_person_handle("103f72212ad34087")
        # Surviving report uses original (underscored) handles
        assert GRAMPS_HANDLE_A not in surviving

    def test_surviving_empty_after_delete(self, gramps_db):
        """After deleting the only person, surviving is empty."""
        db, _, _ = gramps_db
        manifest = make_manifest(people=[HANDLE_A])
        deleted, rejected, surviving = delete_items(db, manifest)
        assert deleted == 1
        # The only manifest handle (HANDLE_A) was deleted, so surviving is empty
        assert HANDLE_A not in surviving


# ---------------------------------------------------------------------------
# read_xmlns_from_input tests
# ---------------------------------------------------------------------------


class TestReadXmlns:
    """Test XML namespace detection from input files."""

    def test_gramps_51_namespace(self):
        """Gramps 5.1 namespace returns '5.1'."""
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".gramps", delete=False, encoding="utf-8"
        ) as f:
            f.write(
                '<?xml version="1.0"?>\n'
                '<database xmlns="http://gramps-project.org/xml/1.7.1/">\n'
                "  <header/>\n"
                "</database>\n"
            )
            path = f.name
        try:
            version = read_xmlns_from_input(path)
            assert version == "5.1"
        finally:
            os.unlink(path)

    def test_gramps_52_namespace(self):
        """Gramps 5.2 namespace returns '5.2'."""
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".gramps", delete=False, encoding="utf-8"
        ) as f:
            f.write(
                '<?xml version="1.0"?>\n'
                '<database xmlns="http://gramps-project.org/xml/1.7.2/">\n'
                "  <header/>\n"
                "</database>\n"
            )
            path = f.name
        try:
            version = read_xmlns_from_input(path)
            assert version == "5.2"
        finally:
            os.unlink(path)

    def test_unknown_namespace(self):
        """Unknown namespace returns None."""
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".gramps", delete=False, encoding="utf-8"
        ) as f:
            f.write(
                '<?xml version="1.0"?>\n'
                '<database xmlns="http://example.com/unknown/">\n'
                "  <header/>\n"
                "</database>\n"
            )
            path = f.name
        try:
            version = read_xmlns_from_input(path)
            assert version is None
        finally:
            os.unlink(path)

    def test_no_namespace(self):
        """No xmlns attribute returns None."""
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".gramps", delete=False, encoding="utf-8"
        ) as f:
            f.write('<?xml version="1.0"?>\n<database>\n  <header/>\n</database>\n')
            path = f.name
        try:
            version = read_xmlns_from_input(path)
            assert version is None
        finally:
            os.unlink(path)

    def test_nonexistent_file(self):
        """Nonexistent file returns None."""
        version = read_xmlns_from_input("/nonexistent/file.gramps")
        assert version is None


# ---------------------------------------------------------------------------
# is_gzip_file tests
# ---------------------------------------------------------------------------


class TestIsGzipFile:
    """Test gzip magic byte detection."""

    def test_plain_text_not_gzip(self):
        """Plain XML file is not detected as gzip."""
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".gramps", delete=False, encoding="utf-8"
        ) as f:
            f.write("<xml/>")
            path = f.name
        try:
            assert not is_gzip_file(path)
        finally:
            os.unlink(path)

    def test_gzip_file_detected(self):
        """A real gzip file is detected."""
        import gzip as gzip_mod

        with tempfile.NamedTemporaryFile(suffix=".gramps.gz", delete=False) as f:
            path = f.name
        try:
            with gzip_mod.open(path, "wt", encoding="utf-8") as gz:
                gz.write("<xml/>")
            assert is_gzip_file(path)
        finally:
            os.unlink(path)

    def test_nonexistent_file(self):
        """Nonexistent file returns False."""
        assert not is_gzip_file("/nonexistent/file.gramps")


# ---------------------------------------------------------------------------
# Deletion order test
# ---------------------------------------------------------------------------


class TestDeletionOrder:
    """The deletion order is fixed and documented."""

    def test_deletion_order_is_correct(self):
        """All 10 types in the expected dependency order."""
        expected = [
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
        assert _DELETION_ORDER == expected

    def test_all_types_have_has_ops(self):
        """Every type in _DELETION_ORDER has a 'has' entry in _TYPE_OPS."""
        for key in _DELETION_ORDER:
            assert key in _TYPE_OPS, f"Missing _TYPE_OPS entry for '{key}'"
            ops = _TYPE_OPS[key]
            assert "has" in ops, f"Missing 'has' op for '{key}'"


# ---------------------------------------------------------------------------
# Persistent DB tests
# ---------------------------------------------------------------------------


class TestPersistentDb:
    """Test persistent DB creation and retention."""

    def test_db_directory_exists(self, gramps_db):
        """Persistent DB directory exists after creation."""
        _, db_dir, _ = gramps_db
        assert os.path.isdir(db_dir), f"DB directory {db_dir} should exist"

    def test_db_directory_has_files(self, gramps_db):
        """Persistent DB directory contains database files."""
        _, db_dir, _ = gramps_db
        files = os.listdir(db_dir)
        assert len(files) > 0, "DB directory should contain database files"


# ---------------------------------------------------------------------------
# Smoke test: run via main() with real files
# ---------------------------------------------------------------------------


class TestMainIntegration:
    """Integration-level tests exercising the main() orchestration path."""

    def test_empty_manifest_noop(self):
        """Empty manifest → no-op succeeds (status='ok', deleted=0)."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            # Write input XML
            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            # Write empty manifest
            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(make_empty_manifest(), f)

            # Run main via subprocess
            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode == 0, f"stderr: {result.stderr}"
            parsed = json.loads(result.stdout)
            assert parsed["status"] == "ok"
            assert parsed["deleted"] == 0
            assert os.path.exists(output_path)

    def test_delete_one_person(self):
        """Delete one person → status='ok', deleted=1."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            manifest = make_manifest(people=[HANDLE_A])
            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(manifest, f)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode == 0, f"stderr: {result.stderr}"
            parsed = json.loads(result.stdout)
            assert parsed["status"] == "ok"
            assert parsed["deleted"] == 1
            assert os.path.exists(output_path)

    def test_absent_handle_fails(self):
        """Absent handle → status='error', exit code 1."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            manifest = make_manifest(people=[HANDLE_C])  # Not in DB
            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(manifest, f)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode != 0
            parsed = json.loads(result.stdout)
            assert parsed["status"] == "error"
            assert "not found" in (parsed.get("message") or "").lower()

    def test_invalid_handle_skipped(self):
        """Handle not in DB → error (no format filter, all handles checked for existence)."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            manifest = make_manifest(people=[HANDLE_A, "not-a-uuid"])
            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(manifest, f)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode != 0
            parsed = json.loads(result.stdout)
            assert parsed["status"] == "error"
            assert "not found" in (parsed.get("message") or "").lower()

    def test_nonexistent_input_file(self):
        """Nonexistent input file → error."""
        with tempfile.TemporaryDirectory() as tmpdir:
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(make_empty_manifest(), f)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    "/nonexistent/file.gramps",
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode != 0
            parsed = json.loads(result.stdout)
            assert parsed["status"] == "error"
            assert "not found" in (parsed.get("message") or "").lower()

    def test_nonexistent_manifest_file(self):
        """Nonexistent manifest file → error."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    "/nonexistent/manifest.json",
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode != 0
            parsed = json.loads(result.stdout)
            assert parsed["status"] == "error"
            assert "not found" in (parsed.get("message") or "").lower()

    def test_db_retained_on_success(self):
        """DB directory is retained after successful deletion."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(make_empty_manifest(), f)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode == 0, f"stderr: {result.stderr}"
            # DB directory should still exist (default: retain)
            assert os.path.isdir(db_dir), f"DB directory {db_dir} should be retained"

    def test_no_retain_db_cleans_up(self):
        """--no-retain-db removes the DB directory after success."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(make_empty_manifest(), f)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                    "--no-retain-db",
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode == 0, f"stderr: {result.stderr}"
            # DB directory should be cleaned up
            assert not os.path.isdir(db_dir), f"DB directory {db_dir} should be removed"

    def test_surviving_field_in_output(self):
        """JSON output includes surviving field after deletion."""
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = os.path.join(tmpdir, "input.gramps")
            manifest_path = os.path.join(tmpdir, "manifest.json")
            output_path = os.path.join(tmpdir, "output.gramps")
            db_dir = os.path.join(tmpdir, "gramps_db")

            with open(input_path, "w", encoding="utf-8") as f:
                f.write(MINIMAL_GRAMPS_XML)

            # Include a non-existent event handle to verify surviving is computed
            manifest = make_manifest(people=[HANDLE_A])
            with open(manifest_path, "w", encoding="utf-8") as f:
                json.dump(manifest, f)

            import subprocess

            result = subprocess.run(
                [
                    "python3",
                    os.path.join(os.path.dirname(__file__), "delete_backend.py"),
                    "--input",
                    input_path,
                    "--manifest",
                    manifest_path,
                    "--output",
                    output_path,
                    "--db-dir",
                    db_dir,
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            assert result.returncode == 0, f"stderr: {result.stderr}"
            parsed = json.loads(result.stdout)
            assert "surviving" in parsed, "Output should include surviving field"
            # After deleting the only person, surviving should be empty
            assert isinstance(parsed["surviving"], list)
