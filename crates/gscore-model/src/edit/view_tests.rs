//! Score views and Guitar Pro's per-track eye.

use crate::interop::from_gp;
use crate::test_support::{GUITAR_6, beat, measure, song as test_song, track};
use crate::{ScoreView, Song, TrackIndex, TrackStaves, ViewTrack};

/// A song of `tracks` one-bar guitar tracks, each with one note.
fn song(tracks: usize) -> Song {
    let tracks = (0..tracks)
        .map(|index| {
            track(
                &format!("Guitar {index}"),
                GUITAR_6,
                vec![measure((4, 4), vec![vec![beat(1, &[(1, 0)])]])],
            )
        })
        .collect();
    from_gp(test_song(tracks)).expect("a test song")
}

fn view(name: &str, tracks: &[usize]) -> ScoreView {
    ScoreView {
        name: name.to_owned(),
        tracks: tracks
            .iter()
            .map(|&track| ViewTrack {
                track: TrackIndex::new(track),
                staves: TrackStaves::TAB_ONLY,
            })
            .collect(),
        hide_empty_staves: true,
    }
}

/// The tracks of each view, as indices.
fn tracks_of(song: &Song) -> Vec<Vec<usize>> {
    song.score_views()
        .iter()
        .map(|view| view.tracks.iter().map(|entry| entry.track.get()).collect())
        .collect()
}

#[test]
fn views_are_set_kept_in_their_order_and_undone() {
    let mut song = song(4);
    let views = [view("Drums", &[3]), view("Strings", &[2, 0])];
    let change = song.set_score_views(&views).expect("set");
    assert_eq!(song.score_views(), views);
    song.revert(&change).expect("undo");
    assert!(song.score_views().is_empty());
}

#[test]
fn a_view_follows_its_tracks_when_they_move_and_are_deleted() {
    let mut song = song(4);
    let _ = song.set_score_views(&[view("Part", &[1, 3])]).expect("set");
    // The view's tracks move with them.
    let _ = song
        .move_track(TrackIndex::new(3), TrackIndex::new(0))
        .expect("move");
    assert_eq!(tracks_of(&song), [vec![2, 0]]);
    // A deleted track leaves the view; undoing the deletion brings it back.
    let deleted = song.delete_track(TrackIndex::new(2)).expect("delete");
    assert_eq!(tracks_of(&song), [vec![0]]);
    song.revert(&deleted).expect("undo");
    assert_eq!(tracks_of(&song), [vec![2, 0]]);
    // A copy of a track is not in the view, nor is a new track.
    let _ = song.duplicate_track(TrackIndex::new(0)).expect("duplicate");
    let _ = song.insert_track(TrackIndex::new(0)).expect("insert");
    // [2, 0]: the copy of track 0 lands at 1, the new track at 0.
    assert_eq!(tracks_of(&song), [vec![4, 1]]);
}

#[test]
fn notes_transposed_keep_the_views() {
    let mut song = song(2);
    let views = [view("Second", &[1])];
    let _ = song.set_score_views(&views).expect("set");
    let _ = song.transpose_notes(None, 2).expect("transpose");
    assert_eq!(song.score_views(), views);
}

#[test]
fn broken_views_are_refused() {
    let mut song = song(2);
    for (views, why) in [
        (vec![view(" ", &[0])], "no name"),
        (
            vec![view("A", &[0]), view("a", &[1])],
            "the same name twice",
        ),
        (vec![view("A", &[])], "no track"),
        (vec![view("A", &[0, 0])], "a track twice"),
        (vec![view("A", &[2])], "no such track"),
        (vec![view(&"x".repeat(61), &[0])], "a name too long"),
    ] {
        assert!(song.set_score_views(&views).is_err(), "{why}");
    }
    assert!(song.score_views().is_empty());
    // Names are kept trimmed.
    let _ = song.set_score_views(&[view(" Bass ", &[1])]).expect("set");
    assert_eq!(song.score_views()[0].name, "Bass");
}

#[test]
fn a_track_is_hidden_from_all_tracks_and_one_always_stays() {
    let mut song = song(2);
    let shown = |song: &Song| -> Vec<bool> { song.tracks().map(|t| t.is_visible()).collect() };
    assert_eq!(shown(&song), [true, true]);
    let change = song
        .set_track_visible(TrackIndex::new(0), false)
        .expect("hide");
    assert_eq!(shown(&song), [false, true]);
    assert!(
        song.set_track_visible(TrackIndex::new(1), false).is_err(),
        "the last shown track stays"
    );
    song.revert(&change).expect("undo");
    assert_eq!(shown(&song), [true, true]);
}
