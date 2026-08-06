//! Pass 1 matcher — exact handle + fuzzy content matching between two graphs.
//!
//! This module implements the first pass of the diff pipeline, which
//! produces a handle map and per-item classifications.
//!
//! # Matching passes
//!
//! **Pass 1a** — exact handle match: items with the same handle in both
//! graphs are compared field-by-field and classified as [`Same`] or
//! [`Modified`][crate::report::Classification::Modified].
//!
//! **Pass 1b** — fuzzy content match: for items that could not be matched
//! by handle, a per-type strong match key is derived from the most
//! distinctive fields (e.g., surname + first_name + birth date for
//! persons). Unmatched items from graph B are indexed by their strong
//! key for O(1) lookup against unmatched items from graph A. Items
//! that match by strong key are compared and classified; items with
//! duplicate strong keys are flagged as
//! [`NeedsReview`][crate::report::Classification::NeedsReview].

use std::collections::HashMap;

use typed_graph::{
    event_type_eq, get_source_handle, graph::node_kind, CitationData, EventData, FamilyData, Graph,
    Handle, MediaData, Node, NodeKind, NoteData, PersonData, PlaceData, RepositoryData, SourceData,
    TagData,
};

use crate::compare::{
    compare_citation, compare_event, compare_family, compare_media, compare_note, compare_person,
    compare_place, compare_repository, compare_source, compare_tag,
};
use crate::normalize::case_fold;
use crate::report::{
    AmbiguousCase, AmbiguousContext, Candidate, Classification, FieldChange, ItemDiff,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The result of Pass 1 matching between two graphs.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchResult {
    /// Maps B-handles → A-handles for matched items.
    pub handle_map: HashMap<Handle, Handle>,
    /// Per-item diff results, one entry per item in the union of both graphs.
    pub item_diffs: Vec<ItemDiff>,
    /// Ambiguous cases that could not be resolved automatically.
    pub ambiguous_cases: Vec<AmbiguousCase>,
}

// ---------------------------------------------------------------------------
// Per-type match key logic
// ---------------------------------------------------------------------------

/// Build a normalized strong match key for a person.
///
/// Strong key: `surname | first_name | birth_date`
/// All fields are case-folded and whitespace-collapsed.
/// Returns `None` when all three fields are empty/missing.
fn person_strong_key(graph: &Graph, data: &PersonData) -> Option<String> {
    let surname = data
        .primary_name
        .surname_list
        .first()
        .and_then(|s| s.surname.as_deref())
        .unwrap_or("");
    let first_name = data.primary_name.first_name.as_deref().unwrap_or("");
    let birth_date = find_birth_date(graph, &data.event_ref_list)
        .map(|d| d.display_text())
        .unwrap_or_default();

    let key = normalize_key(&format!("{surname}|{first_name}|{birth_date}"));
    if key.is_empty() || key == "|" || key == "||" {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for a family.
///
/// Strong key: `father_handle | mother_handle`
/// Returns `None` when both handles are empty.
fn family_strong_key(data: &FamilyData) -> Option<String> {
    let father = data.father_handle.as_deref().unwrap_or("");
    let mother = data.mother_handle.as_deref().unwrap_or("");
    let key = normalize_key(&format!("{father}|{mother}"));
    if key.is_empty() || key == "|" {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for an event.
///
/// Strong key: `event_type | date | description`
/// Returns `None` when event_type is the default and date and description
/// are both empty.
fn event_strong_key(data: &EventData) -> Option<String> {
    let event_type = format!("{:?}", data.event_type);
    let date = data
        .date
        .as_ref()
        .map(|d| d.display_text())
        .unwrap_or_default();
    let desc = data.description.as_deref().unwrap_or("");
    let key = normalize_key(&format!("{event_type}|{date}|{desc}"));
    if key.is_empty() || key == "||" {
        // Event type is always set, so the key is never truly empty
        Some(key)
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for a place.
///
/// Strong key: `city | street`
/// Returns `None` when both fields are empty.
fn place_strong_key(data: &PlaceData) -> Option<String> {
    let city = data.name.city.as_deref().unwrap_or("");
    let street = data.name.street.as_deref().unwrap_or("");
    let key = normalize_key(&format!("{city}|{street}"));
    if key.is_empty() || key == "|" {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for a source.
///
/// Strong key: `title`
/// Returns `None` when title is empty.
fn source_strong_key(data: &SourceData) -> Option<String> {
    let key = normalize_key(&data.title);
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for a citation.
///
/// Strong key: `source_handle | page`
/// Returns `None` when both are empty.
fn citation_strong_key(data: &CitationData) -> Option<String> {
    let source = get_source_handle(&data.source_handle);
    let page = data.page.as_deref().unwrap_or("");
    let key = normalize_key(&format!("{source}|{page}"));
    if key.is_empty() || key == "|" {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for a repository.
///
/// Strong key: `name`
/// Returns `None` when name is empty.
fn repository_strong_key(data: &RepositoryData) -> Option<String> {
    let key = normalize_key(data.name.as_deref().unwrap_or(""));
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for media.
///
/// Strong key: `path | checksum`
/// Returns `None` when both are empty.
fn media_strong_key(data: &MediaData) -> Option<String> {
    let path = data.path.as_deref().unwrap_or("");
    let checksum = data.checksum.as_deref().unwrap_or("");
    let key = normalize_key(&format!("{path}|{checksum}"));
    if key.is_empty() || key == "|" {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for a note.
///
/// Strong key: `text`
/// Returns `None` when text is empty.
fn note_strong_key(data: &NoteData) -> Option<String> {
    let key = normalize_key(&data.text);
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Build a normalized strong match key for a tag.
///
/// Strong key: `name`
/// Returns `None` when name is empty.
fn tag_strong_key(data: &TagData) -> Option<String> {
    let key = normalize_key(&data.name);
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Normalize a string for use as a strong match key.
///
/// Applies case-folding and trims surrounding whitespace.
fn normalize_key(s: &str) -> String {
    case_fold(s).trim().to_string()
}

// ---------------------------------------------------------------------------
// Birth date lookup helper
// ---------------------------------------------------------------------------

/// Find the [`DateValue`] of a person's birth event by traversing their
/// event reference list.
fn find_birth_date(
    graph: &Graph,
    event_refs: &[typed_graph::EventRef],
) -> Option<typed_graph::DateValue> {
    for event_ref in event_refs {
        if let Some(Node::Event(event_data)) = graph.get_node(&event_ref.ref_field) {
            if event_type_eq(&event_data.event_type, typed_graph::EventType::Birth) {
                return event_data.date.clone();
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Type-level grouping
// ---------------------------------------------------------------------------

/// Group graph items by their [`NodeKind`], returning a map from kind to
/// list of handles.
fn group_by_kind(graph: &Graph) -> HashMap<NodeKind, Vec<Handle>> {
    let mut groups: HashMap<NodeKind, Vec<Handle>> = HashMap::new();
    for (handle, node) in graph.iter_nodes() {
        let kind = node_kind(node);
        groups.entry(kind).or_default().push(handle.clone());
    }
    groups
}

// ---------------------------------------------------------------------------
// Classification helpers
// ---------------------------------------------------------------------------

/// Determine the item type string from a [`NodeKind`].
fn item_type_string(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Person => "Person",
        NodeKind::Family => "Family",
        NodeKind::Event => "Event",
        NodeKind::Place => "Place",
        NodeKind::Source => "Source",
        NodeKind::Citation => "Citation",
        NodeKind::Repository => "Repository",
        NodeKind::Media => "Media",
        NodeKind::Note => "Note",
        NodeKind::Tag => "Tag",
    }
}

/// Compare two nodes by their kind, returning the field-level changes.
fn compare_nodes(kind: NodeKind, node_a: &Node, node_b: &Node) -> Vec<FieldChange> {
    match kind {
        NodeKind::Person => {
            if let (Node::Person(a), Node::Person(b)) = (node_a, node_b) {
                compare_person(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Family => {
            if let (Node::Family(a), Node::Family(b)) = (node_a, node_b) {
                compare_family(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Event => {
            if let (Node::Event(a), Node::Event(b)) = (node_a, node_b) {
                compare_event(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Place => {
            if let (Node::Place(a), Node::Place(b)) = (node_a, node_b) {
                compare_place(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Source => {
            if let (Node::Source(a), Node::Source(b)) = (node_a, node_b) {
                compare_source(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Citation => {
            if let (Node::Citation(a), Node::Citation(b)) = (node_a, node_b) {
                compare_citation(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Repository => {
            if let (Node::Repository(a), Node::Repository(b)) = (node_a, node_b) {
                compare_repository(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Media => {
            if let (Node::Media(a), Node::Media(b)) = (node_a, node_b) {
                compare_media(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Note => {
            if let (Node::Note(a), Node::Note(b)) = (node_a, node_b) {
                compare_note(a, b)
            } else {
                vec![]
            }
        }
        NodeKind::Tag => {
            if let (Node::Tag(a), Node::Tag(b)) = (node_a, node_b) {
                compare_tag(a, b)
            } else {
                vec![]
            }
        }
    }
}

/// Extract a display name from a node for use in context information.
fn display_name_for_node(kind: NodeKind, node: &Node) -> String {
    match kind {
        NodeKind::Person => {
            if let Node::Person(data) = node {
                let first = data.primary_name.first_name.as_deref().unwrap_or("");
                let surname = data
                    .primary_name
                    .surname_list
                    .first()
                    .and_then(|s| s.surname.as_deref())
                    .unwrap_or("");
                format!("{} {}", first, surname).trim().to_string()
            } else {
                String::new()
            }
        }
        NodeKind::Family => {
            if let Node::Family(_) = node {
                "Family".to_string()
            } else {
                String::new()
            }
        }
        NodeKind::Event => {
            if let Node::Event(data) = node {
                format!("{:?}", data.event_type)
            } else {
                String::new()
            }
        }
        NodeKind::Place => {
            if let Node::Place(data) = node {
                data.name
                    .city
                    .as_deref()
                    .or(data.name.street.as_deref())
                    .unwrap_or("Place")
                    .to_string()
            } else {
                String::new()
            }
        }
        NodeKind::Source => {
            if let Node::Source(data) = node {
                data.title.clone()
            } else {
                String::new()
            }
        }
        NodeKind::Citation => {
            if let Node::Citation(data) = node {
                data.page.as_deref().unwrap_or("Citation").to_string()
            } else {
                String::new()
            }
        }
        NodeKind::Repository => {
            if let Node::Repository(data) = node {
                data.name.as_deref().unwrap_or("Repository").to_string()
            } else {
                String::new()
            }
        }
        NodeKind::Media => {
            if let Node::Media(data) = node {
                data.desc
                    .as_deref()
                    .or(data.path.as_deref())
                    .unwrap_or("Media")
                    .to_string()
            } else {
                String::new()
            }
        }
        NodeKind::Note => {
            if let Node::Note(data) = node {
                let mut text = data.text.clone();
                text.truncate(80);
                text
            } else {
                String::new()
            }
        }
        NodeKind::Tag => {
            if let Node::Tag(data) = node {
                data.name.clone()
            } else {
                String::new()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Strong key extraction by kind
// ---------------------------------------------------------------------------

/// Extract the strong match key for a node, if one can be derived.
fn strong_key_for_node(kind: NodeKind, graph: &Graph, handle: &Handle) -> Option<String> {
    let node = graph.get_node(handle)?;
    match kind {
        NodeKind::Person => {
            if let Node::Person(data) = node {
                person_strong_key(graph, data)
            } else {
                None
            }
        }
        NodeKind::Family => {
            if let Node::Family(data) = node {
                family_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Event => {
            if let Node::Event(data) = node {
                event_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Place => {
            if let Node::Place(data) = node {
                place_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Source => {
            if let Node::Source(data) = node {
                source_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Citation => {
            if let Node::Citation(data) = node {
                citation_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Repository => {
            if let Node::Repository(data) = node {
                repository_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Media => {
            if let Node::Media(data) = node {
                media_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Note => {
            if let Node::Note(data) = node {
                note_strong_key(data)
            } else {
                None
            }
        }
        NodeKind::Tag => {
            if let Node::Tag(data) = node {
                tag_strong_key(data)
            } else {
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main matching logic
// ---------------------------------------------------------------------------

/// Perform Pass 1 matching between two graphs.
///
/// # Pass 1a — Exact handle match
///
/// Items with the same handle in both graphs are compared field-by-field
/// and classified as [`Classification::Same`] or
/// [`Classification::Modified`].
///
/// # Pass 1b — Fuzzy content match
///
/// For items that could not be matched by handle, a per-type strong match
/// key is derived from the most distinctive fields. Unmatched items from
/// graph B are indexed by their strong key for O(1) lookup against
/// unmatched items from graph A. Items with duplicate strong keys are
/// flagged as [`Classification::NeedsReview`].
///
/// Items that remain unmatched after both passes retain their
/// [`Classification::Added`] or [`Classification::Removed`] status.
pub fn match_graphs(graph_a: &Graph, graph_b: &Graph) -> MatchResult {
    let mut handle_map: HashMap<Handle, Handle> = HashMap::new();
    let mut item_diffs: Vec<ItemDiff> = Vec::new();
    let mut ambiguous_cases: Vec<AmbiguousCase> = Vec::new();

    // Group handles by type for both graphs
    let groups_a = group_by_kind(graph_a);
    let groups_b = group_by_kind(graph_b);

    // All node kinds to process
    let all_kinds = [
        NodeKind::Person,
        NodeKind::Family,
        NodeKind::Event,
        NodeKind::Place,
        NodeKind::Source,
        NodeKind::Citation,
        NodeKind::Repository,
        NodeKind::Media,
        NodeKind::Note,
        NodeKind::Tag,
    ];

    for &kind in &all_kinds {
        let handles_a: Vec<Handle> = groups_a.get(&kind).cloned().unwrap_or_default();
        let handles_b: Vec<Handle> = groups_b.get(&kind).cloned().unwrap_or_default();

        let item_type = item_type_string(kind);

        // Build handle sets for quick lookup
        let set_a: std::collections::HashSet<Handle> = handles_a.iter().cloned().collect();
        let set_b: std::collections::HashSet<Handle> = handles_b.iter().cloned().collect();

        // ------------------------------------------------------------------
        // Pass 1a: Exact handle match
        // ------------------------------------------------------------------
        for handle_a in &handles_a {
            if !set_b.contains(handle_a) {
                continue;
            }
            // handle exists in both graphs
            let handle_b = handle_a.clone();
            let node_a = graph_a.get_node(handle_a).expect("handle exists in A");
            let node_b = graph_b.get_node(&handle_b).expect("handle exists in B");

            let field_changes = compare_nodes(kind, node_a, node_b);
            let classification = if field_changes.is_empty() {
                Classification::Same
            } else {
                Classification::Modified
            };

            handle_map.insert(handle_b.clone(), handle_a.clone());
            item_diffs.push(ItemDiff {
                handle_a: Some(handle_a.clone()),
                handle_b: Some(handle_b),
                gramps_id_a: None,
                gramps_id_b: None,
                display_name_a: None,
                display_name_b: None,
                item_type: item_type.to_string(),
                classification,
                field_changes,
                confidence: 1.0,
            });
        }

        // ------------------------------------------------------------------
        // Identify unmatched items
        // ------------------------------------------------------------------
        let unmatched_a: Vec<Handle> = handles_a
            .iter()
            .filter(|h| !set_b.contains(*h))
            .cloned()
            .collect();
        let unmatched_b: Vec<Handle> = handles_b
            .iter()
            .filter(|h| !set_a.contains(*h))
            .cloned()
            .collect();

        // ------------------------------------------------------------------
        // Pass 1b: Fuzzy content match via strong keys
        // ------------------------------------------------------------------

        // Build strong key index for unmatched A items
        // Maps key → list of handles (multiple = ambiguous)
        let mut strong_key_index: HashMap<String, Vec<Handle>> = HashMap::new();
        for handle in &unmatched_a {
            if let Some(key) = strong_key_for_node(kind, graph_a, handle) {
                strong_key_index
                    .entry(key)
                    .or_default()
                    .push(handle.clone());
            }
        }

        // Track which unmatched A items have been consumed
        let mut consumed_a: std::collections::HashSet<Handle> = std::collections::HashSet::new();
        // Track which unmatched B items have been consumed
        let mut consumed_b: std::collections::HashSet<Handle> = std::collections::HashSet::new();

        for handle_b in &unmatched_b {
            let Some(key) = strong_key_for_node(kind, graph_b, handle_b) else {
                continue;
            };

            let Some(candidates) = strong_key_index.get(&key) else {
                continue;
            };

            let node_b = graph_b.get_node(handle_b).expect("handle exists in B");

            if candidates.len() == 1 {
                // Unique strong key match
                let handle_a = &candidates[0];
                if consumed_a.contains(handle_a) {
                    // Already consumed — this shouldn't happen with unique keys
                    continue;
                }

                let node_a = graph_a.get_node(handle_a).expect("handle exists in A");
                let field_changes = compare_nodes(kind, node_a, node_b);
                let classification = if field_changes.is_empty() {
                    Classification::Same
                } else {
                    Classification::Modified
                };

                handle_map.insert(handle_b.clone(), handle_a.clone());
                item_diffs.push(ItemDiff {
                    handle_a: Some(handle_a.clone()),
                    handle_b: Some(handle_b.clone()),
                    gramps_id_a: None,
                    gramps_id_b: None,
                    display_name_a: None,
                    display_name_b: None,
                    item_type: item_type.to_string(),
                    classification,
                    field_changes,
                    confidence: 0.9,
                });

                consumed_a.insert(handle_a.clone());
                consumed_b.insert(handle_b.clone());
            } else {
                // Multiple candidates with the same strong key → ambiguous
                let context_a = build_context(kind, graph_a, &candidates[0]);
                let candidates_list: Vec<Candidate> = candidates
                    .iter()
                    .map(|h| Candidate {
                        handle_b: h.clone(),
                        score: 1.0,
                        context_b: build_context(kind, graph_b, h),
                    })
                    .collect();

                ambiguous_cases.push(AmbiguousCase {
                    handle_a: candidates[0].clone(),
                    item_type_a: item_type.to_string(),
                    context_a,
                    candidates: candidates_list,
                });

                // Mark all handles in this ambiguous group as consumed
                for h in candidates {
                    consumed_a.insert(h.clone());
                }
                consumed_b.insert(handle_b.clone());
            }
        }

        // ------------------------------------------------------------------
        // Remaining unmatched items → ADDED / REMOVED
        // ------------------------------------------------------------------
        for handle in &unmatched_a {
            if consumed_a.contains(handle) {
                continue;
            }
            item_diffs.push(ItemDiff {
                handle_a: Some(handle.clone()),
                handle_b: None,
                gramps_id_a: None,
                gramps_id_b: None,
                display_name_a: None,
                display_name_b: None,
                item_type: item_type.to_string(),
                classification: Classification::Removed,
                field_changes: vec![],
                confidence: 1.0,
            });
        }

        for handle in &unmatched_b {
            if consumed_b.contains(handle) {
                continue;
            }
            item_diffs.push(ItemDiff {
                handle_a: None,
                handle_b: Some(handle.clone()),
                gramps_id_a: None,
                gramps_id_b: None,
                display_name_a: None,
                display_name_b: None,
                item_type: item_type.to_string(),
                classification: Classification::Added,
                field_changes: vec![],
                confidence: 1.0,
            });
        }
    }

    MatchResult {
        handle_map,
        item_diffs,
        ambiguous_cases,
    }
}

/// Build context information for a node.
fn build_context(kind: NodeKind, graph: &Graph, handle: &Handle) -> AmbiguousContext {
    let node = graph.get_node(handle);
    let display_name = node
        .map(|n| display_name_for_node(kind, n))
        .unwrap_or_default();
    AmbiguousContext {
        display_name,
        related_items: vec![],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use typed_graph::{
        DateValue, EventRef, EventType, FamilyData, Name, NoteData, PersonData, Surname, TagData,
    };

    /// Helper: create a graph with a single person node (no events).
    fn person_graph_simple(handle: &str, first_name: &str, surname: &str) -> Graph {
        let mut graph = Graph::new();
        let h: Handle = handle.to_string();
        let person_data = PersonData {
            handle: h.clone(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some(first_name.to_string()),
                surname_list: vec![Surname {
                    surname: Some(surname.to_string()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            ..PersonData::default()
        };
        graph.add_node(h, Node::Person(person_data)).unwrap();
        graph
    }

    /// Helper: create a graph with a single person node and a birth event.
    fn person_graph(handle: &str, first_name: &str, surname: &str, birth_year: i32) -> Graph {
        let mut graph = Graph::new();
        let h: Handle = handle.to_string();

        // Create birth event with a consistent handle based on the birth year
        // so that two persons with the same birth year get the same event handle
        let event_handle = format!("birth_{}", birth_year);
        let birth_date = if birth_year > 0 {
            Some(DateValue::new(birth_year))
        } else {
            None
        };
        let event_data = EventData {
            handle: event_handle.clone(),
            event_type: Some(EventType::Birth),
            date: birth_date,
            gramps_id: None,
            description: None,
            place_handle: None,
            citation_list: vec![],
            note_list: vec![],
            media_list: vec![],
            tag_list: vec![],
            attribute_list: vec![],
            ..Default::default()
        };
        graph
            .add_node(event_handle.clone(), Node::Event(event_data))
            .unwrap();

        // Create the person
        let person_data = PersonData {
            handle: h.clone(),
            gramps_id: None,
            gender: Some(1),
            primary_name: Name {
                first_name: Some(first_name.to_string()),
                surname_list: vec![Surname {
                    surname: Some(surname.to_string()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            event_ref_list: vec![EventRef {
                ref_field: event_handle,
                role: Some(typed_graph::EventRoleType::Primary),
                ..Default::default()
            }],
            ..PersonData::default()
        };
        graph.add_node(h, Node::Person(person_data)).unwrap();

        graph
    }

    /// Helper: create a graph with a single family node.
    fn family_graph(handle: &str, father: &str, mother: &str) -> Graph {
        let mut graph = Graph::new();
        let h: Handle = handle.to_string();

        let family_data = FamilyData {
            handle: h.clone(),
            gramps_id: None,
            father_handle: if father.is_empty() {
                None
            } else {
                Some(father.to_string())
            },
            mother_handle: if mother.is_empty() {
                None
            } else {
                Some(mother.to_string())
            },
            ..FamilyData::default()
        };
        graph.add_node(h, Node::Family(family_data)).unwrap();

        graph
    }

    /// Helper: create a graph with a single note node.
    fn note_graph(handle: &str, text: &str) -> Graph {
        let mut graph = Graph::new();
        let h: Handle = handle.to_string();
        let note_data = NoteData {
            handle: h.clone(),
            text: text.to_string(),
            gramps_id: None,
            format: None,
            type_field: None,
            citation_list: vec![],
            tag_list: vec![],
        };
        graph.add_node(h, Node::Note(note_data)).unwrap();
        graph
    }

    /// Helper: create a graph with a single tag node.
    fn tag_graph(handle: &str, name: &str, color: &str) -> Graph {
        let mut graph = Graph::new();
        let h: Handle = handle.to_string();
        let tag_data = TagData {
            handle: h.clone(),
            name: name.to_string(),
            color: if color.is_empty() {
                None
            } else {
                Some(color.to_string())
            },
            gramps_id: None,
            priority: None,
            tag_list: vec![],
        };
        graph.add_node(h, Node::Tag(tag_data)).unwrap();
        graph
    }

    // -----------------------------------------------------------------------
    // Pass 1a: Exact handle match
    // -----------------------------------------------------------------------

    #[test]
    fn exact_handle_match_same() {
        let graph_a = person_graph("P001", "John", "Smith", 1850);
        let graph_b = person_graph("P001", "John", "Smith", 1850);

        let result = match_graphs(&graph_a, &graph_b);

        // Two items: Person + Event (birth), both Same
        assert_eq!(result.item_diffs.len(), 2);
        let person_diff = result
            .item_diffs
            .iter()
            .find(|d| d.item_type == "Person")
            .expect("should have Person diff");
        assert_eq!(person_diff.classification, Classification::Same);
        assert_eq!(person_diff.handle_a.as_deref(), Some("P001"));
        assert_eq!(person_diff.handle_b.as_deref(), Some("P001"));
        assert!((person_diff.confidence - 1.0).abs() < 1e-9);
        assert!(result.handle_map.contains_key("P001"));
        assert_eq!(
            result.handle_map.get("P001").map(|s| s.as_str()),
            Some("P001")
        );
    }

    #[test]
    fn exact_handle_match_modified() {
        let graph_a = person_graph("P001", "John", "Smith", 1850);
        let graph_b = person_graph("P001", "John", "Jones", 1850);

        let result = match_graphs(&graph_a, &graph_b);

        // Two items: Person (Modified) + Event (Same, birth event identical)
        assert_eq!(result.item_diffs.len(), 2);
        let person_diff = result
            .item_diffs
            .iter()
            .find(|d| d.item_type == "Person")
            .expect("should have Person diff");
        assert_eq!(person_diff.classification, Classification::Modified);
        assert!(!person_diff.field_changes.is_empty());
    }

    #[test]
    fn exact_handle_no_match_removed_added() {
        let graph_a = person_graph("P001", "John", "Smith", 1850);
        let graph_b = person_graph("P002", "Jane", "Doe", 1860);

        let result = match_graphs(&graph_a, &graph_b);

        // Four items: 2 Persons (P001 removed, P002 added) + 2 Events (birth events)
        // The birth events have different dates (1850 vs 1860) so they don't match
        assert_eq!(result.item_diffs.len(), 4);

        // Find the REMOVED Person item (handle_a = P001)
        let removed = result
            .item_diffs
            .iter()
            .find(|d| d.handle_a.as_deref() == Some("P001"))
            .expect("should have REMOVED Person");
        assert_eq!(removed.classification, Classification::Removed);

        // Find the ADDED Person item (handle_b = P002)
        let added = result
            .item_diffs
            .iter()
            .find(|d| d.handle_b.as_deref() == Some("P002"))
            .expect("should have ADDED Person");
        assert_eq!(added.classification, Classification::Added);
    }

    // -----------------------------------------------------------------------
    // Pass 1b: Fuzzy content match
    // -----------------------------------------------------------------------

    #[test]
    fn fuzzy_match_by_name_and_birth_same() {
        // Same person, different handles, same content
        // Use person_graph_simple (no events) to avoid event_ref_list differences
        let graph_a = person_graph_simple("P001", "John", "Smith");
        let graph_b = person_graph_simple("P002", "John", "Smith");

        let result = match_graphs(&graph_a, &graph_b);

        // Person fuzzy matched by strong key (surname|first_name|)
        assert_eq!(result.item_diffs.len(), 1);
        let person_diff = &result.item_diffs[0];
        assert_eq!(person_diff.classification, Classification::Same);
        // handle_map should map P002 → P001
        assert_eq!(
            result.handle_map.get("P002").map(|s| s.as_str()),
            Some("P001")
        );
    }

    #[test]
    fn fuzzy_match_by_name_and_birth_modified() {
        // Same person, different handles, modified gramps_id (non-key field)
        // Use person_graph_simple, then modify the gramps_id in B
        let mut graph_a = Graph::new();
        let mut graph_b = Graph::new();

        let person_a = PersonData {
            handle: "P001".into(),
            gender: Some(1),
            gramps_id: Some("I001".into()),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            ..PersonData::default()
        };
        let person_b = PersonData {
            handle: "P002".into(),
            gender: Some(1),
            gramps_id: Some("I002".into()),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            ..PersonData::default()
        };
        graph_a
            .add_node("P001".into(), Node::Person(person_a))
            .unwrap();
        graph_b
            .add_node("P002".into(), Node::Person(person_b))
            .unwrap();

        let result = match_graphs(&graph_a, &graph_b);

        // Person fuzzy matched by strong key (same name) → Modified (different gramps_id)
        assert_eq!(result.item_diffs.len(), 1);
        let person_diff = &result.item_diffs[0];
        assert_eq!(person_diff.classification, Classification::Modified);
        assert!(!person_diff.field_changes.is_empty());
        assert_eq!(
            result.handle_map.get("P002").map(|s| s.as_str()),
            Some("P001")
        );
    }

    #[test]
    fn fuzzy_match_no_match_person_no_name() {
        // Person with no name fields → no strong key possible → stays ADDED/REMOVED
        let mut graph_a = Graph::new();
        let mut graph_b = Graph::new();

        let p1 = PersonData {
            handle: "P001".into(),
            ..PersonData::default()
        };
        let p2 = PersonData {
            handle: "P002".into(),
            ..PersonData::default()
        };
        graph_a.add_node("P001".into(), Node::Person(p1)).unwrap();
        graph_b.add_node("P002".into(), Node::Person(p2)).unwrap();

        let result = match_graphs(&graph_a, &graph_b);

        // Two items: Person P001 (Removed) + Person P002 (Added)
        // No fuzzy match because Person with no name has no strong key
        assert_eq!(result.item_diffs.len(), 2);
        let classifications: Vec<Classification> =
            result.item_diffs.iter().map(|d| d.classification).collect();
        assert!(classifications.contains(&Classification::Removed));
        assert!(classifications.contains(&Classification::Added));
    }

    #[test]
    fn fuzzy_match_unicode_name() {
        // Person with Unicode name should match via case-folded key
        let graph_a = person_graph_simple("P001", "José", "Müller");
        let graph_b = person_graph_simple("P002", "José", "Müller");

        let result = match_graphs(&graph_a, &graph_b);

        // Person fuzzy matched by strong key (case-folded surname|first_name|)
        assert_eq!(result.item_diffs.len(), 1);
        let person_diff = &result.item_diffs[0];
        assert_eq!(person_diff.classification, Classification::Same);
    }

    #[test]
    fn fuzzy_match_duplicate_strong_key_ambiguous() {
        // Two people with the same name and birth date → ambiguous
        let mut graph_a = Graph::new();
        let mut graph_b = Graph::new();

        // Create birth event for both
        let birth_handle = "birth_shared";
        let birth_data = EventData {
            handle: birth_handle.into(),
            event_type: Some(EventType::Birth),
            date: Some(DateValue::new(1850)),
            ..EventData::default()
        };
        graph_a
            .add_node(birth_handle.into(), Node::Event(birth_data))
            .unwrap();

        // Create birth event for graph B
        let birth_handle_b = "birth_shared_b";
        let birth_data_b = EventData {
            handle: birth_handle_b.into(),
            event_type: Some(EventType::Birth),
            date: Some(DateValue::new(1850)),
            ..EventData::default()
        };
        graph_b
            .add_node(birth_handle_b.into(), Node::Event(birth_data_b))
            .unwrap();

        // Two people in A with same name/date
        let p1 = PersonData {
            handle: "P001".into(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            event_ref_list: vec![EventRef {
                ref_field: birth_handle.into(),
                role: Some(typed_graph::EventRoleType::Primary),
                ..Default::default()
            }],
            ..PersonData::default()
        };
        let p2 = PersonData {
            handle: "P002".into(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            event_ref_list: vec![EventRef {
                ref_field: birth_handle.into(),
                role: Some(typed_graph::EventRoleType::Primary),
                ..Default::default()
            }],
            ..PersonData::default()
        };
        graph_a.add_node("P001".into(), Node::Person(p1)).unwrap();
        graph_a.add_node("P002".into(), Node::Person(p2)).unwrap();

        // One person in B with same name/date
        let p3 = PersonData {
            handle: "P003".into(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            event_ref_list: vec![EventRef {
                ref_field: birth_handle_b.into(),
                role: Some(typed_graph::EventRoleType::Primary),
                ..Default::default()
            }],
            ..PersonData::default()
        };
        graph_b.add_node("P003".into(), Node::Person(p3)).unwrap();

        let result = match_graphs(&graph_a, &graph_b);

        // Should have ambiguous cases
        assert!(
            !result.ambiguous_cases.is_empty(),
            "should have ambiguous cases with duplicate strong keys"
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn empty_graphs() {
        let graph_a = Graph::new();
        let graph_b = Graph::new();

        let result = match_graphs(&graph_a, &graph_b);

        assert!(result.item_diffs.is_empty());
        assert!(result.handle_map.is_empty());
        assert!(result.ambiguous_cases.is_empty());
    }

    #[test]
    fn one_empty_graph() {
        let graph_a = person_graph("P001", "John", "Smith", 1850);
        let graph_b = Graph::new();

        let result = match_graphs(&graph_a, &graph_b);

        // Two items: Person (Removed) + Event (Removed)
        assert_eq!(result.item_diffs.len(), 2);
        let person_diff = result
            .item_diffs
            .iter()
            .find(|d| d.item_type == "Person")
            .expect("should have Person diff");
        assert_eq!(person_diff.classification, Classification::Removed);
        assert_eq!(person_diff.handle_a.as_deref(), Some("P001"));
        assert!(person_diff.handle_b.is_none());
    }

    #[test]
    fn multiple_types_mixed() {
        // Graph A: one person + one note
        let mut graph_a = Graph::new();
        let person_a = PersonData {
            handle: "P001".into(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            ..PersonData::default()
        };
        let note_a = NoteData {
            handle: "N001".into(),
            text: "Hello".into(),
            ..NoteData::default()
        };
        graph_a
            .add_node("P001".into(), Node::Person(person_a))
            .unwrap();
        graph_a.add_node("N001".into(), Node::Note(note_a)).unwrap();

        // Graph B: same person (different handle) + same note (same handle) + new tag
        let mut graph_b = Graph::new();
        let person_b = PersonData {
            handle: "P002".into(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some("John".into()),
                surname_list: vec![Surname {
                    surname: Some("Smith".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            ..PersonData::default()
        };
        let note_b = NoteData {
            handle: "N001".into(),
            text: "Hello".into(),
            ..NoteData::default()
        };
        let tag_b = TagData {
            handle: "T001".into(),
            name: "important".into(),
            ..TagData::default()
        };
        graph_b
            .add_node("P002".into(), Node::Person(person_b))
            .unwrap();
        graph_b.add_node("N001".into(), Node::Note(note_b)).unwrap();
        graph_b.add_node("T001".into(), Node::Tag(tag_b)).unwrap();

        let result = match_graphs(&graph_a, &graph_b);

        // Note matches by handle → Same
        let note_diff = result
            .item_diffs
            .iter()
            .find(|d| d.item_type == "Note")
            .expect("should have Note diff");
        assert_eq!(note_diff.classification, Classification::Same);
        assert_eq!(note_diff.handle_a.as_deref(), Some("N001"));

        // Person matches by fuzzy strong key → Same
        let person_diff = result
            .item_diffs
            .iter()
            .find(|d| d.item_type == "Person")
            .expect("should have Person diff");
        assert_eq!(person_diff.classification, Classification::Same);

        // Tag is ADDED
        let tag_diff = result
            .item_diffs
            .iter()
            .find(|d| d.item_type == "Tag")
            .expect("should have Tag diff");
        assert_eq!(tag_diff.classification, Classification::Added);
    }

    // -----------------------------------------------------------------------
    // Family matching
    // -----------------------------------------------------------------------

    #[test]
    fn family_exact_handle_match() {
        let graph_a = family_graph("F001", "P001", "P002");
        let graph_b = family_graph("F001", "P001", "P002");

        let result = match_graphs(&graph_a, &graph_b);

        assert_eq!(result.item_diffs.len(), 1);
        assert_eq!(result.item_diffs[0].classification, Classification::Same);
    }

    #[test]
    fn family_fuzzy_match() {
        // Same family, different handles
        let graph_a = family_graph("F001", "P001", "P002");
        let graph_b = family_graph("F002", "P001", "P002");

        let result = match_graphs(&graph_a, &graph_b);

        assert_eq!(result.item_diffs.len(), 1);
        assert_eq!(result.item_diffs[0].classification, Classification::Same);
        assert_eq!(
            result.handle_map.get("F002").map(|s| s.as_str()),
            Some("F001")
        );
    }

    // -----------------------------------------------------------------------
    // Note and Tag matching
    // -----------------------------------------------------------------------

    #[test]
    fn note_exact_handle_match() {
        let graph_a = note_graph("N001", "some text");
        let graph_b = note_graph("N001", "some text");

        let result = match_graphs(&graph_a, &graph_b);

        assert_eq!(result.item_diffs.len(), 1);
        assert_eq!(result.item_diffs[0].classification, Classification::Same);
    }

    #[test]
    fn note_fuzzy_match() {
        // Same note, different handles
        let graph_a = note_graph("N001", "some text");
        let graph_b = note_graph("N002", "some text");

        let result = match_graphs(&graph_a, &graph_b);

        assert_eq!(result.item_diffs.len(), 1);
        assert_eq!(result.item_diffs[0].classification, Classification::Same);
        assert_eq!(
            result.handle_map.get("N002").map(|s| s.as_str()),
            Some("N001")
        );
    }

    #[test]
    fn tag_exact_handle_match() {
        let graph_a = tag_graph("T001", "important", "#ff0000");
        let graph_b = tag_graph("T001", "important", "#ff0000");

        let result = match_graphs(&graph_a, &graph_b);

        assert_eq!(result.item_diffs.len(), 1);
        assert_eq!(result.item_diffs[0].classification, Classification::Same);
    }

    #[test]
    fn tag_fuzzy_match() {
        // Same tag, different handles
        let graph_a = tag_graph("T001", "important", "#ff0000");
        let graph_b = tag_graph("T002", "important", "#ff0000");

        let result = match_graphs(&graph_a, &graph_b);

        assert_eq!(result.item_diffs.len(), 1);
        assert_eq!(result.item_diffs[0].classification, Classification::Same);
        assert_eq!(
            result.handle_map.get("T002").map(|s| s.as_str()),
            Some("T001")
        );
    }

    // -----------------------------------------------------------------------
    // MatchResult type shape
    // -----------------------------------------------------------------------

    #[test]
    fn match_result_debug_and_clone() {
        let result = MatchResult {
            handle_map: HashMap::new(),
            item_diffs: vec![],
            ambiguous_cases: vec![],
        };
        let _debug = format!("{result:?}");
        let _cloned = result.clone();
    }

    #[test]
    fn match_result_partial_eq() {
        let a = MatchResult {
            handle_map: HashMap::new(),
            item_diffs: vec![],
            ambiguous_cases: vec![],
        };
        let b = MatchResult {
            handle_map: HashMap::new(),
            item_diffs: vec![],
            ambiguous_cases: vec![],
        };
        assert_eq!(a, b);
    }

    // -----------------------------------------------------------------------
    // Large-graph optimization: O(1) strong key index
    // -----------------------------------------------------------------------

    #[test]
    fn large_graph_strong_key_index() {
        // Build a graph with 10 persons in A, 10 in B, all different
        // Then add one matching pair with different handles
        let mut graph_a = Graph::new();
        let mut graph_b = Graph::new();

        // 10 persons in A (no birth events — just names)
        for i in 0..10 {
            let handle = format!("P{:03}", i);
            let person = PersonData {
                handle: handle.clone(),
                gender: Some(1),
                primary_name: Name {
                    first_name: Some(format!("FirstName{}", i)),
                    surname_list: vec![Surname {
                        surname: Some(format!("Surname{}", i)),
                        ..Surname::default()
                    }],
                    ..Name::default()
                },
                ..PersonData::default()
            };
            graph_a.add_node(handle, Node::Person(person)).unwrap();
        }

        // 10 persons in B, different names
        for i in 0..10 {
            let handle = format!("Q{:03}", i);
            let person = PersonData {
                handle: handle.clone(),
                gender: Some(1),
                primary_name: Name {
                    first_name: Some(format!("OtherName{}", i)),
                    surname_list: vec![Surname {
                        surname: Some(format!("OtherSurname{}", i)),
                        ..Surname::default()
                    }],
                    ..Name::default()
                },
                ..PersonData::default()
            };
            graph_b.add_node(handle, Node::Person(person)).unwrap();
        }

        // Add a matching pair (different handles, same name, same birth date)
        // Create birth event in A
        let birth_a = EventData {
            handle: "birth_a".into(),
            event_type: Some(EventType::Birth),
            date: Some(DateValue::new(1850)),
            ..EventData::default()
        };
        graph_a
            .add_node("birth_a".into(), Node::Event(birth_a))
            .unwrap();

        let match_a = PersonData {
            handle: "match_a".into(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some("Matching".into()),
                surname_list: vec![Surname {
                    surname: Some("Person".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            event_ref_list: vec![EventRef {
                ref_field: "birth_a".into(),
                role: Some(typed_graph::EventRoleType::Primary),
                ..Default::default()
            }],
            ..PersonData::default()
        };
        graph_a
            .add_node("match_a".into(), Node::Person(match_a))
            .unwrap();

        // Create birth event in B
        let birth_b = EventData {
            handle: "birth_b".into(),
            event_type: Some(EventType::Birth),
            date: Some(DateValue::new(1850)),
            ..EventData::default()
        };
        graph_b
            .add_node("birth_b".into(), Node::Event(birth_b))
            .unwrap();

        let match_b = PersonData {
            handle: "match_b".into(),
            gender: Some(1),
            primary_name: Name {
                first_name: Some("Matching".into()),
                surname_list: vec![Surname {
                    surname: Some("Person".into()),
                    ..Surname::default()
                }],
                ..Name::default()
            },
            event_ref_list: vec![EventRef {
                ref_field: "birth_b".into(),
                role: Some(typed_graph::EventRoleType::Primary),
                ..Default::default()
            }],
            ..PersonData::default()
        };
        graph_b
            .add_node("match_b".into(), Node::Person(match_b))
            .unwrap();

        let result = match_graphs(&graph_a, &graph_b);

        // The matching pair should be found by strong key
        assert!(
            result.handle_map.contains_key("match_b"),
            "match_b should be matched to match_a"
        );
        assert_eq!(
            result.handle_map.get("match_b").map(|s| s.as_str()),
            Some("match_a")
        );

        // All other items are ADDED/REMOVED
        let added_count = result
            .item_diffs
            .iter()
            .filter(|d| d.classification == Classification::Added)
            .count();
        let removed_count = result
            .item_diffs
            .iter()
            .filter(|d| d.classification == Classification::Removed)
            .count();
        assert_eq!(added_count, 10, "10 B items should be ADDED");
        assert_eq!(removed_count, 10, "10 A items should be REMOVED");
    }
}
