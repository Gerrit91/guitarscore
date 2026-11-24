//! Guitar Pro song data: track.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

/// Settings of the track.
#[derive(Debug, Clone)]
pub struct TrackSettings {
    pub tablature: bool,
    pub notation: bool,
    /// GuitarScore addition: GP7's slash-notation flag (`PartConfiguration`, bit 2).
    pub slash: bool,
    pub diagram_are_below: bool,
    pub show_rhythm: bool,
    pub force_horizontal: bool,
    pub force_channels: bool,
    pub diagram_list: bool,
    pub diagram_in_score: bool,
    pub auto_let_ring: bool,
    pub auto_brush: bool,
    pub extend_rhythmic: bool,
}

impl Default for TrackSettings {
    fn default() -> Self {
        TrackSettings {
            tablature: true,
            notation: true,
            slash: false,
            diagram_are_below: false,
            show_rhythm: false,
            force_horizontal: false,
            force_channels: false,
            diagram_list: true,
            diagram_in_score: false,
            auto_let_ring: false,
            auto_brush: false,
            extend_rhythmic: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub number: i32,
    pub offset: i32,
    pub channel_index: usize, //pub channel_id: i32,
    pub solo: bool,
    pub mute: bool,
    /// Shown when all tracks are shown together (GP5's track flag, GP7's
    /// `LayoutConfiguration`: the eye in Guitar Pro's track list).
    pub visible: bool,
    /// GuitarScore addition: the track's second byte in GP7's `LayoutConfiguration`, of
    /// unknown meaning (0xff in every file seen); kept so saving writes it back.
    pub layout_second: u8,
    /// GuitarScore addition: what tells this track apart from the others while the song is
    /// open (GuitarScore's score views refer to tracks by it); never saved, 0 until the
    /// model gives the track one.
    pub session_key: u64,
    pub name: String,
    pub short_name: String,
    /// A guitar string with a special tuning.
    pub strings: Vec<(i8, i8)>,
    pub color: i32,
    pub percussion_track: bool,
    pub twelve_stringed_guitar_track: bool,
    pub banjo_track: bool,
    pub port: u8,
    pub fret_count: u8,
    /// GuitarScore addition: the parts of this track's first staff that GuitarScore does
    /// not model (see [`StaffKept`]), kept so a save writes them back.
    pub staff_kept: StaffKept,
    pub indicate_tuning: bool,
    pub use_rse: bool,
    pub rse: TrackRse,
    pub measures: Vec<Measure>,
    pub settings: TrackSettings,
    /// MIDI program from GPIF (GP6/GP7)
    pub midi_program_gpif: Option<i32>,
    /// Chromatic transposition (GP6/GP7)
    pub transpose_chromatic: i32,
    /// Octave transposition (GP6/GP7)
    pub transpose_octave: i32,
    /// GuitarScore addition: what the track sounds like relative to what is written, in
    /// semitones (positive is higher); 0 when it is not transposed. It changes playback
    /// only: the fret numbers, the tablature and the notation stay as they are. Guitar Pro
    /// has no field for it, so it is kept in the song's notes (`sound_state` in the app).
    pub transpose_sound: i32,
    /// GuitarScore addition: false if the file stores no tuning for this track (e.g. GP7
    /// vocal or piano tracks). `strings` then holds placeholder E-standard strings, which
    /// are not written back.
    pub has_tuning: bool,
    /// GuitarScore addition: GP7 instrument type id (`electricGuitar`, `drumKit`, `voice`,
    /// ...), `None` for formats that do not store one. Not written back yet.
    pub instrument_type: Option<String>,
    /// GuitarScore addition: GP7 instrument display name ("Electric Guitar"), if stored.
    pub instrument_name: Option<String>,
    /// GuitarScore addition: user-given tuning name from the file (GP6/GP7), if set.
    /// Not written back yet.
    pub tuning_label: Option<String>,
    /// GuitarScore addition: at most this many bars on one line of the page; `None` lays
    /// lines out by width alone. GP7 files keep it in a GuitarScore attribute of the track
    /// (`gsBarsPerSystem`).
    pub bars_per_system: Option<u8>,
    /// GuitarScore addition: the symbol the app draws for the track (`electricGuitar`,
    /// `drums`, `piano`, ...), independent of the instrument's sound. GP7 files keep it as
    /// a track property (`GuitarScoreSymbol`); older formats drop it.
    pub symbol: Option<String>,
    /// GuitarScore addition: the track's GP7 `<Sounds>` element exactly as the file had
    /// it (the RSE sound: its path, bank patch, pickups and effect chain), written back
    /// unchanged. `None` for formats without one and for new tracks.
    pub gpif_sounds: Option<String>,
    /// GuitarScore addition: the track's GP7 `<AudioEngineState>` element exactly as the
    /// file had it (`RSE` or `MIDI`), written back unchanged.
    pub gpif_audio_engine: Option<String>,
    /// GuitarScore addition: Guitar Pro's per-track playback settings (palm-mute and
    /// auto-accentuation strength, auto-brush, playing style, one channel per string, the
    /// track icon, a forced sound), each element exactly as the file wrote it. GuitarScore
    /// does not use them; kept so a save does not lose them.
    pub gpif_playback: Vec<String>,
    /// GuitarScore addition: the track's `<SystemsLayout>` (bars per line) exactly as the
    /// file wrote it, and its `<SystemsDefautLayout>`, so an empty or unchanged layout is
    /// written back as it was; the model's line breaks are written when they changed it.
    pub gpif_systems_layout: Option<String>,
    pub gpif_systems_default: Option<String>,
    /// GuitarScore addition: the track's `<RSE><ChannelStrip><ByPassedAutomations>` text
    /// (the automation types Guitar Pro has switched off), kept so a save does not lose it.
    pub gpif_bypassed_automations: Option<String>,
    /// GuitarScore addition: a GP6/GP7 track's typed lyric lines when it is not the song's
    /// lyric track (backing vocals): `(line, start bar from 1, text)`, as
    /// [`crate::model::Lyrics::lines`]; written back on the track.
    pub gpif_lyrics: Vec<(u8, u16, String)>,
    /// GuitarScore addition: the track's own sound settings as an opaque JSON payload
    /// (`gscore-app`'s `TrackSound`), which the Guitar Pro format has no field for. It is
    /// never written to a Guitar Pro file; the app saves it in the song's notes block.
    /// Living on the track means every track edit (insert, delete, duplicate, move) and
    /// its undo carry it along like the name or the color.
    pub sound_state: Option<String>,
    /// GuitarScore addition: the track's staves after the first, as GP6 and GP7 store them
    /// (a piano's or synth's bass staff). The first staff is the track's own `measures`,
    /// `strings`, `offset` and tuning fields, as in GP3-5, which have one staff only; each
    /// further staff has the same number of measures. Until 2026-11-13 they were not read,
    /// and saving dropped them.
    pub staves: Vec<Staff>,
}

/// GuitarScore addition: one staff of a track as the passes over every staff need it.
#[derive(Debug)]
pub struct StaffMut<'a> {
    /// The staff's strings, highest first.
    pub strings: &'a [(i8, i8)],
    /// The capo fret.
    pub offset: i32,
    pub measures: &'a mut Vec<Measure>,
}

impl Track {
    /// GuitarScore addition: every staff's measures, the first staff's first.
    pub fn all_measures(&self) -> impl Iterator<Item = &Measure> {
        self.measures
            .iter()
            .chain(self.staves.iter().flat_map(|staff| staff.measures.iter()))
    }

