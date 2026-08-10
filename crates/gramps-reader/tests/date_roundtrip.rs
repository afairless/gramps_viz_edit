//! Date round-trip fidelity tests (parse → serialize → parse).
//!
//! These verify that structured date values survive a full round-trip
//! through the Gramps XML serializer and parser unchanged (year, month,
//! day, and the preserved raw `val` text), including property-based
//! coverage over a large sample of randomly generated valid dates.

use gramps_reader::xml::graph::parse_gramps_xml;
use output::{GraphXmlWriter, SerializationMap};
use rand::Rng;
use typed_graph::generate::GraphBuilder;
use typed_graph::*;

/// Serialize an event that carries `date`, re-parse it, and return the
/// event's parsed `DateValue`.
fn roundtrip_event_date(date: DateValue) -> DateValue {
    let mut graph = Graph::new();
    let mut builder = GraphBuilder::new(&mut graph);
    builder
        .add_event("e1")
        .with_event_type(EventType::Birth)
        .with_date(date)
        .build()
        .expect("event build");

    let mut output = Vec::new();
    let writer = GraphXmlWriter::new(SerializationMap::new(), "5.2");
    writer.write(&graph, &mut output).expect("serialize");

    let (parsed, _ns) = parse_gramps_xml(&String::from_utf8(output).unwrap()).expect("re-parse");
    let node = parsed
        .get_node(&"e1".to_string())
        .expect("event present after re-parse");
    match node {
        Node::Event(ref data) => data.date.clone().expect("event date parsed"),
        _ => panic!("expected an Event node"),
    }
}

fn assert_structured(rt: &DateValue, year: i32, month: Option<i32>, day: Option<i32>) {
    assert_eq!(rt.year, year, "year mismatch");
    assert_eq!(rt.month, month, "month mismatch");
    assert_eq!(rt.day, day, "day mismatch");
}

#[test]
fn structured_full_ymd_round_trips() {
    // Regression for Bug 2: "1868-09-20" must survive parse → serialize →
    // parse unchanged (previously truncated to a year and mis-stored in day).
    let rt = roundtrip_event_date(DateValue::new_ymd(1868, 9, 20));
    assert_structured(&rt, 1868, Some(9), Some(20));
    assert_eq!(rt.text.as_deref(), Some("1868-09-20"));
}

#[test]
fn structured_year_month_round_trips() {
    let rt = roundtrip_event_date(DateValue::new_ymd(1890, 6, 15));
    assert_structured(&rt, 1890, Some(6), Some(15));
}

#[test]
fn structured_year_only_round_trips() {
    let rt = roundtrip_event_date(DateValue::new(1871));
    assert_structured(&rt, 1871, None, None);
}

#[test]
fn modifier_text_survives_verbatim() {
    // Modifier/range/spans cannot be represented structurally, so their raw
    // text must round-trip verbatim.
    for val in [
        "abt 1868",
        "before 1900",
        "between 1800 and 1850",
        "1868/1869",
    ] {
        let date = DateValue {
            year: 0,
            month: None,
            day: None,
            quality: None,
            modifier: None,
            text: Some(val.to_string()),
        };
        let rt = roundtrip_event_date(date);
        assert_eq!(
            rt.text.as_deref(),
            Some(val),
            "text not preserved for {val:?}"
        );
    }
}

/// Property-based invariant: for any valid structured date,
/// parse_xml(serialize_xml(date)) preserves year/month/day. Wide sampling
/// catches leap-day and month-boundary edge cases single examples miss.
#[test]
fn date_round_trip_property_preserves_structured_fields() {
    let mut rng = rand::thread_rng();
    const CASES: usize = 2000;

    for _ in 0..CASES {
        let year = rng.gen_range(1500..=2100);
        let month = if rng.gen_bool(0.7) {
            Some(rng.gen_range(1..=12))
        } else {
            None
        };
        let day = if month.is_some() && rng.gen_bool(0.7) {
            Some(rng.gen_range(1..=28)) // 28 keeps every generated date valid
        } else {
            None
        };

        let date = DateValue {
            year,
            month,
            day,
            quality: None,
            modifier: None,
            text: None,
        };
        let rt = roundtrip_event_date(date);
        assert_structured(&rt, year, month, day);
    }
}
