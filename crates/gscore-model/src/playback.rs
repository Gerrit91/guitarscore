//! What playback needs from a song: each track's sound (MIDI program, volume, pan), the
//! sounding pitch and velocity of notes, tempo changes and repeats.

use crate::effects::HarmonicKind;
use crate::error::ModelError;
use crate::position::{Fret, MeasureIndex, Pitch, StringNumber, TrackIndex};
use crate::song::Song;
use crate::views::{BeatView, NoteView, TrackView};

/// Largest MIDI data value.
const MIDI_MAX: i64 = 127;
/// General MIDI program used when a track names none: Acoustic Guitar (steel), 0-based.
const DEFAULT_PROGRAM: u8 = 25;

/// How a track sounds on a MIDI synthesizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackSound {
    /// General MIDI program, 0-based (0 is Acoustic Grand Piano, 25 Steel Guitar).
    pub program: u8,
    /// Channel volume, 0-127.
    pub volume: u8,
    /// Pan, 0 (left) to 127 (right); 64 is the middle.
    pub pan: u8,
    /// Reverb send, 0-127 (MIDI controller 91).
    pub reverb: u8,
    /// Chorus send, 0-127 (MIDI controller 93).
    pub chorus: u8,
    /// Drums: notes are General MIDI drum sounds on the percussion channel.
    pub percussion: bool,
}

/// The master volume at which every track plays at its own volume: Guitar Pro's default
/// master fader position (0.76, as GP7 files store it untouched). Lower values turn all
/// tracks down, higher ones up (each track at most to the MIDI maximum).
pub const MASTER_UNITY: u8 = 97;

/// The master mix: a volume for all tracks together and a pan that shifts all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MasterMix {
    /// 0-127; [`MASTER_UNITY`] plays every track at its own volume.
    pub volume: u8,
    /// 0 (left) to 127 (right); 64 leaves every track's pan as it is.
    pub pan: u8,
}

impl Default for MasterMix {
    fn default() -> Self {
        MasterMix {
            volume: MASTER_UNITY,
            pan: 64,
        }
    }
}

impl MasterMix {
    /// How `sound` is played with this master mix: its volume scaled, its pan shifted.
    pub fn apply(self, sound: TrackSound) -> TrackSound {
        // Both factors are at most 127, so neither can overflow; saturating keeps clippy's
        // proof simple.
        let volume = u16::from(sound.volume)
            .saturating_mul(u16::from(self.volume))
            .checked_div(u16::from(MASTER_UNITY))
            .unwrap_or(0)
            .min(127);
        let pan = i16::from(sound.pan)
            .saturating_add(i16::from(self.pan))
            .saturating_sub(64)
            .clamp(0, 127);
        TrackSound {
            volume: u8::try_from(volume).unwrap_or(127),
            pan: u8::try_from(pan).unwrap_or(64),
            ..sound
        }
    }
}

/// A 0.0-1.0 fader position as a 0-127 value (the middle, 0.5, is 64).
pub(crate) fn unit_to_midi(value: f32) -> u8 {
    if (value - 0.5).abs() < f32::EPSILON {
        return 64;
    }
    // Clamped to 0..=127 first, so the conversion cannot fail.
    let midi = (value.clamp(0.0, 1.0) * 127.0).round();
    (0..=127_u8).find(|&v| f32::from(v) >= midi).unwrap_or(127)
}

/// A 0-127 value as a 0.0-1.0 fader position (64 is exactly the middle).
pub(crate) fn midi_to_unit(value: u8) -> f32 {
    if value == 64 {
        0.5
    } else {
        f32::from(value.min(127)) / 127.0
    }
}

/// Repeat signs and alternative endings of a measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Repeat {
    /// A repeat section starts at this measure.
    pub start: bool,
    /// A repeat section ends here; jump back this many times.
    pub end_repeats: Option<u8>,
    /// Alternative endings this measure belongs to, as a bit set: bit 0 is ending 1.
    /// 0 if it is not an alternative ending.
    pub endings: u8,
}

/// A value clamped to the MIDI data range 0-127.
fn midi_value(value: i64) -> u8 {
    u8::try_from(value.clamp(0, MIDI_MAX)).unwrap_or(0)
}

