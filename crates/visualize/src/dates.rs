//! Imputed-date algorithm for persons without explicit birth dates.
//!
//! For each family group (connected component), a multi-source BFS from
//! all dated nodes propagates birth years to undated nodes using the
//! generation difference and a configurable years-per-generation gap.

use std::collections::{HashMap, VecDeque};

use crate::graph_data::{FamilyLink, PersonNode};

/// Impute birth years for persons whose `birth_year` is `None`.
///
/// Returns a map from handle → effective birth year. For nodes that already
/// have a `birth_year`, the original value is returned. For undated nodes,
/// the year is imputed from the nearest dated node(s) in the same family
/// group using the formula:
///
/// ```text
/// imputed_year = source_year + (source_generation - target_generation) × gap
/// ```
///
/// When a node is reachable from multiple dated sources at the same BFS
/// distance, the imputed years are averaged. If no dated node is reachable,
/// the value is `None`.
///
/// # Arguments
///
/// * `persons` — All person nodes with their current generations.
/// * `links` — Family links (spouse + parent-child) connecting persons.
/// * `generation_gap` — Years per generation (default 25, validated 1..=100).
/// * `no_impute` — If `true`, returns the original `birth_year` for every
///   node without imputation.
pub fn impute_dates(
    persons: &[PersonNode],
    links: &[FamilyLink],
    generation_gap: u32,
    no_impute: bool,
) -> HashMap<String, Option<i32>> {
    if no_impute {
        return persons
            .iter()
            .map(|p| (p.handle.clone(), p.birth_year))
            .collect();
    }

    let gap = generation_gap as i32;

    // Build an adjacency list for undirected BFS within each family group.
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for p in persons {
        adjacency.entry(p.handle.as_str()).or_default();
    }
    for link in links {
        if let Some(neighbors) = adjacency.get_mut(link.source.as_str()) {
            neighbors.push(link.target.as_str());
        }
        if let Some(neighbors) = adjacency.get_mut(link.target.as_str()) {
            neighbors.push(link.source.as_str());
        }
    }

    // Pre-compute generation lookup and build a handle → index map.
    let gen_of: HashMap<&str, usize> = persons
        .iter()
        .map(|p| (p.handle.as_str(), p.generation))
        .collect();

    // Separate dated from undated handles.
    let mut dated_handles: Vec<&str> = Vec::new();
    let mut undated_handles: Vec<&str> = Vec::new();
    for p in persons {
        if p.birth_year.is_some() {
            dated_handles.push(p.handle.as_str());
        } else {
            undated_handles.push(p.handle.as_str());
        }
    }

    // If everyone is already dated, return directly.
    if undated_handles.is_empty() {
        return persons
            .iter()
            .map(|p| (p.handle.clone(), p.birth_year))
            .collect();
    }

    let birth_year_of: HashMap<&str, i32> = persons
        .iter()
        .filter_map(|p| p.birth_year.map(|y| (p.handle.as_str(), y)))
        .collect();

    // Multi-source BFS: for each undated handle, collect the set of source
    // handles that reach it at the minimum distance.
    // visited: undated handle → Vec<(source_handle, distance)>
    let mut visited: HashMap<&str, Vec<(&str, u32)>> = HashMap::new();
    let mut queue: VecDeque<(&str, &str, u32)> = VecDeque::new();

    // Initialize queue with all dated nodes (the "source" is themselves).
    for &dh in &dated_handles {
        queue.push_back((dh, dh, 0));
    }

    while let Some((handle, source, dist)) = queue.pop_front() {
        // Skip dated nodes being processed as sources.
        // For undated nodes, check if we've already visited at a shorter distance.
        if let Some(entries) = visited.get(handle) {
            let min_dist = entries.first().map(|&(_, d)| d).unwrap_or(u32::MAX);
            if dist > min_dist {
                continue;
            }
            if dist == min_dist {
                // Add this source at the same distance.
                // (Avoid duplicating the same source.)
                if !entries.iter().any(|&(s, _)| s == source) {
                    // We need mut access — collect into a Vec.
                    let mut new_entries = entries.clone();
                    new_entries.push((source, dist));
                    visited.insert(handle, new_entries);
                }
                // Continue BFS from this node at this distance.
            }
        } else {
            // First time visiting this undated node.
            visited.insert(handle, vec![(source, dist)]);
        }

        // BFS to neighbors.
        if let Some(neighbors) = adjacency.get(handle) {
            for &n in neighbors {
                // Check if neighbor is undated (or dated but we want to explore through it).
                // We always explore through dated nodes too (they act as bridges).
                queue.push_back((n, source, dist + 1));
            }
        }
    }

    // Build result map.
    let mut result: HashMap<String, Option<i32>> = HashMap::new();
    for p in persons {
        if let Some(year) = p.birth_year {
            result.insert(p.handle.clone(), Some(year));
        } else if let Some(sources) = visited.get(p.handle.as_str()) {
            // Compute imputed year = average of source_year + (source_gen - target_gen) * gap
            let mut sum: i64 = 0;
            let mut count = 0;
            for &(source, _dist) in sources {
                if let Some(&src_year) = birth_year_of.get(source) {
                    let src_gen = gen_of[source] as i32;
                    let tgt_gen = gen_of[p.handle.as_str()] as i32;
                    let imputed = src_year as i64 + (tgt_gen - src_gen) as i64 * gap as i64;
                    sum += imputed;
                    count += 1;
                }
            }
            if count > 0 {
                let avg = (sum / count as i64) as i32;
                result.insert(p.handle.clone(), Some(avg));
            } else {
                result.insert(p.handle.clone(), None);
            }
        } else {
            result.insert(p.handle.clone(), None);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_data::LinkType;

    // Helpers to build test data
    fn node(
        handle: &str,
        birth_year: Option<i32>,
        generation: usize,
        family_group: usize,
    ) -> PersonNode {
        PersonNode {
            handle: handle.to_string(),
            name: String::new(),
            birth_date: None,
            death_date: None,
            birth_year,
            is_imputed: false,
            gender: "unknown".to_string(),
            family_group,
            generation,
        }
    }

    fn spouse(source: &str, target: &str) -> FamilyLink {
        FamilyLink {
            source: source.to_string(),
            target: target.to_string(),
            link_type: LinkType::Spouse,
        }
    }

    fn parent_child(parent: &str, child: &str) -> FamilyLink {
        FamilyLink {
            source: parent.to_string(),
            target: child.to_string(),
            link_type: LinkType::ParentChild,
        }
    }

    // -----------------------------------------------------------------------
    // Known ancestor → impute descendant
    // -----------------------------------------------------------------------

    #[test]
    fn impute_known_ancestor() {
        // Parent (gen 0, born 1900) → child (gen 1, unknown)
        let persons = vec![node("p1", Some(1900), 0, 0), node("p2", None, 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let result = impute_dates(&persons, &links, 25, false);
        assert_eq!(result.get("p1"), Some(&Some(1900)));
        // child = 1900 + (0 - 1) * 25 = 1875
        // Wait, that's wrong. If parent is gen 0 and child is gen 1,
        // then child should be born LATER than parent.
        // imputed = source_year + (source_gen - target_gen) * gap
        // = 1900 + (0 - 1) * 25 = 1900 - 25 = 1875
        // That's wrong — the child should be born ~25 years after the parent.
        //
        // The formula has the sign backwards. Let me check:
        // source_gen = 0, target_gen = 1
        // source_year + (source_gen - target_gen) * gap
        // = 1900 + (0 - 1) * 25 = 1875 ❌
        //
        // It should be: source_year + (target_gen - source_gen) * gap
        // = 1900 + (1 - 0) * 25 = 1925 ✅
        //
        // Let me check the plan's formula:
        // "imputed_year = source_dated_year + (gen_diff × gap)
        //  where gen_diff = dated_node.generation - undated_node.generation"
        //
        // So gen_diff = 0 - 1 = -1, and imputed = 1900 + (-1) * 25 = 1875
        // That's wrong — the child should be born after the parent.
        //
        // I think the formula in the plan has the sign backwards for the
        // typical case. Let me think about it:
        //
        // If the dated node is the ancestor (gen 0) and the undated node is
        // the descendant (gen 1), then:
        // gen_diff = ancestor_gen - descendant_gen = 0 - 1 = -1
        // imputed = ancestor_year + (-1) * 25 = ancestor_year - 25
        // That gives the descendant a birth year 25 years BEFORE the ancestor.
        // That's wrong.
        //
        // But if the dated node is the descendant (gen 1) and the undated
        // node is the ancestor (gen 0), then:
        // gen_diff = desc_gen - anc_gen = 1 - 0 = 1
        // imputed = desc_year + 1 * 25 = desc_year + 25
        // That gives the ancestor a birth year 25 years AFTER the descendant.
        // Also wrong.
        //
        // The correct formula should be:
        // imputed = source_year + (target_gen - source_gen) * gap
        //
        // But wait, the plan says "gen_diff = dated_node.generation - undated_node.generation"
        // and the example shows:
        // "(-gap years per generation going backward in time, +gap going forward)"
        //
        // So if the dated node is the ancestor (gen 0) and the undated node
        // is the descendant (gen 1):
        // gen_diff = 0 - 1 = -1
        // imputed = 1900 + (-1) * 25 = 1875
        // But the plan says "-gap years per generation going backward in time"
        // So ancestor → descendant is going forward, should be +gap.
        // Descendant → ancestor is going backward, should be -gap.
        //
        // Hmm, the sign convention in the plan is confusing. Let me look at
        // the note again: "(-gap years per generation going backward in time,
        // +gap going forward)"
        //
        // If the dated node is the ancestor (gen 0) and the undated node is
        // the descendant (gen 1), we're going FORWARD in time (from ancestor
        // to descendant). So imputed = source_year + gap = 1900 + 25 = 1925.
        //
        // The formula gen_diff = dated_gen - undated_gen = 0 - 1 = -1 is wrong
        // for this convention. It should be undated_gen - dated_gen = 1 - 0 = 1.
        //
        // So I should use:
        // imputed = source_year + (target_gen - source_gen) * gap
        //
        // Let me fix the implementation.
        assert_eq!(result.get("p2"), Some(&Some(1925)));
    }

    // -----------------------------------------------------------------------
    // Known descendant → impute ancestor
    // -----------------------------------------------------------------------

    #[test]
    fn impute_known_descendant() {
        // Child (gen 1, born 1950) → parent (gen 0, unknown)
        let persons = vec![node("p1", None, 0, 0), node("p2", Some(1950), 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let result = impute_dates(&persons, &links, 25, false);
        assert_eq!(result.get("p1"), Some(&Some(1925))); // 1950 + (0-1)*25 = 1925
    }

    // -----------------------------------------------------------------------
    // Multi-source averaging
    // -----------------------------------------------------------------------

    #[test]
    fn impute_multi_source_averaging() {
        // Two parents (gen 0, born 1880 and 1890) → child (gen 1, unknown)
        // Both parents are at distance 1 from the child.
        let persons = vec![
            node("p1", Some(1880), 0, 0),
            node("p2", Some(1890), 0, 0),
            node("c1", None, 1, 0),
        ];
        let links = vec![
            spouse("p1", "p2"),
            parent_child("p1", "c1"),
            parent_child("p2", "c1"),
        ];

        let result = impute_dates(&persons, &links, 25, false);
        // p1: 1880 + (0-1)*25 = 1855
        // Wait, that's wrong. Let me recalculate with the correct formula.
        // imputed = source_year + (target_gen - source_gen) * gap
        // From p1: 1880 + (1-0)*25 = 1905
        // From p2: 1890 + (1-0)*25 = 1915
        // Average: (1905 + 1915) / 2 = 1910
        // Hmm, but the child should be born around 1880-1910, not 1905-1915.
        //
        // Actually, the formula gives the child's birth year based on the
        // parent's birth year + generation gap. If parents are born in 1880
        // and 1890, and the generation gap is 25 years, the child would
        // typically be born around 1905-1915 (when parents are 25 years old).
        // That's actually correct!
        assert_eq!(result.get("c1"), Some(&Some(1910)));
    }

    // -----------------------------------------------------------------------
    // No reachable dated node → None
    // -----------------------------------------------------------------------

    #[test]
    fn impute_no_reachable_date() {
        // Two persons, neither dated, same family group.
        let persons = vec![node("p1", None, 0, 0), node("p2", None, 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let result = impute_dates(&persons, &links, 25, false);
        assert_eq!(result.get("p1"), Some(&None));
        assert_eq!(result.get("p2"), Some(&None));
    }

    // -----------------------------------------------------------------------
    // Gap 0 (all equal within component)
    // -----------------------------------------------------------------------

    #[test]
    fn impute_gap_zero() {
        let persons = vec![node("p1", Some(1900), 0, 0), node("p2", None, 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let result = impute_dates(&persons, &links, 0, false);
        // gap 0: imputed = source_year + (target_gen - source_gen) * 0 = source_year
        assert_eq!(result.get("p2"), Some(&Some(1900)));
    }

    // -----------------------------------------------------------------------
    // Fully-dated graph → no-op (no imputation)
    // -----------------------------------------------------------------------

    #[test]
    fn impute_fully_dated_no_op() {
        let persons = vec![node("p1", Some(1900), 0, 0), node("p2", Some(1925), 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let result = impute_dates(&persons, &links, 25, false);
        assert_eq!(result.get("p1"), Some(&Some(1900)));
        assert_eq!(result.get("p2"), Some(&Some(1925)));
    }

    // -----------------------------------------------------------------------
    // no_impute flag
    // -----------------------------------------------------------------------

    #[test]
    fn impute_no_impute_flag() {
        let persons = vec![node("p1", Some(1900), 0, 0), node("p2", None, 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let result = impute_dates(&persons, &links, 25, true);
        // With no_impute, undated nodes stay None.
        assert_eq!(result.get("p1"), Some(&Some(1900)));
        assert_eq!(result.get("p2"), Some(&None));
    }

    // -----------------------------------------------------------------------
    // Determinism: same input → same output
    // -----------------------------------------------------------------------

    #[test]
    fn impute_determinism() {
        let persons = vec![node("p1", Some(1900), 0, 0), node("p2", None, 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let r1 = impute_dates(&persons, &links, 25, false);
        let r2 = impute_dates(&persons, &links, 25, false);
        assert_eq!(r1, r2);
    }

    // -----------------------------------------------------------------------
    // Ancestor ≤ descendant invariant
    // -----------------------------------------------------------------------

    #[test]
    fn ancestor_leq_descendant() {
        // Grandparent → Parent → Child
        let persons = vec![
            node("gp", Some(1850), 0, 0),
            node("p", None, 1, 0),
            node("c", None, 2, 0),
        ];
        let links = vec![parent_child("gp", "p"), parent_child("p", "c")];

        let result = impute_dates(&persons, &links, 25, false);
        let gp = result.get("gp").unwrap().unwrap();
        let p = result.get("p").unwrap().unwrap();
        let c = result.get("c").unwrap().unwrap();
        assert!(gp <= p, "ancestor {gp} > parent {p}");
        assert!(p <= c, "parent {p} > child {c}");
    }

    // -----------------------------------------------------------------------
    // Custom gap
    // -----------------------------------------------------------------------

    #[test]
    fn impute_custom_gap() {
        let persons = vec![node("p1", Some(1900), 0, 0), node("p2", None, 1, 0)];
        let links = vec![parent_child("p1", "p2")];

        let result = impute_dates(&persons, &links, 30, false);
        // 1900 + (1-0)*30 = 1930
        assert_eq!(result.get("p2"), Some(&Some(1930)));
    }

    // -----------------------------------------------------------------------
    // Empty graph
    // -----------------------------------------------------------------------

    #[test]
    fn impute_empty() {
        let result = impute_dates(&[], &[], 25, false);
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // Disconnected component (no links)
    // -----------------------------------------------------------------------

    #[test]
    fn impute_disconnected_component() {
        // Two persons in same family group but no links between them.
        let persons = vec![
            node("p1", Some(1900), 0, 0),
            node("p2", None, 0, 0), // same gen, no links
        ];
        let links = vec![];

        let result = impute_dates(&persons, &links, 25, false);
        // p2 is not reachable from any dated node.
        assert_eq!(result.get("p2"), Some(&None));
    }
}
