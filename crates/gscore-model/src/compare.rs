//! Comparing two songs: what a save-and-reload cycle (or a conversion) changed.
//!
//! Compares what the model exposes: song information, tracks (name, instrument, strings,
//! tuning), measure time signatures, and every beat (kind, length, beat effects) and note
//! (string, fret, kind, effects). Placeholder beats and empty voices are ignored, because
//! formats differ in how many voice slots they store. Not compared: data the model has no
//! accessors for (lyrics, mix tables, grace notes, fingerings, ...).

use std::fmt;

use crate::duration::Duration;
use crate::dynamics::Dynamic;
use crate::effects::{
    AccentKind, Bend, Harmonic, Legato, SlapKind, SlideKind, StrokeDirection, Vibrato, Whammy,
};
use crate::error::ModelError;
use crate::position::{MeasureIndex, VoiceIndex};
use crate::song::Song;
use crate::views::{BeatKind, MeasureView, NoteKind, NoteView, TrackView, VoiceView};

/// One difference between two songs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// Where: "song", "track 2", "track 1, measure 3, voice 1, beat 2", ...
    pub location: String,
    /// What differs, with both values.
    pub detail: String,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.detail)
    }
}

/// Everything that differs between `before` and `after`, in song order. Empty if the
/// compared content is equal.
pub fn differences(before: &Song, after: &Song) -> Vec<Difference> {
    let mut diff = Diffs(Vec::new());
    let song = "song";
    diff.check(song, "title", before.title(), after.title());
    diff.check(song, "subtitle", before.subtitle(), after.subtitle());
    diff.check(song, "artist", before.artist(), after.artist());
    diff.check(song, "album", before.album(), after.album());
    diff.check(song, "lyricist", before.lyricist(), after.lyricist());
    diff.check(song, "composer", before.composer(), after.composer());
    diff.check(song, "copyright", before.copyright(), after.copyright());
    diff.check(song, "tab author", before.tab_author(), after.tab_author());
    diff.check(
        song,
        "instructions",
        before.instructions(),
        after.instructions(),
    );
    diff.check(
        song,
        "notes",
        before.notes().collect::<Vec<_>>(),
        after.notes().collect::<Vec<_>>(),
    );
    diff.check(song, "tempo", before.tempo(), after.tempo());
    diff.check(
        song,
        "measures",
        before.measure_count(),
        after.measure_count(),
    );
    diff.check(song, "tracks", before.track_count(), after.track_count());
    for (a, b) in before.tracks().zip(after.tracks()) {
        compare_track(&mut diff, &a, &b);
    }
    // Song-wide measure headers: tempo changes, signatures, markers and repeats.
    let measures = before.measure_count().max(after.measure_count());
    for index in 0..measures {
        let measure = MeasureIndex::new(index);
        let at = format!("measure {}", index.saturating_add(1));
        diff.check(
            &at,
            "section",
            before.section(measure),
            after.section(measure),
        );
        // Measure 1's tempo is the song's initial tempo, compared above; a redundant
        // header value there is not a difference.
        if index > 0 {
            diff.check(
                &at,
                "tempo change",
                before.tempo_change(measure),
                after.tempo_change(measure),
            );
        }
        diff.check(
            &at,
            "key signature",
            before.key_signature(measure),
            after.key_signature(measure),
        );
        diff.check(&at, "repeat", before.repeat(measure), after.repeat(measure));
    }
    diff.0
}

/// Collects differences; `check` records one if the two values differ.
struct Diffs(Vec<Difference>);

impl Diffs {
    fn check<T: PartialEq + fmt::Debug>(&mut self, location: &str, what: &str, a: T, b: T) {
        if a != b {
            self.0.push(Difference {
                location: location.to_owned(),
                detail: format!("{what} {a:?} became {b:?}"),
            });
        }
    }
}

