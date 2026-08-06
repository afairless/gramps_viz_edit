//! Streaming detail extraction from Gramps XML documents.
//!
//! These extractors read a Gramps XML file in a single streaming pass and
//! produce structured records (`ParsedPerson`, `ParsedFamily`) for
//! downstream processing. They are pure functions over `&str` so they
//! can be unit-tested without filesystem access.

use std::collections::HashMap;

use crate::error::Error;
use crate::types::{ParsedEvent, ParsedFamily, ParsedPerson};
use crate::xml::{read_handle_attr, read_hlink_attr, read_id_attr, strip_prefix};

/// Given persons with event_refs and a lookup of events by handle,
/// populate birth_date/birth_year/death_date for persons whose
/// birth/death was stored as a separate event reference.
///
/// Does **not** overwrite already-populated fields (inline birth/death
/// takes precedence).  Unknown event types (e.g. "Marriage") and missing
/// event handles are silently skipped.
pub fn resolve_event_refs(persons: &mut [ParsedPerson], events: &[ParsedEvent]) {
    // Index events by handle for O(1) lookup.
    let event_map: HashMap<&str, &ParsedEvent> =
        events.iter().map(|e| (e.handle.as_str(), e)).collect();

    for person in persons {
        for hlink in &person.event_refs {
            if let Some(event) = event_map.get(hlink.as_str()) {
                match event.event_type.as_deref() {
                    Some("Birth") if person.birth_year.is_none() => {
                        person.birth_date = event.date_val.clone();
                        person.birth_year = event.date_year;
                    }
                    Some("Death") if person.death_date.is_none() => {
                        person.death_date = event.date_val.clone();
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Extract all events from a Gramps XML document.
///
/// Returns a `Vec<ParsedEvent>` with handle, event type, and date
/// information for every `<event>` element. Unknown or malformed fields
/// are silently `None` — the caller is responsible for handling missing
/// data.
pub fn extract_events(content: &str) -> Result<Vec<ParsedEvent>, Error> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut events = Vec::new();
    let mut current: Option<ParsedEvent> = None;
    let mut in_eventtype = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"event" => {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        current = Some(ParsedEvent {
                            handle,
                            gramps_id,
                            ..ParsedEvent::default()
                        });
                    }
                    b"eventtype" if current.is_some() => in_eventtype = true,
                    b"type" if in_eventtype => {
                        let name = e.name().to_owned();
                        if let Ok(text) = reader.read_text(name) {
                            let t = text.trim().to_string();
                            if !t.is_empty() {
                                if let Some(ref mut ev) = current {
                                    ev.event_type = Some(t);
                                }
                            }
                        }
                    }
                    b"type" if current.is_some() && !in_eventtype => {
                        // Flat format: <type>Birth</type> directly inside <event>
                        // (Gramps XML 1.7.1 / Gramps 5.1)
                        let name = e.name().to_owned();
                        if let Ok(text) = reader.read_text(name) {
                            let t = text.trim().to_string();
                            if !t.is_empty() {
                                if let Some(ref mut ev) = current {
                                    ev.event_type = Some(t);
                                }
                            }
                        }
                        log::debug!("Flat-format event type detected");
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"event" => {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        events.push(ParsedEvent {
                            handle,
                            gramps_id,
                            ..ParsedEvent::default()
                        });
                    }
                    b"dateval" if current.is_some() => {
                        let display = read_dateval_val(e);
                        let year = parse_year_from_val(e);
                        if let Some(ref mut ev) = current {
                            ev.date_val = display;
                            ev.date_year = year;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"event" => {
                        if let Some(ev) = current.take() {
                            events.push(ev);
                        }
                    }
                    b"eventtype" => in_eventtype = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(Error::XmlParseError {
                    message: format!("{} at byte {}", e, reader.error_position()),
                });
            }
        }
    }
    Ok(events)
}

/// Extract all persons from a Gramps XML document.
///
/// Returns a `Vec<ParsedPerson>` with details (name, birth/death dates,
/// gender) for every `<person>` element. Unknown or malformed fields are
/// silently `None` — the caller is responsible for handling missing data.
pub fn extract_persons(content: &str) -> Result<Vec<ParsedPerson>, Error> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut persons = Vec::new();
    // Per-person state
    let mut current: Option<ParsedPerson> = None;
    let mut in_birth = false;
    let mut in_death = false;
    let mut in_name = false;
    let mut in_gender = false;
    let mut given_buf = String::new();
    let mut surname_buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"person" => {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        current = Some(ParsedPerson {
                            handle,
                            gramps_id,
                            ..ParsedPerson::default()
                        });
                    }
                    b"birth" if current.is_some() => in_birth = true,
                    b"death" if current.is_some() => in_death = true,
                    b"name" if current.is_some() => {
                        in_name = true;
                        given_buf.clear();
                        surname_buf.clear();
                    }
                    b"first" if in_name => {
                        // read_text consumes until </first>
                        let name = e.name().to_owned();
                        if let Ok(text) = reader.read_text(name) {
                            given_buf = text.trim().to_string();
                        }
                    }
                    b"surname" if in_name => {
                        let name = e.name().to_owned();
                        if let Ok(text) = reader.read_text(name) {
                            surname_buf = text.trim().to_string();
                        }
                    }
                    b"dateval" if in_birth || in_death => {
                        let display = read_dateval_val(e);
                        let year = parse_year_from_val(e);
                        if let Some(ref mut person) = current {
                            if in_birth {
                                person.birth_date = display;
                                person.birth_year = year;
                            } else {
                                person.death_date = display;
                            }
                        }
                    }
                    b"eventref" if current.is_some() => {
                        if let Some(h) = read_hlink_attr(e) {
                            if let Some(ref mut person) = current {
                                person.event_refs.push(h);
                            }
                        }
                    }
                    b"gender" if current.is_some() => in_gender = true,
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"person" => {
                        // Self-closing person with handle only
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        persons.push(ParsedPerson {
                            handle,
                            gramps_id,
                            ..ParsedPerson::default()
                        });
                    }
                    b"dateval" if in_birth || in_death => {
                        let display = read_dateval_val(e);
                        let year = parse_year_from_val(e);
                        if let Some(ref mut person) = current {
                            if in_birth {
                                person.birth_date = display;
                                person.birth_year = year;
                            } else {
                                person.death_date = display;
                            }
                        }
                    }
                    b"eventref" => {
                        if let Some(h) = read_hlink_attr(e) {
                            if let Some(ref mut person) = current {
                                person.event_refs.push(h);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if let Ok(text) = e.unescape() {
                    let text = text.trim();
                    if !text.is_empty() && in_gender {
                        if let Some(ref mut person) = current {
                            person.gender = Some(text.to_string());
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"person" => {
                        if let Some(person) = current.take() {
                            persons.push(person);
                        }
                    }
                    b"birth" => in_birth = false,
                    b"death" => in_death = false,
                    b"name" => {
                        if in_name {
                            if let Some(ref mut person) = current {
                                let g = given_buf.clone();
                                let s = surname_buf.clone();
                                if !g.is_empty() {
                                    person.given_name = Some(g);
                                }
                                if !s.is_empty() {
                                    person.surname = Some(s);
                                }
                            }
                            in_name = false;
                        }
                    }
                    b"gender" => in_gender = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(Error::XmlParseError {
                    message: format!("{} at byte {}", e, reader.error_position()),
                });
            }
        }
    }
    Ok(persons)
}

/// Extract all families from a Gramps XML document.
///
/// Returns a `Vec<ParsedFamily>` with father/mother/child handles for
/// every `<family>` element. Dangling `hlink` references (handles without
/// a matching `<person>`) are kept as-is — deduplication against persons
/// is the caller's responsibility.
pub fn extract_families(content: &str) -> Result<Vec<ParsedFamily>, Error> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut families = Vec::new();
    let mut current: Option<ParsedFamily> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"family" => {
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        current = Some(ParsedFamily {
                            handle,
                            gramps_id,
                            ..ParsedFamily::default()
                        });
                    }
                    b"father" | b"mother" => {
                        if let Some(ref mut fam) = current {
                            if let Some(h) = read_hlink_attr(e) {
                                if name == b"father" {
                                    fam.father_handle = Some(h);
                                } else {
                                    fam.mother_handle = Some(h);
                                }
                            }
                        }
                    }
                    b"childref" => {
                        if let Some(ref mut fam) = current {
                            if let Some(h) = read_hlink_attr(e) {
                                fam.child_handles.push(h);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                match name {
                    b"family" => {
                        // Self-closing family with handle only
                        let handle = read_handle_attr(e).unwrap_or_default();
                        let gramps_id = read_id_attr(e);
                        families.push(ParsedFamily {
                            handle,
                            gramps_id,
                            ..ParsedFamily::default()
                        });
                    }
                    b"father" | b"mother" => {
                        if let Some(ref mut fam) = current {
                            if let Some(h) = read_hlink_attr(e) {
                                if name == b"father" {
                                    fam.father_handle = Some(h);
                                } else {
                                    fam.mother_handle = Some(h);
                                }
                            }
                        }
                    }
                    b"childref" => {
                        if let Some(ref mut fam) = current {
                            if let Some(h) = read_hlink_attr(e) {
                                fam.child_handles.push(h);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = e.name().as_ref().to_vec();
                let name = strip_prefix(&raw);
                if name == b"family" {
                    if let Some(fam) = current.take() {
                        families.push(fam);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(Error::XmlParseError {
                    message: format!("{} at byte {}", e, reader.error_position()),
                });
            }
        }
    }
    Ok(families)
}

/// Read the `val` attribute from a `<dateval>` element.
fn read_dateval_val(e: &quick_xml::events::BytesStart) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"val" || key.ends_with(b":val") {
            return Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
    None
}

/// Parse the year from a `<dateval>` element's `val` attribute.
///
/// The `val` attribute can be `"1850"`, `"1850-03-15"`, or
/// `"1850-03-15 00:00:00"`. The year is the first component before `-`.
fn parse_year_from_val(e: &quick_xml::events::BytesStart) -> Option<i32> {
    let val = read_dateval_val(e)?;
    let y = val.split('-').next()?;
    y.parse::<i32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a person from an XML snippet.
    fn persons_from(xml: &str) -> Vec<ParsedPerson> {
        extract_persons(xml).unwrap()
    }

    fn single_person(xml: &str) -> ParsedPerson {
        let mut ps = persons_from(xml);
        assert_eq!(ps.len(), 1, "expected exactly one person");
        ps.remove(0)
    }

    // -----------------------------------------------------------------------
    // Full person
    // -----------------------------------------------------------------------

    #[test]
    fn extract_person_full() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p0001">
      <gender>M</gender>
      <name>
        <first>John</first>
        <surname>Smith</surname>
      </name>
      <birth>
        <dateval val="1850-03-15" quality="exact"/>
      </birth>
      <death>
        <dateval val="1920-07-01" quality="exact"/>
      </death>
    </person>
  </people>
</database>"#;
        let p = single_person(xml);
        assert_eq!(p.handle, "p0001");
        assert_eq!(p.given_name.as_deref(), Some("John"));
        assert_eq!(p.surname.as_deref(), Some("Smith"));
        assert_eq!(p.birth_date.as_deref(), Some("1850-03-15"));
        assert_eq!(p.birth_year, Some(1850));
        assert_eq!(p.death_date.as_deref(), Some("1920-07-01"));
        assert_eq!(p.gender.as_deref(), Some("M"));
    }

    // -----------------------------------------------------------------------
    // Minimal person (no name, no dates, no gender)
    // -----------------------------------------------------------------------

    #[test]
    fn extract_person_minimal() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p0002"/>
  </people>
</database>"#;
        let p = single_person(xml);
        assert_eq!(p.handle, "p0002");
        assert!(p.given_name.is_none());
        assert!(p.surname.is_none());
        assert!(p.birth_date.is_none());
        assert!(p.birth_year.is_none());
        assert!(p.death_date.is_none());
        assert!(p.gender.is_none());
    }

    // -----------------------------------------------------------------------
    // Self-closing person
    // -----------------------------------------------------------------------

    #[test]
    fn extract_person_self_closing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p0003"/>
    <person handle="p0004"/>
  </people>
</database>"#;
        let ps = persons_from(xml);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].handle, "p0003");
        assert_eq!(ps[1].handle, "p0004");
    }

    // -----------------------------------------------------------------------
    // Namespace-prefixed elements
    // -----------------------------------------------------------------------

    #[test]
    fn extract_person_namespace_prefixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:people>
    <ns:person ns:handle="p0001">
      <ns:gender>F</ns:gender>
      <ns:name>
        <ns:first>Jane</ns:first>
        <ns:surname>Doe</ns:surname>
      </ns:name>
      <ns:birth>
        <ns:dateval ns:val="1860-01-01" ns:quality="exact"/>
      </ns:birth>
    </ns:person>
  </ns:people>
</ns:database>"#;
        let p = single_person(xml);
        assert_eq!(p.handle, "p0001");
        assert_eq!(p.given_name.as_deref(), Some("Jane"));
        assert_eq!(p.surname.as_deref(), Some("Doe"));
        assert_eq!(p.gender.as_deref(), Some("F"));
        assert_eq!(p.birth_date.as_deref(), Some("1860-01-01"));
        assert_eq!(p.birth_year, Some(1860));
    }

    // -----------------------------------------------------------------------
    // Gender mapping
    // -----------------------------------------------------------------------

    #[test]
    fn extract_person_gender_mapping() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><gender>M</gender></person>
    <person handle="p2"><gender>F</gender></person>
    <person handle="p3"><gender>U</gender></person>
  </people>
</database>"#;
        let ps = persons_from(xml);
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0].gender.as_deref(), Some("M"));
        assert_eq!(ps[1].gender.as_deref(), Some("F"));
        assert_eq!(ps[2].gender.as_deref(), Some("U"));
    }

    // -----------------------------------------------------------------------
    // Birth year parsing
    // -----------------------------------------------------------------------

    #[test]
    fn extract_person_birth_year_parsing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <birth><dateval val="1850"/></birth>
    </person>
    <person handle="p2">
      <birth><dateval val="1850-03-15"/></birth>
    </person>
    <person handle="p3">
      <birth><dateval val="1850-03-15 00:00:00"/></birth>
    </person>
  </people>
</database>"#;
        let ps = persons_from(xml);
        assert_eq!(ps[0].birth_year, Some(1850));
        assert_eq!(ps[1].birth_year, Some(1850));
        assert_eq!(ps[2].birth_year, Some(1850));
    }

    // -----------------------------------------------------------------------
    // Malformed XML
    // -----------------------------------------------------------------------

    #[test]
    fn extract_persons_malformed_xml_returns_error() {
        let result = extract_persons("<database><person></database>");
        match result {
            Err(Error::XmlParseError { .. }) => {}
            other => panic!("Expected XmlParseError, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Empty content
    // -----------------------------------------------------------------------

    #[test]
    fn extract_persons_empty_content() {
        let ps = persons_from("");
        assert!(ps.is_empty());
    }

    // -----------------------------------------------------------------------
    // No people section
    // -----------------------------------------------------------------------

    #[test]
    fn extract_persons_no_people_section() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
</database>"#;
        let ps = persons_from(xml);
        assert!(ps.is_empty());
    }

    // -----------------------------------------------------------------------
    // Multiple persons
    // -----------------------------------------------------------------------

    #[test]
    fn extract_persons_multiple() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1"><name><first>A</first></name><gender>M</gender></person>
    <person handle="p2"><name><first>B</first></name><gender>F</gender></person>
  </people>
</database>"#;
        let ps = persons_from(xml);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].given_name.as_deref(), Some("A"));
        assert_eq!(ps[1].given_name.as_deref(), Some("B"));
    }

    // -----------------------------------------------------------------------
    // Eventref capture in extract_persons
    // -----------------------------------------------------------------------

    #[test]
    fn extract_person_with_eventrefs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <name><first>John</first><surname>Smith</surname></name>
      <eventref hlink="e1"/>
      <eventref hlink="e2"/>
    </person>
  </people>
</database>"#;
        let p = single_person(xml);
        assert_eq!(p.handle, "p1");
        assert_eq!(p.event_refs, vec!["e1", "e2"]);
    }

    #[test]
    fn extract_person_mixed_inline_and_eventrefs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <name><first>John</first><surname>Smith</surname></name>
      <birth><dateval val="1850-03-15"/></birth>
      <eventref hlink="e1"/>
      <eventref hlink="e2"/>
    </person>
  </people>
</database>"#;
        let p = single_person(xml);
        assert_eq!(p.handle, "p1");
        assert_eq!(p.birth_date.as_deref(), Some("1850-03-15"));
        assert_eq!(p.birth_year, Some(1850));
        assert_eq!(p.event_refs, vec!["e1", "e2"]);
    }

    #[test]
    fn extract_person_eventref_no_hlink() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <name><first>John</first><surname>Smith</surname></name>
      <eventref hlink="e1"/>
      <eventref/>
      <eventref hlink="e3"/>
    </person>
  </people>
