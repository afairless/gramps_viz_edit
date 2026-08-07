//! Adapter: raw `ParsedPerson`/`ParsedFamily` records → visualization graph data.
//!
//! This module converts the streaming-extraction types into the structured
//! `PersonNode`, `FamilyLink`, and `GraphData` types that are serialized
//! over Tauri IPC to the frontend.

use std::collections::{HashMap, HashSet};

use gramps_reader::{compute_generations, FamilyRecord, ParsedFamily, ParsedPerson};
use serde::{Deserialize, Serialize};

/// A single person node in the force-directed graph.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PersonNode {
    pub handle: String,
    pub name: String,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
    pub birth_year: Option<i32>,
    pub is_imputed: bool,
    pub gender: String,
    pub family_group: usize,
    pub generation: usize,
}

/// A directed link between two person nodes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FamilyLink {
    pub source: String,
    pub target: String,
    pub link_type: LinkType,
}

/// The kind of relationship between two linked persons.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LinkType {
    Spouse,
    ParentChild,
}

/// Metadata about one connected component (family group).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FamilyGroupMeta {
    pub id: usize,
    pub size: usize,
    pub span: usize,
}

/// Complete graph data ready for serialization.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GraphData {
    pub nodes: Vec<PersonNode>,
    pub links: Vec<FamilyLink>,
    pub family_groups: Vec<FamilyGroupMeta>,
}

/// A person selected for export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedPerson {
    pub handle: String,
    pub name: String,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
    pub gender: String,
    pub family_group: usize,
}

/// Export payload for selected persons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionExport {
    pub exported_at: String,
    pub file: String,
    pub selections: Vec<SelectedPerson>,
}

