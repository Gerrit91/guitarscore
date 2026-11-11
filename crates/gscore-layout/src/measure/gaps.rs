//! How close two neighbouring columns may stand: the notation staff and the tab are
//! checked apart, and on the tab string by string. A fast run that moves across the
//! strings packs tighter than one that stays on a string, as fret numbers only collide
//! with numbers on their own string (and must not overlap the neighbouring strings').

/// The room a column takes left and right of its center.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Extent {
    /// Notation left of the center (accidentals, grace notes).
    pub(super) notation_left: f32,
    /// Notation right of the center (noteheads, dots, flags).
    pub(super) notation_right: f32,
    /// What spans every string on the tab, left of the center (a rest, a brush arrow,
    /// grace notes).
    pub(super) tab_left: f32,
    /// What spans every string on the tab, right of the center (a rest, a dead slap).
    pub(super) tab_right: f32,
    /// The fret numbers: their line and half their width.
    pub(super) frets: Vec<(usize, f32)>,
}

impl Extent {
    fn widest_fret(&self) -> f32 {
        self.frets.iter().map(|&(_, half)| half).fold(0.0, f32::max)
    }
}

/// The least distance between the centers of column `a` and the column `b` after it,
/// keeping `gap` between what would otherwise touch.
pub(super) fn distance(a: &Extent, b: &Extent, gap: f32) -> f32 {
    let notation = a.notation_right + b.notation_left;
    // Whatever spans the strings meets every fret number of the other column.
    let mut tab = 0.0_f32;
    if a.tab_right > 0.0 {
        tab = tab.max(a.tab_right + b.tab_left.max(b.widest_fret()));
    }
    if b.tab_left > 0.0 {
        tab = tab.max(a.tab_right.max(a.widest_fret()) + b.tab_left);
    }
    for &(line, right) in &a.frets {
        for &(other, left) in &b.frets {
            let reach = right + left;
            if line == other {
                tab = tab.max(reach);
            } else if line.abs_diff(other) == 1 {
                // Neighbouring strings: side by side without the gap, never overlapping.
                tab = tab.max(reach - gap);
            }
        }
    }
    notation.max(tab) + gap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frets(frets: &[(usize, f32)]) -> Extent {
        Extent {
            frets: frets.to_vec(),
            ..Extent::default()
        }
    }

    #[test]
    fn numbers_on_the_same_string_keep_the_gap() {
        let a = frets(&[(2, 0.9)]);
        let b = frets(&[(2, 0.9)]);
        assert!((distance(&a, &b, 0.5) - 2.3).abs() < 1e-5);
    }

    #[test]
    fn numbers_on_other_strings_pack_tighter() {
        let a = frets(&[(2, 0.9)]);
        // The next string: touching, no gap.
        assert!((distance(&a, &frets(&[(3, 0.9)]), 0.5) - 1.8).abs() < 1e-5);
        // Two strings away: only the gap.
        assert!((distance(&a, &frets(&[(4, 0.9)]), 0.5) - 0.5).abs() < 1e-5);
        // A chord: its string nearest the other column's decides.
        let chord = frets(&[(0, 0.5), (2, 0.9)]);
        assert!((distance(&chord, &frets(&[(0, 0.9)]), 0.5) - 1.9).abs() < 1e-5);
    }

    #[test]
    fn rests_and_notation_meet_everything() {
        let rest = Extent {
            tab_left: 0.6,
            tab_right: 0.6,
            ..Extent::default()
        };
        let note = frets(&[(5, 0.9)]);
        assert!((distance(&rest, &note, 0.5) - 2.0).abs() < 1e-5);
        assert!((distance(&note, &rest, 0.5) - 2.0).abs() < 1e-5);
        let staff = Extent {
            notation_left: 0.4,
            notation_right: 1.6,
            ..frets(&[(0, 0.5)])
        };
        let other = Extent {
            notation_left: 1.2,
            ..frets(&[(4, 0.5)])
        };
        assert!((distance(&staff, &other, 0.5) - 3.3).abs() < 1e-5);
    }
}