</database>"#;
        let p = single_person(xml);
        // eventref without hlink is silently skipped (no None entry)
        assert_eq!(p.event_refs, vec!["e1", "e3"]);
    }

    #[test]
    fn extract_person_eventref_namespace_prefixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:people>
    <ns:person ns:handle="p1">
      <ns:name><ns:first>John</ns:first><ns:surname>Smith</ns:surname></ns:name>
      <ns:eventref ns:hlink="e1"/>
      <ns:eventref ns:hlink="e2"/>
    </ns:person>
  </ns:people>
</ns:database>"#;
        let p = single_person(xml);
        assert_eq!(p.event_refs, vec!["e1", "e2"]);
    }

    #[test]
    fn extract_person_eventref_inline_birth_still_works() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <people>
    <person handle="p1">
      <name><first>John</first><surname>Smith</surname></name>
      <birth><dateval val="1850-03-15"/></birth>
      <death><dateval val="1920-07-01"/></death>
      <eventref hlink="e1"/>
    </person>
  </people>
</database>"#;
        let p = single_person(xml);
        assert_eq!(p.birth_date.as_deref(), Some("1850-03-15"));
        assert_eq!(p.birth_year, Some(1850));
        assert_eq!(p.death_date.as_deref(), Some("1920-07-01"));
        assert_eq!(p.event_refs, vec!["e1"]);
    }

    // -----------------------------------------------------------------------
    // Event extraction
    // -----------------------------------------------------------------------

    fn events_from(xml: &str) -> Vec<ParsedEvent> {
        extract_events(xml).unwrap()
    }

    fn single_event(xml: &str) -> ParsedEvent {
        let mut es = events_from(xml);
        assert_eq!(es.len(), 1, "expected exactly one event");
        es.remove(0)
    }

    #[test]
    fn extract_event_birth() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0001">
      <eventtype><type>Birth</type></eventtype>
      <dateval val="1850-07-13"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0001");
        assert_eq!(e.event_type.as_deref(), Some("Birth"));
        assert_eq!(e.date_val.as_deref(), Some("1850-07-13"));
        assert_eq!(e.date_year, Some(1850));
    }

    #[test]
    fn extract_event_death() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0002">
      <eventtype><type>Death</type></eventtype>
      <dateval val="1920-07-01"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0002");
        assert_eq!(e.event_type.as_deref(), Some("Death"));
        assert_eq!(e.date_val.as_deref(), Some("1920-07-01"));
        assert_eq!(e.date_year, Some(1920));
    }

    #[test]
    fn extract_event_marriage() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0003">
      <eventtype><type>Marriage</type></eventtype>
      <dateval val="1875-06-15"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0003");
        assert_eq!(e.event_type.as_deref(), Some("Marriage"));
        assert_eq!(e.date_val.as_deref(), Some("1875-06-15"));
        assert_eq!(e.date_year, Some(1875));
    }

    #[test]
    fn extract_event_no_dateval() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0004">
      <eventtype><type>Birth</type></eventtype>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0004");
        assert_eq!(e.event_type.as_deref(), Some("Birth"));
        assert!(e.date_val.is_none());
        assert!(e.date_year.is_none());
    }

    #[test]
    fn extract_event_self_closing_dateval() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0005">
      <eventtype><type>Birth</type></eventtype>
      <dateval/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0005");
        assert_eq!(e.event_type.as_deref(), Some("Birth"));
        // Self-closing dateval with no val attribute should produce None
        assert!(e.date_val.is_none());
        assert!(e.date_year.is_none());
    }

    #[test]
    fn extract_events_multiple() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e1">
      <eventtype><type>Birth</type></eventtype>
      <dateval val="1850"/>
    </event>
    <event handle="e2">
      <eventtype><type>Death</type></eventtype>
      <dateval val="1920"/>
    </event>
  </events>