    /// GuitarScore addition: every staff's measures, to change.
    pub fn all_measures_mut(&mut self) -> impl Iterator<Item = &mut Measure> {
        self.measures.iter_mut().chain(
            self.staves
                .iter_mut()
                .flat_map(|staff| staff.measures.iter_mut()),
        )
    }

    /// GuitarScore addition: each staff's measures, the first staff's first.
    pub fn measure_lists(&self) -> impl Iterator<Item = &Vec<Measure>> {
        std::iter::once(&self.measures).chain(self.staves.iter().map(|staff| &staff.measures))
    }

    /// GuitarScore addition: each staff's measures, to change (insert or remove a measure
    /// in every staff alike).
    pub fn measure_lists_mut(&mut self) -> impl Iterator<Item = &mut Vec<Measure>> {
        std::iter::once(&mut self.measures)
            .chain(self.staves.iter_mut().map(|staff| &mut staff.measures))
    }

    /// GuitarScore addition: every staff with its strings and capo, the first staff's
    /// first.
    pub fn staves_mut(&mut self) -> Vec<StaffMut<'_>> {
        let mut staves = vec![StaffMut {
            strings: &self.strings,
            offset: self.offset,
            measures: &mut self.measures,
        }];
        for staff in &mut self.staves {
            staves.push(StaffMut {
                strings: &staff.strings,
                offset: staff.offset,
                measures: &mut staff.measures,
            });
        }
        staves
    }
}

