//! Guitar Pro's own RSE sound (`<Sounds>` in a GP7 file) mapped to our nearest built-in
//! modelled tone. The RSE presets themselves are Arobas's material and are never used: only
//! the path and the effect names are read, to start a track on a fitting tone instead of a
//! generic one. The author's RSE name is shown next to our tone.

use gscore_model::{InstrumentKind, Song, TrackIndex};

use crate::settings::PresetTone;
use crate::tone_library;

use super::TrackSound;

/// An RSE sound and the built-in tone it maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mapped {
    /// The name Guitar Pro shows for it (`Blues`, `Steel Guitar`, ...).
    pub(super) name: String,
    /// The built-in tone's name in [`tone_library`].
    pub(super) tone: &'static str,
}

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

/// The ids of the effects in the RSE chain (`E30_EqGEq`, `A02_ComboAmericanClean`, ...).
fn effect_ids(xml: &str) -> Vec<&str> {
    let mut ids = Vec::new();
    let mut rest = xml;
    while let Some(index) = rest.find("id=\"") {
        let start = index.saturating_add(4);
        let Some(after) = rest.get(start..) else {
            break;
        };
        let Some(end) = after.find('"') else {
            break;
        };
        if let Some(id) = after.get(..end) {
            ids.push(id);
        }
        rest = after.get(end.saturating_add(1)..).unwrap_or_default();
    }
    ids
}

/// The built-in tone nearest to an RSE sound: its name, path and effect chain. The sound's
/// own name decides first (Guitar Pro's "Distortion Guitar" is a distorted guitar even when
/// the stored path still says "Clean Guitar" and the chain ends in a British stack), then
/// the path names the instrument and the preset, then an amp or pedal narrows it down.
fn tone_for(path: &str, name: &str, effects: &[&str]) -> Option<&'static str> {
    let path = path.to_lowercase();
    let name = name.to_lowercase();
    let effects = effects.join(" ").to_lowercase();
    let has = |needle: &str| effects.contains(needle);
    // The sound's own name is trusted for the instrument and for an explicit style
    // ("Distortion Guitar", "Electric Bass (pick)"), but not for a genre word like "Blues":
    // a clean American combo named "Blues" is still a clean sound.
    let named = |needle: &str| name.contains(needle);
    // The instrument first: a bass never takes a guitar's tone. Guitar Pro often keeps a
    // stale path ("Electric Guitars") for a bass sound named "Electric Bass (pick)".
    if path.contains("bass") || path.contains("basses") || named("bass") {
        return Some(if has("slap") || named("slap") {
            "Slap"
        } else if has("pick") || named("pick") {
            "Pick"
        } else if has("overdrive") || has("drive") || has("distortion") {
            "Driven"
        } else {
            "Clean Finger"
        });
    }
    if path.contains("nylon") || named("nylon") {
        return Some("Nylon Classical");
    }
    if path.contains("acoustic") || path.contains("steel") || named("acoustic") || named("steel") {
        return Some("Steel Fingerpicked");
    }
    // Electric guitars and other string instruments: read the sound's name, the preset and
    // the chain.
    Some(if has("djent") || has("tight") || named("djent") {
        "Djent Tight"
    } else if has("metal")
        || has("distortion")
        || named("metal")
        || named("distortion")
        || path.contains("distortion")
    {
        "Modern Metal Rhythm"
    } else if has("blues") || path.contains("blues") {
        "Texas Blues"
    } else if has("brit") {
        "British Crunch"
    } else if has("funk") || path.contains("funk") {
        "Funk Rhythm"
    } else if has("jazz") || path.contains("jazz") {
        "Warm Jazz"
    } else if has("lead") || path.contains("lead") {
        "80s Lead"
    } else if has("clean") || path.contains("clean") {
        "Clean Single Coil"
    } else if has("overdrive")
        || has("crunch")
        || path.contains("overdrive")
        || path.contains("crunch")
    {
        "Classic Rock Rhythm"
    } else {
        return None;
    })
}

/// The RSE sound of `track` of `song`, mapped to a built-in tone, if it has one.
pub(super) fn of_song(song: &Song, track: usize) -> Option<Mapped> {
    let view = song.track(TrackIndex::new(track))?;
    if view.instrument_kind() != InstrumentKind::Strings || view.is_percussion() {
        return None;
    }
    let xml = view.rse_sound()?;
    let path = element(xml, "Path").unwrap_or_default();
    let effects = effect_ids(xml);
    // The path's last segment names the sound when Guitar Pro stored no name of its own.
    let fallback = path.rsplit('/').next().unwrap_or_default();
    let name = element(xml, "Name")
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
        .to_owned();
    let tone = tone_for(path, &name, &effects)?;
    (!name.is_empty()).then_some(Mapped { name, tone })
}

