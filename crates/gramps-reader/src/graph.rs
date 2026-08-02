//! Person-graph algorithms: connected components (DSU) and generation
//! layering over family records.
//!
//! A **family group** is a connected component of the person graph — all
//! people linked by any family relationship (parent, child, marriage). A
//! family group may contain one or more Gramps `<family>` elements, or
//! none at all (an isolated person).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::types::FamilyRecord;

/// Maximum generation depth before the layering assumes a cycle. Generation
/// values are clamped to this bound so a pathological cycle cannot produce
/// unbounded layering; a warning is emitted when a cycle is detected.
pub const MAX_GENERATION: usize = 50;

/// Cross-classification of family groups (connected components of the person
/// graph) by size and generation span.
///
/// The outer map key is family-group size (as a string), the inner map key is
/// the generation span (as a string) plus `"total"` for the row marginal. This
/// shape serializes directly to JSON:
///
/// ```json
/// { "3": { "2": 1, "total": 1 } }
/// ```
pub type FamilyGroupGenerationTable = BTreeMap<String, BTreeMap<String, usize>>;

/// Disjoint-set union over person handles (handle → parent handle in the
/// union tree). Used to find connected components of the person graph.
pub struct Dsu {
    parent: HashMap<String, String>,
}

impl Dsu {
    pub fn new() -> Self {
        Dsu {
            parent: HashMap::new(),
        }
    }

    /// Ensure `handle` has its own singleton set.
    pub fn make_set(&mut self, handle: &str) {
        self.parent
            .entry(handle.to_string())
            .or_insert_with(|| handle.to_string());
    }

    /// Find the root of `handle`, compressing the path along the way.
    pub fn find(&mut self, handle: &str) -> String {
        let mut root = handle.to_string();
        while let Some(p) = self.parent.get(&root).cloned() {
            if p == root {
                break;
            }
            root = p;
        }
        let mut current = handle.to_string();
        while let Some(p) = self.parent.get(&current).cloned() {
            if p == root {
                break;
            }
            self.parent.insert(current.clone(), root.clone());
            current = p;
        }
        root
    }

    /// Union the sets containing `a` and `b`.
    pub fn union(&mut self, a: &str, b: &str) {
        self.make_set(a);
        self.make_set(b);
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a != root_b {
            self.parent.insert(root_b, root_a);
        }
    }
}

impl Default for Dsu {
    fn default() -> Self {
        Self::new()
    }
}

/// DFS state for cycle detection.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    InProgress,
    Done,
}

/// Detect whether the parent→child graph contains a cycle (DFS back edges).
fn has_cycle(adjacency: &HashMap<&str, Vec<&str>>) -> bool {
    let mut state: HashMap<&str, VisitState> = adjacency
        .keys()
        .map(|k| (*k, VisitState::Unvisited))
        .collect();
    for start in adjacency.keys() {
        if state[*start] == VisitState::Unvisited && dfs_cycle(start, adjacency, &mut state) {
            return true;
        }
    }
    false
}

fn dfs_cycle<'a>(
    node: &'a str,
    adjacency: &HashMap<&str, Vec<&'a str>>,
    state: &mut HashMap<&'a str, VisitState>,
) -> bool {
    state.insert(node, VisitState::InProgress);
    if let Some(children) = adjacency.get(node) {
        for child in children {
            match state[*child] {
                VisitState::InProgress => return true, // back edge → cycle
                VisitState::Done => {}
                VisitState::Unvisited => {
                    if dfs_cycle(child, adjacency, state) {
                        return true;
                    }
                }
            }
        }
    }
    state.insert(node, VisitState::Done);
    false
}

/// Compute the family-group-size × generation-span contingency table.
///
/// Returns `(table, warnings)`. Each row of the table maps a generation span
/// to a family-group count plus a `"total"` marginal. Connected components
/// are built over person handles that exist in `all_handles`; dangling
/// references (family refs without a matching `<person>` element) are
/// skipped, so a family group's generation span reflects only its
/// existing-person subset. Cycle warnings are appended to `warnings`.
/// Intermediate result of the person-graph analysis shared by
/// [`compute_generations`] and [`compute_generation_table`].
struct GenerationAnalysis {
    /// Existing handles grouped by connected-component root.
    components: HashMap<String, Vec<String>>,
    /// Per-handle generation level (longest-path layering, capped).
    gen: HashMap<String, usize>,
    /// Number of cyclic connected components.
    cycle_count: usize,
}

