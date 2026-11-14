//! Which tracks the score shows: the cursor's track (as before), every track (Guitar
//! Pro's multitrack view), or a chosen set. Several tracks are stacked in every system with
//! their beats lined up; the cursor's track is always among them.

use gscore_edit::{ActionId, Command, Cursor};
use gscore_model::{
    BeatIndex, BeatKind, Duration, MeasureIndex, Song, SystemBreak, TrackIndex, VoiceIndex,
};

/// The tracks a tab's score shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ScoreTracks {
    /// Only the cursor's track.
    #[default]
    Cursor,
    /// Every track of the song.
    All,
    /// These tracks (and the cursor's), in song order.
    Chosen(Vec<usize>),
}

impl ScoreTracks {
    /// The tracks to lay out, top to bottom, for a song of `count` tracks whose cursor is
    /// on `cursor`: valid indices only, in song order, the cursor's track always included.
    pub(crate) fn tracks(&self, cursor: usize, count: usize) -> Vec<usize> {
        let mut tracks: Vec<usize> = match self {
            ScoreTracks::Cursor => Vec::new(),
            ScoreTracks::All => (0..count).collect(),
            ScoreTracks::Chosen(chosen) => chosen.clone(),
        };
        tracks.push(cursor);
        tracks.retain(|&track| track < count);
        tracks.sort_unstable();
        tracks.dedup();
        tracks
    }

    /// Every track, or back to the cursor's alone (the View menu's "All tracks").
    pub(crate) fn toggle_all(&mut self) {
        *self = match self {
            ScoreTracks::Cursor => ScoreTracks::All,
            ScoreTracks::All | ScoreTracks::Chosen(_) => ScoreTracks::Cursor,
        };
    }

    /// Whether `track` is shown (the cursor's always is).
    pub(crate) fn shows(&self, track: usize, cursor: usize, count: usize) -> bool {
        self.tracks(cursor, count).contains(&track)
    }

    /// Shows or hides `track` in a chosen set made from what is shown now. The cursor's
    /// track stays; a set of every track becomes [`ScoreTracks::All`], a set of the
    /// cursor's alone [`ScoreTracks::Cursor`].
    pub(crate) fn toggle(&mut self, track: usize, cursor: usize, count: usize) {
        let mut tracks = self.tracks(cursor, count);
        if tracks.contains(&track) {
            if track != cursor {
                tracks.retain(|&shown| shown != track);
            }
        } else if track < count {
            tracks.push(track);
        }
        tracks.sort_unstable();
        *self = if tracks.len() == count && count > 1 {
            ScoreTracks::All
        } else if tracks.len() <= 1 {
            ScoreTracks::Cursor
        } else {
            ScoreTracks::Chosen(tracks)
        };
    }

    /// The track shown above (`step` -1) or below (`step` 1) `cursor`'s, if there is one:
    /// where Up past the top string and Down past the bottom one go.
    pub(crate) fn neighbour(&self, cursor: usize, count: usize, step: isize) -> Option<usize> {
        let tracks = self.tracks(cursor, count);
        let at = tracks.iter().position(|&track| track == cursor)?;
        let next = at.checked_add_signed(step)?;
        tracks.get(next).copied()
    }
}

impl super::GuitarScoreApp {
    /// The View menu's choice of tracks: the cursor's track, all tracks, or a set ticked
    /// track by track (the cursor's track cannot be unticked).
    pub(super) fn score_view_menu(&mut self, ui: &mut egui::Ui) {
        let hint = super::toolbar::key_hint(&self.keymap, ActionId::ToggleMultitrack);
        let Some(tab) = self.tabs.get_mut(self.active) else {
            ui.add_enabled(false, egui::Button::new("All tracks"));
            return;
        };
        let count = tab.document.song().track_count();
        if ui
            .radio(tab.score == ScoreTracks::Cursor, "One track")
            .on_hover_text("Only the track the cursor is on")
            .clicked()
        {
            tab.score = ScoreTracks::Cursor;
            ui.close();
        }
        if ui
            .radio(tab.score == ScoreTracks::All, "All tracks")
            .on_hover_text(format!(
                "Every track in one score, their beats lined up{hint}"
            ))
            .clicked()
        {
            tab.score = ScoreTracks::All;
            ui.close();
        }
        ui.checkbox(&mut self.view.hide_empty_staves, "Hide empty staves")
            .on_hover_text(
                "With several tracks: leave a track out of a line where it plays nothing \
                 (the first line and the cursor's track always show)",
            );
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        ui.menu_button("Tracks shown", |ui| {
            let names: Vec<String> = tab
                .document
                .song()
                .tracks()
                .map(|track| {
                    let name = track.name().trim();
                    let number = track.index().get().saturating_add(1);
                    if name.is_empty() {
                        format!("Track {number}")
                    } else {
                        format!("{number}. {name}")
                    }
                })
                .collect();
            for (index, name) in names.into_iter().enumerate() {
                let mut shown = tab.score.shows(index, tab.track, count);
                let cursor = index == tab.track;
                let response = ui
                    .add_enabled(!cursor, egui::Checkbox::new(&mut shown, name))
                    .on_disabled_hover_text("The cursor's track is always shown");
                if response.clicked() {
                    tab.score.toggle(index, tab.track, count);
                }
            }
        });
    }
}