impl Song {
    /// How `track` sounds, or `None` if there is no such track. Values outside the MIDI
    /// range are clamped.
    pub fn track_sound(&self, track: TrackIndex) -> Option<TrackSound> {
        let inner = self.inner().tracks.get(track.get())?;
        let channel = self.inner().channels.get(inner.channel_index);
        // GP7 names the program on the track; GP3-5 in the song's channel table.
        let program = inner
            .midi_program_gpif
            .or_else(|| channel.map(|c| c.instrument))
            .map_or(DEFAULT_PROGRAM, |p| midi_value(i64::from(p)));
        Some(TrackSound {
            program,
            volume: channel.map_or(104, |c| midi_value(i64::from(c.volume))),
            pan: channel.map_or(64, |c| midi_value(i64::from(c.balance))),
            reverb: channel.map_or(0, |c| midi_value(i64::from(c.reverb))),
            chorus: channel.map_or(0, |c| midi_value(i64::from(c.chorus))),
            percussion: inner.percussion_track,
        })
    }

    /// The master mix; songs without one have the neutral [`MasterMix::default`].
    pub fn master_mix(&self) -> MasterMix {
        self.inner()
            .master_volume_pan
            .map_or_else(MasterMix::default, |(volume, pan)| MasterMix {
                volume: unit_to_midi(volume),
                pan: unit_to_midi(pan),
            })
    }

    /// How the track is played: its [`Song::track_sound`] with the master mix applied.
    pub fn played_sound(&self, track: TrackIndex) -> Option<TrackSound> {
        let master = self.master_mix();
        self.track_sound(track).map(|sound| master.apply(sound))
    }

    /// The repeat signs at `measure` (the same for every track), or `None` if there is no
    /// such measure.
    pub fn repeat(&self, measure: MeasureIndex) -> Option<Repeat> {
        let header = self.inner().measure_headers.get(measure.get())?;
        Some(Repeat {
            start: header.repeat_open,
            end_repeats: u8::try_from(header.repeat_close).ok().filter(|&n| n > 0),
            endings: header.repeat_alternative,
        })
    }

    /// Whether `measure` is in free time (Guitar Pro's "no metronome"), or `None` if
    /// there is no such measure.
    pub fn free_time(&self, measure: MeasureIndex) -> Option<bool> {
        self.inner()
            .measure_headers
            .get(measure.get())
            .map(|header| header.free_time)
    }

    /// Whether `measure` ends with a double barline (the same for every track), or `None`
    /// if there is no such measure.
    pub fn double_bar(&self, measure: MeasureIndex) -> Option<bool> {
        self.inner()
            .measure_headers
            .get(measure.get())
            .map(|header| header.double_bar)
    }

    /// The feel (triplet feel) `measure` starts with, or `None` if there is no such
    /// measure.
    pub fn feel(&self, measure: MeasureIndex) -> Option<crate::marks::Feel> {
        let header = self.inner().measure_headers.get(measure.get())?;
        Some(crate::marks::Feel::from_gp(&header.triplet_feel))
    }

    /// The fermata at `measure`, or `None` if it has none or there is no such measure.
    pub fn fermata(&self, measure: MeasureIndex) -> Option<crate::marks::Fermata> {
        let header = self.inner().measure_headers.get(measure.get())?;
        header
            .fermatas
            .first()
            .map(|(kind, _)| crate::marks::Fermata::from_gp(kind))
    }

    /// The navigation sign at `measure` (a coda, segno, fine or D.C./D.S. direction), or
    /// `None` if it has none or there is no such measure.
    pub fn direction(&self, measure: MeasureIndex) -> Option<crate::marks::Direction> {
        let header = self.inner().measure_headers.get(measure.get())?;
        header
            .direction
            .as_ref()
            .map(crate::marks::Direction::from_gp)
    }

    /// The tempo (beats per minute) set at the start of `measure` (GP6 and later store tempo
    /// changes per measure), if any.
    pub fn tempo_change(&self, measure: MeasureIndex) -> Option<u32> {
        let header = self.inner().measure_headers.get(measure.get())?;
        u32::try_from(header.tempo).ok().filter(|&tempo| tempo > 0)
    }
}

