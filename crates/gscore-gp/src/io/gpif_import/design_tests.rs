//! GuitarScore tests for Guitar Pro's design-mode layout data: a bar's width scale
//! (`<XProperty>`) and the beat offsets GuitarScore keeps in a GuitarScore property.

use crate::io::Song;
use crate::model::{BAR_SCALE, MASTER_BAR_SCALE, SCALE_RANGE};

use super::display_tests::{MULTI_STAFF, with_gpif};

/// Where the first `</tag>` that is not part of a longer name (`</Bars>`, `</Beats>`)
/// starts.
fn find_closing(text: &str, tag: &str) -> Option<usize> {
    let needle = format!("</{tag}>");
    text.match_indices(&needle).find_map(|(at, _)| {
        let after = at.saturating_add(needle.len());
        let next = text.get(after..).and_then(|rest| rest.chars().next());
        (!next.is_some_and(|c| c.is_ascii_alphanumeric())).then_some(at)
    })
}

/// `text` with `snippet` just before the first `</tag>`.
fn insert_before(text: &str, tag: &str, snippet: &str) -> String {
    match find_closing(text, tag) {
        Some(at) => {
            let mut out = String::with_capacity(text.len().saturating_add(snippet.len()));
            out.push_str(text.get(..at).unwrap_or_default());
            out.push_str(snippet);
            out.push_str(text.get(at..).unwrap_or_default());
            out
        }
        None => text.to_owned(),
    }
}

/// `text` with `snippet` just before the `</closing>` that ends the element starting at
/// the first `start` marker (the first `<Bar id=` rather than an automation's `<Bar>`).
fn insert_inside(text: &str, start: &str, closing: &str, snippet: &str) -> String {
    let Some(at) = text.find(start) else {
        return text.to_owned();
    };
    let Some(end) = text.get(at..).and_then(|rest| rest.find(closing)) else {
        return text.to_owned();
    };
    let at = at.saturating_add(end);
    let mut out = String::with_capacity(text.len().saturating_add(snippet.len()));
    out.push_str(text.get(..at).unwrap_or_default());
    out.push_str(snippet);
    out.push_str(text.get(at..).unwrap_or_default());
    out
}

/// `text` with `snippet` just after the first `marker`.
fn insert_after(text: &str, marker: &str, snippet: &str) -> String {
    match text.find(marker) {
        Some(at) => {
            let at = at.saturating_add(marker.len());
            let mut out = String::with_capacity(text.len().saturating_add(snippet.len()));
            out.push_str(text.get(..at).unwrap_or_default());
            out.push_str(snippet);
            out.push_str(text.get(at..).unwrap_or_default());
            out
        }
        None => text.to_owned(),
    }
}

/// A GP7 file with one bar and the first master bar resized, and the first beat moved.
fn designed() -> Vec<u8> {
    with_gpif(MULTI_STAFF, |gpif| {
        // The bar's own scale, and a property GuitarScore does not understand, which must
        // survive a save all the same.
        let gpif = insert_inside(
            gpif,
            "<Bar id=",
            "</Bar>",
            &format!(
                "<XProperties><XProperty id=\"{BAR_SCALE}\"><Double>1.5</Double></XProperty>\
                 <XProperty id=\"1124204545\"><Int>1</Int></XProperty></XProperties>"
            ),
        );
        // The song-wide scale.
        let gpif = insert_before(
            &gpif,
            "MasterBar",
            &format!(
                "<XProperties><XProperty id=\"{MASTER_BAR_SCALE}\">\
                 <Double>0.5</Double></XProperty></XProperties>"
            ),
        );
        // A moved beat: the first one, inside its own `<Properties>` (a beat has at most
        // one, so a second would not read).
        let property = "<Property name=\"GuitarScoreSpacing\"><Float>0.75</Float></Property>";
        match gpif.find("<Beat id=") {
            Some(start) => {
                let end = gpif
                    .get(start..)
                    .and_then(|rest| rest.find("</Beat>"))
                    .map_or(gpif.len(), |at| start.saturating_add(at));
                let beat = gpif.get(start..end).unwrap_or_default();
                let edited = if beat.contains("<Properties>") {
                    insert_after(beat, "<Properties>", property)
                } else {
                    insert_before(
                        beat,
                        "Beat",
                        &format!("<Properties>{property}</Properties>"),
                    )
                };
                let mut out = String::with_capacity(gpif.len().saturating_add(property.len()));
                out.push_str(gpif.get(..start).unwrap_or_default());
                out.push_str(&edited);
                out.push_str(gpif.get(end..).unwrap_or_default());
                out
            }
            None => gpif,
        }
    })
}

