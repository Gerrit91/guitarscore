//! The order in which measures are played: repeats, alternative endings and navigation
//! signs (D.C., D.S., coda, fine) unrolled.

use gscore_model::{Direction, MeasureIndex, Repeat, Song};

/// Upper bound on played measures, so damaged repeat or direction data cannot make the
/// order huge.
const MAX_PLAYED_PER_MEASURE: usize = 64;

/// The measures of `song` in playing order. Repeats and alternative endings are played
/// first; then D.C./D.S. directions jump back to the start or the segno, and to the coda,
/// and Fine stops the piece.
pub fn playback_order(song: &Song) -> Vec<MeasureIndex> {
    let repeated = repeat_order(song);
    direction_order(song, &repeated)
}

/// The repeat groups of a song, and the alternative endings unrolled over their passes.
///
/// Guitar Pro marks only the first measure of an alternative ending; the measures after it
/// belong to the same pass ("the alternate endings are implicitly extended", Guitar Pro's
/// manual). An ending therefore runs from its measure through the last repeat close of its
/// repeat group, so a group `|: … [1. A B :| [2. C` plays A and B on the first pass only.
/// The measures after the group's last close (C) are played on every pass: an earlier pass
/// already jumped back at the close, so only the final pass reaches them.
struct Repeats {
    /// Every measure's repeat signs, in order.
    marks: Vec<Repeat>,
    /// The passes every measure is played on, as a bit set (bit 0 is pass 1), or 0 for every
    /// pass. The measure carrying an ending extends it over the following measures.
    endings: Vec<u8>,
    /// Whether a repeat close is the last one of its repeat group.
    last_closing: Vec<bool>,
    /// Whether a measure's repeat group has alternative endings; its repeat closes then
    /// share one pass counter.
    shared_pass: Vec<bool>,
}

impl Repeats {
    fn new(song: &Song) -> Repeats {
        let count = song.measure_count();
        let marks: Vec<Repeat> = (0..count)
            .map(|index| song.repeat(MeasureIndex::new(index)).unwrap_or_default())
            .collect();

        // The repeat groups: from a repeat open (or the previous close) through the last
        // close, with the pass counter shared when the group has alternative endings.
        // Measures outside a group are free to play on every pass.
        let mut last_closing = vec![false; count];
        let mut group: Vec<Option<usize>> = vec![None; count];
        let mut shared_pass = vec![false; count];
        for repeats in song.repeat_groups() {
            let (start, end) = (repeats.start.get(), repeats.end.get());
            let has_endings = marks
                .get(start..=end)
                .unwrap_or_default()
                .iter()
                .any(|mark| mark.endings != 0);
            if let Some(slots) = group.get_mut(start..=end) {
                slots.fill(Some(end));
            }
            if let Some(slots) = shared_pass.get_mut(start..=end) {
                slots.fill(has_endings);
            }
            if let Some(slot) = last_closing.get_mut(end) {
                *slot = true;
            }
        }

        // The endings every measure is played on: the implicit extension of the last
        // marking, within its repeat group.
        let mut endings = vec![0_u8; count];
        let mut current = 0_u8;
        let mut active_group = None;
        for index in 0..count {
            let end = group.get(index).copied().flatten();
            if end != active_group {
                current = 0;
                active_group = end;
            }
            let Some(mark) = marks.get(index) else {
                continue;
            };
            if end.is_none() {
                continue;
            }
            if mark.endings != 0 {
                current = mark.endings;
            }
            if let Some(slot) = endings.get_mut(index) {
                *slot = current;
            }
        }

        Repeats {
            marks,
            endings,
            last_closing,
            shared_pass,
        }
    }

    /// The passes `measure` is played on (bit 0 is pass 1), or 0 for every pass.
    fn endings(&self, measure: usize) -> u8 {
        self.endings.get(measure).copied().unwrap_or(0)
    }

    /// Whether a repeat close is the last one of its repeat group.
    fn is_last_closing(&self, measure: usize) -> bool {
        self.last_closing.get(measure).copied().unwrap_or(false)
    }

    /// Whether a repeat group's closes share one pass counter (it has alternative
    /// endings).
    fn has_shared_pass(&self, measure: usize) -> bool {
        self.shared_pass.get(measure).copied().unwrap_or(false)
    }
}