/// The break the score shows before `measure`: the track's own, or with `several` tracks
/// shown, the multitrack line break (a page break stays the track's).
pub(crate) fn shown_break(
    song: &Song,
    track: TrackIndex,
    measure: MeasureIndex,
    several: bool,
) -> SystemBreak {
    let own = own_break(song, track, measure);
    if !several || own == SystemBreak::Page {
        return own;
    }
    if song
        .score_line_breaks()
        .get(measure.get())
        .copied()
        .unwrap_or(false)
    {
        SystemBreak::Line
    } else {
        SystemBreak::None
    }
}

fn own_break(song: &Song, track: TrackIndex, measure: MeasureIndex) -> SystemBreak {
    song.track(track)
        .and_then(|view| view.measure(measure))
        .map_or(SystemBreak::None, |view| view.system_break())
}

/// `command` as the score view means it: with `several` tracks shown, a line break (from
/// the Bar menu or the palette) is the multitrack layout's, not the track's own; a page
/// break stays the track's. Other commands are unchanged.
pub(crate) fn for_score(command: Command, song: &Song, several: bool) -> Command {
    let Command::SetSystemBreak {
        track,
        measure,
        kind,
    } = command
    else {
        return command;
    };
    if !several {
        return command;
    }
    let own = own_break(song, track, measure);
    let score = song
        .score_line_breaks()
        .get(measure.get())
        .copied()
        .unwrap_or(false);
    let mut commands = Vec::new();
    match kind {
        SystemBreak::Page => return command,
        SystemBreak::Line => {
            commands.push(Command::SetScoreLineBreak { measure, on: true });
            if own == SystemBreak::Page {
                commands.push(Command::SetSystemBreak {
                    track,
                    measure,
                    kind: SystemBreak::None,
                });
            }
        }
        SystemBreak::None => {
            if score {
                commands.push(Command::SetScoreLineBreak { measure, on: false });
            }
            if own == SystemBreak::Page {
                commands.push(command.clone());
            }
        }
    }
    match commands.len() {
        // Nothing to remove: the original does no harm.
        0 => command,
        1 => commands.pop().unwrap_or(command),
        _ => Command::Batch { commands },
    }
}

/// The cursor moved to `track`: the same measure, the beat of its first voice that
/// sounds when the cursor's beat starts, on its top string (`top`) or bottom string.
pub(crate) fn cursor_in_track(song: &Song, from: Cursor, track: usize, top: bool) -> Cursor {
    let track = TrackIndex::new(track);
    let strings = song.track(track).map_or(1, |view| view.string_count());
    let string = if top {
        1
    } else {
        u8::try_from(strings.max(1)).unwrap_or(1)
    };
    let onset = beat_onset(song, from.track, from.measure, from.voice, from.beat);
    let voice = VoiceIndex::new(0);
    // The last beat that starts at or before the cursor's beat.
    let mut beat = BeatIndex::new(0);
    if let Some(onset) = onset
        && let Some(view) = song
            .track(track)
            .and_then(|view| view.measure(from.measure))
            .and_then(|measure| measure.voice(voice))
    {
        let mut at = Duration::ZERO;
        for candidate in view.beats() {
            if candidate.kind() == BeatKind::Empty || candidate.is_grace() {
                continue;
            }
            if at > onset {
                break;
            }
            beat = candidate.index();
            let Some(next) = candidate
                .duration()
                .ok()
                .and_then(|d| at.checked_add(d).ok())
            else {
                break;
            };
            at = next;
        }
    }
    Cursor {
        staff: gscore_model::StaffIndex::new(0),
        track,
        measure: from.measure,
        voice,
        beat,
        string,
    }
}