impl TrackView<'_> {
    /// The capo fret (0 without a capo).
    pub fn capo(&self) -> Result<u8, ModelError> {
        let offset = self.staff_data().offset;
        u8::try_from(offset)
            .ok()
            .filter(|&capo| i64::from(capo) <= MIDI_MAX)
            .ok_or_else(|| ModelError::Invalid(format!("capo at fret {offset}")))
    }

    /// How far the track sounds from what is written: Guitar Pro's track transpose, in
    /// semitones (positive is higher), 0 when it is not transposed or when the song's
    /// transposition is switched off ([`crate::Song::transpose_active`]). The written notes
    /// (their fret numbers and the score) never change with it; playback sounds this much
    /// above or below them. Drum kits are not transposed.
    pub fn transpose(&self) -> i32 {
        if self.transpose_active() {
            self.transposition()
        } else {
            0
        }
    }

    /// The pitch `note` sounds at: [`Self::written_pitch`] moved by [`Self::transpose`].
    pub fn sounding_pitch(&self, note: &NoteView<'_>) -> Result<Pitch, ModelError> {
        self.transposed(self.written_pitch(note)?)
    }

    /// The pitch `note` is written at (what the score and the tab show): the open string's
    /// pitch plus fret and capo, or the harmonic's pitch for natural, artificial, tapped and
    /// pinch harmonics. On drum tracks the "fret" is the General MIDI drum sound itself.
    pub fn written_pitch(&self, note: &NoteView<'_>) -> Result<Pitch, ModelError> {
        // Guitar Pro writes a voice or keyboard note below its stand-in string as a
        // negative fret: the pitch is still the open string's plus the fret.
        if note.inner.value < 0 && !self.is_percussion() {
            let open = self.written_pitch_at(note.string()?, Fret::new(0)?)?;
            let pitch = i32::from(open.get()).saturating_add(i32::from(note.inner.value));
            let pitch = u8::try_from(pitch)
                .map_err(|_| ModelError::Invalid(format!("pitch {pitch} is below MIDI's range")))?;
            return Pitch::new(pitch);
        }
        let fret = note.fret()?.get();
        if self.is_percussion() {
            return Pitch::new(fret);
        }
        let string = note.string()?;
        let root = u16::from(self.written_pitch_at(string, Fret::new(0)?)?.get());
        let fretted = root.saturating_add(u16::from(fret));
        let pitch = match note.harmonic() {
            // A natural harmonic is touched at the written fret over the open string.
            Some(h) if h.kind == HarmonicKind::Natural => {
                harmonic_interval(fret).map_or(fretted, |interval| root.saturating_add(interval))
            }
            // Artificial harmonics divide the string from the fretted note to the node.
            Some(h)
                if matches!(
                    h.kind,
                    HarmonicKind::Artificial | HarmonicKind::Tapped | HarmonicKind::Pinch
                ) =>
            {
                h.node
                    .and_then(|node| node.checked_sub(fret))
                    .and_then(harmonic_interval)
                    .map_or(fretted, |interval| fretted.saturating_add(interval))
            }
            _ => fretted,
        };
        let pitch = u8::try_from(pitch)
            .map_err(|_| ModelError::Invalid(format!("pitch {pitch} is above MIDI's range")))?;
        Pitch::new(pitch)
    }

    /// The pitch `fret` on `string` sounds at: [`Self::written_pitch_at`] moved by
    /// [`Self::transpose`].
    pub fn sounding_pitch_at(&self, string: StringNumber, fret: Fret) -> Result<Pitch, ModelError> {
        self.transposed(self.written_pitch_at(string, fret)?)
    }

    /// Moves a written pitch by the track's transposition, kept inside MIDI's range.
    fn transposed(&self, written: Pitch) -> Result<Pitch, ModelError> {
        let shift = self.transpose();
        if shift == 0 {
            return Ok(written);
        }
        let pitch = i32::from(written.get()).saturating_add(shift);
        let pitch = u8::try_from(pitch)
            .map_err(|_| ModelError::Invalid(format!("pitch {pitch} is outside MIDI's range")))?;
        Pitch::new(pitch)
    }

    /// The pitch of `fret` on `string` as written, without any harmonic (grace notes read
    /// their pitch this way) and without the track's transposition. On drum tracks the
    /// "fret" is the drum sound itself.
    pub fn written_pitch_at(&self, string: StringNumber, fret: Fret) -> Result<Pitch, ModelError> {
        if self.is_percussion() {
            return Pitch::new(fret.get());
        }
        let open = self
            .strings()?
            .into_iter()
            .find(|s| s.string == string)
            .ok_or_else(|| ModelError::Invalid(format!("the track has no {string}")))?;
        let root = u16::from(open.pitch.get()).saturating_add(u16::from(self.capo()?));
        let pitch = u8::try_from(root.saturating_add(u16::from(fret.get())))
            .map_err(|_| ModelError::Invalid("pitch is above MIDI's range".to_owned()))?;
        Pitch::new(pitch)
    }
}