/// Gives every string track without a tone of its own the built-in tone nearest to the RSE
/// sound Guitar Pro stored, so a file made with RSE starts on a fitting modelled sound.
/// An explicit tone in GuitarScore's own state always wins.
pub(super) fn apply_to_song(song: &mut Song) {
    for track in 0..song.track_count() {
        let Some(mapped) = of_song(song, track) else {
            continue;
        };
        let Some(midi) = song.track_sound(TrackIndex::new(track)) else {
            continue;
        };
        // A bass always plays the owner's saved bass preset, so an RSE Pick or Slap tone
        // is not stored (it would only be ignored, and would travel with the song).
        if matches!(midi.program, 32..=39) {
            continue;
        }
        let mut state = TrackSound::of_song(song, track);
        if state.tone.is_some() {
            continue;
        }
        let Some(built) = tone_library::find(mapped.tone) else {
            continue;
        };
        state.tone = Some(PresetTone::of(midi.program, &built.tone));
        let _ = song.set_track_sound_state(TrackIndex::new(track), state.json());
    }
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
    fn the_repository_files_map_to_their_nearest_tone() {
        for (file, expected) in [
            ("basic/staff.gp", "Clean Single Coil"),
            ("basic/note_lengths.gp", "Warm Jazz"),
            ("basic/steel_guitar_dadgad.gp", "Steel Fingerpicked"),
        ] {
            let mut song = song(file);
            let mapped = of_song(&song, 0).expect(file);
            assert_eq!(mapped.tone, expected, "{file}");
            apply_to_song(&mut song);
            let state = TrackSound::of_song(&song, 0);
            let tone = state
                .tone
                .expect("the tone was given")
                .tone()
                .expect("a tone");
            assert_eq!(tone_library::find(expected).unwrap().tone.kind, tone.kind);
            assert_eq!(tone, tone_library::find(expected).unwrap().tone, "{file}");
        }
    }

    #[test]
    fn a_drum_track_has_no_rse_tone_and_an_explicit_one_wins() {
        let kit = song("basic/multi_staff.gp");
        for track in 0..kit.track_count() {
            let view = kit.track(TrackIndex::new(track)).unwrap();
            if view.is_percussion() {
                assert!(of_song(&kit, track).is_none(), "a drum kit is not mapped");
            }
        }
        // A tone GuitarScore already keeps (from a reopened song) is left alone.
        let mut song = song("basic/staff.gp");
        let custom = tone_library::find("Djent Tight").unwrap().tone.clone();
        let program = song.track_sound(TrackIndex::new(0)).unwrap().program;
        let state = TrackSound {
            tone: Some(PresetTone::of(program, &custom)),
            ..TrackSound::default()
        };
        let _ = song
            .set_track_sound_state(TrackIndex::new(0), state.json())
            .expect("the state is set");
        apply_to_song(&mut song);
        assert_eq!(
            TrackSound::of_song(&song, 0).tone.unwrap().tone(),
            Some(custom)
        );
    }

    #[test]
    fn a_path_and_an_amp_name_narrow_the_choice() {
        assert_eq!(
            tone_for(
                "Stringed/Electric Guitars/Overdrive Guitar",
                "Overdrive Guitar",
                &["A02_ComboAmericanClean"]
            ),
            Some("Clean Single Coil")
        );
        assert_eq!(
            tone_for(
                "Stringed/Electric Guitars/Distortion Guitar",
                "Distortion Guitar",
                &["A10_MetalLead"]
            ),
            Some("Modern Metal Rhythm")
        );
        assert_eq!(
            tone_for(
                "Stringed/Electric Guitars/Distortion Guitar",
                "Distortion Guitar",
                &["A11_DjentTight"]
            ),
            Some("Djent Tight")
        );
        assert_eq!(
            tone_for(
                "Stringed/Basses/Clean Bass",
                "Clean Bass",
                &["E20_SlapComp"]
            ),
            Some("Slap")
        );
        assert_eq!(
            tone_for(
                "Stringed/Acoustic Guitars/Nylon Guitar",
                "Nylon Guitar",
                &[]
            ),
            Some("Nylon Classical")
        );
        assert_eq!(tone_for("Stringed/Unknown", "Unknown", &[]), None);
        // The stale path says electric guitar, but the sound is a picked bass.
        assert_eq!(
            tone_for(
                "Stringed/Electric Guitars/Clean Guitar",
                "Electric Bass (pick)",
                &["A10_StackClassic"]
            ),
            Some("Pick")
        );
        // The sound's own name wins over a stale path and a British stack: Guitar Pro's
        // "Distortion Guitar" is a high-gain sound even when it is stored under
        // "Clean Guitar" with a British amp.
        assert_eq!(
            tone_for(
                "Stringed/Electric Guitars/Clean Guitar",
                "Distortion Guitar",
                &["E03_OverdriveScreamer", "A06_StackBritishStack"]
            ),
            Some("Modern Metal Rhythm")
        );
    }

    #[test]
    fn the_mapped_name_is_the_rse_name_or_the_paths_last_part() {
        let song = song("basic/staff.gp");
        assert_eq!(of_song(&song, 0).unwrap().name, "Blues");
        let xml =
            "<Sounds><Sound><Name></Name><Path>Stringed/Basses/Pick Bass</Path></Sound></Sounds>";
        assert_eq!(element(xml, "Path"), Some("Stringed/Basses/Pick Bass"));
        assert_eq!(text("<![CDATA[Blues]]>"), "Blues");
    }
}