/// When beat `beat` of `voice` starts in its measure; `None` if the model cannot tell.
fn beat_onset(
    song: &Song,
    track: TrackIndex,
    measure: MeasureIndex,
    voice: VoiceIndex,
    beat: BeatIndex,
) -> Option<Duration> {
    let view = song.track(track)?.measure(measure)?.voice(voice)?;
    let mut at = Duration::ZERO;
    for candidate in view.beats() {
        if candidate.index() == beat {
            return Some(at);
        }
        if candidate.kind() == BeatKind::Empty || candidate.is_grace() {
            continue;
        }
        at = at.checked_add(candidate.duration().ok()?).ok()?;
    }
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursors_track_is_always_shown_in_song_order() {
        assert_eq!(ScoreTracks::Cursor.tracks(2, 4), [2]);
        assert_eq!(ScoreTracks::All.tracks(2, 4), [0, 1, 2, 3]);
        assert_eq!(ScoreTracks::Chosen(vec![3, 0]).tracks(2, 4), [0, 2, 3]);
        // Tracks deleted meanwhile drop out.
        assert_eq!(ScoreTracks::Chosen(vec![1, 7]).tracks(0, 3), [0, 1]);
    }

    #[test]
    fn toggling_builds_a_set_and_falls_back_to_one_or_all() {
        let mut shown = ScoreTracks::Cursor;
        shown.toggle(3, 1, 4);
        assert_eq!(shown, ScoreTracks::Chosen(vec![1, 3]));
        // The cursor's track cannot be hidden.
        shown.toggle(1, 1, 4);
        assert_eq!(shown, ScoreTracks::Chosen(vec![1, 3]));
        shown.toggle(0, 1, 4);
        shown.toggle(2, 1, 4);
        assert_eq!(shown, ScoreTracks::All);
        let mut shown = ScoreTracks::Chosen(vec![1, 3]);
        shown.toggle(3, 1, 4);
        assert_eq!(shown, ScoreTracks::Cursor);
    }

    #[test]
    fn neighbours_are_the_shown_tracks_above_and_below() {
        let shown = ScoreTracks::Chosen(vec![0, 2, 5]);
        assert_eq!(shown.neighbour(2, 6, -1), Some(0));
        assert_eq!(shown.neighbour(2, 6, 1), Some(5));
        assert_eq!(shown.neighbour(0, 6, -1), None);
        assert_eq!(shown.neighbour(5, 6, 1), None);
        assert_eq!(ScoreTracks::Cursor.neighbour(1, 3, 1), None);
    }

    fn line_break(measure: usize, kind: SystemBreak) -> Command {
        Command::SetSystemBreak {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(measure),
            kind,
        }
    }

    #[test]
    fn with_several_tracks_a_line_break_is_the_multitrack_layouts() {
        let song = gscore_model::testing::song(&[gscore_model::testing::GUITAR_6], 4).unwrap();
        // One track: unchanged.
        let one = for_score(line_break(2, SystemBreak::Line), &song, false);
        assert_eq!(one, line_break(2, SystemBreak::Line));
        // Several: the multitrack break.
        let several = for_score(line_break(2, SystemBreak::Line), &song, true);
        assert_eq!(
            several,
            Command::SetScoreLineBreak {
                measure: MeasureIndex::new(2),
                on: true
            }
        );
        // A page break stays the track's; removing nothing changes nothing.
        let page = for_score(line_break(2, SystemBreak::Page), &song, true);
        assert_eq!(page, line_break(2, SystemBreak::Page));
        let none = for_score(line_break(2, SystemBreak::None), &song, true);
        assert_eq!(none, line_break(2, SystemBreak::None));
        // Other commands pass through.
        let tempo = Command::SetTempo { tempo: 90 };
        assert_eq!(for_score(tempo.clone(), &song, true), tempo);
    }
}
