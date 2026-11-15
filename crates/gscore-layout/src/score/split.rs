//! A system taller than a page, split between its tracks onto the pages that follow.

use std::ops::Range;

use gscore_model::TrackIndex;

/// The pieces a system of staves `heights` (in `parts`' order) is placed in: one when it
/// fits the first of `rooms` (or there are no pages), else as many as needed, each as tall
/// as the room allows (the first piece's room, then the later pages'). A piece ends only
/// between tracks: a track's staves stay together, and a track taller than a page makes a
/// piece of its own.
pub(super) fn split_system(
    heights: &[f32],
    tracks: &[TrackIndex],
    rooms: Option<(f32, f32)>,
) -> Vec<Range<usize>> {
    let all = 0..heights.len();
    let Some((first, later)) = rooms else {
        return vec![all];
    };
    if heights.iter().sum::<f32>() <= first {
        return vec![all];
    }
    let mut pieces = Vec::new();
    let (mut start, mut used, mut index) = (0, 0.0_f32, 0);
    while index < heights.len() {
        let track = tracks.get(index);
        let end = (index..heights.len())
            .find(|&next| tracks.get(next) != track)
            .unwrap_or(heights.len());
        let group: f32 = heights
            .get(index..end)
            .map_or(0.0, |group| group.iter().sum());
        let limit = if pieces.is_empty() { first } else { later };
        if used + group > limit && index > start {
            pieces.push(start..index);
            start = index;
            used = 0.0;
        }
        used += group;
        index = end;
    }
    pieces.push(start..heights.len());
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(of: &[usize]) -> Vec<TrackIndex> {
        of.iter().copied().map(TrackIndex::new).collect()
    }

    #[test]
    fn a_system_that_fits_or_has_no_page_stays_whole() {
        let staves = tracks(&[0, 1, 2]);
        let whole: Vec<Range<usize>> = std::iter::once(0..3).collect();
        assert_eq!(split_system(&[10.0; 3], &staves, Some((30.0, 30.0))), whole);
        assert_eq!(split_system(&[10.0; 3], &staves, None), whole);
    }

    #[test]
    fn pieces_end_between_tracks_only() {
        // Track 1 has two staves; the first page has less room (the title block).
        let staves = tracks(&[0, 1, 1, 2, 3]);
        assert_eq!(
            split_system(&[10.0; 5], &staves, Some((15.0, 30.0))),
            [0..1, 1..4, 4..5]
        );
        // A track taller than a page is a piece of its own.
        assert_eq!(
            split_system(&[10.0, 50.0, 10.0], &tracks(&[0, 1, 2]), Some((20.0, 20.0))),
            [0..1, 1..2, 2..3]
        );
    }
}
