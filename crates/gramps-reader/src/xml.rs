//! Low-level streaming XML helpers shared by the extractors.
//!
//! These helpers operate on raw `quick_xml` events and are the common
//! building blocks for both the streaming counter (`xml::count`) and the
//! detail extractors (`xml::extract`).

pub mod count;
pub mod extract;
pub mod header;

/// Strip an optional namespace prefix from an element name.
///
/// `prefix:person` → `"person"`, `person` → `"person"`.
pub fn strip_prefix(name: &[u8]) -> &[u8] {
    name.iter()
        .rposition(|&b| b == b':')
        .map_or(name, |pos| &name[pos + 1..])
}

/// Read the `handle` attribute from an element.
///
/// Returns `None` when the element has no `handle` attribute (whether
/// namespaced or bare).
pub fn read_handle_attr(e: &quick_xml::events::BytesStart) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"handle" || key.ends_with(b":handle") {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}

/// Read the `hlink` attribute from an element.
///
/// Returns `None` when the element has no `hlink` attribute (whether
/// namespaced or bare).
pub fn read_hlink_attr(e: &quick_xml::events::BytesStart) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"hlink" || key.ends_with(b":hlink") {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::events::Event;
    use quick_xml::Reader;

    /// Parse the first start/empty element from `xml` into an owned `BytesStart`.
    fn start_event(xml: &str) -> quick_xml::events::BytesStart<'static> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event().expect("valid XML") {
                Event::Start(e) => return e.into_owned(),
                Event::Empty(e) => return e.into_owned(),
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // strip_prefix
    // -----------------------------------------------------------------------

    #[test]
    fn strip_prefix_with_prefix() {
        assert_eq!(strip_prefix(b"ns:person"), b"person");
    }

    #[test]
    fn strip_prefix_without_prefix() {
        assert_eq!(strip_prefix(b"person"), b"person");
    }

    #[test]
    fn strip_prefix_empty() {
        assert_eq!(strip_prefix(b""), b"");
    }

    #[test]
    fn strip_prefix_just_colon() {
        assert_eq!(strip_prefix(b":"), b"");
    }

    // -----------------------------------------------------------------------
    // read_handle_attr
    // -----------------------------------------------------------------------

    #[test]
    fn read_handle_attr_plain() {
        let e = start_event(r#"<person handle="p0001"/>"#);
        assert_eq!(read_handle_attr(&e).as_deref(), Some("p0001"));
    }

    #[test]
    fn read_handle_attr_namespace_prefixed() {
        let e = start_event(r#"<ns:person ns:handle="p0001"/>"#);
        assert_eq!(read_handle_attr(&e).as_deref(), Some("p0001"));
    }

    #[test]
    fn read_handle_attr_missing() {
        let e = start_event(r#"<person/>"#);
        assert_eq!(read_handle_attr(&e), None);
    }

    #[test]
    fn read_handle_attr_not_handle() {
        let e = start_event(r#"<person id="xyz"/>"#);
        assert_eq!(read_handle_attr(&e), None);
    }

    // -----------------------------------------------------------------------
    // read_hlink_attr
    // -----------------------------------------------------------------------

    #[test]
    fn read_hlink_attr_plain() {
        let e = start_event(r#"<father hlink="p0001"/>"#);
        assert_eq!(read_hlink_attr(&e).as_deref(), Some("p0001"));
    }

    #[test]
    fn read_hlink_attr_namespace_prefixed() {
        let e = start_event(r#"<ns:father ns:hlink="p0001"/>"#);
        assert_eq!(read_hlink_attr(&e).as_deref(), Some("p0001"));
    }

    #[test]
    fn read_hlink_attr_self_closing() {
        let e = start_event(r#"<childref hlink="p0002"/>"#);
        assert_eq!(read_hlink_attr(&e).as_deref(), Some("p0002"));
    }

    #[test]
    fn read_hlink_attr_missing() {
        let e = start_event(r#"<father/>"#);
        assert_eq!(read_hlink_attr(&e), None);
    }
}
