//! Shared data records extracted from `.gramps` XML documents.
//!
//! These types are produced by the streaming extractors in [`crate::xml`]
//! and consumed by downstream crates (`cli` stats, `visualize` graph data).

/// Raw extracted event data from streaming XML parse.
///
/// Produced by [`crate::xml::extract::extract_events`]; consumed by
/// [`resolve_event_refs`] to populate person birth/death dates from
/// event references.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedEvent {
    pub handle: String,
    pub event_type: Option<String>,  // "Birth", "Death", "Marriage", etc.
    pub date_val: Option<String>,    // e.g. "1850-07-13"
    pub date_year: Option<i32>,      // e.g. 1850
}

/// Raw extracted person data from streaming XML parse.
///
/// Produced by [`crate::xml::extract::extract_persons`]; consumed by
/// visualization graph data adapters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedPerson {
    pub handle: String,
    pub given_name: Option<String>,
    pub surname: Option<String>,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
    pub birth_year: Option<i32>,
    pub gender: Option<String>,
    /// Handles of `<eventref>` elements referencing events in the
    /// `<events>` section. Used by [`resolve_event_refs`] to populate
    /// birth/death dates from separate event elements.
    pub event_refs: Vec<String>,
}

/// Raw extracted family data from streaming XML parse.
///
/// Produced by [`crate::xml::extract::extract_families`]; consumed by
/// visualization graph data adapters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedFamily {
    pub handle: String,
    pub father_handle: Option<String>,
    pub mother_handle: Option<String>,
    pub child_handles: Vec<String>,
}

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

    // -----------------------------------------------------------------------
    // ParsedEvent
    // -----------------------------------------------------------------------

    #[test]
    fn parsed_event_default() {
        let e = ParsedEvent::default();
        assert_eq!(e.handle, "");
        assert!(e.event_type.is_none());
        assert!(e.date_val.is_none());
        assert!(e.date_year.is_none());
    }

    #[test]
    fn parsed_event_full() {
        let e = ParsedEvent {
            handle: "e0001".to_string(),
            event_type: Some("Birth".to_string()),
            date_val: Some("1850-07-13".to_string()),
            date_year: Some(1850),
        };
        assert_eq!(e.handle, "e0001");
        assert_eq!(e.event_type.as_deref(), Some("Birth"));
        assert_eq!(e.date_val.as_deref(), Some("1850-07-13"));
        assert_eq!(e.date_year, Some(1850));
    }

    // -----------------------------------------------------------------------
    // ParsedPerson event_refs
    // -----------------------------------------------------------------------

    #[test]
    fn parsed_person_event_refs_default() {
        let p = ParsedPerson::default();
        assert!(p.event_refs.is_empty());
    }

    #[test]
    fn parsed_person_event_refs_populated() {
        let p = ParsedPerson {
            event_refs: vec!["e1".to_string(), "e2".to_string()],
            ..ParsedPerson::default()
        };
        assert_eq!(p.event_refs, vec!["e1", "e2"]);
    }
}
