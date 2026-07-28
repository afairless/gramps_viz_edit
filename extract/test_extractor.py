"""Mock Gramps classes for testing the schema extractor.

Provides MockGrampsLib that simulates gramps.gen.lib with synthetic
class hierarchies, enum types, and mixin bases matching the Gramps 5.2 model.
"""

from __future__ import annotations

import enum
import os
import sys
import unittest
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

# Ensure extract/ directory is on the path for importing extract_schema
_script_dir = os.path.dirname(os.path.abspath(__file__))
if _script_dir not in sys.path:
    sys.path.insert(0, _script_dir)


class EventType(enum.Enum):
    """Gramps event type enum."""
    BIRTH = "Birth"
    DEATH = "Death"
    MARRIAGE = "Marriage"
    DIVORCE = "Divorce"
    CENSUS = "Census"
    BURIAL = "Burial"
    ADOPTION = "Adoption"
    EMIGRATION = "Emigration"
    IMMIGRATION = "Immigration"
    NATURALIZATION = "Naturalization"
    RESIDENCE = "Residence"
    OCCUPATION = "Occupation"
    TITLE = "Title"
    EDUCATION = "Education"
    MILITARY_SERVICE = "Military Service"


class EventRoleType(enum.Enum):
    """Gramps event role type enum."""
    PRIMARY = "Primary"
    FAMILY = "Family"
    WITNESS = "Witness"
    CLERGY = "Clergy"
    BRIDE = "Bride"
    GROOM = "Groom"
    PARENT = "Parent"
    CHILD = "Child"
    OFFICIATOR = "Officiator"
    OTHER = "Other"


class ChildRefType(enum.Enum):
    """Gramps child reference type enum."""
    BIRTH = "Birth"
    ADOPTED = "Adopted"
    STEPCHILD = "Stepchild"
    FOSTER = "Foster"
    SPONSOR = "Sponsor"
    GODCHILD = "Godchild"
    CREATED = "Created"
    OTHER = "Other"


class Gender(enum.IntEnum):
    """Gramps gender enum (0-3)."""
    MALE = 0
    FEMALE = 1
    UNKNOWN = 2
    OTHER = 3


class NameType(enum.Enum):
    """Gramps name type enum."""
    BIRTH = "Birth"
    MARRIED = "Married"
    ALSO_KNOWN = "Also Known As"
    AKN = "Akn"
    CALLED = "Called"
    FORMAL = "Formal"
    PATRONYMIC = "Patronymic"
    RELIGIOUS = "Religious"


class DateQuality(enum.Enum):
    """Gramps date quality enum."""
    EXACT = "Exact"
    ESTIMATED = "Estimated"
    CALCULATED = "Calculated"


class DateModifier(enum.Enum):
    """Gramps date modifier enum."""
    NONE = "None"
    BEFORE = "Before"
    AFTER = "After"
    ABOUT = "About"
    RANGE = "Range"
    SPAN = "Span"


# ---- Mixin bases ----

class CitationBase:
    """Mixin: provides citation_list field."""
    pass


class NoteBase:
    """Mixin: provides note_list field."""
    pass


class MediaBase:
    """Mixin: provides media_list field."""
    pass


class AttributeBase:
    """Mixin: provides attribute_list field."""
    pass


class AddressBase:
    """Mixin: provides address_list field."""
    pass


class UrlBase:
    """Mixin: provides url_list field."""
    pass


class LdsOrdBase:
    """Mixin: provides lds_ord_list field."""
    pass


class TagBase:
    """Mixin: provides tag_list field."""
    pass


# ---- Primary types ----

