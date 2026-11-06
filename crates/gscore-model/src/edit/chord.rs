//! Setting (or removing) the chord diagram shown at a beat.

use crate::error::ModelError;
use crate::position::BeatPosition;
use crate::song::Song;
use crate::techniques::ChordDiagram;

use super::{Change, beat_in};

impl Song {
    /// Sets the chord diagram shown at `at`; `None` removes it. Returns the change for undo.
    pub fn set_chord_diagram(
        &mut self,
        at: BeatPosition,
        diagram: Option<&ChordDiagram>,
    ) -> Result<Change, ModelError> {
        if let Some(diagram) = diagram {
            diagram.validate()?;
        }
        let previous = self
            .beat(at)
            .ok_or_else(|| ModelError::Invalid(format!("{at}: no such beat")))?
            .chord_diagram();
        let diagram = diagram.map(ChordDiagram::to_gp);
        self.edit_measure(at.track, at.measure, move |measure| {
            beat_in(measure, at)?.chord_diagram = diagram.clone();
            Ok(Change::chord_diagram(at, previous))
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::test_support::{self, GUITAR_6};
    use crate::{BeatIndex, BeatPosition, MeasureIndex, TrackIndex, VoiceIndex, interop::from_gp};

    use super::*;

    fn song() -> Song {
        from_gp(test_support::song(vec![test_support::track(
            "Guitar",
            GUITAR_6,
            vec![test_support::measure(
                (4, 4),
                vec![vec![
                    test_support::beat(4, &[(1, 3)]),
                    test_support::beat(4, &[]),
                    test_support::beat(4, &[]),
                    test_support::beat(4, &[]),
                ]],
            )],
        )]))
        .unwrap()
    }

    fn at(beat: usize) -> BeatPosition {
        BeatPosition {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(beat),
        }
    }

    #[test]
    fn a_chord_diagram_is_set_and_removed_with_undo() {
        let mut song = song();
        let mut chord = ChordDiagram::new("C");
        chord.frets = vec![(0, -1), (1, 3), (2, 2), (3, 0), (4, 1), (5, 0)];
        let change = song.set_chord_diagram(at(0), Some(&chord)).unwrap();
        assert_eq!(
            song.beat(at(0)).unwrap().chord_diagram(),
            Some(chord.clone())
        );
        song.revert(&change).unwrap();
        assert_eq!(song.beat(at(0)).unwrap().chord_diagram(), None);

        let _set = song.set_chord_diagram(at(0), Some(&chord)).unwrap();
        let _removed = song.set_chord_diagram(at(0), None).unwrap();
        assert_eq!(song.beat(at(0)).unwrap().chord_diagram(), None);
    }

    #[test]
    fn a_diagram_outside_its_grid_is_rejected() {
        let mut song = song();
        let mut chord = ChordDiagram::new("C");
        chord.frets = vec![(9, 3)];
        assert!(song.set_chord_diagram(at(0), Some(&chord)).is_err());
        chord.frets = vec![(0, 99)];
        assert!(song.set_chord_diagram(at(0), Some(&chord)).is_err());
        chord.string_count = 0;
        chord.frets.clear();
        assert!(song.set_chord_diagram(at(0), Some(&chord)).is_err());
    }

    #[test]
    fn a_gp7_file_keeps_the_chord_diagram() {
        let mut song = song();
        let mut chord = ChordDiagram::new("Am");
        chord.base_fret = 1;
        chord.frets = vec![(0, -1), (1, 0), (2, 2), (3, 2), (4, 2), (5, 0)];
        let _set = song.set_chord_diagram(at(0), Some(&chord)).unwrap();
        let bytes = song.inner().write_gp().unwrap();
        let mut reread = gscore_gp::Song::default();
        reread.read_gp(&bytes).unwrap();
        let after = from_gp(reread).unwrap();
        assert_eq!(after.beat(at(0)).unwrap().chord_diagram(), Some(chord));
    }
}