/// Build a `GraphData` from extracted person and family records.
///
/// This is the main entry point of the adapter. It:
///
/// 1. Converts every `ParsedPerson` into a `PersonNode` (gender mapping
///    M→"male", F→"female", null→"unknown"; name assembly from given+surname).
/// 2. Converts every `ParsedFamily` into `FamilyLink`s (spouse between parents,
///    parent-child between each parent and each child).
/// 3. Runs DSU + generation layering via `gramps_reader::compute_generations`.
/// 4. Assigns family-group IDs and assembles the final `GraphData`.
pub fn build_graph_data(persons: &[ParsedPerson], families: &[ParsedFamily]) -> GraphData {
    // ---- 1. Convert persons to nodes ----
    let mut nodes: Vec<PersonNode> = persons.iter().map(person_to_node).collect();

    // ---- 2. Build FamilyRecord vec for generation layering ----
    let family_records: Vec<FamilyRecord> = families
        .iter()
        .map(|f| {
            let mut parent_handles = Vec::new();
            if let Some(ref h) = f.father_handle {
                parent_handles.push(h.clone());
            }
            if let Some(ref h) = f.mother_handle {
                parent_handles.push(h.clone());
            }
            FamilyRecord {
                parent_handles,
                child_handles: f.child_handles.clone(),
            }
        })
        .collect();

    let all_handles: HashSet<String> = persons.iter().map(|p| p.handle.clone()).collect();

    // ---- 3. Compute generations ----
    let gens = compute_generations(&family_records, &all_handles);

    // ---- 4. Assign generations and family-group IDs ----
    // DSU over persons to find connected components.
    let mut dsu = gramps_reader::Dsu::new();
    for p in persons {
        dsu.make_set(&p.handle);
    }
    for f in families {
        let mut members = Vec::new();
        if let Some(ref h) = f.father_handle {
            if all_handles.contains(h) {
                members.push(h.as_str());
            }
        }
        if let Some(ref h) = f.mother_handle {
            if all_handles.contains(h) {
                members.push(h.as_str());
            }
        }
        for h in &f.child_handles {
            if all_handles.contains(h) {
                members.push(h.as_str());
            }
        }
        for i in 1..members.len() {
            dsu.union(members[0], members[i]);
        }
    }

    // Map each handle to its component root.
    let mut component_of: HashMap<String, String> = HashMap::new();
    for p in persons {
        let root = dsu.find(&p.handle);
        component_of.insert(p.handle.clone(), root);
    }

    // Assign numeric IDs to components (sort roots for determinism).
    let mut roots: Vec<&str> = component_of.values().map(|s| s.as_str()).collect();
    roots.sort_unstable();
    roots.dedup();
    let component_id: HashMap<&str, usize> =
        roots.iter().enumerate().map(|(i, r)| (*r, i)).collect();

    // ---- 5. Apply generations and component IDs to nodes ----
    for node in &mut nodes {
        node.generation = gens.get(&node.handle).copied().unwrap_or(0);
        if let Some(root) = component_of.get(&node.handle) {
            node.family_group = component_id.get(root.as_str()).copied().unwrap_or(0);
        }
    }

    // ---- 6. Build links ----
    let mut links: Vec<FamilyLink> = Vec::new();
    for f in families {
        // Spouse link between father and mother (if both exist).
        if let (Some(father), Some(mother)) = (&f.father_handle, &f.mother_handle) {
            if all_handles.contains(father) && all_handles.contains(mother) {
                links.push(FamilyLink {
                    source: father.clone(),
                    target: mother.clone(),
                    link_type: LinkType::Spouse,
                });
            }
        }
        // Parent-child links.
        for child in &f.child_handles {
            if !all_handles.contains(child) {
                continue;
            }
            if let Some(ref father) = f.father_handle {
                if all_handles.contains(father) {
                    links.push(FamilyLink {
                        source: father.clone(),
                        target: child.clone(),
                        link_type: LinkType::ParentChild,
                    });
                }
            }
            if let Some(ref mother) = f.mother_handle {
                if all_handles.contains(mother) {
                    links.push(FamilyLink {
                        source: mother.clone(),
                        target: child.clone(),
                        link_type: LinkType::ParentChild,
                    });
                }
            }
        }
    }

    // ---- 7. Build family group metadata ----
    let mut family_groups: Vec<FamilyGroupMeta> = Vec::new();
    for (root, id) in &component_id {
        let component_handles: Vec<&str> = component_of
            .iter()
            .filter(|(_, r)| r.as_str() == *root)
            .map(|(h, _)| h.as_str())
            .collect();
        let size = component_handles.len();
        let max_gen = component_handles
            .iter()
            .filter_map(|h| gens.get(*h))
            .max()
            .copied()
            .unwrap_or(0);
        family_groups.push(FamilyGroupMeta {
            id: *id,
            size,
            span: max_gen + 1,
        });
    }

    GraphData {
        nodes,
        links,
        family_groups,
    }
}

/// Convert a raw `ParsedPerson` into a `PersonNode` for visualization.
fn person_to_node(p: &ParsedPerson) -> PersonNode {
    let name = assemble_name(p);
    let gender = match p.gender.as_deref() {
        Some("M") => "male".to_string(),
        Some("F") => "female".to_string(),
        _ => "unknown".to_string(),
    };
    PersonNode {
        handle: p.handle.clone(),
        name,
        birth_date: p.birth_date.clone(),
        death_date: p.death_date.clone(),
        birth_year: p.birth_year,
        is_imputed: false,
        gender,
        family_group: 0, // filled in later
        generation: 0,   // filled in later
    }
}

