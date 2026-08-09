//! Interactive review CLI for deletion candidates.
//!
//! Presents deletion candidates to the user grouped by type in dependency
//! order. For each type, the user can confirm deletion, skip the type,
//! remove specific handles from the deletion set, list all candidates,
//! show a summary of remaining types, or abort the entire operation.

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{BufRead, Write};

use typed_graph::{get_source_handle, Edge, Graph, Handle, Node};

use crate::types::{DeletePlan, NodeKindLabel};

/// The result of reviewing a single type.
#[derive(Clone, Debug, PartialEq)]
pub enum ReviewAction {
    /// Delete all candidates for this type.
    DeleteAll,
    /// Keep all candidates for this type (skip).
    KeepAll,
    /// Delete all candidates except the specified kept handles.
    RemoveHandles(HashSet<Handle>),
    /// Abort the entire operation.
    Abort,
}

/// The result of the full interactive review.
#[derive(Clone, Debug, PartialEq)]
pub enum ReviewResult {
    /// All review decisions collected successfully.
    /// Maps from type name (e.g. "people") to the set of handles to delete.
    Confirmed(HashMap<String, HashSet<Handle>>),
    /// User aborted the operation.
    Aborted,
}

/// Run the interactive review loop.
///
/// Presents deletion candidates to the user grouped by type in dependency
/// order. Returns the final set of handles to delete per type, or `Aborted`
/// if the user quit.
///
/// When `yes_mode` is `true`, skips all prompts and confirms every type.
pub fn run_interactive_review(
    plan: &DeletePlan,
    graph: &Graph,
    yes_mode: bool,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> ReviewResult {
    if yes_mode {
        // Skip all prompts — confirm everything
        let mut confirmed = HashMap::new();
        for (label, handles) in &plan.per_type {
            let set: HashSet<Handle> = handles.iter().cloned().collect();
            confirmed.insert(label.plural().to_string(), set);
        }
        // Also include seeds even if they have no per_type entry
        for handle in &plan.seed_people {
            let type_name = if let Some(node) = graph.get_node(handle) {
                crate::cascade::node_kind_to_label(node)
                    .plural()
                    .to_string()
            } else {
                continue;
            };
            confirmed
                .entry(type_name)
                .or_insert_with(HashSet::new)
                .insert(handle.clone());
        }
        return ReviewResult::Confirmed(confirmed);
    }

    let type_order = [
        NodeKindLabel::Person,
        NodeKindLabel::Family,
        NodeKindLabel::Event,
        NodeKindLabel::Place,
        NodeKindLabel::Citation,
        NodeKindLabel::Source,
        NodeKindLabel::Repository,
        NodeKindLabel::Media,
        NodeKindLabel::Note,
        NodeKindLabel::Tag,
    ];

    let mut confirmed: HashMap<String, HashSet<Handle>> = HashMap::new();
    let total_types = type_order
        .iter()
        .filter(|nk| plan.per_type.get(nk).is_some_and(|h| !h.is_empty()))
        .count();
    let mut current_step = 0;

    for node_kind in &type_order {
        let type_name = node_kind.plural();
        let handles = match plan.per_type.get(node_kind) {
            Some(h) => h,
            None => continue,
        };

        if handles.is_empty() {
            continue;
        }

        current_step += 1;

        let action = review_type_prompt(
            node_kind,
            handles,
            graph,
            current_step,
            total_types,
            input,
            output,
        );

        match action {
            ReviewAction::DeleteAll => {
                let set: HashSet<Handle> = handles.iter().cloned().collect();
                confirmed.insert(type_name.to_string(), set);
            }
            ReviewAction::KeepAll => {
                // Don't add anything to confirmed
            }
            ReviewAction::RemoveHandles(kept) => {
                let set: HashSet<Handle> = handles
                    .iter()
                    .filter(|h| !kept.contains(*h))
                    .cloned()
                    .collect();
                confirmed.insert(type_name.to_string(), set);
            }
            ReviewAction::Abort => return ReviewResult::Aborted,
        }
    }

    ReviewResult::Confirmed(confirmed)
}

/// Prompt the user for input on a single type.
fn review_type_prompt(
    node_kind: &NodeKindLabel,
    handles: &[Handle],
    graph: &Graph,
    step: usize,
    total: usize,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> ReviewAction {
    let type_name = node_kind.plural();
    let count = handles.len();

    // Generate sample descriptions (up to 3)
    let samples: Vec<String> = handles
        .iter()
        .take(3)
        .map(|h| {
            let (desc, gramps_id) = describe_node(graph, h);
            let id_str = format_gramps_id(&gramps_id);
            format!("  • {}{} — {}", h, id_str, desc)
        })
        .collect();

    loop {
        let _ = writeln!(
            output,
            "\n═══ Step {}/{}: {} ═══",
            step,
            total,
            to_title(type_name)
        );
        let _ = writeln!(
            output,
            "{} {} would be deleted.",
            count,
            if count == 1 {
                to_singular(type_name)
            } else {
                type_name
            }
        );

        if !samples.is_empty() {
            let _ = writeln!(output, "\nSample candidates:");
            for s in &samples {
                let _ = writeln!(output, "{}", s);
            }
            if count > 3 {
                let _ = writeln!(output, "  ... and {} more", count - 3);
            }
        }

        let _ = writeln!(
            output,
            "\nActions:\n\
             [y] Delete all {} {} and continue\n\
             [n] Skip this type (keep all {} {})\n\
             [l] List all {} candidates with details\n\
             [r] Remove specific handles\n\
             [s] Show summary of remaining types\n\
             [q] Abort the entire operation",
            count, type_name, count, type_name, count,
        );
        let _ = write!(output, "\nChoice: ");
        let _ = output.flush();

        let mut line = String::new();
        if input.read_line(&mut line).is_err() {
            return ReviewAction::Abort;
        }

        let trimmed = line.trim().to_lowercase();

        match trimmed.as_str() {
            "y" => return ReviewAction::DeleteAll,
            "n" => return ReviewAction::KeepAll,
            "l" => {
                let _ = writeln!(output, "\nAll {} candidates:", type_name);
                for h in handles {
                    let (desc, gramps_id) = describe_node(graph, h);
                    let id_str = format_gramps_id(&gramps_id);
                    let _ = writeln!(output, "  • {}{} — {}", h, id_str, desc);
                }
                let _ = writeln!(output, "\n[Press Enter to continue]");
                let mut _pause = String::new();
                let _ = input.read_line(&mut _pause);
                continue;
            }
            "r" => {
                let _ = write!(
                    output,
                    "Enter handles to REMOVE from deletion (space-separated): "
                );
                let _ = output.flush();
                let mut remove_line = String::new();
                if input.read_line(&mut remove_line).is_err() {
                    continue;
                }
                let to_keep: HashSet<Handle> = remove_line
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                if to_keep.is_empty() {
                    continue;
                }
                // Validate that all entered handles are actually candidates
                let candidate_set: HashSet<Handle> = handles.iter().cloned().collect();
                let valid: HashSet<Handle> =
                    to_keep.intersection(&candidate_set).cloned().collect();
                let invalid: Vec<&Handle> = to_keep.difference(&candidate_set).collect();
                if !invalid.is_empty() {
                    let _ = writeln!(
                        output,
                        "Warning: {} handles are not in the candidate list and were ignored: {:?}",
                        invalid.len(),
                        invalid
                    );
                }
                if valid.is_empty() {
                    let _ = writeln!(output, "No valid handles entered. Keeping all candidates.");
                    continue;
                }
                return ReviewAction::RemoveHandles(valid);
            }
            "s" => {
                // Summary is handled at the ReviewResult level
                let _ = writeln!(output, "\n[Summary not yet available at per-type level]");
                continue;
            }
            "q" => return ReviewAction::Abort,
            _ => {
                let _ = writeln!(
                    output,
                    "Unknown command. Type 'y', 'n', 'l', 'r', 's', or 'q'."
                );
            }
        }
    }
}

/// Build a full name string from a PersonData node.
fn person_full_name(p: &typed_graph::PersonData) -> String {
    let first = p.primary_name.first_name.as_deref().unwrap_or("Unknown");
    let surname = p
        .primary_name
        .surname_list
        .first()
        .and_then(|s| s.surname.as_deref())
        .unwrap_or("");
    if surname.is_empty() {
        first.to_string()
    } else {
        format!("{} {}", first, surname)
    }
}

/// Format an optional gramps_id for display, e.g. `[I0001]`.
fn format_gramps_id(gramps_id: &Option<String>) -> String {
    gramps_id
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|id| format!(" [{}]", id))
        .unwrap_or_default()
}

/// Build a fallback description string when a Source has no title or author.
fn fallback_source(data: &typed_graph::SourceData) -> String {
    if let Some(ref pubinfo) = data.pubinfo {
        if !pubinfo.is_empty() {
            return format!("Source: {}", pubinfo);
        }
    }
    "Unnamed Source".to_string()
}

/// Generate a human-readable description of a node and its optional gramps_id.
///
/// Returns `(description, gramps_id)` where `gramps_id` is `None` when the
/// node type has no gramps_id set.
fn describe_node(graph: &Graph, handle: &Handle) -> (String, Option<String>) {
    match graph.get_node(handle) {
        Some(Node::Person(data)) => {
            let full_name = person_full_name(data);
            // Resolve birth year via birth_ref_index into event_ref_list
            let resolve_year = |idx_opt: &Option<i32>| -> Option<String> {
                idx_opt.and_then(|idx| {
                    data.event_ref_list
                        .get(idx as usize)
                        .and_then(|eref| graph.get_node(&eref.ref_field))
                        .and_then(|n| {
                            if let Node::Event(ed) = n {
                                ed.date.as_ref()
                            } else {
                                None
                            }
                        })
                        .and_then(|d| d.text.clone().or_else(|| Some(format!("{:04}", d.year))))
                })
            };
            let birth = resolve_year(&data.birth_ref_index);
            let death = resolve_year(&data.death_ref_index);
            let desc = match (birth, death) {
                (Some(b), Some(d)) => format!("{} ({}-{})", full_name, b, d),
                (Some(b), None) => format!("{} (b. {})", full_name, b),
                (None, Some(d)) => format!("{} (d. {})", full_name, d),
                (None, None) => full_name,
            };
            (desc, data.gramps_id.clone())
        }
        Some(Node::Family(data)) => {
            // Use edge traversal for father, mother, and children
            let incident = graph.edges_incident_to(handle);

            let father_name = incident
                .iter()
                .filter_map(|e| match e {
                    Edge::FamilyFather { target, .. } => Some(target.clone()),
                    _ => None,
                })
                .next()
                .and_then(|h| graph.get_node(&h))
                .and_then(|n| {
                    if let Node::Person(p) = n {
                        Some(person_full_name(p))
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let mother_name = incident
                .iter()
                .filter_map(|e| match e {
                    Edge::FamilyMother { target, .. } => Some(target.clone()),
                    _ => None,
                })
                .next()
                .and_then(|h| graph.get_node(&h))
                .and_then(|n| {
                    if let Node::Person(p) = n {
                        Some(person_full_name(p))
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            // Children via FamilyChildRef edges (from family TO child)
            let child_handles: Vec<Handle> = incident
                .iter()
                .filter_map(|e| match e {
                    Edge::FamilyChildRef { target, .. } => Some(target.clone()),
                    _ => None,
                })
                .collect();
            let child_count = child_handles.len();
            let child_names: Vec<String> = child_handles
                .iter()
                .take(2)
                .filter_map(|h| graph.get_node(h))
                .filter_map(|n| {
                    if let Node::Person(p) = n {
                        Some(person_full_name(p))
                    } else {
                        None
                    }
                })
                .collect();

            let parents = match (father_name.is_empty(), mother_name.is_empty()) {
                (true, true) => "no parents".to_string(),
                (false, true) => father_name,
                (true, false) => mother_name,
                (false, false) => format!("{} & {}", father_name, mother_name),
            };

            let desc = match child_names.len() {
                0 => format!("Family: {} ({})", parents, handle),
                1 if child_count == 1 => {
                    format!("Family: {} | child: {}", parents, child_names[0])
                }
                2 if child_count == 2 => {
                    format!(
                        "Family: {} | children: {}, {}",
                        parents, child_names[0], child_names[1]
                    )
                }
                _ => {
                    format!(
                        "Family: {} | children: {}, {} (+{} more)",
                        parents,
                        child_names[0],
                        child_names[1],
                        child_count - 2
                    )
                }
            };
            (desc, data.gramps_id.clone())
        }
        Some(Node::Event(data)) => {
            let event_type = data
                .event_type
                .as_ref()
                .map(|t| format!("{:?}", t))
                .unwrap_or_else(|| "Unknown".to_string());
            let date_str = if let Some(ref d) = data.date {
                d.text.clone().unwrap_or_else(|| format!("{:04}", d.year))
            } else {
                "no date".to_string()
            };
            // Find associated people (up to 3) by checking
            // PersonEventRef edges and FamilyEventRef edges.
            let incident = graph.edges_incident_to(handle);
            let mut people: Vec<String> = Vec::new();

            // Collect people from PersonEventRef edges (Person → Event)
            for person_h in incident.iter().filter_map(|e| match e {
                Edge::PersonEventRef { source, target, .. } if target == handle => {
                    Some(source.clone())
                }
                _ => None,
            }) {
                if people.len() >= 3 {
                    break;
                }
                if let Some(Node::Person(p)) = graph.get_node(&person_h) {
                    people.push(person_full_name(p));
                }
            }

            // Also check FamilyEventRef edges (Family → Event)
            if people.len() < 3 {
                for family_edge in incident.iter().filter_map(|e| match e {
                    Edge::FamilyEventRef { source, .. } => Some(source.clone()),
                    _ => None,
                }) {
                    // Resolve father/mother names for this family
                    let family_incident = graph.edges_incident_to(&family_edge);
                    for parent_h in family_incident.iter().filter_map(|e| match e {
                        Edge::FamilyFather { target, .. } | Edge::FamilyMother { target, .. } => {
                            Some(target.clone())
                        }
                        _ => None,
                    }) {
                        if people.len() >= 3 {
                            break;
                        }
                        if let Some(Node::Person(p)) = graph.get_node(&parent_h) {
                            people.push(person_full_name(p));
                        }
                    }
                }
            }
            let desc = match people.len() {
                0 => format!("{} event ({})", event_type, date_str),
                1 => format!("{} event ({}) — {}", event_type, date_str, people[0]),
                _ => format!(
                    "{} event ({}) — {}",
                    event_type,
                    date_str,
                    people.join(", ")
                ),
            };
            (desc, data.gramps_id.clone())
        }
        Some(Node::Place(data)) => {
            let desc = data.title.as_deref().unwrap_or("Unnamed Place").to_string();
            (desc, data.gramps_id.clone())
        }
        Some(Node::Source(data)) => {
            let desc = if !data.title.is_empty() {
                data.title.clone()
            } else if let Some(ref author) = data.author {
                if !author.is_empty() {
                    format!("Source by {}", author)
                } else {
                    fallback_source(data)
                }
            } else {
                fallback_source(data)
            };
            (desc, data.gramps_id.clone())
        }
        Some(Node::Citation(data)) => {
            // Try data field first, then fall back to CitationSource edge
            let source_handle = get_source_handle(&data.source_handle);
            let source_handle = if source_handle.is_empty() {
                // Fall back to CitationSource edge
                graph
                    .edges_incident_to(handle)
                    .iter()
                    .filter_map(|e| match e {
                        Edge::CitationSource { source, target } if source == handle => {
                            Some(target.clone())
                        }
                        _ => None,
                    })
                    .next()
                    .unwrap_or_default()
            } else {
                source_handle
            };
            let source_desc = if !source_handle.is_empty() {
                graph
                    .get_node(&source_handle)
                    .map(|n| {
                        if let Node::Source(s) = n {
                            let id = format_gramps_id(&s.gramps_id);
                            if s.title.is_empty() {
                                format!("source({})", s.handle)
                            } else {
                                format!("source{} \"{}\"", id, s.title)
                            }
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let source_str = if source_desc.is_empty() {
                "no source".to_string()
            } else {
                source_desc
            };
            let page = data.page.as_deref().unwrap_or("no page");
            let desc = format!("Citation → {}, p. {}", source_str, page);
            (desc, data.gramps_id.clone())
        }
        Some(Node::Repository(data)) => {
            let desc = data
                .name
                .as_deref()
                .unwrap_or("Unnamed Repository")
                .to_string();
            (desc, data.gramps_id.clone())
        }
        Some(Node::Media(data)) => {
            if let Some(ref desc) = data.desc {
                if !desc.is_empty() {
                    return (desc.clone(), data.gramps_id.clone());
                }
            };
            let desc = if let Some(ref path) = data.path {
                if !path.is_empty() {
                    format!("Media: {}", path)
                } else if let Some(ref date) = data.date {
                    let d = date
                        .text
                        .clone()
                        .unwrap_or_else(|| format!("{:04}", date.year));
                    format!("Media from {}", d)
                } else {
                    "Unnamed Media".to_string()
                }
            } else if let Some(ref date) = data.date {
                let d = date
                    .text
                    .clone()
                    .unwrap_or_else(|| format!("{:04}", date.year));
                format!("Media from {}", d)
            } else {
                "Unnamed Media".to_string()
            };
            (desc, data.gramps_id.clone())
        }
        Some(Node::Note(data)) => {
            let text_preview = if data.text.len() > 150 {
                let trunc = &data.text[..150];
                // Break at last space within the 150-char window if possible
                if let Some(last_space) = trunc.rfind(' ') {
                    format!("{}...", &trunc[..last_space])
                } else {
                    format!("{}...", trunc)
                }
            } else if data.text.is_empty() {
                "empty note".to_string()
            } else {
                data.text.clone()
            };
            let desc = format!("Note: \"{}\"", text_preview);
            (desc, data.gramps_id.clone())
        }
        Some(Node::Tag(data)) => {
            let desc = if data.name.is_empty() {
                format!("Tag ({})", handle)
            } else {
                data.name.clone()
            };
            (desc, data.gramps_id.clone())
        }
        None => (format!("Unknown node ({})", handle), None),
    }
}

/// Convert a plural type name to a title for display.
fn to_title(plural: &str) -> &str {
    match plural {
        "people" => "People",
        "families" => "Families",
        "events" => "Events",
        "places" => "Places",
        "citations" => "Citations",
        "sources" => "Sources",
        "repositories" => "Repositories",
        "media" => "Media",
        "notes" => "Notes",
        "tags" => "Tags",
        _ => plural,
    }
}

/// Convert a plural type name to a singular form.
fn to_singular(plural: &str) -> &str {
    match plural {
        "people" => "person",
        "families" => "family",
        "events" => "event",
        "places" => "place",
        "citations" => "citation",
        "sources" => "source",
        "repositories" => "repository",
        "media" => "media object",
        "notes" => "note",
        "tags" => "tag",
        _ => plural,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a graph with a person node.
    fn person_graph() -> (Graph, Handle) {
        let mut graph = Graph::new();
        let h = "p0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: h.clone(),
                    gramps_id: Some("I0001".to_string()),
                    primary_name: typed_graph::Name {
                        first_name: Some("John".to_string()),
                        surname_list: vec![typed_graph::Surname {
                            surname: Some("Smith".to_string()),
                            ..typed_graph::Surname::default()
                        }],
                        ..typed_graph::Name::default()
                    },
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        (graph, h)
    }

    // -----------------------------------------------------------------------
    // ReviewAction tests
    // -----------------------------------------------------------------------

    #[test]
    fn review_action_variants() {
        assert_eq!(ReviewAction::DeleteAll, ReviewAction::DeleteAll);
        assert_eq!(ReviewAction::KeepAll, ReviewAction::KeepAll);
        assert_eq!(ReviewAction::Abort, ReviewAction::Abort);
    }

    #[test]
    fn review_action_remove_handles_eq() {
        let mut set1 = HashSet::new();
        set1.insert("p1".to_string());
        let mut set2 = HashSet::new();
        set2.insert("p1".to_string());
        assert_eq!(
            ReviewAction::RemoveHandles(set1),
            ReviewAction::RemoveHandles(set2)
        );
    }

    // -----------------------------------------------------------------------
    // ReviewResult tests
    // -----------------------------------------------------------------------

    #[test]
    fn review_result_aborted() {
        assert_eq!(ReviewResult::Aborted, ReviewResult::Aborted);
    }

    #[test]
    fn review_result_confirmed_empty() {
        let map = HashMap::new();
        assert_eq!(
            ReviewResult::Confirmed(map.clone()),
            ReviewResult::Confirmed(map)
        );
    }

    // -----------------------------------------------------------------------
    // describe_node tests
    // -----------------------------------------------------------------------

    #[test]
    fn describe_person_node() {
        let (graph, handle) = person_graph();
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "John Smith");
        assert_eq!(gramps_id, Some("I0001".to_string()));
    }

    #[test]
    fn describe_unknown_node() {
        let graph = Graph::new();
        let (desc, gramps_id) = describe_node(&graph, &"nonexistent".to_string());
        assert_eq!(desc, "Unknown node (nonexistent)");
        assert_eq!(gramps_id, None);
    }

    #[test]
    fn describe_event_node() {
        let mut graph = Graph::new();
        let h = "e0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Event(typed_graph::EventData {
                    handle: h.clone(),
                    gramps_id: Some("E0001".to_string()),
                    event_type: Some(typed_graph::EventType::Birth),
                    date: Some(typed_graph::DateValue {
                        year: 1850,
                        ..typed_graph::DateValue::default()
                    }),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        let (desc, gramps_id) = describe_node(&graph, &h);
        assert!(desc.contains("Birth"));
        assert_eq!(gramps_id, Some("E0001".to_string()));
    }

    #[test]
    fn describe_place_node() {
        let mut graph = Graph::new();
        let h = "pl0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Place(typed_graph::PlaceData {
                    handle: h.clone(),
                    gramps_id: Some("P0001".to_string()),
                    title: Some("New York".to_string()),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();
        let (desc, gramps_id) = describe_node(&graph, &h);
        assert_eq!(desc, "New York");
        assert_eq!(gramps_id, Some("P0001".to_string()));
    }

    #[test]
    fn describe_source_node() {
        let mut graph = Graph::new();
        let h = "s0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: h.clone(),
                    gramps_id: Some("S0001".to_string()),
                    title: "Census Record".to_string(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        let (desc, gramps_id) = describe_node(&graph, &h);
        assert_eq!(desc, "Census Record");
        assert_eq!(gramps_id, Some("S0001".to_string()));
    }

    #[test]
    fn describe_empty_source() {
        let mut graph = Graph::new();
        let h = "s0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: h.clone(),
                    gramps_id: Some("S0001".to_string()),
                    title: String::new(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        let (desc, gramps_id) = describe_node(&graph, &h);
        assert_eq!(desc, "Unnamed Source");
        assert_eq!(gramps_id, Some("S0001".to_string()));
    }

    // -----------------------------------------------------------------------
    // yes_mode test
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // person_full_name tests
    // -----------------------------------------------------------------------

    #[test]
    fn person_full_name_with_surname() {
        let data = typed_graph::PersonData {
            primary_name: typed_graph::Name {
                first_name: Some("John".to_string()),
                surname_list: vec![typed_graph::Surname {
                    surname: Some("Smith".to_string()),
                    ..typed_graph::Surname::default()
                }],
                ..typed_graph::Name::default()
            },
            ..typed_graph::PersonData::default()
        };
        assert_eq!(person_full_name(&data), "John Smith");
    }

    #[test]
    fn person_full_name_no_surname() {
        let data = typed_graph::PersonData {
            primary_name: typed_graph::Name {
                first_name: Some("Mona".to_string()),
                ..typed_graph::Name::default()
            },
            ..typed_graph::PersonData::default()
        };
        assert_eq!(person_full_name(&data), "Mona");
    }

    #[test]
    fn person_full_name_unknown_first() {
        let data = typed_graph::PersonData {
            primary_name: typed_graph::Name::default(),
            ..typed_graph::PersonData::default()
        };
        assert_eq!(person_full_name(&data), "Unknown");
    }

    #[test]
    fn person_full_name_first_only() {
        let data = typed_graph::PersonData {
            primary_name: typed_graph::Name {
                first_name: Some("Cher".to_string()),
                surname_list: vec![typed_graph::Surname {
                    surname: Some("".to_string()),
                    ..typed_graph::Surname::default()
                }],
                ..typed_graph::Name::default()
            },
            ..typed_graph::PersonData::default()
        };
        assert_eq!(person_full_name(&data), "Cher");
    }

    // -----------------------------------------------------------------------
    // Enhanced Person description tests
    // -----------------------------------------------------------------------

    /// Helper: create a graph with a person who has a birth and/or death event.
    fn person_with_events_graph(
        gramps_id: Option<&str>,
        first_name: Option<&str>,
        surname: Option<&str>,
        birth_year: Option<i32>,
        death_year: Option<i32>,
    ) -> (Graph, Handle) {
        let mut graph = Graph::new();
        let handle = "p0001".to_string();

        let mut event_ref_list = Vec::new();
        let mut birth_ref_index = None;
        let mut death_ref_index = None;

        if let Some(year) = birth_year {
            let event_h = format!("e_birth_{}", handle);
            graph
                .add_node(
                    event_h.clone(),
                    Node::Event(typed_graph::EventData {
                        handle: event_h.clone(),
                        gramps_id: None,
                        event_type: Some(typed_graph::EventType::Birth),
                        date: Some(typed_graph::DateValue {
                            year,
                            text: Some(format!("{:04}", year)),
                            ..typed_graph::DateValue::default()
                        }),
                        ..typed_graph::EventData::default()
                    }),
                )
                .unwrap();
            birth_ref_index = Some(event_ref_list.len() as i32);
            event_ref_list.push(typed_graph::EventRef {
                ref_field: event_h,
                ..typed_graph::EventRef::default()
            });
        }

        if let Some(year) = death_year {
            let event_h = format!("e_death_{}", handle);
            graph
                .add_node(
                    event_h.clone(),
                    Node::Event(typed_graph::EventData {
                        handle: event_h.clone(),
                        gramps_id: None,
                        event_type: Some(typed_graph::EventType::Death),
                        date: Some(typed_graph::DateValue {
                            year,
                            text: Some(format!("{:04}", year)),
                            ..typed_graph::DateValue::default()
                        }),
                        ..typed_graph::EventData::default()
                    }),
                )
                .unwrap();
            death_ref_index = Some(event_ref_list.len() as i32);
            event_ref_list.push(typed_graph::EventRef {
                ref_field: event_h,
                ..typed_graph::EventRef::default()
            });
        }

        let mut primary_name = typed_graph::Name {
            ..typed_graph::Name::default()
        };
        if let Some(fn_val) = first_name {
            primary_name.first_name = Some(fn_val.to_string());
        }
        if let Some(sn_val) = surname {
            primary_name.surname_list = vec![typed_graph::Surname {
                surname: Some(sn_val.to_string()),
                ..typed_graph::Surname::default()
            }];
        }

        graph
            .add_node(
                handle.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: handle.clone(),
                    gramps_id: gramps_id.map(|s| s.to_string()),
                    primary_name,
                    birth_ref_index,
                    death_ref_index,
                    event_ref_list,
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        (graph, handle)
    }

    #[test]
    fn describe_person_birth_and_death() {
        let (graph, handle) = person_with_events_graph(
            Some("I0001"),
            Some("John"),
            Some("Smith"),
            Some(1800),
            Some(1875),
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "John Smith (1800-1875)");
        assert_eq!(gramps_id, Some("I0001".to_string()));
    }

    #[test]
    fn describe_person_only_birth() {
        let (graph, handle) =
            person_with_events_graph(Some("I0002"), Some("Jane"), Some("Doe"), Some(1810), None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Jane Doe (b. 1810)");
        assert_eq!(gramps_id, Some("I0002".to_string()));
    }

    #[test]
    fn describe_person_only_death() {
        let (graph, handle) =
            person_with_events_graph(Some("I0003"), Some("Bob"), Some("White"), None, Some(1900));
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Bob White (d. 1900)");
        assert_eq!(gramps_id, Some("I0003".to_string()));
    }

    #[test]
    fn describe_person_no_dates() {
        let (graph, handle) =
            person_with_events_graph(Some("I0004"), Some("Eve"), Some("Brown"), None, None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Eve Brown");
        assert_eq!(gramps_id, Some("I0004".to_string()));
    }

    #[test]
    fn describe_person_no_gramps_id() {
        let (graph, handle) =
            person_with_events_graph(None, Some("No"), Some("Id"), Some(1950), None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "No Id (b. 1950)");
        assert_eq!(gramps_id, None);
    }

    // -----------------------------------------------------------------------
    // Enhanced Family description tests
    // -----------------------------------------------------------------------

    /// Helper: create a graph with a family with father, mother, and children.
    fn family_graph(
        gramps_id: Option<&str>,
        father_name: Option<(&str, &str)>,
        mother_name: Option<(&str, &str)>,
        child_names: Vec<(&str, &str)>,
    ) -> (Graph, Handle) {
        let mut graph = Graph::new();

        let add_person = |graph: &mut Graph, handle: &str, first: &str, surname: &str| {
            graph
                .add_node(
                    handle.to_string(),
                    Node::Person(typed_graph::PersonData {
                        handle: handle.to_string(),
                        gramps_id: None,
                        primary_name: typed_graph::Name {
                            first_name: Some(first.to_string()),
                            surname_list: vec![typed_graph::Surname {
                                surname: Some(surname.to_string()),
                                ..typed_graph::Surname::default()
                            }],
                            ..typed_graph::Name::default()
                        },
                        ..typed_graph::PersonData::default()
                    }),
                )
                .unwrap();
        };

        let father_handle = "f1".to_string();
        let mother_handle = "m1".to_string();
        let family_h = "fam0001".to_string();

        if let Some((first, surname)) = father_name {
            add_person(&mut graph, &father_handle, first, surname);
        }
        if let Some((first, surname)) = mother_name {
            add_person(&mut graph, &mother_handle, first, surname);
        }

        let child_ref_list: Vec<typed_graph::ChildRef> = child_names
            .iter()
            .enumerate()
            .map(|(i, (first, surname))| {
                let child_h = format!("c{}", i);
                add_person(&mut graph, &child_h, first, surname);
                typed_graph::ChildRef {
                    ref_field: child_h,
                    ..typed_graph::ChildRef::default()
                }
            })
            .collect();

        graph
            .add_node(
                family_h.clone(),
                Node::Family(typed_graph::FamilyData {
                    handle: family_h.clone(),
                    gramps_id: gramps_id.map(|s| s.to_string()),
                    father_handle: father_name.map(|_| father_handle.clone()),
                    mother_handle: mother_name.map(|_| mother_handle.clone()),
                    child_ref_list,
                    ..typed_graph::FamilyData::default()
                }),
            )
            .unwrap();

        // Add edges for edge-based display (Step 3 of delete-tool fix)
        if father_name.is_some() {
            graph
                .add_edge(Edge::FamilyFather {
                    source: family_h.clone(),
                    target: father_handle.clone(),
                })
                .unwrap();
        }
        if mother_name.is_some() {
            graph
                .add_edge(Edge::FamilyMother {
                    source: family_h.clone(),
                    target: mother_handle.clone(),
                })
                .unwrap();
        }
        for (i, _) in child_names.iter().enumerate() {
            let child_h = format!("c{}", i);
            graph
                .add_edge(Edge::FamilyChildRef {
                    source: family_h.clone(),
                    target: child_h,
                    metadata: Box::new(typed_graph::ChildRef {
                        ref_field: family_h.clone(),
                        ..typed_graph::ChildRef::default()
                    }),
                })
                .unwrap();
        }

        (graph, family_h)
    }

    #[test]
    fn describe_family_with_both_parents_two_children() {
        let (graph, handle) = family_graph(
            Some("F0001"),
            Some(("John", "Smith")),
            Some(("Jane", "Doe")),
            vec![("Alice", "Smith"), ("Bob", "Smith")],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(
            desc,
            "Family: John Smith & Jane Doe | children: Alice Smith, Bob Smith"
        );
        assert_eq!(gramps_id, Some("F0001".to_string()));
    }

    #[test]
    fn describe_family_with_parents_one_child() {
        let (graph, handle) = family_graph(
            Some("F0002"),
            Some(("John", "Smith")),
            Some(("Jane", "Doe")),
            vec![("Alice", "Smith")],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Family: John Smith & Jane Doe | child: Alice Smith");
        assert_eq!(gramps_id, Some("F0002".to_string()));
    }

    #[test]
    fn describe_family_with_more_children_than_shown() {
        let (graph, handle) = family_graph(
            Some("F0003"),
            Some(("John", "Smith")),
            Some(("Jane", "Doe")),
            vec![
                ("Alice", "Smith"),
                ("Bob", "Smith"),
                ("Charlie", "Smith"),
                ("Diana", "Smith"),
            ],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(
            desc,
            "Family: John Smith & Jane Doe | children: Alice Smith, Bob Smith (+2 more)"
        );
        assert_eq!(gramps_id, Some("F0003".to_string()));
    }

    #[test]
    fn describe_family_no_parents_no_children() {
        let (graph, handle) = family_graph(Some("F0004"), None, None, vec![]);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert!(desc.contains("no parents"));
        assert_eq!(gramps_id, Some("F0004".to_string()));
    }

    #[test]
    fn describe_family_only_father() {
        let (graph, handle) = family_graph(
            Some("F0005"),
            Some(("John", "Smith")),
            None,
            vec![("Alice", "Smith")],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Family: John Smith | child: Alice Smith");
        assert_eq!(gramps_id, Some("F0005".to_string()));
    }

    #[test]
    fn describe_family_no_gramps_id() {
        let (graph, handle) =
            family_graph(None, Some(("John", "Smith")), Some(("Jane", "Doe")), vec![]);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert!(desc.contains("John Smith & Jane Doe"));
        assert_eq!(gramps_id, None);
    }

    // -----------------------------------------------------------------------
    // Enhanced Event description tests
    // -----------------------------------------------------------------------

    /// Helper: create a graph with an event and optionally associated people.
    fn event_with_people_graph(
        gramps_id: Option<&str>,
        event_type: Option<typed_graph::EventType>,
        year: Option<i32>,
        people: Vec<(&str, &str)>,
    ) -> (Graph, Handle) {
        let mut graph = Graph::new();
        let event_h = "e0001".to_string();

        graph
            .add_node(
                event_h.clone(),
                Node::Event(typed_graph::EventData {
                    handle: event_h.clone(),
                    gramps_id: gramps_id.map(|s| s.to_string()),
                    event_type,
                    date: year.map(|y| typed_graph::DateValue {
                        year: y,
                        text: Some(format!("{:04}", y)),
                        ..typed_graph::DateValue::default()
                    }),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();

        for (i, (first, surname)) in people.iter().enumerate() {
            let person_h = format!("p{}", i);
            graph
                .add_node(
                    person_h.clone(),
                    Node::Person(typed_graph::PersonData {
                        handle: person_h.clone(),
                        gramps_id: None,
                        primary_name: typed_graph::Name {
                            first_name: Some(first.to_string()),
                            surname_list: vec![typed_graph::Surname {
                                surname: Some(surname.to_string()),
                                ..typed_graph::Surname::default()
                            }],
                            ..typed_graph::Name::default()
                        },
                        ..typed_graph::PersonData::default()
                    }),
                )
                .unwrap();
            graph
                .add_edge(Edge::PersonEventRef {
                    source: person_h,
                    target: event_h.clone(),
                    metadata: Box::new(typed_graph::EventRef::default()),
                })
                .unwrap();
        }

        (graph, event_h)
    }

    #[test]
    fn describe_event_with_one_person() {
        let (graph, handle) = event_with_people_graph(
            Some("E0001"),
            Some(typed_graph::EventType::Birth),
            Some(1850),
            vec![("John", "Smith")],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Birth event (1850) — John Smith");
        assert_eq!(gramps_id, Some("E0001".to_string()));
    }

    #[test]
    fn describe_event_with_multiple_people() {
        let (graph, handle) = event_with_people_graph(
            Some("E0002"),
            Some(typed_graph::EventType::Marriage),
            Some(1900),
            vec![("John", "Smith"), ("Jane", "Doe")],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Marriage event (1900) — John Smith, Jane Doe");
        assert_eq!(gramps_id, Some("E0002".to_string()));
    }

    #[test]
    fn describe_event_with_no_people() {
        let (graph, handle) = event_with_people_graph(
            Some("E0003"),
            Some(typed_graph::EventType::Census),
            None,
            vec![],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Census event (no date)");
        assert_eq!(gramps_id, Some("E0003".to_string()));
    }

    #[test]
    fn describe_event_no_gramps_id() {
        let (graph, handle) = event_with_people_graph(
            None,
            Some(typed_graph::EventType::Birth),
            Some(2000),
            vec![("Alice", "Brown")],
        );
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Birth event (2000) — Alice Brown");
        assert_eq!(gramps_id, None);
    }

    #[test]
    fn describe_event_with_family_event_ref() {
        let mut graph = Graph::new();

        // Create family with father and mother
        let family_h = "fam0001".to_string();
        let father_h = "f1".to_string();
        let mother_h = "m1".to_string();

        graph
            .add_node(
                father_h.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: father_h.clone(),
                    primary_name: typed_graph::Name {
                        first_name: Some("John".to_string()),
                        surname_list: vec![typed_graph::Surname {
                            surname: Some("Smith".to_string()),
                            ..typed_graph::Surname::default()
                        }],
                        ..typed_graph::Name::default()
                    },
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                mother_h.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: mother_h.clone(),
                    primary_name: typed_graph::Name {
                        first_name: Some("Jane".to_string()),
                        surname_list: vec![typed_graph::Surname {
                            surname: Some("Smith".to_string()),
                            ..typed_graph::Surname::default()
                        }],
                        ..typed_graph::Name::default()
                    },
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                family_h.clone(),
                Node::Family(typed_graph::FamilyData {
                    handle: family_h.clone(),
                    ..typed_graph::FamilyData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::FamilyFather {
                source: family_h.clone(),
                target: father_h.clone(),
            })
            .unwrap();
        graph
            .add_edge(Edge::FamilyMother {
                source: family_h.clone(),
                target: mother_h.clone(),
            })
            .unwrap();

        // Create event with FamilyEventRef
        let event_h = "e0001".to_string();
        graph
            .add_node(
                event_h.clone(),
                Node::Event(typed_graph::EventData {
                    handle: event_h.clone(),
                    gramps_id: Some("E0001".to_string()),
                    event_type: Some(typed_graph::EventType::Marriage),
                    date: Some(typed_graph::DateValue {
                        year: 1900,
                        text: Some("1900".to_string()),
                        ..typed_graph::DateValue::default()
                    }),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::FamilyEventRef {
                source: family_h,
                target: event_h.clone(),
                metadata: Box::new(typed_graph::EventRef {
                    ref_field: event_h.clone(),
                    ..typed_graph::EventRef::default()
                }),
            })
            .unwrap();

        let (desc, gramps_id) = describe_node(&graph, &event_h);
        assert_eq!(desc, "Marriage event (1900) — John Smith, Jane Smith");
        assert_eq!(gramps_id, Some("E0001".to_string()));
    }

    // -----------------------------------------------------------------------
    // Enhanced Citation description tests
    // -----------------------------------------------------------------------

    /// Helper: create a graph with a citation and optionally a source.
    fn citation_graph(
        gramps_id: Option<&str>,
        page: Option<&str>,
        source_title: Option<&str>,
    ) -> (Graph, Handle) {
        let mut graph = Graph::new();
        let citation_h = "c0001".to_string();
        let source_h = "s0001".to_string();

        if let Some(title) = source_title {
            graph
                .add_node(
                    source_h.clone(),
                    Node::Source(typed_graph::SourceData {
                        handle: source_h.clone(),
                        gramps_id: Some("S0001".to_string()),
                        title: title.to_string(),
                        ..typed_graph::SourceData::default()
                    }),
                )
                .unwrap();
        }

        let source_handle_val = if source_title.is_some() {
            source_h.clone()
        } else {
            String::new()
        };

        graph
            .add_node(
                citation_h.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: citation_h.clone(),
                    gramps_id: gramps_id.map(|s| s.to_string()),
                    source_handle: Some(source_handle_val),
                    page: page.map(|s| s.to_string()),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();

        // Add CitationSource edge for edge-based display
        if source_title.is_some() {
            graph
                .add_edge(Edge::CitationSource {
                    source: citation_h.clone(),
                    target: source_h.clone(),
                })
                .unwrap();
        }

        (graph, citation_h)
    }

    #[test]
    fn describe_citation_with_source() {
        let (graph, handle) = citation_graph(Some("C0001"), Some("42"), Some("Census Record"));
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Citation → source [S0001] \"Census Record\", p. 42");
        assert_eq!(gramps_id, Some("C0001".to_string()));
    }

    #[test]
    fn describe_citation_no_source() {
        let (graph, handle) = citation_graph(Some("C0002"), Some("5"), None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Citation → no source, p. 5");
        assert_eq!(gramps_id, Some("C0002".to_string()));
    }

    #[test]
    fn describe_citation_no_page() {
        let (graph, handle) = citation_graph(Some("C0003"), None, Some("Marriage Record"));
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(
            desc,
            "Citation → source [S0001] \"Marriage Record\", p. no page"
        );
        assert_eq!(gramps_id, Some("C0003".to_string()));
    }

    #[test]
    fn describe_citation_no_gramps_id() {
        let (graph, handle) = citation_graph(None, Some("10"), Some("Census Record"));
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert!(desc.contains("Census Record"));
        assert_eq!(gramps_id, None);
    }

    #[test]
    fn describe_citation_edge_fallback_no_data() {
        // Citation with no source_handle in data but with CitationSource edge
        let mut graph = Graph::new();
        let citation_h = "c0001".to_string();
        let source_h = "s0001".to_string();

        graph
            .add_node(
                source_h.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: source_h.clone(),
                    gramps_id: Some("S0001".to_string()),
                    title: "Marriage Record".to_string(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        graph
            .add_node(
                citation_h.clone(),
                Node::Citation(typed_graph::CitationData {
                    handle: citation_h.clone(),
                    gramps_id: Some("C0001".to_string()),
                    source_handle: None,
                    page: Some("p. 12".to_string()),
                    ..typed_graph::CitationData::default()
                }),
            )
            .unwrap();
        graph
            .add_edge(Edge::CitationSource {
                source: citation_h.clone(),
                target: source_h.clone(),
            })
            .unwrap();

        let (desc, gramps_id) = describe_node(&graph, &citation_h);
        assert_eq!(
            desc,
            "Citation → source [S0001] \"Marriage Record\", p. p. 12"
        );
        assert_eq!(gramps_id, Some("C0001".to_string()));
    }

    // -----------------------------------------------------------------------
    // Enhanced Source description tests
    // -----------------------------------------------------------------------

    /// Helper: create a graph with a Source node.
    fn source_data(
        gramps_id: Option<&str>,
        title: &str,
        author: Option<&str>,
        pubinfo: Option<&str>,
    ) -> (Graph, Handle) {
        let mut graph = Graph::new();
        let h = "s0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Source(typed_graph::SourceData {
                    handle: h.clone(),
                    gramps_id: gramps_id.map(|s| s.to_string()),
                    title: title.to_string(),
                    author: author.map(|s| s.to_string()),
                    pubinfo: pubinfo.map(|s| s.to_string()),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        (graph, h)
    }

    #[test]
    fn describe_source_with_title() {
        let (graph, handle) = source_data(Some("S0001"), "Census Record", None, None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Census Record");
        assert_eq!(gramps_id, Some("S0001".to_string()));
    }

    #[test]
    fn describe_source_fallback_to_author() {
        let (graph, handle) = source_data(Some("S0002"), "", Some("John Doe"), None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Source by John Doe");
        assert_eq!(gramps_id, Some("S0002".to_string()));
    }

    #[test]
    fn describe_source_fallback_to_pubinfo() {
        let (graph, handle) = source_data(Some("S0003"), "", None, Some("Genealogical Society"));
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Source: Genealogical Society");
        assert_eq!(gramps_id, Some("S0003".to_string()));
    }

    #[test]
    fn describe_source_all_fallbacks_empty() {
        let (graph, handle) = source_data(Some("S0004"), "", None, None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Unnamed Source");
        assert_eq!(gramps_id, Some("S0004".to_string()));
    }

    #[test]
    fn describe_source_no_gramps_id() {
        let (graph, handle) = source_data(None, "Census Record", None, None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Census Record");
        assert_eq!(gramps_id, None);
    }

    // -----------------------------------------------------------------------
    // Enhanced Media description tests
    // -----------------------------------------------------------------------

    /// Helper: create a graph with a Media node.
    fn media_data(
        gramps_id: Option<&str>,
        desc: Option<&str>,
        path: Option<&str>,
        date_year: Option<i32>,
    ) -> (Graph, Handle) {
        let mut graph = Graph::new();
        let h = "m0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Media(typed_graph::MediaData {
                    handle: h.clone(),
                    gramps_id: gramps_id.map(|s| s.to_string()),
                    desc: desc.map(|s| s.to_string()),
                    path: path.map(|s| s.to_string()),
                    date: date_year.map(|y| typed_graph::DateValue {
                        year: y,
                        text: Some(format!("{:04}", y)),
                        ..typed_graph::DateValue::default()
                    }),
                    ..typed_graph::MediaData::default()
                }),
            )
            .unwrap();
        (graph, h)
    }

    #[test]
    fn describe_media_with_desc() {
        let (graph, handle) = media_data(Some("M0001"), Some("Wedding photo"), None, None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Wedding photo");
        assert_eq!(gramps_id, Some("M0001".to_string()));
    }

    #[test]
    fn describe_media_fallback_to_path() {
        let (graph, handle) = media_data(Some("M0002"), None, Some("photos/wedding.jpg"), None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Media: photos/wedding.jpg");
        assert_eq!(gramps_id, Some("M0002".to_string()));
    }

    #[test]
    fn describe_media_fallback_to_date() {
        let (graph, handle) = media_data(Some("M0003"), None, None, Some(1980));
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Media from 1980");
        assert_eq!(gramps_id, Some("M0003".to_string()));
    }

    #[test]
    fn describe_media_all_fallbacks_empty() {
        let (graph, handle) = media_data(Some("M0004"), None, None, None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Unnamed Media");
        assert_eq!(gramps_id, Some("M0004".to_string()));
    }

    #[test]
    fn describe_media_no_gramps_id() {
        let (graph, handle) = media_data(None, Some("Portrait"), None, None);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Portrait");
        assert_eq!(gramps_id, None);
    }

    // -----------------------------------------------------------------------
    // Enhanced Note description tests
    // -----------------------------------------------------------------------

    /// Helper: create a graph with a Note node.
    fn note_data(gramps_id: Option<&str>, text: &str) -> (Graph, Handle) {
        let mut graph = Graph::new();
        let h = "n0001".to_string();
        graph
            .add_node(
                h.clone(),
                Node::Note(typed_graph::NoteData {
                    handle: h.clone(),
                    gramps_id: gramps_id.map(|s| s.to_string()),
                    text: text.to_string(),
                    ..typed_graph::NoteData::default()
                }),
            )
            .unwrap();
        (graph, h)
    }

    #[test]
    fn describe_note_short_text() {
        let (graph, handle) = note_data(Some("N0001"), "Short note");
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Note: \"Short note\"");
        assert_eq!(gramps_id, Some("N0001".to_string()));
    }

    #[test]
    fn describe_note_empty_text() {
        let (graph, handle) = note_data(Some("N0002"), "");
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert!(desc.contains("empty note"));
        assert_eq!(gramps_id, Some("N0002".to_string()));
    }

    #[test]
    fn describe_note_long_text_word_boundary() {
        // Create a note longer than 150 chars with a space at the boundary
        let long_text = "Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam quis nostrud.";
        assert!(long_text.len() > 150); // verify it triggers truncation
        let (graph, handle) = note_data(Some("N0003"), long_text);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert!(desc.ends_with("...\""));
        assert!(desc.len() < long_text.len() + 15);
        assert!(desc.len() > 20);
        assert_eq!(gramps_id, Some("N0003".to_string()));
    }

    #[test]
    fn describe_note_long_text_no_space() {
        // Create a 160-char string with no spaces in first 150 chars
        let long_text = "a".repeat(160);
        let (graph, handle) = note_data(Some("N0004"), &long_text);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert!(desc.ends_with("...\""));
        assert_eq!(gramps_id, Some("N0004".to_string()));
    }

    #[test]
    fn describe_note_exact_150_chars() {
        let exact_text = "x".repeat(150);
        let (graph, handle) = note_data(Some("N0005"), &exact_text);
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert!(!desc.ends_with("...\"")); // no truncation needed
        assert!(desc.contains("Note:"));
        assert_eq!(gramps_id, Some("N0005".to_string()));
    }

    #[test]
    fn describe_note_no_gramps_id() {
        let (graph, handle) = note_data(None, "Hello");
        let (desc, gramps_id) = describe_node(&graph, &handle);
        assert_eq!(desc, "Note: \"Hello\"");
        assert_eq!(gramps_id, None);
    }

    // -----------------------------------------------------------------------
    // Step counter tests
    // -----------------------------------------------------------------------

    #[test]
    fn step_counter_dynamic_total() {
        // Set up a plan with only 3 types having candidates (out of 10)
        let mut graph = Graph::new();
        let mut per_type = HashMap::new();
        let mut seed_people = HashSet::new();
        let mut pre_connectivity = HashMap::new();

        // Add a person
        let p_h = "p0001".to_string();
        graph
            .add_node(
                p_h.clone(),
                Node::Person(typed_graph::PersonData {
                    handle: p_h.clone(),
                    gramps_id: Some("I0001".to_string()),
                    primary_name: typed_graph::Name {
                        first_name: Some("John".to_string()),
                        surname_list: vec![typed_graph::Surname {
                            surname: Some("Smith".to_string()),
                            ..typed_graph::Surname::default()
                        }],
                        ..typed_graph::Name::default()
                    },
                    ..typed_graph::PersonData::default()
                }),
            )
            .unwrap();
        seed_people.insert(p_h.clone());
        pre_connectivity.insert(p_h.clone(), 1usize);

        // 3 types with candidates
        per_type.insert(NodeKindLabel::Person, vec![p_h.clone()]);
        per_type.insert(NodeKindLabel::Event, vec!["e0001".to_string()]);
        per_type.insert(NodeKindLabel::Place, vec!["pl0001".to_string()]);

        let plan = DeletePlan {
            to_delete: seed_people.clone(),
            pre_connectivity,
            seed_people,
            per_type,
        };

        // Feed 'y' for each type to confirm
        let input_data = b"y\ny\ny\n";
        let mut input = std::io::BufReader::new(&input_data[..]);
        let mut output = Vec::new();

        let result = run_interactive_review(&plan, &graph, false, &mut input, &mut output);
        let output_str = String::from_utf8(output).unwrap();

        // Verify the step counter shows dynamic total (3, not 10)
        assert!(
            output_str.contains("Step 1/3"),
            "Expected 'Step 1/3' in output, got: {}",
            output_str
        );
        assert!(
            output_str.contains("Step 2/3"),
            "Expected 'Step 2/3' in output"
        );
        assert!(
            output_str.contains("Step 3/3"),
            "Expected 'Step 3/3' in output"
        );
        assert!(!output_str.contains("Step 4/"), "Should not have a Step 4");

        match result {
            ReviewResult::Confirmed(confirmed) => {
                assert_eq!(confirmed.len(), 3, "Expected 3 confirmed types");
            }
            ReviewResult::Aborted => panic!("Expected confirmed"),
        }
    }

    // -----------------------------------------------------------------------
    // format_gramps_id tests
    // -----------------------------------------------------------------------

    #[test]
    fn format_gramps_id_some() {
        let id = Some("I0001".to_string());
        assert_eq!(format_gramps_id(&id), " [I0001]");
    }

    #[test]
    fn format_gramps_id_none() {
        let id: Option<String> = None;
        assert_eq!(format_gramps_id(&id), "");
    }

    #[test]
    fn format_gramps_id_empty() {
        let id = Some(String::new());
        assert_eq!(format_gramps_id(&id), "");
    }

    // -----------------------------------------------------------------------
    // fallback_source tests
    // -----------------------------------------------------------------------

    #[test]
    fn fallback_source_with_pubinfo() {
        let data = typed_graph::SourceData {
            pubinfo: Some("Genealogical Society".to_string()),
            ..typed_graph::SourceData::default()
        };
        assert_eq!(fallback_source(&data), "Source: Genealogical Society");
    }

    #[test]
    fn fallback_source_empty_pubinfo() {
        let data = typed_graph::SourceData {
            pubinfo: Some(String::new()),
            ..typed_graph::SourceData::default()
        };
        assert_eq!(fallback_source(&data), "Unnamed Source");
    }

    #[test]
    fn fallback_source_no_pubinfo() {
        let data = typed_graph::SourceData {
            pubinfo: None,
            ..typed_graph::SourceData::default()
        };
        assert_eq!(fallback_source(&data), "Unnamed Source");
    }

    // -----------------------------------------------------------------------
    // yes_mode test
    // -----------------------------------------------------------------------

    #[test]
    fn yes_mode_confirms_all() {
        let (graph, _) = person_graph();

        // Create a simple DeletePlan with one person
        let mut per_type = HashMap::new();
        let mut seed_people = HashSet::new();
        seed_people.insert("p0001".to_string());
        let mut pre_connectivity = HashMap::new();
        pre_connectivity.insert("p0001".to_string(), 1usize);

        let handles = vec!["p0001".to_string()];
        per_type.insert(NodeKindLabel::Person, handles);

        let plan = DeletePlan {
            to_delete: seed_people.clone(),
            pre_connectivity,
            seed_people,
            per_type,
        };

        let mut input = std::io::BufReader::new(std::io::Cursor::new(Vec::new()));
        let mut output = Vec::new();

        let result = run_interactive_review(&plan, &graph, true, &mut input, &mut output);
        match result {
            ReviewResult::Confirmed(confirmed) => {
                assert!(confirmed.contains_key("people"));
                let people = confirmed.get("people").unwrap();
                assert!(people.contains("p0001"));
            }
            ReviewResult::Aborted => panic!("Expected confirmed in yes_mode"),
        }
    }
}
