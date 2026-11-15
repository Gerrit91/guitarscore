//! The order in which measures are played: repeats, alternative endings and navigation
//! signs (D.C., D.S., coda, fine) unrolled.

use gscore_model::{Direction, MeasureIndex, Song};

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

/// The measures of `song` with only repeats and alternative endings unrolled.
fn repeat_order(song: &Song) -> Vec<MeasureIndex> {
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