#[test]
fn design_data_is_read() {
    let mut song = Song::default();
    song.read_gp(&designed()).expect("reads");
    assert_eq!(song.tracks[0].measures[0].display_scale(), Some(1.5));
    assert_eq!(song.measure_headers[0].display_scale(), Some(0.5));
    let first_beat = song.tracks[0].measures[0]
        .voices
        .first()
        .and_then(|voice| voice.beats.first())
        .expect("a beat");
    assert_eq!(first_beat.spacing, Some(0.75));
    // The bar's other property is kept as the file wrote it.
    assert!(
        song.tracks[0].measures[0]
            .x_properties
            .iter()
            .any(|property| property.id == "1124204545" && property.value == "1")
    );
}

#[test]
fn design_data_survives_saving() {
    let mut song = Song::default();
    song.read_gp(&designed()).expect("reads");
    let mut reread = Song::default();
    reread
        .read_gp(&song.write_gp().expect("writes"))
        .expect("reads back");
    assert_eq!(reread.tracks[0].measures[0].display_scale(), Some(1.5));
    assert_eq!(reread.measure_headers[0].display_scale(), Some(0.5));
    let first_beat = reread.tracks[0].measures[0]
        .voices
        .first()
        .and_then(|voice| voice.beats.first())
        .expect("a beat");
    assert_eq!(first_beat.spacing, Some(0.75));
    assert!(
        reread.tracks[0].measures[0]
            .x_properties
            .iter()
            .any(|property| property.id == "1124204545" && property.value == "1"),
        "an unknown property survived"
    );
}

/// A GP7 file whose first beat carries Guitar Pro's own beat layout properties (as GP8
/// writes them for beaming and its design mode).
fn with_beat_properties() -> Vec<u8> {
    with_gpif(MULTI_STAFF, |gpif| {
        insert_inside(
            gpif,
            "<Beat id=",
            "</Beat>",
            "<XProperties>\
             <XProperty id=\"1124204545\"><Int>1</Int></XProperty>\
             <XProperty id=\"1124204552\"><Int>8</Int></XProperty>\
             </XProperties>",
        )
    })
}

#[test]
fn guitar_pros_own_beat_layout_properties_survive_saving() {
    let mut song = Song::default();
    song.read_gp(&with_beat_properties()).expect("reads");
    let kept = |song: &Song| {
        song.tracks[0].measures[0]
            .voices
            .first()
            .and_then(|voice| voice.beats.first())
            .map(|beat| {
                beat.x_properties
                    .iter()
                    .map(|property| (property.id.clone(), property.value.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let properties = kept(&song);
    assert_eq!(properties.len(), 2, "{properties:?}");
    assert!(properties.contains(&("1124204545".to_owned(), "1".to_owned())));
    // A save and reload keeps them, which a GP8 file's design data needs.
    let mut reread = Song::default();
    reread
        .read_gp(&song.write_gp().expect("writes"))
        .expect("reads back");
    assert_eq!(kept(&reread), properties);
}

#[test]
fn a_hostile_scale_is_clamped() {
    let file = with_gpif(MULTI_STAFF, |gpif| {
        insert_inside(
            gpif,
            "<Bar id=",
            "</Bar>",
            &format!(
                "<XProperties><XProperty id=\"{BAR_SCALE}\"><Double>1e9</Double>\
                 </XProperty></XProperties>"
            ),
        )
    });
    let mut song = Song::default();
    song.read_gp(&file).expect("reads");
    assert_eq!(
        song.tracks[0].measures[0].display_scale(),
        Some(SCALE_RANGE.1)
    );
}

#[test]
fn a_scale_set_in_the_model_replaces_the_file_value() {
    let mut song = Song::default();
    song.read_gp(MULTI_STAFF).expect("reads");
    assert_eq!(song.tracks[0].measures[0].display_scale(), None);
    song.tracks[0].measures[0].set_display_scale(Some(2.0));
    let mut reread = Song::default();
    reread
        .read_gp(&song.write_gp().expect("writes"))
        .expect("reads back");
    assert_eq!(reread.tracks[0].measures[0].display_scale(), Some(2.0));
    // Removing it writes no property at all.
    let mut plain = Song::default();
    plain.read_gp(MULTI_STAFF).expect("reads");
    plain.tracks[0].measures[0].set_display_scale(Some(2.0));
    plain.tracks[0].measures[0].set_display_scale(None);
    let mut reread = Song::default();
    reread
        .read_gp(&plain.write_gp().expect("writes"))
        .expect("reads back");
    assert_eq!(reread.tracks[0].measures[0].display_scale(), None);
}