fn compare_track(diff: &mut Diffs, a: &TrackView<'_>, b: &TrackView<'_>) {
    let at = a.index().to_string();
    diff.check(&at, "name", a.name(), b.name());
    diff.check(&at, "instrument", a.instrument_kind(), b.instrument_kind());
    diff.check(
        &at,
        "instrument name",
        a.instrument_name(),
        b.instrument_name(),
    );
    diff.check(&at, "strings", a.string_count(), b.string_count());
    diff.check(&at, "tuning", a.tuning().ok(), b.tuning().ok());
    diff.check(&at, "capo", a.capo().ok(), b.capo().ok());
    diff.check(
        &at,
        "tuning name",
        a.tuning_name().ok(),
        b.tuning_name().ok(),
    );
    diff.check(&at, "measures", a.measures().len(), b.measures().len());
    for (ma, mb) in a.measures().zip(b.measures()) {
        compare_measure(diff, &at, &ma, &mb);
    }
}

fn compare_measure(diff: &mut Diffs, track: &str, a: &MeasureView<'_>, b: &MeasureView<'_>) {
    let at = format!("{track}, {}", a.index());
    diff.check(
        &at,
        "time signature",
        a.time_signature().ok(),
        b.time_signature().ok(),
    );
    let count = a.voices().len().max(b.voices().len());
    for index in (0..count).map(VoiceIndex::new) {
        let beats_a = beats(a.voice(index));
        let beats_b = beats(b.voice(index));
        let at = format!("{at}, {index}");
        diff.check(&at, "beats", beats_a.len(), beats_b.len());
        for (number, (ba, bb)) in (1_usize..).zip(beats_a.iter().zip(&beats_b)) {
            compare_beat(diff, &format!("{at}, beat {number}"), ba, bb);
        }
    }
}

/// What is compared of a beat: kind, length, beat effects and its notes (by string).
#[derive(Debug, PartialEq)]
struct BeatContent {
    kind: BeatKind,
    duration: Result<Duration, ModelError>,
    pick_stroke: Option<StrokeDirection>,
    brush_stroke: Option<StrokeDirection>,
    slap: Option<SlapKind>,
    fade_in: bool,
    whammy_bar: Option<Whammy>,
    legato: Option<Legato>,
    vibrato: Option<Vibrato>,
    notes: Vec<NoteContent>,
}

/// What is compared of a note: position, kind and every effect the model exposes.
#[derive(Debug, PartialEq)]
struct NoteContent {
    string: Option<u8>,
    fret: Option<u8>,
    kind: NoteKind,
    hammer: bool,
    bend: Option<Bend>,
    slides: Vec<SlideKind>,
    harmonic: Option<Harmonic>,
    ghost: bool,
    accent: Option<AccentKind>,
    palm_mute: bool,
    let_ring: bool,
    staccato: bool,
    trill: bool,
    tremolo_picking: bool,
    vibrato: Option<Vibrato>,
    dynamic: Dynamic,
}

impl NoteContent {
    fn of(note: &NoteView<'_>) -> Self {
        NoteContent {
            string: note.string().ok().map(|s| s.get()),
            fret: note.fret().ok().map(|f| f.get()),
            kind: note.kind(),
            hammer: note.is_hammer(),
            bend: note.bend(),
            slides: note.slides().collect(),
            harmonic: note.harmonic(),
            ghost: note.is_ghost(),
            accent: note.accent(),
            palm_mute: note.is_palm_muted(),
            let_ring: note.is_let_ring(),
            staccato: note.is_staccato(),
            trill: note.has_trill(),
            tremolo_picking: note.has_tremolo_picking(),
            vibrato: note.vibrato(),
            dynamic: note.dynamic(),
        }
    }
}

/// The non-placeholder beats of a voice; a missing voice has none.
fn beats(voice: Option<VoiceView<'_>>) -> Vec<BeatContent> {
    let Some(voice) = voice else {
        return Vec::new();
    };
    voice
        .beats()
        .filter(|b| b.kind() != BeatKind::Empty)
        .map(|beat| {
            let mut notes: Vec<NoteContent> = beat.notes().map(|n| NoteContent::of(&n)).collect();
            notes.sort_by_key(|note| note.string);
            BeatContent {
                kind: beat.kind(),
                duration: beat.duration(),
                pick_stroke: beat.pick_stroke(),
                brush_stroke: beat.brush_stroke(),
                slap: beat.slap(),
                fade_in: beat.has_fade_in(),
                whammy_bar: beat.whammy_bar(),
                legato: beat.legato(),
                vibrato: beat.vibrato(),
                notes,
            }
        })
        .collect()
}

