//! A GP7 file's fingerprint: how often each element, property and automation occurs in its
//! `score.gpif`, keyed by where it sits. Comparing the fingerprints of a file and of the
//! same file saved again shows what a save leaves out, including what the reader never
//! picked up (which a comparison of the songs cannot see). GuitarScore addition.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};

/// The elements a key names as its place: the nearest one around an element.
const PLACES: [&str; 15] = [
    "Score",
    "MasterTrack",
    "Track",
    "MasterBar",
    "Bar",
    "Voice",
    "Beat",
    "Note",
    "Rhythm",
    "Automation",
    "Staff",
    "Sound",
    "RSE",
    "Lyrics",
    "InstrumentSet",
];

/// The largest `score.gpif` read (far above a real one).
const MAX_GPIF_BYTES: u64 = 256 * 1024 * 1024;

/// The fingerprint of the GP7 file `data` (a zip holding `Content/score.gpif`): each key
/// with how often it occurs. Keys are `Place <Element>`, `Place Property Name` (and
/// `XProperty`), `Automation type Name`, `Beat dynamic Value` and `Lyrics text` (a lyrics
/// line that holds text).
pub fn gpif_fingerprint(data: &[u8]) -> Result<BTreeMap<String, usize>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(data))
        .map_err(|error| format!("not a GP7 file: {error}"))?;
    let file = zip
        .by_name("Content/score.gpif")
        .map_err(|error| format!("no score.gpif: {error}"))?;
    let mut xml = String::new();
    file.take(MAX_GPIF_BYTES)
        .read_to_string(&mut xml)
        .map_err(|error| format!("score.gpif: {error}"))?;
    xml_fingerprint(&xml)
}

/// [`gpif_fingerprint`] of the XML itself.
pub fn xml_fingerprint(xml: &str) -> Result<BTreeMap<String, usize>, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut add = |key: String| {
        let count = counts.entry(key).or_insert(0);
        *count = count.saturating_add(1);
    };
    // The open elements, and the text of the one being read.
    let mut open: Vec<String> = Vec::new();
    let mut text = String::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("score.gpif is not well-formed XML: {error}"))?;
        match event {
            Event::Start(element) => {
                add(key_of(&element, place(&open)));
                open.push(name_of(&element));
                text.clear();
            }
            Event::Empty(element) => add(key_of(&element, place(&open))),
            Event::Text(content) => {
                text.push_str(&content.into_inner());
            }
            Event::CData(content) => {
                text.push_str(&content.into_inner());
            }
            Event::End(_) => {
                let closed = open.pop().unwrap_or_default();
                let parent = open.last().map(String::as_str);
                let value = text.trim();
                match (closed.as_str(), parent) {
                    ("Type", Some("Automation")) => add(format!("Automation type {value}")),
                    ("Dynamic", Some("Beat")) => add(format!("Beat dynamic {value}")),
                    ("Text", Some("Line")) if !value.is_empty() => add("Lyrics text".to_owned()),
                    _ => {}
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(counts)
}

/// The nearest place among the open elements (`GPIF` at the top).
fn place(open: &[String]) -> &str {
    open.iter()
        .rev()
        .find(|name| PLACES.contains(&name.as_str()))
        .map_or("GPIF", String::as_str)
}

fn name_of(element: &BytesStart<'_>) -> String {
    element.name().as_ref().to_owned()
}

/// The key of `element` inside `place`: a property by its name, anything else by its tag.
fn key_of(element: &BytesStart<'_>, place: &str) -> String {
    let tag = name_of(element);
    let attribute = |name: &str| {
        element
            .attributes()
            .flatten()
            .find(|attribute| attribute.key.as_ref() == name)
            .map(|attribute| attribute.value.into_owned())
    };
    match tag.as_str() {
        "Property" => format!("{place} Property {}", attribute("name").unwrap_or_default()),
        "XProperty" => format!("{place} XProperty {}", attribute("id").unwrap_or_default()),
        _ => format!("{place} <{tag}>"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::xml_fingerprint;

    #[test]
    fn elements_and_properties_are_counted_where_they_sit() {
        let xml = r#"<GPIF><Beats><Beat id="0"><Dynamic>F</Dynamic><Properties>
            <Property name="Slapped"><Enable /></Property></Properties></Beat>
            <Beat id="1"><Dynamic>MF</Dynamic></Beat></Beats>
            <Notes><Note id="0"><Properties><Property name="Muted"><Enable/></Property>
            </Properties></Note></Notes>
            <MasterTrack><Automations><Automation><Type>Tempo</Type></Automation>
            </Automations></MasterTrack>
            <Tracks><Track><Lyrics><Line><Text><![CDATA[la la]]></Text></Line>
            <Line><Text><![CDATA[]]></Text></Line></Lyrics></Track></Tracks></GPIF>"#;
        let counts = xml_fingerprint(xml).unwrap();
        let count = |key: &str| counts.get(key).copied().unwrap_or(0);
        assert_eq!(count("GPIF <Beat>"), 2);
        assert_eq!(count("Beat <Dynamic>"), 2);
        assert_eq!(count("Beat dynamic F"), 1);
        assert_eq!(count("Beat dynamic MF"), 1);
        assert_eq!(count("Beat Property Slapped"), 1);
        assert_eq!(count("Beat <Enable>"), 1);
        assert_eq!(count("Note Property Muted"), 1);
        assert_eq!(count("Automation type Tempo"), 1);
        assert_eq!(count("Lyrics text"), 1);
        assert_eq!(count("Lyrics <Line>"), 2);
        assert!(xml_fingerprint("<GPIF><Beat></GPIF>").is_err());
    }
}
