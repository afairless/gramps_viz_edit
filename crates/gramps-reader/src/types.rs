//! Shared data records extracted from `.gramps` XML documents.
//!
//! These types are produced by the streaming extractors in [`crate::xml`]
//! and consumed by downstream crates (`cli` stats, `visualize` graph data).

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

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(parents: &[&str], children: &[&str]) -> FamilyRecord {
        FamilyRecord {
            parent_handles: parents.iter().map(|s| s.to_string()).collect(),
            child_handles: children.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn family_record_size_empty() {
        let r = rec(&[], &[]);
        assert_eq!(r.size(), 0);
    }

    #[test]
    fn family_record_size_parents_and_children() {
        let r = rec(&["p1", "p2"], &["p3", "p4"]);
        assert_eq!(r.size(), 4);
    }

    #[test]
    fn family_record_size_counts_duplicates_once() {
        // Same handle appears as both parent and child.
        let r = rec(&["p1", "p2"], &["p1", "p3"]);
        assert_eq!(r.size(), 3);
    }

    #[test]
    fn family_record_size_single_parent() {
        let r = rec(&["p1"], &[]);
        assert_eq!(r.size(), 1);
    }
}