</database>"#;
        let es = events_from(xml);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].handle, "e1");
        assert_eq!(es[0].event_type.as_deref(), Some("Birth"));
        assert_eq!(es[0].date_year, Some(1850));
        assert_eq!(es[1].handle, "e2");
        assert_eq!(es[1].event_type.as_deref(), Some("Death"));
        assert_eq!(es[1].date_year, Some(1920));
    }

    #[test]
    fn extract_events_empty() {
        let es = events_from("");
        assert!(es.is_empty());
    }

    #[test]
    fn extract_events_no_events_section() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <header><created date="2025-01-01" version="5.2"/></header>
</database>"#;
        let es = events_from(xml);
        assert!(es.is_empty());
    }

    #[test]
    fn extract_events_namespace_prefixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:events>
    <ns:event ns:handle="e0001">
      <ns:eventtype><ns:type>Birth</ns:type></ns:eventtype>
      <ns:dateval ns:val="1850-07-13"/>
    </ns:event>
  </ns:events>
</ns:database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0001");
        assert_eq!(e.event_type.as_deref(), Some("Birth"));
        assert_eq!(e.date_val.as_deref(), Some("1850-07-13"));
        assert_eq!(e.date_year, Some(1850));
    }

    // -----------------------------------------------------------------------
    // Flat-format event type parsing (Gramps XML 1.7.1 / Gramps 5.1)
    // -----------------------------------------------------------------------

    #[test]
    fn extract_event_birth_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0001">
      <type>Birth</type>
      <dateval val="2000-03-03"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0001");
        assert_eq!(e.event_type.as_deref(), Some("Birth"));
        assert_eq!(e.date_val.as_deref(), Some("2000-03-03"));
        assert_eq!(e.date_year, Some(2000));
    }

    #[test]
    fn extract_event_death_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0002">
      <type>Death</type>
      <dateval val="1920-07-01"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0002");
        assert_eq!(e.event_type.as_deref(), Some("Death"));
        assert_eq!(e.date_val.as_deref(), Some("1920-07-01"));
        assert_eq!(e.date_year, Some(1920));
    }

    #[test]
    fn extract_event_marriage_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0003">
      <type>Marriage</type>
      <dateval val="1875-06-15"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0003");
        assert_eq!(e.event_type.as_deref(), Some("Marriage"));
        assert_eq!(e.date_val.as_deref(), Some("1875-06-15"));
        assert_eq!(e.date_year, Some(1875));
    }

    #[test]
    fn extract_event_no_dateval_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0004">
      <type>Birth</type>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0004");
        assert_eq!(e.event_type.as_deref(), Some("Birth"));
        assert!(e.date_val.is_none());
        assert!(e.date_year.is_none());
    }

    #[test]
    fn extract_events_mixed_formats() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0001">
      <eventtype><type>Birth</type></eventtype>
      <dateval val="1850-07-13"/>
    </event>
    <event handle="e0002">
      <type>Death</type>
      <dateval val="1920-07-01"/>
    </event>
  </events>