/// GuitarScore addition: a staff after a track's first (see [`Track::staves`]).
#[derive(Debug, Clone)]
pub struct Staff {
    /// One per measure header, as the track's own.
    pub measures: Vec<Measure>,
    /// The staff's strings (number, open pitch), highest first: its notes are a string and
    /// a fret on them (for a keyboard's staff, a stand-in tuning that gives the pitches).
    pub strings: Vec<(i8, i8)>,
    /// Whether the file stored a tuning for the staff (see [`Track::has_tuning`]).
    pub has_tuning: bool,
    /// The tuning's name, if the file gave one.
    pub tuning_label: Option<String>,
    /// The capo fret.
    pub offset: i32,
    /// GuitarScore addition: the number of frets (GP7 `FretCount`).
    pub fret_count: u8,
    /// GuitarScore addition: the parts of the staff's properties GuitarScore does not
    /// model, kept so a save writes them back (see [`StaffKept`]).
    pub kept: StaffKept,
}

/// GuitarScore addition: the parts of a GP7 staff's `<Properties>` that GuitarScore does
/// not model, kept so a save writes them back: a partial capo, a tuning shown with flats,
/// the tuning's name, instrument and label visibility, and the track's whole chord and
/// diagram libraries (whole `<Property>` elements, verbatim).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaffKept {
    /// `PartialCapoFret`'s `<Fret>`, if the file has one.
    pub partial_capo_fret: Option<i32>,
    /// `PartialCapoStringFlags`'s `<Bitset>` text.
    pub partial_capo_flags: Option<String>,
    /// Whether the staff has a `<TuningFlat><Enable/></TuningFlat>`.
    pub tuning_flat: bool,
    /// Whether the tuning has a `<Flat></Flat>` child (a tuning shown with flats).
    pub tuning_flat_child: bool,
    /// The tuning's `<Instrument>` ("Guitar", "Bass").
    pub tuning_instrument: Option<String>,
    /// The tuning's `<LabelVisible>` text.
    pub label_visible: Option<String>,
    /// The staff's `<Name>` (the tuning's name, "Standard").
    pub name: Option<String>,
    /// The staff's chord and diagram collections (`ChordCollection`,
    /// `ChordWorkingSet`, `DiagramCollection`, `DiagramWorkingSet`), each whole
    /// `<Property>` element exactly as written.
    pub chord_libraries: Vec<String>,
}

impl Default for Staff {
    fn default() -> Self {
        Staff {
            measures: Vec::new(),
            strings: vec![(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40)],
            has_tuning: false,
            tuning_label: None,
            offset: 0,
            fret_count: 24,
            kept: StaffKept::default(),
        }
    }
}

impl Default for Track {
    fn default() -> Self {
        Track {
            number: 1,
            offset: 0,
            channel_index: 0, //channel_id: 25,
            solo: false,
            mute: false,
            visible: true,
            layout_second: 0xff,
            session_key: 0,
            name: String::from("Track 1"),
            short_name: String::new(),
            strings: vec![(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40)],
            banjo_track: false,
            twelve_stringed_guitar_track: false,
            percussion_track: false,
            fret_count: 24,
            staff_kept: StaffKept::default(),
            color: 0xff0000,
            port: 1,
            indicate_tuning: false,
            use_rse: false,
            rse: TrackRse::default(),
            measures: Vec::new(),
            settings: TrackSettings::default(),
            midi_program_gpif: None,
            transpose_chromatic: 0,
            transpose_octave: 0,
            transpose_sound: 0,
            has_tuning: true,
            instrument_type: None,
            instrument_name: None,
            tuning_label: None,
            bars_per_system: None,
            symbol: None,
            gpif_sounds: None,
            gpif_audio_engine: None,
            gpif_playback: Vec::new(),
            gpif_systems_layout: None,
            gpif_systems_default: None,
            gpif_bypassed_automations: None,
            gpif_lyrics: Vec::new(),
            sound_state: None,
            staves: Vec::new(),
        }
    }
}