/// Assemble a display name from given and surname, falling back to
/// handle when both are missing.
fn assemble_name(p: &ParsedPerson) -> String {
    match (p.given_name.as_deref(), p.surname.as_deref()) {
        (Some(g), Some(s)) => format!("{} {}", g, s),
        (Some(g), None) => g.to_string(),
        (None, Some(s)) => s.to_string(),
        (None, None) => p.handle.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Name assembly
    // -----------------------------------------------------------------------

    #[test]
    fn assemble_name_given_and_surname() {
        let p = ParsedPerson {
            given_name: Some("John".to_string()),
            surname: Some("Smith".to_string()),
            ..ParsedPerson::default()
        };
        assert_eq!(assemble_name(&p), "John Smith");
    }

    #[test]
    fn assemble_name_given_only() {
        let p = ParsedPerson {
            given_name: Some("John".to_string()),
            ..ParsedPerson::default()
        };
        assert_eq!(assemble_name(&p), "John");
    }

    #[test]
    fn assemble_name_surname_only() {
        let p = ParsedPerson {
            surname: Some("Smith".to_string()),
            ..ParsedPerson::default()
        };
        assert_eq!(assemble_name(&p), "Smith");
    }

    #[test]
    fn assemble_name_handle_fallback() {
        let p = ParsedPerson {
            handle: "p0001".to_string(),
            ..ParsedPerson::default()
        };
        assert_eq!(assemble_name(&p), "p0001");
    }

    // -----------------------------------------------------------------------
    // Gender mapping
    // -----------------------------------------------------------------------

    #[test]
    fn gender_mapping() {
        let p_m = ParsedPerson {
            gender: Some("M".to_string()),
            ..ParsedPerson::default()
        };
        let p_f = ParsedPerson {
            gender: Some("F".to_string()),
            ..ParsedPerson::default()
        };
        let p_u = ParsedPerson {
            gender: Some("U".to_string()),
            ..ParsedPerson::default()
        };
        let p_none = ParsedPerson::default();
        assert_eq!(person_to_node(&p_m).gender, "male");
        assert_eq!(person_to_node(&p_f).gender, "female");
        assert_eq!(person_to_node(&p_u).gender, "unknown");
        assert_eq!(person_to_node(&p_none).gender, "unknown");
    }

    // -----------------------------------------------------------------------
    // build_graph_data — integration
    // -----------------------------------------------------------------------

    fn p(
        handle: &str,
        given: Option<&str>,
        surname: Option<&str>,
        _gender: Option<&str>,
    ) -> ParsedPerson {
        ParsedPerson {
            handle: handle.to_string(),
            given_name: given.map(|s| s.to_string()),
            surname: surname.map(|s| s.to_string()),
            ..ParsedPerson::default()
        }
    }

    fn f(
        handle: &str,
        father: Option<&str>,
        mother: Option<&str>,
        children: &[&str],
    ) -> ParsedFamily {
        ParsedFamily {
            handle: handle.to_string(),
            gramps_id: None,
            father_handle: father.map(|s| s.to_string()),
            mother_handle: mother.map(|s| s.to_string()),
            child_handles: children.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn build_graph_data_simple_family() {
        let persons = vec![
            p("p1", Some("John"), Some("Smith"), Some("M")),
            p("p2", Some("Jane"), Some("Smith"), Some("F")),
            p("p3", Some("Jim"), Some("Smith"), Some("M")),
        ];
        let families = vec![f("f1", Some("p1"), Some("p2"), &["p3"])];

        let gd = build_graph_data(&persons, &families);
        assert_eq!(gd.nodes.len(), 3);
        assert_eq!(gd.links.len(), 3); // 1 spouse + 2 parent-child

        // Check spouse link
        let spouse_link = gd
            .links
            .iter()
            .find(|l| l.link_type == LinkType::Spouse)
            .unwrap();
        assert_eq!(spouse_link.source, "p1");
        assert_eq!(spouse_link.target, "p2");

        // Check parent-child links
        let pc_links: Vec<&FamilyLink> = gd
            .links
            .iter()
            .filter(|l| l.link_type == LinkType::ParentChild)
            .collect();
        assert_eq!(pc_links.len(), 2);
        assert!(pc_links
            .iter()
            .any(|l| l.source == "p1" && l.target == "p3"));
        assert!(pc_links
            .iter()
            .any(|l| l.source == "p2" && l.target == "p3"));

        // Check generations
        let p1 = gd.nodes.iter().find(|n| n.handle == "p1").unwrap();
        let p2 = gd.nodes.iter().find(|n| n.handle == "p2").unwrap();
        let p3 = gd.nodes.iter().find(|n| n.handle == "p3").unwrap();
        assert_eq!(p1.generation, 0);
        assert_eq!(p2.generation, 0);
        assert_eq!(p3.generation, 1);

        // Same family group
        assert_eq!(p1.family_group, 0);
        assert_eq!(p2.family_group, 0);
        assert_eq!(p3.family_group, 0);
    }

    #[test]
    fn build_graph_data_two_components() {
        let persons = vec![
            p("p1", Some("A"), None, Some("M")),
            p("p2", Some("B"), None, Some("F")),
            p("p3", Some("C"), None, Some("M")),
            p("p4", Some("D"), None, None), // isolated
        ];
        let families = vec![f("f1", Some("p1"), Some("p2"), &["p3"])];

        let gd = build_graph_data(&persons, &families);
        assert_eq!(gd.nodes.len(), 4);
        assert_eq!(gd.family_groups.len(), 2);

        // p1, p2, p3 are in one group; p4 isolated
        let p1_g = gd
            .nodes
            .iter()
            .find(|n| n.handle == "p1")
            .unwrap()
            .family_group;
        let p4_g = gd
            .nodes
            .iter()
            .find(|n| n.handle == "p4")
            .unwrap()
            .family_group;
        assert_ne!(p1_g, p4_g);
    }

    #[test]
    fn build_graph_data_no_families() {
        let persons = vec![
            p("p1", Some("A"), None, None),
            p("p2", Some("B"), None, None),
        ];
        let families: Vec<ParsedFamily> = vec![];

        let gd = build_graph_data(&persons, &families);
        assert_eq!(gd.nodes.len(), 2);
        assert!(gd.links.is_empty());
        // Each person is its own family group
        assert_eq!(gd.family_groups.len(), 2);
    }

    #[test]
    fn build_graph_data_duplicate_handles_deduped() {
        // p1 is in two families
        let persons = vec![
            p("p1", Some("A"), None, Some("M")),
            p("p2", Some("B"), None, Some("F")),
            p("p3", Some("C"), None, None),
            p("p4", Some("D"), None, Some("F")),
        ];
        let families = vec![
            f("f1", Some("p1"), Some("p2"), &["p3"]),
            f("f2", Some("p1"), Some("p4"), &[]),
        ];

        let gd = build_graph_data(&persons, &families);
        // All 4 persons in one component
        assert_eq!(gd.family_groups.len(), 1);
        assert_eq!(gd.family_groups[0].size, 4);
        // Spouse links: p1-p2, p1-p4
        let spouse_links: Vec<&FamilyLink> = gd
            .links
            .iter()
            .filter(|l| l.link_type == LinkType::Spouse)
            .collect();
        assert_eq!(spouse_links.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Compile-time: SelectionExport is Deserialize
    // -----------------------------------------------------------------------

    #[test]
    fn selection_export_is_deserializable() {
        fn assert_deserialize<'de, T: serde::Deserialize<'de>>() {}
        assert_deserialize::<SelectionExport>();
    }

    // -----------------------------------------------------------------------
    // Property-based: every node generation in [0, MAX_GENERATION]
    // -----------------------------------------------------------------------

    #[test]
    fn generation_in_range() {
        let persons = vec![
            p("p1", Some("A"), None, Some("M")),
            p("p2", Some("B"), None, Some("F")),
            p("p3", Some("C"), None, None),
        ];
        let families = vec![f("f1", Some("p1"), Some("p2"), &["p3"])];

        let gd = build_graph_data(&persons, &families);
        for node in &gd.nodes {
            assert!(
                node.generation <= gramps_reader::MAX_GENERATION,
                "node {} generation {} exceeds MAX_GENERATION {}",
                node.handle,
                node.generation,
                gramps_reader::MAX_GENERATION
            );
        }
    }
}
