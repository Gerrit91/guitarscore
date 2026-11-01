//! The order in which measures are played: repeats and alternative endings unrolled.

use gscore_model::{MeasureIndex, Song};

/// Upper bound on played measures, so damaged repeat data cannot make the order huge.
const MAX_PLAYED_PER_MEASURE: usize = 64;

/// The measures of `song` in playing order. A repeat end jumps back to the last repeat
/// start (or the start of the piece, or just after the previous repeat section) as often as
/// it says; alternative endings are played only on their pass. Directions (D.C., D.S.,
/// coda) are not followed yet.
pub fn playback_order(song: &Song) -> Vec<MeasureIndex> {
    let count = song.measure_count();
    let limit = count.saturating_mul(MAX_PLAYED_PER_MEASURE);
    let mut order = Vec::with_capacity(count);
    // How often each repeat end has jumped back so far.
    let mut jumps = vec![0_u8; count];
    let mut section_start = 0;
    // Pass through the current repeat section, 1-based.
    let mut pass: u32 = 1;
    // After a section's last pass, its endings still use that pass number; the next measure
    // that is not an ending starts counting again.
    let mut reset_pending = false;
    let mut index = 0;

    while index < count && order.len() < limit {
        let measure = MeasureIndex::new(index);
        let repeat = song.repeat(measure).unwrap_or_default();
        if repeat.start && index != section_start {
            section_start = index;
            pass = 1;
            reset_pending = false;
        }
        if repeat.endings == 0 {
            if reset_pending {
                pass = 1;
                reset_pending = false;
            }
        } else {
            let on_this_pass = pass
                .checked_sub(1)
                .and_then(|bit| 1_u32.checked_shl(bit))
                .is_some_and(|mask| u32::from(repeat.endings) & mask != 0);
            if !on_this_pass {
                index = index.saturating_add(1);
                continue;
            }
        }
        order.push(measure);

        if let Some(repeats) = repeat.end_repeats
            && let Some(done) = jumps.get_mut(index)
        {
            if *done < repeats {
                *done = done.saturating_add(1);
                pass = pass.saturating_add(1);
                index = section_start;
                continue;
            }
            // This section is finished; the next one starts after it.
            *done = 0;
            section_start = index.saturating_add(1);
            reset_pending = true;
        }
        index = index.saturating_add(1);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use gscore_model::testing;

    /// A song of `count` empty measures with repeat data set through `edit`.
    fn order_of(count: usize, marks: &[(usize, bool, i8, u8)]) -> Vec<usize> {
        let song = testing::song_with_repeats(count, marks).unwrap();
        playback_order(&song).into_iter().map(|m| m.get()).collect()
    }

    #[test]
    fn without_repeats_every_measure_plays_once() {
        assert_eq!(order_of(3, &[]), [0, 1, 2]);
    }

    #[test]
    fn a_repeat_plays_its_section_again() {
        // |: 0 1 :| 2
        assert_eq!(
            order_of(3, &[(0, true, -1, 0), (1, false, 1, 0)]),
            [0, 1, 0, 1, 2]
        );
        // 0 |: 1 :|x3 2  (three repeats: four plays)
        assert_eq!(
            order_of(3, &[(1, true, -1, 0), (1, false, 3, 0)]),
            [0, 1, 1, 1, 1, 2]
        );
    }

    #[test]
    fn a_repeat_end_without_a_start_goes_back_to_the_beginning() {
        assert_eq!(order_of(3, &[(1, false, 1, 0)]), [0, 1, 0, 1, 2]);
    }

    #[test]
    fn alternative_endings_play_on_their_pass() {
        // |: 0 [1. 1 :| [2. 2 | 3
        let marks = [(0, true, -1, 0), (1, false, 1, 0b01), (2, false, -1, 0b10)];
        assert_eq!(order_of(4, &marks), [0, 1, 0, 2, 3]);
    }

    #[test]
    fn consecutive_sections_count_their_own_passes() {
        // |: 0 :| |: 1 [1. 2 :| [2. 3 |
        let marks = [
            (0, true, -1, 0),
            (0, false, 1, 0),
            (1, true, -1, 0),
            (2, false, 1, 0b01),
            (3, false, -1, 0b10),
        ];
        assert_eq!(order_of(4, &marks), [0, 0, 1, 2, 1, 3]);
    }
}