/// Highest harmonic considered (the 8th sounds three octaves up).
const MAX_HARMONIC: u32 = 8;
/// How close (as a share of the string) a fret must be to a harmonic node.
const NODE_TOLERANCE: f64 = 0.05;

/// The interval in semitones of the harmonic touched `frets` frets above the string's
/// root, if a harmonic node lies there. Fret 12 is the 2nd harmonic (an octave), 7 and
/// 19 the 3rd (+19), 5 and 24 the 4th (+24), 4, 9 and 16 the 5th (+28), ...
pub(crate) fn harmonic_interval(frets: u8) -> Option<u16> {
    // Where the fret divides the string, measured from the nut (0 nut, 1 bridge).
    let position = 1.0 - 2_f64.powf(-f64::from(frets) / 12.0);
    (2..=MAX_HARMONIC).find_map(|harmonic| {
        let multiple = position * f64::from(harmonic);
        let node = multiple.round();
        let is_node =
            node >= 1.0 && node < f64::from(harmonic) && (multiple - node).abs() < NODE_TOLERANCE;
        // 12 * log2(harmonic), rounded: 12, 19, 24, 28, 31, 34, 36.
        is_node.then(|| (12.0 * f64::from(harmonic).log2()).round() as u16)
    })
}

impl NoteView<'_> {
    /// How hard the note is played, as a MIDI velocity 1-127 (from the dynamics).
    pub fn velocity(&self) -> u8 {
        midi_value(i64::from(self.inner.velocity)).max(1)
    }
}

