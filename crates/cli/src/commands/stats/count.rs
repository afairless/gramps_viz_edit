//! Streaming counting logic for the `stats` command.
//!
//! `count_gramps_xml` scans a `.gramps` XML document in a single
//! streaming pass and produces a [`StatsReport`] without
//! reconstructing the full typed graph. It is a pure function over
//! `&str` so it can be unit-tested without filesystem access.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::CliError;

/// A record of a single nuclear family's parent and child handles.
///
/// During the streaming pass, `parent_handles` collects `father`/`mother`
/// refs and `child_handles` collects `childref` refs. After the pass,
/// these records are used to build the person graph for generation-layering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyRecord {
    pub parent_handles: Vec<String>,
    pub child_handles: Vec<String>,
}

impl FamilyRecord {
    /// Number of distinct person handles across both parents and children.
    pub fn size(&self) -> usize {
        let mut distinct = std::collections::HashSet::new();
        for h in &self.parent_handles {
            distinct.insert(h.as_str());
        }
        for h in &self.child_handles {
            distinct.insert(h.as_str());
        }
        distinct.len()
    }
}

/// Cross-classification of family groups (connected components of the person
/// graph) by size and generation span.
///
/// A family group is a connected component of the person graph — all people
/// linked by any family relationship (parent, child, marriage). A family group
/// may contain one or more Gramps `<family>` elements, or none at all (an
/// isolated person).
///
/// The outer map key is family-group size (as a string), the inner map key is
/// the generation span (as a string) plus `"total"` for the row marginal. This
/// shape serializes directly to JSON:
///
/// ```json
/// { "3": { "2": 1, "total": 1 } }
/// ```
pub type FamilyGroupGenerationTable = BTreeMap<String, BTreeMap<String, usize>>;

/// Statistics collected from a single `.gramps` XML document.
///
/// # Gramps families vs. family groups
///
/// Throughout this report, two related but distinct concepts share the word
/// "family":
///
/// - **Gramps families** — nuclear families represented by `<family>` XML
///   elements (parents + children). Counted by `family_size_distribution`.
/// - **Family groups** — connected components of the person graph, built by
///   linking people across all `<family>` elements. Tabulated by
///   `family_group_generation_table` and `family_group_distribution`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StatsReport {
    /// Path of the analyzed file (filled in by the CLI).
    pub file: String,
    /// Counts per primary object type.
    pub counts: PrimaryTypeCounts,
    /// Gramps-family size → number of families, ascending by size.
    /// Counts `<family>` XML elements by nuclear-family size.
    pub family_size_distribution: BTreeMap<usize, usize>,
    /// Family-group-size × generation-span contingency table.
    /// Each row is a connected component (family group) of the person graph.
    pub family_group_generation_table: FamilyGroupGenerationTable,
    /// Family-group size → number of groups (connected components).
    /// Populated during the same component iteration that builds the
    /// generation table.
    pub family_group_distribution: BTreeMap<usize, usize>,
    /// People whose handle never appears in any family.
    pub people_not_in_family: usize,
    /// Family `ref` handles without a matching `<person>`.
    pub dangling_refs: usize,
    /// Non-fatal warnings emitted during the scan.
    pub warnings: Vec<String>,
}

/// Counts for the ten primary object types.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrimaryTypeCounts {
    pub people: usize,
    pub families: usize,
    pub events: usize,
    pub places: usize,
    pub sources: usize,
    pub citations: usize,
    pub repositories: usize,
    pub media: usize,
    pub notes: usize,
    pub tags: usize,
}

/// Strip an optional namespace prefix from an element name.
///
/// `prefix:person` → `"person"`, `person` → `"person"`.
fn strip_prefix(name: &[u8]) -> &[u8] {
    name.iter()
        .rposition(|&b| b == b':')
        .map_or(name, |pos| &name[pos + 1..])
}

