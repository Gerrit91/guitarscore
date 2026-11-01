//! Names of common tunings, recognized from the open-string pitches.
//!
//! Recognition works on the intervals between neighbouring strings, so every transposition
//! is named: "E standard", "B standard", "Drop D", "Drop A", "Open G", "DADGAD". Enharmonic
//! spelling uses sharps ("D# standard"), like [`Pitch`]'s display.

use crate::position::Pitch;

/// The tuning's name, or `None` if it is not a common one. `pitches` lists the strings from
/// the lowest up.
pub(crate) fn recognize(pitches: &[Pitch]) -> Option<String> {
    let intervals: Vec<u8> = pitches
        .windows(2)
        .map(|pair| match pair {
            [low, high] => high.get().checked_sub(low.get()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let lowest = pitches.first()?;
    let second = pitches.get(1)?;
    let root = lowest.note_name();

    if is_standard(&intervals) {
        return Some(format!("{root} standard"));
    }
    // Drop tuning: standard, but the lowest string a whole step lower (a fifth to the next).
    if let Some((7, rest)) = intervals.split_first() {
        let raised: Vec<u8> = std::iter::once(5).chain(rest.iter().copied()).collect();
        if is_standard(&raised) {
            return Some(format!("Drop {root}"));
        }
    }
    match intervals.as_slice() {
        [7, 5, 5, 4, 3] => Some(format!("Double drop {root}")),
        [7, 5, 4, 3, 5] | [7, 5, 7, 5, 4] => Some(format!("Open {root}")),
        [5, 7, 5, 4, 3] => Some(format!("Open {}", second.note_name())),
        // DADGAD and its transpositions are named by their notes.
        [7, 5, 5, 2, 5] => Some(pitches.iter().map(|p| p.note_name()).collect()),
        _ => None,
    }
}

/// Standard tuning of an instrument with at least four strings: all fourths (5 semitones,
/// basses), or, with six or more strings, all fourths except a major third (4) as the
/// second-highest interval (the guitar's G-B pair).
fn is_standard(intervals: &[u8]) -> bool {
    if intervals.len() < 3 {
        return false;
    }
    let third_at = intervals.len().checked_sub(2);
    let all_fourths = intervals.iter().all(|&i| i == 5);
    let guitar = intervals.len() >= 5
        && intervals
            .iter()
            .enumerate()
            .all(|(i, &interval)| interval == if Some(i) == third_at { 4 } else { 5 });
    all_fourths || guitar
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(pitches: &[u8]) -> Option<String> {
        let pitches: Vec<Pitch> = pitches.iter().map(|&p| Pitch::new(p).unwrap()).collect();
        recognize(&pitches)
    }

    #[test]
    fn recognizes_common_tunings() {
        let cases: [(&[u8], &str); 14] = [
            (&[40, 45, 50, 55, 59, 64], "E standard"),
            (&[38, 43, 48, 53, 57, 62], "D standard"),
            (&[39, 44, 49, 54, 58, 63], "D# standard"),
            (&[35, 40, 45, 50, 55, 59, 64], "B standard"), // 7-string guitar
            (&[30, 35, 40, 45, 50, 55, 59, 64], "F# standard"), // 8-string guitar
            (&[28, 33, 38, 43], "E standard"),             // 4-string bass
            (&[23, 28, 33, 38, 43], "B standard"),         // 5-string bass
            (&[38, 45, 50, 55, 59, 64], "Drop D"),
            (&[33, 40, 45, 50, 54, 59], "Drop A"), // owner's guitars
            (&[21, 28, 33, 38, 43], "Drop A"),     // owner's 5-string bass
            (&[38, 45, 50, 55, 59, 62], "Double drop D"),
            (&[38, 43, 50, 55, 59, 62], "Open G"),
            (&[38, 45, 50, 54, 57, 62], "Open D"),
            (&[38, 45, 50, 55, 57, 62], "DADGAD"),
        ];
        for (pitches, expected) in cases {
            assert_eq!(name(pitches).as_deref(), Some(expected), "{pitches:?}");
        }
        assert_eq!(name(&[36, 43, 48, 55, 60, 64]).as_deref(), Some("Open C"));
        assert_eq!(name(&[36, 43, 48, 53, 55, 60]).as_deref(), Some("CGCFGC"));
    }

    #[test]
    fn unknown_or_odd_tunings_have_no_name() {
        for pitches in [
            &[][..],
            &[40],
            &[40, 45, 50, 54, 59, 64], // third in the wrong place
            &[40, 44, 49, 54, 58, 63], // two thirds
            &[64, 59, 55, 50, 45, 40], // highest first (not low to high)
            &[0, 0, 0, 0, 0, 0],
        ] {
            assert_eq!(name(pitches), None, "{pitches:?}");
        }
    }
}