/// Run the shared person-graph analysis: DSU components over existing
/// handles, parent→child edges, and longest-path generation layering.
fn analyze_generations(
    family_records: &[FamilyRecord],
    all_handles: &HashSet<String>,
) -> GenerationAnalysis {
    // Connected components via DSU over existing person handles.
    let mut dsu = Dsu::new();
    for handle in all_handles {
        dsu.make_set(handle);
    }
    for record in family_records {
        let mut members: Vec<&str> = Vec::new();
        for h in &record.parent_handles {
            if all_handles.contains(h) {
                members.push(h.as_str());
            }
        }
        for h in &record.child_handles {
            if all_handles.contains(h) {
                members.push(h.as_str());
            }
        }
        for i in 1..members.len() {
            dsu.union(members[0], members[i]);
        }
    }

    // Group existing handles by component root.
    let mut components: HashMap<String, Vec<String>> = HashMap::new();
    for handle in all_handles {
        let root = dsu.find(handle);
        components.entry(root).or_default().push(handle.clone());
    }

    // Parent → child edges restricted to existing handles.
    let mut edges: Vec<(String, String)> = Vec::new();
    for record in family_records {
        for p in &record.parent_handles {
            if !all_handles.contains(p) {
                continue;
            }
            for c in &record.child_handles {
                if all_handles.contains(c) {
                    edges.push((p.clone(), c.clone()));
                }
            }
        }
    }

    // Longest-path layering per component, capped at MAX_GENERATION.
    let mut gen: HashMap<String, usize> = all_handles.iter().map(|h| (h.clone(), 0)).collect();
    let mut cycle_count = 0usize;

    for component in components.values() {
        let component_set: HashSet<&str> = component.iter().map(|s| s.as_str()).collect();

        // Adjacency within this component.
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for person in component {
            adjacency.insert(person.as_str(), Vec::new());
        }
        for (parent, child) in &edges {
            if component_set.contains(parent.as_str()) && component_set.contains(child.as_str()) {
                if let Some(children) = adjacency.get_mut(parent.as_str()) {
                    children.push(child.as_str());
                }
            }
        }

        // Iterative relaxation: gen[child] = max(gen[child], gen[parent] + 1).
        for _ in 0..=MAX_GENERATION {
            let mut changed = false;
            for (parent, children) in &adjacency {
                let parent_gen = gen[*parent];
                for child in children {
                    let new_gen = (parent_gen + 1).min(MAX_GENERATION);
                    if new_gen > gen[*child] {
                        gen.insert((*child).to_string(), new_gen);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        if has_cycle(&adjacency) {
            cycle_count += 1;
        }
    }

    GenerationAnalysis {
        components,
        gen,
        cycle_count,
    }
}

/// Compute the per-handle generation level for every person in
/// `all_handles`, using longest-path layering within each connected
/// component (capped at [`MAX_GENERATION`]).
///
/// Handles that exist in `all_handles` but appear in no family record
/// (isolated persons) are assigned generation 0. Dangling references
/// (family refs without a matching handle) are skipped.
pub fn compute_generations(
    family_records: &[FamilyRecord],
    all_handles: &HashSet<String>,
) -> HashMap<String, usize> {
    analyze_generations(family_records, all_handles).gen
}

/// Compute the family-group-size × generation-span contingency table.
///
/// Returns `(table, warnings)`. Each row of the table maps a generation span
/// to a family-group count plus a `"total"` marginal. Connected components
/// are built over person handles that exist in `all_handles`; dangling
/// references (family refs without a matching `<person>` element) are
/// skipped, so a family group's generation span reflects only its
/// existing-person subset. Cycle warnings are appended to `warnings`.
pub fn compute_generation_table(
    family_records: &[FamilyRecord],
    all_handles: &HashSet<String>,
) -> (
    FamilyGroupGenerationTable,
    BTreeMap<usize, usize>,
    Vec<String>,
) {
    let analysis = analyze_generations(family_records, all_handles);
    let GenerationAnalysis {
        components,
        gen,
        cycle_count,
    } = analysis;

    let mut warnings = Vec::new();
    if cycle_count > 0 {
        warnings.push(format!(
            "WARNING: detected {} cycle(s) in the family graph; generation layering may be approximate",
            cycle_count
        ));
    }

    // Generation span per component = max generation + 1.
    let mut span_by_root: HashMap<String, usize> = HashMap::new();
    for (root, component) in &components {
        let max_gen = component.iter().map(|h| gen[h]).max().unwrap_or(0);
        span_by_root.insert(root.clone(), max_gen + 1);
    }

    // Tabulate family groups by (size, span) and build distribution.
    // Iterate over all components, including isolated-person components
    // (size 1, span 1) that have zero family records.
    let mut table: FamilyGroupGenerationTable = BTreeMap::new();
    let mut distribution: BTreeMap<usize, usize> = BTreeMap::new();
    for (root, component) in &components {
        let size = component.len();
        let span = *span_by_root.get(root).unwrap_or(&1);

        // Table: one row per component by (size, span).
        let row = table.entry(size.to_string()).or_default();
        *row.entry(span.to_string()).or_insert(0) += 1;
        *row.entry("total".to_string()).or_insert(0) += 1;

        // Distribution: one entry per component by size.
        *distribution.entry(size).or_insert(0) += 1;
    }

    (table, distribution, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------
    // DSU tests
    // ---------------------------------------------------------------------

    #[test]
    fn dsu_singleton() {
        let mut dsu = Dsu::new();
        dsu.make_set("p1");
        assert_eq!(dsu.find("p1"), "p1");
    }

    #[test]
    fn dsu_union_connects_sets() {
        let mut dsu = Dsu::new();
        dsu.union("p1", "p2");
        dsu.union("p3", "p4");
        assert_eq!(dsu.find("p1"), dsu.find("p2"));
        assert_eq!(dsu.find("p3"), dsu.find("p4"));
        assert_ne!(dsu.find("p1"), dsu.find("p3"));
    }

    #[test]
    fn dsu_union_transitive() {
        let mut dsu = Dsu::new();
        dsu.union("p1", "p2");
        dsu.union("p2", "p3");
        assert_eq!(dsu.find("p1"), dsu.find("p3"));
    }

    #[test]
    fn dsu_find_unknown_returns_self() {
        let mut dsu = Dsu::new();
        assert_eq!(dsu.find("p9"), "p9");
    }

    // ---------------------------------------------------------------------
    // Generation table tests
    // ---------------------------------------------------------------------

    fn rec(parents: &[&str], children: &[&str]) -> FamilyRecord {
        FamilyRecord {
            parent_handles: parents.iter().map(|s| s.to_string()).collect(),
            child_handles: children.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn collect_handles(records: &[FamilyRecord]) -> HashSet<String> {
        let mut all = HashSet::new();
        for r in records {
            all.extend(r.parent_handles.iter().cloned());
            all.extend(r.child_handles.iter().cloned());
        }
        all
    }

    fn assert_cell(table: &FamilyGroupGenerationTable, size: usize, span: usize, count: usize) {
        let row = table.get(&size.to_string()).expect("row exists");
        assert_eq!(
            row.get(&span.to_string()),
            Some(&count),
            "cell size={size} span={span}"
        );
    }

    fn assert_row_total(table: &FamilyGroupGenerationTable, size: usize, total: usize) {
        let row = table.get(&size.to_string()).expect("row exists");
        assert_eq!(row.get("total"), Some(&total), "row total size={size}");
    }

    #[test]
    fn generation_table_empty() {
        let records: Vec<FamilyRecord> = Vec::new();
        let all: HashSet<String> = HashSet::new();
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert!(table.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn generation_table_single_family_no_children() {
        let records = vec![rec(&["p1", "p2"], &[])];
        let all = collect_handles(&records);
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        assert_cell(&table, 2, 1, 1);
        assert_row_total(&table, 2, 1);
    }

    #[test]
    fn generation_table_single_family_with_children() {
        let records = vec![rec(&["p1", "p2"], &["p3"])];
        let all = collect_handles(&records);
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        assert_cell(&table, 3, 2, 1);
        assert_row_total(&table, 3, 1);
    }

    #[test]
    fn generation_table_two_family_chain() {
        // Family A: p1+p2 → p3; Family B: p3+p4 → p5 → 3-generation component.
        let records = vec![rec(&["p1", "p2"], &["p3"]), rec(&["p3", "p4"], &["p5"])];
        let all = collect_handles(&records);
        let (table, distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // Both families collapse into a single 5-person, 3-generation component.
        assert_cell(&table, 5, 3, 1);
        assert_row_total(&table, 5, 1);
        assert_eq!(distribution.get(&5), Some(&1));
    }

    #[test]
    fn generation_table_two_family_chain_with_extra_children() {
        // Family A: p1+p2 → p3, p4; Family B: p3+p5 → p6.
        let records = vec![
            rec(&["p1", "p2"], &["p3", "p4"]),
            rec(&["p3", "p5"], &["p6"]),
        ];
        let all = collect_handles(&records);
        let (table, distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // Gens: p1,p2,p5 = 0; p3,p4 = 1; p6 = 2 → span 3.
        // Single component of 6 people (p1..p6).
        assert_cell(&table, 6, 3, 1);
        assert_row_total(&table, 6, 1);
        assert_eq!(distribution.get(&6), Some(&1));
    }

    #[test]
    fn generation_table_three_generation_chain() {
        // grandparent → parent → child → grandchild (4 gens)
        let records = vec![
            rec(&["p1", "p2"], &["p3"]),
            rec(&["p3", "p4"], &["p5"]),
            rec(&["p5", "p6"], &["p7"]),
        ];
        let all = collect_handles(&records);
        let (table, distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // All 7 people form a single 4-generation component.
        assert_cell(&table, 7, 4, 1);
        assert_row_total(&table, 7, 1);
        assert_eq!(distribution.get(&7), Some(&1));
    }

    #[test]
    fn generation_table_isolated_person() {
        let records: Vec<FamilyRecord> = Vec::new();
        let all: HashSet<String> = HashSet::from(["p1".to_string()]);
        let (table, distribution, warnings) = compute_generation_table(&records, &all);
        // An isolated person is a size-1, span-1 family group.
        assert_cell(&table, 1, 1, 1);
        assert_row_total(&table, 1, 1);
        assert_eq!(distribution.get(&1), Some(&1));
        assert!(warnings.is_empty());
    }

    #[test]
    fn generation_table_disconnected_components() {
        let records = vec![
            rec(&["p1", "p2"], &["p3"]), // size 3, span 2
            rec(&["p4", "p5"], &[]),     // size 2, span 1
        ];
        let all = collect_handles(&records);
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        assert_cell(&table, 3, 2, 1);
        assert_cell(&table, 2, 1, 1);
        assert_row_total(&table, 3, 1);
        assert_row_total(&table, 2, 1);
    }

    #[test]
    fn generation_table_pedigree_collapse() {
        // G1+G2 → P1, P2 (siblings); P1+X → C1; P2+Y → C2; C1+C2 → Z (cousins marry).
        let records = vec![
            rec(&["g1", "g2"], &["p1", "p2"]),
            rec(&["p1", "x"], &["c1"]),
            rec(&["p2", "y"], &["c2"]),
            rec(&["c1", "c2"], &["z"]),
        ];
        let all = collect_handles(&records);
        let (table, distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // All 9 people form one 4-generation component.
        assert_cell(&table, 9, 4, 1);
        assert_row_total(&table, 9, 1);
        assert_eq!(distribution.get(&9), Some(&1));
    }

    #[test]
    fn generation_table_cycle() {
        // Artificial cycle: p1 → p2 → p1.
        let records = vec![rec(&["p1"], &["p2"]), rec(&["p2"], &["p1"])];
        let all = collect_handles(&records);
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("cycle"),
            "Expected cycle warning, got: {}",
            warnings[0]
        );
        // Layering is capped: spans must not exceed MAX_GENERATION + 1.
        for (size, row) in &table {
            for span in row.keys() {
                if span == "total" {
                    continue;
                }
                let span: usize = span.parse().unwrap();
                assert!(
                    span <= MAX_GENERATION + 1,
                    "size {} span {} exceeds cap",
                    size,
                    span
                );
            }
        }
        // Single 2-person component in the cyclic component.
        assert_cell(&table, 2, MAX_GENERATION + 1, 1);
        assert_row_total(&table, 2, 1);
    }

    #[test]
    fn generation_table_duplicate_handles_across_families() {
        // p1 is a parent in two families.
        let records = vec![rec(&["p1", "p2"], &["p3"]), rec(&["p1", "p4"], &["p5"])];
        let all = collect_handles(&records);
        let (table, distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // Both families collapse into a single 5-person, 2-generation component.
        assert_cell(&table, 5, 2, 1);
        assert_row_total(&table, 5, 1);
        assert_eq!(distribution.get(&5), Some(&1));
    }

    #[test]
    fn generation_table_single_parent_family() {
        let records = vec![rec(&["p1"], &["p2"])];
        let all = collect_handles(&records);
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // p1 gen 0, p2 gen 1 → span 2, size 2.
        assert_cell(&table, 2, 2, 1);
        assert_row_total(&table, 2, 1);
    }

    #[test]
    fn generation_table_child_only_family() {
        let records = vec![rec(&[], &["p1", "p2"])];
        let all = collect_handles(&records);
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // No parents → both gen 0 → span 1, size 2.
        assert_cell(&table, 2, 1, 1);
        assert_row_total(&table, 2, 1);
    }

    #[test]
    fn generation_table_single_member_family() {
        let records = vec![rec(&["p1"], &[])];
        let all = collect_handles(&records);
        let (table, _distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        // One parent, no children → size 1, span 1.
        assert_cell(&table, 1, 1, 1);
        assert_row_total(&table, 1, 1);
    }

    #[test]
    fn generation_table_dangling_refs_skipped() {
        // p2 is dangling (not in all_handles); the component is built from
        // p1 only → a size-1, span-1 family group.
        let records = vec![rec(&["p1", "p2"], &[])];
        let mut all = collect_handles(&records);
        all.remove("p2");
        let (table, distribution, warnings) = compute_generation_table(&records, &all);
        assert!(warnings.is_empty());
        assert_cell(&table, 1, 1, 1);
        assert_row_total(&table, 1, 1);
        assert_eq!(distribution.get(&1), Some(&1));
    }

    // ---------------------------------------------------------------------
    // compute_generations tests
    // ---------------------------------------------------------------------

    fn gen_map(records: &[FamilyRecord], all: &HashSet<String>) -> HashMap<String, usize> {
        compute_generations(records, all)
    }

    #[test]
    fn compute_generations_chain() {
        // p1+p2 → p3 → p4 (3 levels: 0, 0, 1, 2)
        let records = vec![rec(&["p1", "p2"], &["p3"]), rec(&["p3"], &["p4"])];
        let all = collect_handles(&records);
        let gens = gen_map(&records, &all);
        assert_eq!(gens.get("p1"), Some(&0));
        assert_eq!(gens.get("p2"), Some(&0));
        assert_eq!(gens.get("p3"), Some(&1));
        assert_eq!(gens.get("p4"), Some(&2));
    }

    #[test]
    fn compute_generations_isolated_person() {
        let records: Vec<FamilyRecord> = Vec::new();
        let all = HashSet::from(["p1".to_string()]);
        let gens = gen_map(&records, &all);
        assert_eq!(gens.get("p1"), Some(&0));
    }

    #[test]
    fn compute_generations_child_only_family() {
        // No parents → both gen 0.
        let records = vec![rec(&[], &["c1", "c2"])];
        let all = collect_handles(&records);
        let gens = gen_map(&records, &all);
        assert_eq!(gens.get("c1"), Some(&0));
        assert_eq!(gens.get("c2"), Some(&0));
    }

    #[test]
    fn compute_generations_cycle_capped() {
        // p1 → p2 → p1: layering must not exceed MAX_GENERATION.
        let records = vec![rec(&["p1"], &["p2"]), rec(&["p2"], &["p1"])];
        let all = collect_handles(&records);
        let gens = gen_map(&records, &all);
        for g in gens.values() {
            assert!(*g <= MAX_GENERATION, "generation {g} exceeds cap");
        }
    }

    #[test]
    fn compute_generations_matches_table_span() {
        // The max generation + 1 per component must equal the table span.
        let records = vec![rec(&["p1", "p2"], &["p3"]), rec(&["p3", "p4"], &["p5"])];
        let all = collect_handles(&records);
        let gens = gen_map(&records, &all);
        let (table, _dist, _warnings) = compute_generation_table(&records, &all);
        let max_gen = gens.values().copied().max().unwrap_or(0);
        // Single component of 5 → span 3.
        let row = table.get("5").expect("row exists");
        assert_eq!(row.get(&(max_gen + 1).to_string()), Some(&1));
    }
}