/// Records each field of two beats that differs, then each note's.
fn compare_beat(diff: &mut Diffs, at: &str, a: &BeatContent, b: &BeatContent) {
    diff.check(at, "kind", a.kind, b.kind);
    diff.check(at, "length", &a.duration, &b.duration);
    diff.check(at, "pick stroke", a.pick_stroke, b.pick_stroke);
    diff.check(at, "brush stroke", a.brush_stroke, b.brush_stroke);
    diff.check(at, "slap", a.slap, b.slap);
    diff.check(at, "fade in", a.fade_in, b.fade_in);
    diff.check(at, "whammy bar", a.whammy_bar, b.whammy_bar);
    diff.check(at, "legato", a.legato, b.legato);
    diff.check(at, "vibrato", a.vibrato, b.vibrato);
    diff.check(at, "notes", a.notes.len(), b.notes.len());
    for (na, nb) in a.notes.iter().zip(&b.notes) {
        let at = match na.string {
            Some(string) => format!("{at}, string {string}"),
            None => format!("{at}, note"),
        };
        diff.check(&at, "string", na.string, nb.string);
        diff.check(&at, "fret", na.fret, nb.fret);
        diff.check(&at, "kind", na.kind, nb.kind);
        diff.check(&at, "hammer-on/pull-off", na.hammer, nb.hammer);
        diff.check(&at, "bend", na.bend, nb.bend);
        diff.check(&at, "slides", &na.slides, &nb.slides);
        diff.check(&at, "harmonic", na.harmonic, nb.harmonic);
        diff.check(&at, "ghost note", na.ghost, nb.ghost);
        diff.check(&at, "accent", na.accent, nb.accent);
        diff.check(&at, "palm mute", na.palm_mute, nb.palm_mute);
        diff.check(&at, "let ring", na.let_ring, nb.let_ring);
        diff.check(&at, "staccato", na.staccato, nb.staccato);
        diff.check(&at, "trill", na.trill, nb.trill);
        diff.check(
            &at,
            "tremolo picking",
            na.tremolo_picking,
            nb.tremolo_picking,
        );
        diff.check(&at, "vibrato", na.vibrato, nb.vibrato);
        diff.check(&at, "dynamic", na.dynamic, nb.dynamic);
    }
}

/// Settings that a save-and-reload may lose even when the musical content is kept: track
/// color, mute, solo, volume, pan and shown staves, and the master mix. The Guitar Pro 7 writer does not store all of them,
/// so the save path reports these before it writes (`docs/gaps.md`).
pub fn settings_differences(before: &Song, after: &Song) -> Vec<Difference> {
    let mut diff = Diffs(Vec::new());
    diff.check(
        "song",
        "master mix",
        before.master_mix(),
        after.master_mix(),
    );
    diff.check(
        "song",
        "page setup",
        before.page_setup(),
        after.page_setup(),
    );
    for (a, b) in before.tracks().zip(after.tracks()) {
        let at = a.index().to_string();
        diff.check(
            &at,
            "bars per line",
            a.bars_per_system(),
            b.bars_per_system(),
        );
        for (old, new) in a.measures().zip(b.measures()) {
            diff.check(
                &format!("{at}, {}", old.index()),
                "line break",
                old.system_break(),
                new.system_break(),
            );
        }
        diff.check(&at, "color", a.color(), b.color());
        diff.check(&at, "staves", a.staves(), b.staves());
        diff.check(&at, "mute", a.is_muted(), b.is_muted());
        diff.check(&at, "solo", a.is_soloed(), b.is_soloed());
        let sound = |song: &Song, track| song.track_sound(track);
        let old = sound(before, a.index());
        let new = sound(after, b.index());
        diff.check(
            &at,
            "instrument program",
            old.map(|sound| sound.program),
            new.map(|sound| sound.program),
        );
        diff.check(
            &at,
            "volume",
            old.map(|sound| sound.volume),
            new.map(|sound| sound.volume),
        );
        diff.check(
            &at,
            "pan",
            old.map(|sound| sound.pan),
            new.map(|sound| sound.pan),
        );
        diff.check(
            &at,
            "reverb",
            old.map(|sound| sound.reverb),
            new.map(|sound| sound.reverb),
        );
        diff.check(
            &at,
            "chorus",
            old.map(|sound| sound.chorus),
            new.map(|sound| sound.chorus),
        );
    }
    diff.0
}