</database>"#;
        let es = events_from(xml);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].handle, "e0001");
        assert_eq!(es[0].event_type.as_deref(), Some("Birth"));
        assert_eq!(es[0].date_year, Some(1850));
        assert_eq!(es[1].handle, "e0002");
        assert_eq!(es[1].event_type.as_deref(), Some("Death"));
        assert_eq!(es[1].date_year, Some(1920));
    }

    #[test]
    fn extract_event_type_self_closing_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0005">
      <type/>
      <dateval val="2000-03-03"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0005");
        assert!(
            e.event_type.is_none(),
            "self-closing <type/> should not set event_type"
        );
        assert_eq!(e.date_val.as_deref(), Some("2000-03-03"));
    }

    #[test]
    fn extract_event_type_empty_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0006">
      <type></type>
      <dateval val="2000-03-03"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0006");
        assert!(
            e.event_type.is_none(),
            "empty <type></type> should not set event_type"
        );
        assert_eq!(e.date_val.as_deref(), Some("2000-03-03"));
    }

    #[test]
    fn extract_event_custom_type_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <events>
    <event handle="e0007">
      <type>Birth of Christ</type>
      <dateval val="0001-12-25"/>
    </event>
  </events>
</database>"#;
        let e = single_event(xml);
        assert_eq!(e.handle, "e0007");
        assert_eq!(
            e.event_type.as_deref(),
            Some("Birth of Christ"),
            "multi-word event type should be captured"
        );
        assert_eq!(e.date_val.as_deref(), Some("0001-12-25"));
        assert_eq!(e.date_year, Some(1));
    }

    // -----------------------------------------------------------------------
    // Event reference resolution
    // -----------------------------------------------------------------------

    fn event(handle: &str, event_type: &str, date_val: &str) -> ParsedEvent {
        ParsedEvent {
            handle: handle.to_string(),
            gramps_id: None,
            event_type: Some(event_type.to_string()),
            date_val: Some(date_val.to_string()),
            date_year: date_val.split('-').next().and_then(|y| y.parse().ok()),
        }
    }

    fn person(handle: &str, event_refs: &[&str]) -> ParsedPerson {
        ParsedPerson {
            handle: handle.to_string(),
            event_refs: event_refs.iter().map(|s| s.to_string()).collect(),
            ..ParsedPerson::default()
        }
    }

    #[test]
    fn resolve_event_refs_populates_dates() {
        let events = vec![
            event("e-birth", "Birth", "1850-07-13"),
            event("e-death", "Death", "1910-08-21"),
        ];
        let mut persons = vec![person("p1", &["e-birth", "e-death"])];

        resolve_event_refs(&mut persons, &events);

        assert_eq!(persons[0].birth_date.as_deref(), Some("1850-07-13"));
        assert_eq!(persons[0].birth_year, Some(1850));
        assert_eq!(persons[0].death_date.as_deref(), Some("1910-08-21"));
    }

    #[test]
    fn resolve_event_refs_no_overwrite() {
        let events = vec![
            event("e-birth", "Birth", "1850-07-13"),
            event("e-death", "Death", "1910-08-21"),
        ];
        let mut persons = vec![ParsedPerson {
            handle: "p1".to_string(),
            birth_date: Some("1845-01-01".to_string()),
            birth_year: Some(1845),
            death_date: Some("1905-05-05".to_string()),
            event_refs: vec!["e-birth".to_string(), "e-death".to_string()],
            ..ParsedPerson::default()
        }];

        resolve_event_refs(&mut persons, &events);

        // Inline values take precedence — event refs do NOT overwrite.
        assert_eq!(persons[0].birth_date.as_deref(), Some("1845-01-01"));
        assert_eq!(persons[0].birth_year, Some(1845));
        assert_eq!(persons[0].death_date.as_deref(), Some("1905-05-05"));
    }

    #[test]
    fn resolve_event_refs_unknown_event_type() {
        let events = vec![event("e-marriage", "Marriage", "1875-06-15")];
        let mut persons = vec![person("p1", &["e-marriage"])];

        resolve_event_refs(&mut persons, &events);

        // Marriage events do not populate birth/death fields.
        assert!(persons[0].birth_date.is_none());
        assert!(persons[0].birth_year.is_none());
        assert!(persons[0].death_date.is_none());
    }

    #[test]
    fn resolve_event_refs_missing_handle() {
        let events = vec![event("e-birth", "Birth", "1850-07-13")];
        let mut persons = vec![person("p1", &["e-nonexistent"])];

        resolve_event_refs(&mut persons, &events);

        // Missing event handle is silently skipped.
        assert!(persons[0].birth_date.is_none());
        assert!(persons[0].birth_year.is_none());
    }

    #[test]
    fn resolve_event_refs_empty_events() {
        let events = vec![];
        let mut persons = vec![person("p1", &["e-birth"])];

        resolve_event_refs(&mut persons, &events);

        assert!(persons[0].birth_date.is_none());
        assert!(persons[0].birth_year.is_none());
    }

    #[test]
    fn resolve_event_refs_empty_persons() {
        let events = vec![event("e-birth", "Birth", "1850-07-13")];
        let mut persons: Vec<ParsedPerson> = vec![];

        resolve_event_refs(&mut persons, &events);

        assert!(persons.is_empty());
    }

    // -----------------------------------------------------------------------
    // Family extraction helpers
    // -----------------------------------------------------------------------

    fn families_from(xml: &str) -> Vec<ParsedFamily> {
        extract_families(xml).unwrap()
    }

    fn single_family(xml: &str) -> ParsedFamily {
        let mut fs = families_from(xml);
        assert_eq!(fs.len(), 1, "expected exactly one family");
        fs.remove(0)
    }

    // -----------------------------------------------------------------------
    // Full family with parents and children
    // -----------------------------------------------------------------------

    #[test]
    fn extract_family_full() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <families>
    <family handle="f0001">
      <father hlink="p0001"/>
      <mother hlink="p0002"/>
      <childref hlink="p0003"/>
      <childref hlink="p0004"/>
    </family>
  </families>
