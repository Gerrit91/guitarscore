//! Swing timing: maps an unwarped tick offset inside a measure to the tick where it sounds
//! under a triplet, dotted or Scottish feel.
//!
//! Each pair of the swung note value is split unevenly: two thirds / one third for a
//! triplet feel, three quarters / one quarter for a dotted feel, one quarter / three
//! quarters for a Scottish feel. An incomplete final pair is left straight, so the measure's end does not move.

use gscore_model::Feel;

use crate::tempo::PPQ;

/// The pair length in ticks and the share of it the first note takes, as `num/den`.
fn ratio(feel: Feel) -> Option<(u64, u64, u64)> {
    let (num, den) = feel.first_share()?;
    let quarter = u64::from(PPQ);
    let unit = if feel.is_sixteenth() {
        quarter / 2
    } else {
        quarter
    };
    Some((unit, u64::from(num), u64::from(den)))
}

/// Where `offset` (ticks from the measure start) sounds with `feel`. `measure_ticks` keeps
/// an incomplete final pair straight so the measure's end does not move.
pub(crate) fn swing_offset(offset: u64, measure_ticks: u64, feel: Feel) -> u64 {
    let Some((unit, first_num, first_den)) = ratio(feel) else {
        return offset;
    };
    let unit = unit.max(1);
    let full_end = measure_ticks
        .checked_div(unit)
        .unwrap_or(0)
        .saturating_mul(unit);
    if offset > full_end {
        // The last, incomplete pair is not swung.
        return offset;
    }
    warp_within(offset, unit, first_num, first_den)
}

/// A piecewise linear map: the first half of each unit becomes `first` ticks, the second
/// half the rest.
fn warp_within(offset: u64, unit: u64, first_num: u64, first_den: u64) -> u64 {
    let first = unit
        .saturating_mul(first_num)
        .checked_div(first_den.max(1))
        .unwrap_or(0);
    let whole = offset.checked_div(unit).unwrap_or(0);
    let rem = offset.checked_rem(unit).unwrap_or(0);
    let half = unit.checked_div(2).unwrap_or(0);
    let warped_rem = if rem <= half {
        rem.saturating_mul(first)
            .checked_div(half.max(1))
            .unwrap_or(0)
    } else {
        let rest = rem.saturating_sub(half);
        let rest_unit = unit.saturating_sub(half).max(1);
        first.saturating_add(
            rest.saturating_mul(unit.saturating_sub(first))
                .checked_div(rest_unit)
                .unwrap_or(0),
        )
    };
    whole.saturating_mul(unit).saturating_add(warped_rem)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::arithmetic_side_effects)]

    use super::*;

    fn quarter() -> u64 {
        u64::from(PPQ)
    }

    #[test]
    fn eighth_swing_splits_each_quarter_two_to_one() {
        let measure = quarter() * 4;
        assert_eq!(swing_offset(0, measure, Feel::None), 0);
        assert_eq!(
            swing_offset(quarter() / 2, measure, Feel::None),
            quarter() / 2
        );
        assert_eq!(swing_offset(0, measure, Feel::Eighth), 0);
        assert_eq!(
            swing_offset(quarter() / 2, measure, Feel::Eighth),
            quarter() * 2 / 3
        );
        assert_eq!(swing_offset(quarter(), measure, Feel::Eighth), quarter());
        assert_eq!(
            swing_offset(quarter() + quarter() / 2, measure, Feel::Eighth),
            quarter() + quarter() * 2 / 3
        );
        assert_eq!(swing_offset(measure, measure, Feel::Eighth), measure);
    }

    #[test]
    fn dotted_eighth_swing_splits_each_quarter_three_to_one() {
        let measure = quarter() * 4;
        assert_eq!(
            swing_offset(quarter() / 2, measure, Feel::DottedEighth),
            quarter() * 3 / 4
        );
        assert_eq!(
            swing_offset(quarter() + quarter() / 2, measure, Feel::DottedEighth),
            quarter() + quarter() * 3 / 4
        );
    }

    #[test]
    fn scottish_eighth_feel_snaps_the_first_note_short() {
        let measure = quarter() * 4;
        assert_eq!(
            swing_offset(quarter() / 2, measure, Feel::ScottishEighth),
            quarter() / 4
        );
        assert_eq!(
            swing_offset(quarter() * 2, measure, Feel::ScottishEighth),
            quarter() * 2
        );
    }

    #[test]
    fn sixteenth_swing_waits_for_the_sixteenth_pair() {
        let measure = quarter() * 4;
        // An eighth-note boundary is also a sixteenth-pair boundary: it does not move.
        assert_eq!(
            swing_offset(quarter() / 2, measure, Feel::Sixteenth),
            quarter() / 2
        );
        assert_eq!(
            swing_offset(quarter() / 4, measure, Feel::Sixteenth),
            quarter() / 3
        );
        assert_eq!(
            swing_offset(quarter() / 2 + quarter() / 4, measure, Feel::Sixteenth),
            quarter() / 2 + quarter() / 3
        );
    }

    #[test]
    fn an_incomplete_final_pair_is_left_straight() {
        // 5/8: two full quarters swing, the last eighth stays put.
        let measure = quarter() * 5 / 2;
        assert_eq!(
            swing_offset(measure, measure, Feel::Eighth),
            measure,
            "the measure end never moves"
        );
        let last_eighth = quarter() * 2 + quarter() / 2;
        assert_eq!(
            swing_offset(last_eighth, measure, Feel::Eighth),
            last_eighth
        );
    }
}