impl BeatView<'_> {
    /// The tempo (beats per minute) a mix table on this beat sets (GP3-5 store tempo
    /// changes this way), if any.
    pub fn tempo_change(&self) -> Option<u32> {
        self.inner
            .effect
            .mix_table_change
            .as_ref()
            .and_then(|mix| mix.tempo.as_ref())
            .map(|tempo| u32::from(tempo.value))
            .filter(|&tempo| tempo > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interop::from_gp;
    use crate::test_support::{GUITAR_6, beat, measure, song as test_song, track};
    use crate::{BeatIndex, BeatPosition, VoiceIndex};

    fn first_note_pitch(song: &Song) -> Pitch {
        let track = song.track(TrackIndex::new(0)).unwrap();
        let beat = song
            .beat(BeatPosition {
                staff: crate::StaffIndex::new(0),
                track: TrackIndex::new(0),
                measure: MeasureIndex::new(0),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(0),
            })
            .unwrap();
        let note = beat.notes().next().unwrap();
        track.sounding_pitch(&note).unwrap()
    }

    #[test]
    fn pitches_add_fret_and_capo_to_the_open_string() {
        // String 6 of a guitar is E2 (40); fret 3 is G2 (43).
        let mut gp = test_song(vec![track(
            "Guitar",
            GUITAR_6,
            vec![measure((4, 4), vec![vec![beat(1, &[(6, 3)])]])],
        )]);
        assert_eq!(first_note_pitch(&from_gp(gp.clone()).unwrap()).get(), 43);
        gp.tracks[0].offset = 2;
        assert_eq!(first_note_pitch(&from_gp(gp.clone()).unwrap()).get(), 45);
        gp.tracks[0].offset = -1;
        let song = from_gp(gp).unwrap();
        assert!(song.track(TrackIndex::new(0)).unwrap().capo().is_err());
    }

    #[test]
    fn a_voice_note_below_its_stand_in_string_has_a_negative_fret() {
        // Guitar Pro writes a voice's A1 (33) as fret -7 on the stand-in low E (40).
        let mut gp = test_song(vec![track(
            "Vocals",
            GUITAR_6,
            vec![measure((4, 4), vec![vec![beat(1, &[(6, -7)])]])],
        )]);
        gp.tracks[0].instrument_type = Some("voice".to_owned());
        let song = from_gp(gp).unwrap();
        assert_eq!(first_note_pitch(&song).get(), 33);
        let view = song.track(TrackIndex::new(0)).unwrap();
        // Not a string instrument, so no tuning is shown, but the notes have pitches.
        assert!(view.tuning().unwrap().is_none());
        assert!(view.note_tuning().unwrap().is_some());
    }

    #[test]
    fn harmonic_nodes_give_their_intervals() {
        let cases = [
            (12, 12),
            (7, 19),
            (19, 19),
            (5, 24),
            (24, 24),
            (4, 28),
            (9, 28),
        ];
        for (fret, interval) in cases {
            assert_eq!(harmonic_interval(fret), Some(interval), "fret {fret}");
        }
        assert_eq!(harmonic_interval(16), Some(28));
        assert_eq!(harmonic_interval(3), Some(31));
        assert_eq!(harmonic_interval(0), None, "no node at the nut");
        assert_eq!(harmonic_interval(1), None);
    }

    #[test]
    fn drum_notes_are_the_drum_sound_itself() {
        let mut gp = test_song(vec![track(
            "Drums",
            GUITAR_6,
            vec![measure((4, 4), vec![vec![beat(1, &[(1, 38)])]])],
        )]);
        gp.tracks[0].percussion_track = true;
        let song = from_gp(gp).unwrap();
        assert_eq!(first_note_pitch(&song).get(), 38);
        assert!(song.track_sound(TrackIndex::new(0)).unwrap().percussion);
    }

    #[test]
    fn track_sound_prefers_the_gp7_program_and_clamps_values() {
        let mut gp = test_song(vec![track(
            "Guitar",
            GUITAR_6,
            vec![measure((4, 4), vec![])],
        )]);
        gp.channels = vec![gscore_gp::MidiChannel {
            instrument: 30,
            volume: -5,
            balance: 100,
            ..gscore_gp::MidiChannel::default()
        }];
        gp.tracks[0].channel_index = 0;
        let song = from_gp(gp.clone()).unwrap();
        let sound = song.track_sound(TrackIndex::new(0)).unwrap();
        assert_eq!((sound.program, sound.volume, sound.pan), (30, 0, 100));
        gp.tracks[0].midi_program_gpif = Some(29);
        let song = from_gp(gp).unwrap();
        assert_eq!(song.track_sound(TrackIndex::new(0)).unwrap().program, 29);
        assert_eq!(song.track_sound(TrackIndex::new(1)), None);
    }

    #[test]
    fn repeats_and_tempo_changes_come_from_the_measure_headers() {
        let mut gp = test_song(vec![track(
            "Guitar",
            GUITAR_6,
            vec![measure((4, 4), vec![]), measure((4, 4), vec![])],
        )]);
        gp.measure_headers[0].repeat_open = true;
        gp.measure_headers[1].repeat_close = 2;
        gp.measure_headers[1].repeat_alternative = 0b10;
        gp.measure_headers[1].tempo = 140;
        gp.measure_headers[1].direction = Some(gscore_gp::DirectionSign::DaCapoAlCoda);
        let song = from_gp(gp).unwrap();
        assert_eq!(
            song.repeat(MeasureIndex::new(0)).unwrap(),
            Repeat {
                start: true,
                end_repeats: None,
                endings: 0
            }
        );
        let second = song.repeat(MeasureIndex::new(1)).unwrap();
        assert_eq!((second.end_repeats, second.endings), (Some(2), 0b10));
        assert_eq!(song.tempo_change(MeasureIndex::new(0)), None);
        assert_eq!(song.tempo_change(MeasureIndex::new(1)), Some(140));
        assert_eq!(song.repeat(MeasureIndex::new(2)), None);
        assert_eq!(
            song.direction(MeasureIndex::new(1)),
            Some(crate::marks::Direction::DaCapoAlCoda)
        );
        assert_eq!(song.direction(MeasureIndex::new(0)), None);
    }
}