class Person(CitationBase, NoteBase, MediaBase, AttributeBase, AddressBase, UrlBase, LdsOrdBase, TagBase):
    """Mock Gramps Person class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        gender: int = Gender.UNKNOWN,
        primary_name: Optional[dict] = None,
        alternate_names: Optional[List[dict]] = None,
        event_ref_list: Optional[List[dict]] = None,
        family_list: Optional[List[str]] = None,
        parent_family_list: Optional[List[str]] = None,
        person_ref_list: Optional[List[dict]] = None,
        media_list: Optional[List[dict]] = None,
        citation_list: Optional[List[str]] = None,
        note_list: Optional[List[str]] = None,
        tag_list: Optional[List[str]] = None,
        attribute_list: Optional[List[dict]] = None,
        address_list: Optional[List[dict]] = None,
        url_list: Optional[List[dict]] = None,
        lds_ord_list: Optional[List[dict]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.gender = gender
        self.primary_name = primary_name or {}
        self.alternate_names = alternate_names or []
        self.event_ref_list = event_ref_list or []
        self.family_list = family_list or []
        self.parent_family_list = parent_family_list or []
        self.person_ref_list = person_ref_list or []
        self.media_list = media_list or []
        self.citation_list = citation_list or []
        self.note_list = note_list or []
        self.tag_list = tag_list or []
        self.attribute_list = attribute_list or []
        self.address_list = address_list or []
        self.url_list = url_list or []
        self.lds_ord_list = lds_ord_list or []


class Family(CitationBase, NoteBase, MediaBase, AttributeBase, TagBase):
    """Mock Gramps Family class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        father_handle: Optional[str] = None,
        mother_handle: Optional[str] = None,
        child_ref_list: Optional[List[dict]] = None,
        event_ref_list: Optional[List[dict]] = None,
        media_list: Optional[List[dict]] = None,
        citation_list: Optional[List[str]] = None,
        note_list: Optional[List[str]] = None,
        tag_list: Optional[List[str]] = None,
        attribute_list: Optional[List[dict]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.father_handle = father_handle
        self.mother_handle = mother_handle
        self.child_ref_list = child_ref_list or []
        self.event_ref_list = event_ref_list or []
        self.media_list = media_list or []
        self.citation_list = citation_list or []
        self.note_list = note_list or []
        self.tag_list = tag_list or []
        self.attribute_list = attribute_list or []


class Event(CitationBase, NoteBase, MediaBase, AttributeBase, TagBase):
    """Mock Gramps Event class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        event_type: EventType = EventType.BIRTH,
        date: Optional[dict] = None,
        place_handle: Optional[str] = None,
        description: str = "",
        media_list: Optional[List[dict]] = None,
        citation_list: Optional[List[str]] = None,
        note_list: Optional[List[str]] = None,
        tag_list: Optional[List[str]] = None,
        attribute_list: Optional[List[dict]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.event_type = event_type
        self.date = date
        self.place_handle = place_handle
        self.description = description
        self.media_list = media_list or []
        self.citation_list = citation_list or []
        self.note_list = note_list or []
        self.tag_list = tag_list or []
        self.attribute_list = attribute_list or []


class Place(CitationBase, NoteBase, MediaBase, AttributeBase, TagBase):
    """Mock Gramps Place class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        name: Optional[dict] = None,
        place_ref_list: Optional[List[dict]] = None,
        media_list: Optional[List[dict]] = None,
        citation_list: Optional[List[str]] = None,
        note_list: Optional[List[str]] = None,
        tag_list: Optional[List[str]] = None,
        attribute_list: Optional[List[dict]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.name = name or {}
        self.place_ref_list = place_ref_list or []
        self.media_list = media_list or []
        self.citation_list = citation_list or []
        self.note_list = note_list or []
        self.tag_list = tag_list or []
        self.attribute_list = attribute_list or []


class RepoRefBase:
    """Mixin for repository references."""
    pass


class Source(CitationBase, NoteBase, MediaBase, AttributeBase, RepoRefBase, TagBase):
    """Mock Gramps Source class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        title: str = "",
        author: str = "",
        pubinfo: str = "",
        reporef_list: Optional[List[dict]] = None,
        note_list: Optional[List[str]] = None,
        media_list: Optional[List[dict]] = None,
        tag_list: Optional[List[str]] = None,
        attribute_list: Optional[List[dict]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.title = title
        self.author = author
        self.pubinfo = pubinfo
        self.reporef_list = reporef_list or []
        self.note_list = note_list or []
        self.media_list = media_list or []
        self.tag_list = tag_list or []
        self.attribute_list = attribute_list or []


class Citation(CitationBase, NoteBase, MediaBase, AttributeBase, TagBase):
    """Mock Gramps Citation class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        source_handle: Optional[str] = None,
        page: str = "",
        confidence: int = 2,
        note_list: Optional[List[str]] = None,
        media_list: Optional[List[dict]] = None,
        tag_list: Optional[List[str]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.source_handle = source_handle
        self.page = page
        self.confidence = confidence
        self.note_list = note_list or []
        self.media_list = media_list or []
        self.tag_list = tag_list or []


class Repository(CitationBase, NoteBase, MediaBase, AddressBase, UrlBase, TagBase):
    """Mock Gramps Repository class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        type: str = "Library",
        name: str = "",
        note_list: Optional[List[str]] = None,
        address_list: Optional[List[dict]] = None,
        url_list: Optional[List[dict]] = None,
        media_list: Optional[List[dict]] = None,
        tag_list: Optional[List[str]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.type = type
        self.name = name
        self.note_list = note_list or []
        self.address_list = address_list or []
        self.url_list = url_list or []
        self.media_list = media_list or []
        self.tag_list = tag_list or []


class Media(CitationBase, NoteBase, AttributeBase, TagBase):
    """Mock Gramps Media class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        path: str = "",
        mime_type: str = "image/jpeg",
        desc: str = "",
        checksum: str = "",
        citation_list: Optional[List[str]] = None,
        note_list: Optional[List[str]] = None,
        attribute_list: Optional[List[dict]] = None,
        tag_list: Optional[List[str]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.path = path
        self.mime_type = mime_type
        self.desc = desc
        self.checksum = checksum
        self.citation_list = citation_list or []
        self.note_list = note_list or []
        self.attribute_list = attribute_list or []
        self.tag_list = tag_list or []


class Note(CitationBase, TagBase):
    """Mock Gramps Note class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        text: str = "",
        format: int = 0,
        type: str = "General",
        citation_list: Optional[List[str]] = None,
        tag_list: Optional[List[str]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.text = text
        self.format = format
        self.type = type
        self.citation_list = citation_list or []
        self.tag_list = tag_list or []


class Tag(CitationBase, NoteBase):
    """Mock Gramps Tag class."""
    def __init__(
        self,
        handle: str = "",
        gramps_id: str = "",
        name: str = "",
        color: str = "",
        priority: int = 0,
        tag_list: Optional[List[str]] = None,
    ):
        self.handle = handle
        self.gramps_id = gramps_id
        self.name = name
        self.color = color
        self.priority = priority
        self.tag_list = tag_list or []


# ---- Secondary/embedded types ----

class EventRef:
    """Mock Gramps EventRef class."""
    def __init__(self, ref: str = "", role: EventRoleType = EventRoleType.PRIMARY):
        self.ref = ref
        self.role = role


class ChildRef:
    """Mock Gramps ChildRef class."""
    def __init__(self, ref: str = "", relation: ChildRefType = ChildRefType.BIRTH):
        self.ref = ref
        self.relation = relation


class MockGrampsLib:
    """Namespace that simulates gramps.gen.lib with mock classes.

    Provides all PRIMARY_TYPES, SECONDARY_TYPES, and ENUM_TYPES as attributes.
    """

    # Primary types
    Person = Person
    Family = Family
    Event = Event
    Place = Place
    Source = Source
    Citation = Citation
    Repository = Repository
    Media = Media
    Note = Note
    Tag = Tag

    # Secondary types
    EventRef = EventRef
    ChildRef = ChildRef

    # Enum types
    EventType = EventType
    EventRoleType = EventRoleType
    ChildRefType = ChildRefType
    Gender = Gender
    NameType = NameType
    DateQuality = DateQuality
    DateModifier = DateModifier


# ---- Tests ----


class TestSchemaExtractor(unittest.TestCase):
    """Tests for the schema extraction functions."""

    def test_discover_mixins_person(self):
        """Person should discover all 8 mixin bases."""
        from extract_schema import discover_mixins

        mixins = discover_mixins(Person)
        expected = [
            "CitationBase",
            "NoteBase",
            "MediaBase",
            "AttributeBase",
            "AddressBase",
            "UrlBase",
            "LdsOrdBase",
            "TagBase",
        ]
        for m in expected:
            self.assertIn(m, mixins)
        self.assertEqual(len(mixins), 8)

    def test_discover_mixins_tag(self):
        """Tag should discover 2 mixin bases."""
        from extract_schema import discover_mixins

        mixins = discover_mixins(Tag)
        self.assertIn("CitationBase", mixins)
        self.assertIn("NoteBase", mixins)
        self.assertEqual(len(mixins), 2)

    def test_discover_enum_values_event_type(self):
        """EventType enum should have 15 values."""
        from extract_schema import discover_enum_values

        values = discover_enum_values(EventType)
        self.assertIn("Birth", values)
        self.assertIn("Death", values)
        self.assertIn("Marriage", values)
        self.assertEqual(len(values), 15)

    def test_discover_enum_values_gender(self):
        """Gender IntEnum should have 4 integer values."""
        from extract_schema import discover_enum_values

        values = discover_enum_values(Gender)
        self.assertIn(0, values)
        self.assertIn(1, values)
        self.assertIn(2, values)
        self.assertIn(3, values)
        self.assertEqual(len(values), 4)

    def test_extract_primary_type_person(self):
        """extract_primary_type should return fields and mixins."""
        from extract_schema import extract_primary_type

        result = extract_primary_type(Person, "Person")
        self.assertIn("fields", result)
        self.assertIn("inherit_mixins", result)
        self.assertEqual(len(result["inherit_mixins"]), 8)

    def test_extract_enum_type_event_type(self):
        """Enum type extraction should return values list."""
        from extract_schema import extract_enum_type

        result = extract_enum_type(EventType, "EventType")
        self.assertIn("values", result)
        self.assertIn("Birth", result["values"])

    def test_is_handle_ref(self):
        """is_handle_ref should detect handle refs by naming convention."""
        from extract_schema import is_handle_ref

        self.assertTrue(is_handle_ref("father_handle", str))
        self.assertTrue(is_handle_ref("mother_handle", str))
        self.assertTrue(is_handle_ref("place_handle", str))
        self.assertTrue(is_handle_ref("source_handle", str))
        self.assertFalse(is_handle_ref("handle", str))
        self.assertFalse(is_handle_ref("gramps_id", str))
        self.assertFalse(is_handle_ref("description", str))

    def test_resolve_target_type(self):
        """resolve_target_type should map handle field names to types."""
        from extract_schema import resolve_target_type

        self.assertEqual(resolve_target_type("father_handle", str), "Person")
        self.assertEqual(resolve_target_type("mother_handle", str), "Person")
        self.assertEqual(resolve_target_type("place_handle", str), "Place")
        self.assertEqual(resolve_target_type("source_handle", str), "Source")
        self.assertEqual(resolve_target_type("citation_handle", str), "Citation")
        self.assertIsNone(resolve_target_type("handle", str))

    def test_resolve_embedded_target_type(self):
        """resolve_embedded_target_type should return edge info."""
        from extract_schema import resolve_embedded_target_type

        event_edges = resolve_embedded_target_type("event_ref_list")
        self.assertIsNotNone(event_edges)
        if event_edges is not None:
            self.assertEqual(event_edges[0]["target"], "Event")

        child_edges = resolve_embedded_target_type("child_ref_list")
        self.assertIsNotNone(child_edges)
        if child_edges is not None:
            self.assertEqual(child_edges[0]["target"], "Person")

        self.assertIsNone(resolve_embedded_target_type("description"))

    def test_primary_types_count(self):
        """Extract schema with mock lib should find 10 primary types."""
        from extract_schema import extract_schema

        lib = MockGrampsLib()
        schema = extract_schema(lib)
        self.assertEqual(len(schema["primary_types"]), 10)
        for pt in ["Person", "Family", "Event", "Place", "Source", "Citation",
                   "Repository", "Media", "Note", "Tag"]:
            self.assertIn(pt, schema["primary_types"])

    def test_enum_types_count(self):
        """Extract schema with mock lib should find all enum types."""
        from extract_schema import extract_schema

        lib = MockGrampsLib()
        schema = extract_schema(lib)
        self.assertEqual(len(schema["enum_types"]), 7)
        for et in ["EventType", "EventRoleType", "ChildRefType", "Gender",
                   "NameType", "DateQuality", "DateModifier"]:
            self.assertIn(et, schema["enum_types"])


if __name__ == "__main__":
    unittest.main()
