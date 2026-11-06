//! Naming a chord from its notes ("Am", "G7", "C/E"), for a new chord diagram.

use crate::position::Pitch;

/// Note names, with the flats and sharps chord symbols usually use.
const NAMES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// Chord qualities by their intervals above the root (semitones, the root's 0 included),
/// the plain triads first so they win over a richer reading of the same notes.
const QUALITIES: [(&[u8], &str); 16] = [
    (&[0, 4, 7], ""),
    (&[0, 3, 7], "m"),
    (&[0, 7], "5"),
    (&[0, 4, 7, 10], "7"),
    (&[0, 4, 7, 11], "maj7"),
    (&[0, 3, 7, 10], "m7"),
    (&[0, 3, 7, 11], "mMaj7"),
    (&[0, 4, 7, 9], "6"),
    (&[0, 3, 7, 9], "m6"),
    (&[0, 2, 7], "sus2"),
    (&[0, 5, 7], "sus4"),
    (&[0, 5, 7, 10], "7sus4"),
    (&[0, 2, 4, 7], "add9"),
    (&[0, 3, 6], "dim"),
    (&[0, 3, 6, 10], "m7b5"),
    (&[0, 4, 8], "aug"),
];

/// The name of the chord `pitches` sound, or `None` when they are no chord this knows
/// (fewer than two different notes, or no known quality). The lowest note is the bass:
/// a chord over another bass note is written with a slash ("C/E").
pub fn chord_name(pitches: &[Pitch]) -> Option<String> {
    let bass = pitches.iter().map(|pitch| pitch.get()).min()? % 12;
    let mut classes: Vec<u8> = pitches.iter().map(|pitch| pitch.get() % 12).collect();
    classes.sort_unstable();
    classes.dedup();
    if classes.len() < 2 {
        return None;
    }
    // The bass as the root first, then the others: a root position reads before an
    // inversion of another chord.
    let roots = std::iter::once(bass).chain(classes.iter().copied().filter(|&c| c != bass));
    for root in roots {
        let mut intervals: Vec<u8> = classes
            .iter()
            .map(|&class| class.wrapping_add(12).wrapping_sub(root) % 12)
            .collect();
        intervals.sort_unstable();
        // A power chord is only one over its own root: a fourth is no chord.
        if let Some((_, quality)) = QUALITIES
            .iter()
            .filter(|(_, quality)| *quality != "5" || root == bass)
            .find(|(wanted, _)| *wanted == intervals.as_slice())
        {
            let note = |class: u8| NAMES.get(usize::from(class)).copied().unwrap_or("?");
            let mut name = format!("{}{quality}", note(root));
            if root != bass {
                name.push('/');
                name.push_str(note(bass));
            }
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn name(midi: &[u8]) -> Option<String> {
        let pitches: Vec<Pitch> = midi.iter().map(|&m| Pitch::new(m).unwrap()).collect();
        chord_name(&pitches)
    }

    #[test]
    fn common_guitar_chords_are_named() {
        // Open chords, lowest string first.
        assert_eq!(name(&[48, 52, 55, 60, 64]).as_deref(), Some("C"));
        assert_eq!(name(&[45, 52, 57, 60, 64]).as_deref(), Some("Am"));
        assert_eq!(name(&[43, 47, 50, 55, 59, 65]).as_deref(), Some("G7"));
        assert_eq!(name(&[50, 57, 62, 66]).as_deref(), Some("D"));
        assert_eq!(name(&[40, 47, 52]).as_deref(), Some("E5"));
        assert_eq!(name(&[50, 57, 62, 67]).as_deref(), Some("Dsus4"));
        // C over its third in the bass.
        assert_eq!(name(&[52, 55, 60, 64]).as_deref(), Some("C/E"));
        assert_eq!(name(&[46, 53, 58, 62]).as_deref(), Some("Bb"));
    }

    #[test]
    fn single_notes_and_clusters_have_no_name() {
        assert_eq!(name(&[]), None);
        assert_eq!(name(&[40]), None);
        assert_eq!(name(&[40, 52]), None, "an octave is one note");
        assert_eq!(name(&[40, 69]), None, "a fourth is no power chord");
        assert_eq!(name(&[60, 61, 62]), None);
    }
}
