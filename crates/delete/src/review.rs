//! Interactive review CLI for deletion candidates.
//!
//! Presents deletion candidates to the user grouped by type in dependency
//! order. For each type, the user can confirm deletion, skip the type,
//! remove specific handles from the deletion set, list all candidates,
//! show a summary of remaining types, or abort the entire operation.

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{BufRead, Write};

use typed_graph::{Graph, Handle, Node};

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
                crate::cascade::node_kind_to_label(node).plural().to_string()
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
    let total_types = type_order.len();
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
        .map(|h| format!("  • {} — {}", h, describe_node(graph, h)))
        .collect();

    loop {
        let _ = writeln!(
            output,
            "\n═══ Step {}/{}: {} ═══",
            step, total, to_title(type_name)
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
                    let _ = writeln!(
                        output,
                        "  • {} — {}",
                        h,
                        describe_node(graph, h)
                    );
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
                let valid: HashSet<Handle> = to_keep.intersection(&candidate_set).cloned().collect();
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
                let _ = writeln!(output, "Unknown command. Type 'y', 'n', 'l', 'r', 's', or 'q'.");
            }
        }
    }
}

/// Build a full name string from a PersonData node.
#[allow(dead_code)]
fn person_full_name(p: &typed_graph::PersonData) -> String {
    let first = p
        .primary_name
        .first_name
        .as_deref()
        .unwrap_or("Unknown");
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
#[allow(dead_code)]
fn format_gramps_id(gramps_id: &Option<String>) -> String {
    gramps_id
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|id| format!(" [{}]", id))
        .unwrap_or_default()
}

/// Build a fallback description string when a Source has no title or author.
#[allow(dead_code)]
fn fallback_source(data: &typed_graph::SourceData) -> String {
    if let Some(ref pubinfo) = data.pubinfo {
        if !pubinfo.is_empty() {
            return format!("Source: {}", pubinfo);
        }
    }
    "Unnamed Source".to_string()
}

/// Generate a human-readable description of a node.
fn describe_node(graph: &Graph, handle: &Handle) -> String {
    match graph.get_node(handle) {
        Some(Node::Person(data)) => {
            let name = data
                .primary_name
                .first_name
                .as_deref()
                .unwrap_or("Unknown");
            let surname = data
                .primary_name
                .surname_list
                .first()
                .and_then(|s| s.surname.as_deref())
                .unwrap_or("");
            if surname.is_empty() {
                name.to_string()
            } else {
                format!("{} {}", name, surname)
            }
        }
        Some(Node::Family(_data)) => {
            format!("Family ({})", handle)
        }
        Some(Node::Event(data)) => {
            let event_type = data
                .event_type
                .as_ref()
                .map(|t| format!("{:?}", t))
                .unwrap_or_else(|| "Unknown".to_string());
            let date_str = if let Some(ref d) = data.date {
                d.text
                    .clone()
                    .unwrap_or_else(|| format!("{:04}", d.year))
            } else {
                "no date".to_string()
            };
            format!("{} event ({})", event_type, date_str)
        }
        Some(Node::Place(data)) => {
            data.title
                .as_deref()
                .unwrap_or("Unnamed Place")
                .to_string()
        }
        Some(Node::Source(data)) => {
            if data.title.is_empty() {
                "Unnamed Source".to_string()
            } else {
                data.title.clone()
            }
        }
        Some(Node::Citation(data)) => {
            let page = data
                .page
                .as_deref()
                .unwrap_or("no page");
            format!("Citation (page: {})", page)
        }
        Some(Node::Repository(data)) => {
            data.name
                .as_deref()
                .unwrap_or("Unnamed Repository")
                .to_string()
        }
        Some(Node::Media(data)) => {
            data.desc
                .as_deref()
                .unwrap_or("Unnamed Media")
                .to_string()
        }
        Some(Node::Note(data)) => {
            let text_preview = if data.text.len() > 60 {
                format!("{}...", &data.text[..57])
            } else if data.text.is_empty() {
                "empty note".to_string()
            } else {
                data.text.clone()
            };
            format!("Note: \"{}\"", text_preview)
        }
        Some(Node::Tag(data)) => {
            if data.name.is_empty() {
                format!("Tag ({})", handle)
            } else {
                data.name.clone()
            }
        }
        None => format!("Unknown node ({})", handle),
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
        let desc = describe_node(&graph, &handle);
        assert_eq!(desc, "John Smith");
    }

    #[test]
    fn describe_unknown_node() {
        let graph = Graph::new();
        let desc = describe_node(&graph, &"nonexistent".to_string());
        assert_eq!(desc, "Unknown node (nonexistent)");
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
                    event_type: Some(typed_graph::EventType::Birth),
                    date: Some(typed_graph::DateValue {
                        year: 1850,
                        ..typed_graph::DateValue::default()
                    }),
                    ..typed_graph::EventData::default()
                }),
            )
            .unwrap();
        let desc = describe_node(&graph, &h);
        assert!(desc.contains("Birth"));
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
                    title: Some("New York".to_string()),
                    ..typed_graph::PlaceData::default()
                }),
            )
            .unwrap();
        let desc = describe_node(&graph, &h);
        assert_eq!(desc, "New York");
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
                    title: "Census Record".to_string(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        let desc = describe_node(&graph, &h);
        assert_eq!(desc, "Census Record");
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
                    title: String::new(),
                    ..typed_graph::SourceData::default()
                }),
            )
            .unwrap();
        let desc = describe_node(&graph, &h);
        assert_eq!(desc, "Unnamed Source");
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