/// Scan a `.gramps` XML document and produce a [`StatsReport`].
///
/// Returns `Err(CliError::XmlParseError)` when the content is not
/// well-formed XML.
pub fn count_gramps_xml(content: &str) -> Result<StatsReport, CliError> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut report = StatsReport::default();
    let mut all_handles: HashSet<String> = HashSet::new();
    let mut ref_handles: HashSet<String> = HashSet::new();
    let mut family_records: Vec<FamilyRecord> = Vec::new();
    let mut completed_records: Vec<FamilyRecord> = Vec::new();
    let mut histogram: HashMap<usize, usize> = HashMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = e.name().as_ref().to_vec();
                let name = strip_prefix(&name);
                match name {
                    b"person" => {
                        report.counts.people += 1;
                        if let Some(h) = read_handle_attr(e) {
                            all_handles.insert(h);
                        }
                    }
                    b"family" => {
                        report.counts.families += 1;
                        family_records.push(FamilyRecord {
                            parent_handles: Vec::new(),
                            child_handles: Vec::new(),
                        });
                    }
                    b"event" => report.counts.events += 1,
                    b"placeobj" => report.counts.places += 1,
                    b"source" => report.counts.sources += 1,
                    b"citation" => report.counts.citations += 1,
                    b"repository" => report.counts.repositories += 1,
                    b"object" => report.counts.media += 1,
                    b"note" => report.counts.notes += 1,
                    b"tag" => report.counts.tags += 1,
                    b"father" | b"mother" => {
                        if let Some(ref_handle) = read_hlink_attr(e) {
                            if let Some(current) = family_records.last_mut() {
                                current.parent_handles.push(ref_handle.clone());
                            }
                            ref_handles.insert(ref_handle);
                        }
                    }
                    b"childref" => {
                        if let Some(ref_handle) = read_hlink_attr(e) {
                            if let Some(current) = family_records.last_mut() {
                                current.child_handles.push(ref_handle.clone());
                            }
                            ref_handles.insert(ref_handle);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name().as_ref().to_vec();
                let name = strip_prefix(&name);
                match name {
                    b"person" => {
                        report.counts.people += 1;
                        if let Some(h) = read_handle_attr(e) {
                            all_handles.insert(h);
                        }
                    }
                    b"family" => {
                        report.counts.families += 1;
                        // Self-closing family: size 0
                        completed_records.push(FamilyRecord {
                            parent_handles: Vec::new(),
                            child_handles: Vec::new(),
                        });
                        *histogram.entry(0).or_insert(0) += 1;
                    }
                    b"event" => report.counts.events += 1,
                    b"placeobj" => report.counts.places += 1,
                    b"source" => report.counts.sources += 1,
                    b"citation" => report.counts.citations += 1,
                    b"repository" => report.counts.repositories += 1,
                    b"object" => report.counts.media += 1,
                    b"note" => report.counts.notes += 1,
                    b"tag" => report.counts.tags += 1,
                    b"father" | b"mother" => {
                        if let Some(ref_handle) = read_hlink_attr(e) {
                            if let Some(current) = family_records.last_mut() {
                                current.parent_handles.push(ref_handle.clone());
                            }
                            ref_handles.insert(ref_handle);
                        }
                    }
                    b"childref" => {
                        if let Some(ref_handle) = read_hlink_attr(e) {
                            if let Some(current) = family_records.last_mut() {
                                current.child_handles.push(ref_handle.clone());
                            }
                            ref_handles.insert(ref_handle);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name().as_ref().to_vec();
                let name = strip_prefix(&name);
                if name == b"family" {
                    if let Some(record) = family_records.pop() {
                        let size = record.size();
                        *histogram.entry(size).or_insert(0) += 1;
                        completed_records.push(record);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(CliError::XmlParseError {
                    message: format!("{} at byte {}", e, reader.error_position()),
                });
            }
        }
    }

    // Build sorted histogram
    let mut sizes: Vec<usize> = histogram.keys().copied().collect();
    sizes.sort();
    for size in sizes {
        report
            .family_size_distribution
            .insert(size, histogram[&size]);
    }

    // Compute people_not_in_family and dangling_refs
    report.people_not_in_family = all_handles
        .len()
        .saturating_sub(ref_handles.intersection(&all_handles).count());
    report.dangling_refs = ref_handles.len() - ref_handles.intersection(&all_handles).count();

    // Build generation table
    let (table, distribution, gen_warnings) =
        compute_generation_table(&completed_records, &all_handles);
    report.family_group_generation_table = table;
    report.family_group_distribution = distribution;
    report.warnings.extend(gen_warnings);

    Ok(report)
}

/// Maximum generation depth before the layering assumes a cycle. Generation
/// values are clamped to this bound so a pathological cycle cannot produce
/// unbounded layering; a warning is emitted when a cycle is detected.
const MAX_GENERATION: usize = 50;

/// Disjoint-set union over person handles (handle → parent handle in the
/// union tree). Used to find connected components of the person graph.
struct Dsu {
    parent: HashMap<String, String>,
}

impl Dsu {
    fn new() -> Self {
        Dsu {
            parent: HashMap::new(),
        }
    }

    /// Ensure `handle` has its own singleton set.
    fn make_set(&mut self, handle: &str) {
        self.parent
            .entry(handle.to_string())
            .or_insert_with(|| handle.to_string());
    }

    /// Find the root of `handle`, compressing the path along the way.
    fn find(&mut self, handle: &str) -> String {
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
    fn union(&mut self, a: &str, b: &str) {
        self.make_set(a);
        self.make_set(b);
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a != root_b {
            self.parent.insert(root_b, root_a);
        }
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
pub fn compute_generation_table(
    family_records: &[FamilyRecord],
    all_handles: &HashSet<String>,
) -> (FamilyGroupGenerationTable, BTreeMap<usize, usize>, Vec<String>) {
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

/// Read the `handle` attribute from an element.
fn read_handle_attr(e: &quick_xml::events::BytesStart) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"handle" || key.ends_with(b":handle") || key.ends_with(b"\"handle") {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}

/// Read the `hlink` attribute from an element.
fn read_hlink_attr(e: &quick_xml::events::BytesStart) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"hlink" || key.ends_with(b":hlink") || key.ends_with(b"\"hlink") {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_empty_content_returns_zeroed_report() {
        let report = count_gramps_xml("").unwrap();
        assert_eq!(report, StatsReport::default());
        assert_eq!(report.counts.people, 0);
        assert_eq!(report.counts.families, 0);
        assert!(report.family_size_distribution.is_empty());
        assert_eq!(report.people_not_in_family, 0);
        assert_eq!(report.dangling_refs, 0);
    }

    #[test]
    fn count_malformed_xml_returns_xml_parse_error() {
        let result = count_gramps_xml("<database><person></database>");
        match result {
            Err(CliError::XmlParseError { .. }) => {}
            other => panic!("Expected XmlParseError, got: {:?}", other),
        }
    }

    #[test]
    fn count_all_ten_types() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <tags><tag handle="t1"/><tag handle="t2"/></tags>
  <events><event handle="e1"/><event handle="e2"/><event handle="e3"/></events>
  <people>
    <person handle="p1"/>
    <person handle="p2"/>
    <person handle="p3"/>
    <person handle="p4"/>
  </people>
  <families>
    <family handle="f1"/>
    <family handle="f2"/>
  </families>
  <citations>
    <citation handle="c1"/>
  </citations>
  <sources>
    <source handle="s1"/>
    <source handle="s2"/>
    <source handle="s3"/>
  </sources>
  <places>
    <placeobj handle="pl1"/>
  </places>
  <objects>
    <object handle="o1"/>
    <object handle="o2"/>
    <object handle="o3"/>
    <object handle="o4"/>
  </objects>
  <repositories>
    <repository handle="r1"/>
  </repositories>
  <notes>
    <note handle="n1"/>
    <note handle="n2"/>
  </notes>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();
        assert_eq!(report.counts.people, 4);
        assert_eq!(report.counts.families, 2);
        assert_eq!(report.counts.events, 3);
        assert_eq!(report.counts.places, 1);
        assert_eq!(report.counts.sources, 3);
        assert_eq!(report.counts.citations, 1);
        assert_eq!(report.counts.repositories, 1);
        assert_eq!(report.counts.media, 4);
        assert_eq!(report.counts.notes, 2);
        assert_eq!(report.counts.tags, 2);
    }

    #[test]
    fn count_empty_database_returns_zero() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();
        let zero = PrimaryTypeCounts::default();
        assert_eq!(report.counts, zero);
    }

    #[test]
    fn count_self_closing_elements() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"/>
  </people>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();
        assert_eq!(report.counts.people, 1);
    }

    #[test]
    fn count_namespace_prefixed_input() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:header><ns:created date="2025-01-01" version="5.2"/></ns:header>
  <ns:people>
    <ns:person handle="p1"/>
    <ns:person handle="p2"/>
  </ns:people>
  <ns:families>
    <ns:family handle="f1"/>
  </ns:families>
</ns:database>"#;
        let report = count_gramps_xml(xml).unwrap();
        assert_eq!(report.counts.people, 2);
        assert_eq!(report.counts.families, 1);
    }

    #[test]
    fn count_family_histogram_example_scenario() {
        // 7 families: sizes 10, 10, 3, 3, 3, 3, 3
        // 15 isolated people (not in any family)
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people>
    <person handle="p01"/><person handle="p02"/><person handle="p03"/>
    <person handle="p04"/><person handle="p05"/><person handle="p06"/>
    <person handle="p07"/><person handle="p08"/><person handle="p09"/>
    <person handle="p10"/><person handle="p11"/><person handle="p12"/>
    <person handle="p13"/><person handle="p14"/><person handle="p15"/>
    <person handle="p16"/><person handle="p17"/><person handle="p18"/>
    <person handle="p19"/><person handle="p20"/><person handle="p21"/>
    <person handle="p22"/><person handle="p23"/><person handle="p24"/>
    <person handle="p25"/><person handle="p26"/><person handle="p27"/>
    <person handle="p28"/><person handle="p29"/><person handle="p30"/>
    <person handle="p31"/><person handle="p32"/><person handle="p33"/>
    <person handle="p34"/><person handle="p35"/><person handle="p36"/>
    <person handle="p37"/><person handle="p38"/><person handle="p39"/>
    <person handle="p40"/><person handle="p41"/><person handle="p42"/>
    <person handle="p43"/><person handle="p44"/><person handle="p45"/>
    <person handle="p46"/><person handle="p47"/><person handle="p48"/>
    <person handle="p49"/><person handle="p50"/>
  </people>
  <families>
    <!-- Family size 10 -->
    <family handle="f01">
      <father hlink="p01"/><mother hlink="p02"/>
      <childref hlink="p03"/><childref hlink="p04"/><childref hlink="p05"/>
      <childref hlink="p06"/><childref hlink="p07"/><childref hlink="p08"/>
      <childref hlink="p09"/><childref hlink="p10"/>
    </family>
    <!-- Family size 10 -->
    <family handle="f02">
      <father hlink="p11"/><mother hlink="p12"/>
      <childref hlink="p13"/><childref hlink="p14"/><childref hlink="p15"/>
      <childref hlink="p16"/><childref hlink="p17"/><childref hlink="p18"/>
      <childref hlink="p19"/><childref hlink="p20"/>
    </family>
    <!-- Family size 3 -->
    <family handle="f03"><father hlink="p21"/><mother hlink="p22"/><childref hlink="p23"/></family>
    <!-- Family size 3 -->
    <family handle="f04"><father hlink="p24"/><mother hlink="p25"/><childref hlink="p26"/></family>
    <!-- Family size 3 -->
    <family handle="f05"><father hlink="p27"/><mother hlink="p28"/><childref hlink="p29"/></family>
    <!-- Family size 3 -->
    <family handle="f06"><father hlink="p30"/><mother hlink="p31"/><childref hlink="p32"/></family>
    <!-- Family size 3 -->
    <family handle="f07"><father hlink="p33"/><mother hlink="p34"/><childref hlink="p35"/></family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        // 35 people in families (p01-p35), 15 isolated (p36-p50)
        assert_eq!(report.counts.people, 50);
        assert_eq!(report.counts.families, 7);

        // Histogram: size 10: 2 families, size 3: 5 families
        assert_eq!(report.family_size_distribution.len(), 2);
        assert_eq!(report.family_size_distribution.get(&3), Some(&5));
        assert_eq!(report.family_size_distribution.get(&10), Some(&2));

        // 15 people not in any family (p36-p50)
        assert_eq!(report.people_not_in_family, 15);
        assert_eq!(report.dangling_refs, 0);
    }

    #[test]
    fn count_family_duplicate_refs_counted_once() {
        // Same handle appears as both father and childref
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people>
    <person handle="p1"/><person handle="p2"/><person handle="p3"/>
  </people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p1"/>
      <childref hlink="p1"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        // Size 3: p1, p2, p3 (p1 counted once)
        assert_eq!(report.family_size_distribution.len(), 1);
        assert_eq!(report.family_size_distribution.get(&3), Some(&1));

        // All 3 people are in families
        assert_eq!(report.people_not_in_family, 0);
        assert_eq!(report.dangling_refs, 0);
    }

    #[test]
    fn count_empty_family_size_zero() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people><person handle="p1"/></people>
  <families>
    <!-- Empty family with no refs -->
    <family handle="f1">
    </family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        assert_eq!(report.family_size_distribution.len(), 1);
        assert_eq!(report.family_size_distribution.get(&0), Some(&1));

        // p1 is not in any family
        assert_eq!(report.people_not_in_family, 1);
    }

    #[test]
    fn count_dangling_refs_reported() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people><person handle="p1"/></people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        // p1 is in a family, p2 and p3 are dangling
        assert_eq!(report.people_not_in_family, 0);
        assert_eq!(report.dangling_refs, 2);

        // Family size is 3 (p1, p2, p3) even though p2/p3 are dangling
        assert_eq!(report.family_size_distribution.get(&3), Some(&1));
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
    // StatsReport wiring tests
    // ---------------------------------------------------------------------

    #[test]
    fn report_default_empty_table() {
        let report = StatsReport::default();
        assert!(report.family_group_generation_table.is_empty());
        assert!(report.family_group_distribution.is_empty());
    }

    #[test]
    fn json_output_contains_generation_table() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
  <people>
    <person handle="p1"/><person handle="p2"/><person handle="p3"/>
  </people>
  <families>
    <family handle="f1">
      <father hlink="p1"/><mother hlink="p2"/>
      <childref hlink="p3"/>
    </family>
  </families>
</database>"#;
        let report = count_gramps_xml(xml).unwrap();

        // count_gramps_xml populates the table: size 3, span 2.
        assert_cell(&report.family_group_generation_table, 3, 2, 1);
        assert_row_total(&report.family_group_generation_table, 3, 1);

        // JSON round-trip includes the new field.
        let json = serde_json::to_string(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["family_group_generation_table"].is_object());
        assert_eq!(parsed["family_group_generation_table"]["3"]["2"], 1);
        assert_eq!(parsed["family_group_generation_table"]["3"]["total"], 1);

        let back: StatsReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
    }

    #[test]
    fn generation_table_integration() {
        use output::GraphXmlWriter;
        use output::SerializationMap;
        use typed_graph::generate::GraphBuilder;

        let mut graph = typed_graph::Graph::new();
        let mut builder = GraphBuilder::new(&mut graph);

        // Alice + Bob (parents) + Charlie (child); Dana isolated.
        let alice = builder
            .add_person_auto()
            .with_name("Alice", "Smith")
            .with_gender(1)
            .build()
            .unwrap();
        let bob = builder
            .add_person_auto()
            .with_name("Bob", "Smith")
            .with_gender(0)
            .build()
            .unwrap();
        let charlie = builder
            .add_person_auto()
            .with_name("Charlie", "Smith")
            .build()
            .unwrap();
        let _dana = builder
            .add_person_auto()
            .with_name("Dana", "Smith")
            .build()
            .unwrap();

        let _ = builder
            .add_family_auto()
            .with_father(&alice)
            .with_mother(&bob)
            .add_child_birth(&charlie)
            .build()
            .unwrap();

        let _ = builder.into_graph();

        let map = SerializationMap::new();
        let writer = GraphXmlWriter::new(map, "5.2.0");
        let mut buffer = Vec::new();
        writer
            .write(&graph, &mut std::io::BufWriter::new(&mut buffer))
            .unwrap();
        let xml = String::from_utf8(buffer).unwrap();

        let report = count_gramps_xml(&xml).unwrap();
        assert_eq!(report.counts.families, 1);
        assert_cell(&report.family_group_generation_table, 3, 2, 1);
        assert_row_total(&report.family_group_generation_table, 3, 1);
    }
}
