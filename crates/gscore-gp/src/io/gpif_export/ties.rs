//! Where ties start. The model marks only the tied (second) note; Guitar Pro also needs the
//! note a tie starts from marked (`<Tie origin="true">`), or it strikes the tied note anew.

use std::collections::{HashMap, HashSet};

use super::*;

/// The notes a tie starts from, as (measure, voice, beat, string): the beat counted in the
/// voice's own beats, placeholders included.
pub(super) struct TieOrigins(HashSet<(usize, usize, usize, i8)>);

impl TieOrigins {
    /// Finds the origins on one staff. A tied note continues the last note on its string in
    /// the same voice, also across bar lines; a rest ends it, a grace note is passed over.
    pub(super) fn of(measures: &[Measure]) -> Self {
        let mut origins = HashSet::new();
        let voices = measures.iter().map(|m| m.voices.len()).max().unwrap_or(0);
        for v_idx in 0..voices {
            let mut last: HashMap<i8, (usize, usize)> = HashMap::new();
            for (m_idx, measure) in measures.iter().enumerate() {
                let Some(voice) = measure.voices.get(v_idx) else {
                    last.clear();
                    continue;
                };
                for (b_idx, beat) in voice.beats.iter().enumerate() {
                    match beat.status {
                        BeatStatus::Empty => continue,
                        BeatStatus::Rest => {
                            last.clear();
                            continue;
                        }
                        BeatStatus::Normal => {}
                    }
                    if beat.grace {
                        continue;
                    }
                    for note in &beat.notes {
                        if note.kind == NoteType::Tie
                            && let Some(&(m, b)) = last.get(&note.string)
                        {
                            origins.insert((m, v_idx, b, note.string));
                        }
                        last.insert(note.string, (m_idx, b_idx));
                    }
                }
            }
        }
        TieOrigins(origins)
    }

    /// Whether a tie starts from the note on `string` of the given beat.
    pub(super) fn starts(&self, measure: usize, voice: usize, beat: usize, string: i8) -> bool {
        self.0.contains(&(measure, voice, beat, string))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(string: i8, kind: NoteType) -> Note {
        Note {
            string,
            kind,
            ..Note::default()
        }
    }

    fn beat(notes: Vec<Note>) -> Beat {
        Beat {
            status: BeatStatus::Normal,
            notes,
            ..Beat::default()
        }
    }

    fn measure(beats: Vec<Beat>) -> Measure {
        Measure {
            voices: vec![Voice {
                beats,
                ..Voice::default()
            }],
            ..Measure::default()
        }
    }

    #[test]
    fn a_tie_across_two_bar_lines_starts_once_and_runs_through_the_middle() {
        let measures = vec![
            measure(vec![beat(vec![note(6, NoteType::Normal)])]),
            measure(vec![beat(vec![note(6, NoteType::Tie)])]),
            measure(vec![beat(vec![note(6, NoteType::Tie)])]),
        ];
        let origins = TieOrigins::of(&measures);
        assert!(origins.starts(0, 0, 0, 6));
        assert!(origins.starts(1, 0, 0, 6));
        assert!(!origins.starts(2, 0, 0, 6));
    }

    #[test]
    fn only_the_tied_string_of_a_chord_starts_a_tie() {
        let measures = vec![measure(vec![
            beat(vec![note(5, NoteType::Normal), note(6, NoteType::Normal)]),
            beat(vec![note(6, NoteType::Tie)]),
        ])];
        let origins = TieOrigins::of(&measures);
        assert!(origins.starts(0, 0, 0, 6));
        assert!(!origins.starts(0, 0, 0, 5));
    }

    #[test]
    fn a_rest_ends_what_a_tie_could_continue() {
        let mut rest = beat(Vec::new());
        rest.status = BeatStatus::Rest;
        let measures = vec![measure(vec![
            beat(vec![note(6, NoteType::Normal)]),
            rest,
            beat(vec![note(6, NoteType::Tie)]),
        ])];
        assert!(!TieOrigins::of(&measures).starts(0, 0, 0, 6));
    }

    /// A note tied over two bar lines, written as Guitar Pro writes it: origin on the first
    /// note, both on the middle one, destination on the last. Only the destinations used to
    /// be written, so Guitar Pro struck every tied note again.
    #[test]
    fn a_saved_tie_marks_both_ends_and_reads_back() {
        use super::super::SongGpifExportOps;
        let mut song = Song::default();
        song.tracks.push(Default::default());
        for (index, kind) in [NoteType::Normal, NoteType::Tie, NoteType::Tie]
            .into_iter()
            .enumerate()
        {
            song.measure_headers.push(MeasureHeader {
                number: u16::try_from(index + 1).unwrap(),
                ..Default::default()
            });
            let mut bar = measure(vec![beat(vec![note(6, kind)])]);
            bar.header_index = index;
            song.tracks[0].measures.push(bar);
        }

        let xml = song.write_gpif_xml().expect("written");
        let ties: Vec<&str> = xml
            .match_indices("<Tie ")
            .filter_map(|(at, _)| Some(&xml[at..at + xml[at..].find("/>")? + 2]))
            .collect();
        assert_eq!(
            ties,
            [
                "<Tie origin=\"true\" destination=\"false\" />",
                "<Tie origin=\"true\" destination=\"true\" />",
                "<Tie origin=\"false\" destination=\"true\" />",
            ]
        );

        let mut reread = Song::default();
        reread
            .read_gp(&song.write_gp().expect("written"))
            .expect("read back");
        let kinds: Vec<NoteType> = reread.tracks[0]
            .measures
            .iter()
            .map(|m| m.voices[0].beats[0].notes[0].kind.clone())
            .collect();
        assert_eq!(kinds, [NoteType::Normal, NoteType::Tie, NoteType::Tie]);
    }
}