</database>"#;
        let f = single_family(xml);
        assert_eq!(f.handle, "f0001");
        assert_eq!(f.father_handle.as_deref(), Some("p0001"));
        assert_eq!(f.mother_handle.as_deref(), Some("p0002"));
        assert_eq!(f.child_handles, vec!["p0003", "p0004"]);
    }

    // -----------------------------------------------------------------------
    // Empty family (no parents, no children)
    // -----------------------------------------------------------------------

    #[test]
    fn extract_family_empty() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <families>
    <family handle="f0001">
    </family>
  </families>
</database>"#;
        let f = single_family(xml);
        assert_eq!(f.handle, "f0001");
        assert!(f.father_handle.is_none());
        assert!(f.mother_handle.is_none());
        assert!(f.child_handles.is_empty());
    }

    // -----------------------------------------------------------------------
    // Self-closing family
    // -----------------------------------------------------------------------

    #[test]
    fn extract_family_self_closing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <families>
    <family handle="f0001"/>
  </families>
</database>"#;
        let f = single_family(xml);
        assert_eq!(f.handle, "f0001");
        assert!(f.father_handle.is_none());
        assert!(f.child_handles.is_empty());
    }

    // -----------------------------------------------------------------------
    // Multiple childref elements
    // -----------------------------------------------------------------------

    #[test]
    fn extract_family_multi_childref() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <families>
    <family handle="f0001">
      <childref hlink="c1"/>
      <childref hlink="c2"/>
      <childref hlink="c3"/>
    </family>
  </families>