/// The measures of `song` with only repeats and alternative endings unrolled.
fn repeat_order(song: &Song) -> Vec<MeasureIndex> {
    let count = song.measure_count();
    let limit = count.saturating_mul(MAX_PLAYED_PER_MEASURE);
    let repeats = Repeats::new(song);
    let mut order = Vec::with_capacity(count);
    // How often each repeat close has jumped back so far (without alternative endings).
    let mut jumps = vec![0_u8; count];
    let mut section_start = 0;
    // Pass through the current repeat section, 1-based.
    let mut pass: u32 = 1;
    // After a section's last pass its endings still use that pass number; the next measure
    // that is played on every pass starts counting again.
    let mut reset_pending = false;
    let mut index = 0;

    while index < count && order.len() < limit {
        let Some(&repeat) = repeats.marks.get(index) else {
            break;
        };
        if repeat.start && index != section_start {
            section_start = index;
            pass = 1;
            reset_pending = false;
        }
        let endings = repeats.endings(index);
        let on_this_pass = endings == 0
            || pass
                .checked_sub(1)
                .and_then(|bit| 1_u32.checked_shl(bit))
                .is_some_and(|mask| u32::from(endings) & mask != 0);
        if on_this_pass {
            if reset_pending && endings == 0 {
                pass = 1;
                reset_pending = false;
            }
            order.push(MeasureIndex::new(index));
        }

        // A repeat close in a measure skipped for its ending is passed over, except the
        // group's last close: Guitar Pro's endings often close a group only there, and its
        // passes must still be counted.
        if let Some(close) = repeat
            .end_repeats
            .filter(|_| on_this_pass || repeats.is_last_closing(index))
            && let Some(done) = jumps.get_mut(index)
        {
            let more = if repeats.has_shared_pass(index) {
                pass <= u32::from(close)
            } else {
                *done < close
            };
            if more {
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

/// The first position in `order` holding a measure with `direction`.
fn target(order: &[MeasureIndex], song: &Song, direction: Direction) -> Option<usize> {
    order
        .iter()
        .position(|&measure| song.has_direction(measure, direction))
}

/// Which navigation signs have been seen, so a jump happens once.
#[derive(Debug, Default)]
struct Navigation {
    /// A D.C. or D.S. has jumped back already.
    returned: bool,
    /// A "to coda" jump has happened.
    to_coda: bool,
    /// The return asks for the (first) coda.
    wants_coda: bool,
    /// The return asks for the double coda.
    wants_double_coda: bool,
    /// Stop at Fine when returning.
    stop_at_fine: bool,
}

/// Where the jump targets are in the repeated order.
#[derive(Debug, Clone, Copy)]
struct Targets {
    coda: Option<usize>,
    double_coda: Option<usize>,
    segno: Option<usize>,
    segno_segno: Option<usize>,
}

/// What a sign does on this pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// Go on at this position of the order.
    Jump(usize),
    /// The song ends after this measure.
    Stop,
}

/// Applies the navigation signs of `song` to the repeated `order`.
fn direction_order(song: &Song, order: &[MeasureIndex]) -> Vec<MeasureIndex> {
    let limit = order.len().saturating_mul(MAX_PLAYED_PER_MEASURE);
    let mut played = Vec::with_capacity(order.len());
    let targets = Targets {
        coda: target(order, song, Direction::Coda),
        double_coda: target(order, song, Direction::DoubleCoda),
        segno: target(order, song, Direction::Segno),
        segno_segno: target(order, song, Direction::SegnoSegno),
    };
    let mut navigation = Navigation::default();
    let mut at = 0;

    while at < order.len() && played.len() < limit {
        let Some(&measure) = order.get(at) else {
            break;
        };
        played.push(measure);
        // A measure may hold several signs (a fine and a D.C. al fine, a segno and a to
        // coda): the first that acts on this pass wins, a fine before any jump.
        let flow = song
            .directions(measure)
            .into_iter()
            .find_map(|direction| apply(direction, &mut navigation, targets));
        match flow {
            Some(Flow::Stop) => break,
            Some(Flow::Jump(next)) => at = next,
            None => at = at.saturating_add(1),
        }
    }
    played
}

/// What `direction` does with `navigation` as it is, updating it when the sign acts.
fn apply(direction: Direction, navigation: &mut Navigation, targets: Targets) -> Option<Flow> {
    use Direction as D;
    match direction {
        D::Fine if navigation.returned && navigation.stop_at_fine => Some(Flow::Stop),
        D::DaCapo | D::DaCapoAlCoda | D::DaCapoAlDoubleCoda | D::DaCapoAlFine
            if !navigation.returned =>
        {
            navigation.returned = true;
            navigation.stop_at_fine = direction == D::DaCapoAlFine;
            navigation.wants_coda = direction == D::DaCapoAlCoda;
            navigation.wants_double_coda = direction == D::DaCapoAlDoubleCoda;
            Some(Flow::Jump(0))
        }
        D::DaSegno | D::DaSegnoAlCoda | D::DaSegnoAlDoubleCoda | D::DaSegnoAlFine
            if !navigation.returned =>
        {
            let segno = targets.segno?;
            navigation.returned = true;
            navigation.stop_at_fine = direction == D::DaSegnoAlFine;
            navigation.wants_coda = direction == D::DaSegnoAlCoda;
            navigation.wants_double_coda = direction == D::DaSegnoAlDoubleCoda;
            Some(Flow::Jump(segno))
        }
        D::DaSegnoSegno
        | D::DaSegnoSegnoAlCoda
        | D::DaSegnoSegnoAlDoubleCoda
        | D::DaSegnoSegnoAlFine
            if !navigation.returned =>
        {
            let segno = targets.segno_segno?;
            navigation.returned = true;
            navigation.stop_at_fine = direction == D::DaSegnoSegnoAlFine;
            navigation.wants_coda = direction == D::DaSegnoSegnoAlCoda;
            navigation.wants_double_coda = direction == D::DaSegnoSegnoAlDoubleCoda;
            Some(Flow::Jump(segno))
        }
        D::DaCoda if navigation.returned && navigation.wants_coda && !navigation.to_coda => {
            let coda = targets.coda?;
            navigation.to_coda = true;
            Some(Flow::Jump(coda))
        }
        D::DaDoubleCoda
            if navigation.returned && navigation.wants_double_coda && !navigation.to_coda =>
        {
            let coda = targets.double_coda?;
            navigation.to_coda = true;
            Some(Flow::Jump(coda))
        }
        _ => None,
    }
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

    /// A song of `count` empty measures with navigation signs.
    fn directions_of(count: usize, marks: &[(usize, Direction)]) -> Vec<usize> {
        let song = testing::song_with_directions(count, marks).unwrap();
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
    fn an_ending_covers_the_bars_after_it() {
        // |: 0 [1. 1 2 :| [2. 3 | 4 — only bar 1 carries the ending, 2 is its second bar
        // (the repeat's close), and 3 is the second ending (holywars.gp: bars 105-117 and
        // the second ending at 118).
        let marks = [
            (0, true, -1, 0),
            (1, false, -1, 0b01),
            (2, false, 1, 0),
            (3, false, -1, 0b10),
        ];
        assert_eq!(order_of(5, &marks), [0, 1, 2, 0, 3, 4]);
    }

    #[test]
    fn a_multi_bar_ending_counts_every_pass() {
        // Three passes with the close inside the ending's bars: the skipped bars must still
        // count the close, or the last pass never comes.
        let marks = [
            (0, true, -1, 0),
            (1, false, -1, 0b01),
            (2, false, 2, 0),
            (3, false, -1, 0b10),
        ];
        assert_eq!(order_of(5, &marks), [0, 1, 2, 0, 0, 3, 4]);
    }

    #[test]
    fn the_second_ending_does_not_disturb_the_next_section() {
        // |: 0 [1. 1 2 :| [2. 3 |: 4 5 :| 6
        let marks = [
            (0, true, -1, 0),
            (1, false, -1, 0b01),
            (2, false, 1, 0),
            (3, false, -1, 0b10),
            (4, true, -1, 0),
            (5, false, 1, 0),
        ];
        assert_eq!(order_of(7, &marks), [0, 1, 2, 0, 3, 4, 5, 4, 5, 6]);
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

    #[test]
    fn da_capo_replays_from_the_beginning() {
        // 0 1 D.C. 2 3
        assert_eq!(
            directions_of(4, &[(1, Direction::DaCapo)]),
            [0, 1, 0, 1, 2, 3]
        );
    }

    #[test]
    fn da_capo_al_fine_stops_at_fine() {
        // 0 1 Fine 2 D.C. al Fine 3
        assert_eq!(
            directions_of(5, &[(1, Direction::Fine), (3, Direction::DaCapoAlFine)]),
            [0, 1, 2, 3, 0, 1]
        );
    }

    #[test]
    fn da_capo_al_coda_jumps_to_the_coda_on_the_return() {
        // 0 1 To coda 2 Coda 3 D.C. al coda 4
        assert_eq!(
            directions_of(
                5,
                &[
                    (2, Direction::DaCoda),
                    (3, Direction::Coda),
                    (4, Direction::DaCapoAlCoda),
                ]
            ),
            [0, 1, 2, 3, 4, 0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn dal_segno_jumps_to_the_segno() {
        // 0 Segno 1 2 D.S. 3
        assert_eq!(
            directions_of(4, &[(0, Direction::Segno), (2, Direction::DaSegno)]),
            [0, 1, 2, 0, 1, 2, 3]
        );
    }

    #[test]
    fn a_to_coda_before_the_return_is_passed_through() {
        // 0 To coda 1 Coda 2 3 D.C. (not al coda) 4: the coda is not used.
        assert_eq!(
            directions_of(
                5,
                &[
                    (1, Direction::DaCoda),
                    (2, Direction::Coda),
                    (4, Direction::DaCapo),
                ]
            ),
            [0, 1, 2, 3, 4, 0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn one_measure_may_hold_a_segno_and_a_to_coda() {
        // 0 1[Segno, To coda] 2 3[D.S. al coda] 4 Coda: back to 1, then to the coda.
        assert_eq!(
            directions_of(
                5,
                &[
                    (1, Direction::Segno),
                    (1, Direction::DaCoda),
                    (3, Direction::DaSegnoAlCoda),
                    (4, Direction::Coda),
                ]
            ),
            [0, 1, 2, 3, 1, 4]
        );
    }

    #[test]
    fn a_fine_and_a_da_capo_al_fine_in_one_measure() {
        // The first pass jumps back, the second stops there.
        assert_eq!(
            directions_of(3, &[(1, Direction::Fine), (1, Direction::DaCapoAlFine)]),
            [0, 1, 0, 1]
        );
    }
}
