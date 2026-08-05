# Implementation Plan: Step 6 — Field-Level Compare Module

Source: `docs/research/gramps-diff-plan.md` (Step 6 only)

## Prerequisites

Steps 1–5 are complete:

- ✅ Step 1: Full XML-to-Graph parser
- ✅ Step 2: Diff crate skeleton (`DiffError`, `run_diff` stub, `Cargo.toml`)
- ✅ Step 3: Similarity module (`levenshtein`, `token_jaccard`, `tokenized_levenshtein`)
- ✅ Step 4: Normalization module (`case_fold`, `collapse_whitespace`, `strip_html_tags`, etc.)
- ✅ Step 5: Report types (`DiffReport`, `FieldChange`, `FieldKind`, `Classification`, etc.)

## Design

Each sub-step adds one or more `compare_xxx(a, b) -> Vec<FieldChange>` functions to
`crates/diff/src/compare.rs`. All functions delegate text fields to the `similarity`
module with normalization via the `normalize` module, use set comparison (unordered)
for `Vec<Handle>` arrays, and construct `FieldChange` values from the `report` module.

The shared field-comparison helpers (scaffold step) handle:

- **Text fields**: normalize both values, score via `tokenized_levenshtein`, produce `FieldChange { kind: Text, similarity }`.
- **Option fields**: `None` on both → skip; `Some`/`None` → `FieldChange` with similarity 0.0; `Some`/`Some` → delegate to type-specific comparator.
- **Handle-ref arrays**: unordered set comparison — reorder alone produces no change.
- **Date fields**: compare `DateValue` structs textually via `Display` → `FieldChange { kind: Date }`.
- **Enum fields**: compare discriminant; produce `FieldChange { kind: Enum }`.
- **Ref-struct arrays** (e.g. `EventRef`, `MediaRef`, `ChildRef`): compare by `ref_field` handle identity (unordered set); metadata changes produce separate `FieldChange` entries.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add compare module scaffold with shared field helpers` | Compare scaffold + helpers | `crates/diff/src/compare.rs` — module scaffold, `compare_field_text`, `compare_field_optional_text`, `compare_handle_array`, `compare_date_value`, `compare_enum_discriminant`, `compare_ref_array` helpers. `crates/diff/src/lib.rs` — add `pub mod compare;`. | Unit (text field: identical → empty, different → `FieldChange` with scores; optional: both `None` → skip, one `None` → 0.0; handle array: identical order → empty, reordered → empty, different → `FieldChange`; date: identical → empty, different → `FieldChange`; enum: same → empty, different → `FieldChange`; ref array: same refs → empty, reordered → empty) |
| 2 | `feat: add compare_person function` | Person compare | `crates/diff/src/compare.rs` — `compare_person(a: &PersonData, b: &PersonData) -> Vec<FieldChange>`. Compares all 17 fields including `primary_name` (nested `Name` with `Surname` list), `alternate_names`, `gender`, `event_ref_list`, `family_list`, `parent_family_list`, `person_ref_list`, `citation_list`, `note_list`, `media_list`, `tag_list`, `address_list`, `attribute_list`, `lds_ord_list`, `url_list`, `gramps_id`. | Unit (identical → empty; change surname → one `FieldChange`; change gender → `Enum` change; add alternate name → `FieldChange`; reorder `family_list` → empty; change `gramps_id` → `FieldChange`; empty PersonData vs empty → empty) |
| 3 | `feat: add compare_family and compare_event functions` | Family + Event compare | `crates/diff/src/compare.rs` — `compare_family(a: &FamilyData, b: &FamilyData) -> Vec<FieldChange>` and `compare_event(a: &EventData, b: &EventData) -> Vec<FieldChange>`. Family: `father_handle`, `mother_handle`, `child_ref_list`, `event_ref_list`, `attribute_list`, `citation_list`, `media_list`, `note_list`, `tag_list`, `gramps_id`. Event: `event_type`, `date`, `description`, `place_handle`, `attribute_list`, `citation_list`, `media_list`, `note_list`, `tag_list`, `gramps_id`. | Unit (identical → empty; change father handle → `HandleRef` change; reorder child_ref_list → empty; change event_type → `Enum` change; change date → `Date` change; change description → `Text` change with similarity score) |
| 4 | `feat: add compare_place, compare_source, compare_citation, compare_repository functions` | Place + Source + Citation + Repository compare | `crates/diff/src/compare.rs` — `compare_place(...)`, `compare_source(...)`, `compare_citation(...)`, `compare_repository(...)`. Place: `name` (Location), `place_ref_list`, `citation_list`, `media_list`, `note_list`, `tag_list`, `gramps_id`. Source: `title`, `author`, `pubinfo`, `reporef_list`, `attribute_list`, `media_list`, `note_list`, `tag_list`, `gramps_id`. Citation: `source_handle`, `page`, `confidence`, `media_list`, `note_list`, `tag_list`, `gramps_id`. Repository: `name`, `type_field`, `address_list`, `url_list`, `media_list`, `note_list`, `tag_list`, `gramps_id`. | Unit (identical → empty; change source title → `Text` change; change citation page → `Text` change; change repository type → `Enum` change; change place location street → nested `FieldChange`; reorder `reporef_list` → empty) |
| 5 | `feat: add compare_media, compare_note, and compare_tag functions` | Media + Note + Tag compare | `crates/diff/src/compare.rs` — `compare_media(...)`, `compare_note(...)`, `compare_tag(...)`. Media: `desc`, `path`, `mime_type`, `checksum`, `attribute_list`, `citation_list`, `note_list`, `tag_list`, `gramps_id`. Note: `text`, `format`, `type_field`, `citation_list`, `tag_list`, `gramps_id`. Tag: `name`, `color`, `priority`, `tag_list`, `gramps_id`. | Unit (identical → empty; change note text → `Text` change with similarity; change tag color → `Text` change (normalized); change tag name → `Text` change; change media desc → `Text` change; reorder tag_list → empty) |
