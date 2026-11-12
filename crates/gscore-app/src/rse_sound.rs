//! Guitar Pro's own RSE sound of a track (`<Sounds>` in a GP7 file), read only for its
//! name. The RSE presets themselves are Arobas's material and are never used: the name is
//! a placeholder the tone library can map to one of its tones (`cargo xtask tones rse`),
//! so a track made with that sound starts on the tone the owner dialed in for it.

use gscore_model::{InstrumentKind, Song, TrackIndex};

/// The text of the first `<tag>` in `xml`, with CDATA unwrapped.
fn element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?.checked_add(open.len())?;
    let rest = xml.get(start..)?;
    let end = rest.find(&close)?;
    Some(text(rest.get(..end)?))
}

/// `raw` without the CDATA wrapper or surrounding whitespace.
fn text(raw: &str) -> &str {
    let raw = raw.trim();
    raw.strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
        .unwrap_or(raw)
        .trim()
}

/// The name the tone library knows the first sound of a `<Sounds>` element by: the
/// instrument (the last part of its path) and the sound's own name, as
/// `Distortion Guitar/Hard Rock`. The same name under another instrument is another sound
/// (`Clean Guitar/Chorus` is not `Slap Bass/Chorus`). `None` when it names neither.
pub(crate) fn key(xml: &str) -> Option<String> {
    let path = element(xml, "Path").unwrap_or_default();
    let instrument = path.rsplit('/').next().unwrap_or_default().trim();
    let name = element(xml, "Name").unwrap_or_default();
    match (instrument.is_empty(), name.is_empty()) {
        (true, true) => None,
        (false, true) => Some(instrument.to_owned()),
        (true, false) => Some(name.to_owned()),
        (false, false) => Some(format!("{instrument}/{name}")),
    }
}

/// The RSE sound of `track` of `song` (see [`key`]), if it is a string track with one.
pub(crate) fn of_track(song: &Song, track: usize) -> Option<String> {
    let view = song.track(TrackIndex::new(track))?;
    if view.instrument_kind() != InstrumentKind::Strings || view.is_percussion() {
        return None;
    }
    key(view.rse_sound()?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use gscore_io::import_guitar_pro_from_bytes;

    /// The repository test file at `relative` (`testdata/`), loaded.
    fn song(relative: &str) -> Song {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata")
            .join(relative);
        let bytes = std::fs::read(path).expect("the test file");
        import_guitar_pro_from_bytes(&bytes).expect("it reads").song
    }

    #[test]
    fn a_sound_is_known_by_its_instrument_and_its_name() {
        let xml = "<Sounds><Sound><Name><![CDATA[Hard Rock]]></Name>\
                   <Path>Stringed/Electric Guitars/Distortion Guitar</Path></Sound>\
                   <Sound><Name>Wah</Name><Path>Stringed/Other</Path></Sound></Sounds>";
        // The first sound counts; a later one is what an automation switches to.
        assert_eq!(key(xml).as_deref(), Some("Distortion Guitar/Hard Rock"));
        let unnamed = "<Sounds><Sound><Name></Name><Path>Stringed/Basses/Pick Bass</Path>\
                       </Sound></Sounds>";
        assert_eq!(key(unnamed).as_deref(), Some("Pick Bass"));
        assert_eq!(
            key("<Sounds><Sound><Name>Blues</Name></Sound></Sounds>").as_deref(),
            Some("Blues")
        );
        assert_eq!(key("<Sounds></Sounds>"), None);
    }

    #[test]
    fn a_string_track_of_a_guitar_pro_file_has_its_sound_and_a_drum_kit_none() {
        let staff = song("basic/staff.gp");
        assert_eq!(
            of_track(&staff, 0).as_deref(),
            Some("Overdrive Guitar/Blues")
        );
        let kit = song("basic/multi_staff.gp");
        for track in 0..kit.track_count() {
            if kit.track(TrackIndex::new(track)).unwrap().is_percussion() {
                assert_eq!(of_track(&kit, track), None);
            }
        }
        assert_eq!(of_track(&staff, 99), None);
    }
}