</database>"#;
        let f = single_family(xml);
        assert_eq!(f.child_handles, vec!["c1", "c2", "c3"]);
    }

    // -----------------------------------------------------------------------
    // Dangling hlink (handle that doesn't exist)
    // -----------------------------------------------------------------------

    #[test]
    fn extract_family_dangling_hlink() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <families>
    <family handle="f0001">
      <father hlink="missing_person"/>
    </family>
  </families>
</database>"#;
        let f = single_family(xml);
        // The hlink is captured even if the person doesn't exist
        assert_eq!(f.father_handle.as_deref(), Some("missing_person"));
    }

    // -----------------------------------------------------------------------
    // Namespace-prefixed family
    // -----------------------------------------------------------------------

    #[test]
    fn extract_family_namespace_prefixed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:database xmlns:ns="http://gramps-project.org/xml/1.7.2/">
  <ns:families>
    <ns:family ns:handle="f0001">
      <ns:father ns:hlink="p0001"/>
      <ns:mother ns:hlink="p0002"/>
      <ns:childref ns:hlink="p0003"/>
    </ns:family>
  </ns:families>
</ns:database>"#;
        let f = single_family(xml);
        assert_eq!(f.handle, "f0001");
        assert_eq!(f.father_handle.as_deref(), Some("p0001"));
        assert_eq!(f.mother_handle.as_deref(), Some("p0002"));
        assert_eq!(f.child_handles, vec!["p0003"]);
    }

    // -----------------------------------------------------------------------
    // Multiple families
    // -----------------------------------------------------------------------

    #[test]
    fn extract_families_multiple() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<database xmlns="http://gramps-project.org/xml/1.7.2/">
  <families>
    <family handle="f1"><father hlink="p1"/></family>
    <family handle="f2"><mother hlink="p2"/></family>
  </families>
</database>"#;
        let fs = families_from(xml);
        assert_eq!(fs.len(), 2);
        assert_eq!(fs[0].handle, "f1");
        assert_eq!(fs[0].father_handle.as_deref(), Some("p1"));
        assert_eq!(fs[1].handle, "f2");
        assert_eq!(fs[1].mother_handle.as_deref(), Some("p2"));
    }
}
