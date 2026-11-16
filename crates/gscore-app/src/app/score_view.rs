//! Which tracks the score shows: the cursor's track (as before), every track Guitar Pro
//! shows together (its multitrack view; a track hidden with its eye stays out), or one of
//! the song's named score views (GuitarScore's own: tracks in their order, their staves,
//! and whether empty staves are left out). Several tracks are stacked in every system with
//! their beats lined up; on screen the cursor's track is always among them.

use gscore_edit::{ActionId, Command, Cursor};
use gscore_model::{
    BeatIndex, BeatKind, Duration, MeasureIndex, Song, SystemBreak, TrackIndex, TrackStaves,
    VoiceIndex,
};

/// The tracks a tab's score shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ScoreTracks {
    /// Only the cursor's track.
    #[default]
    Cursor,
    /// Every track shown in Guitar Pro's multitrack view, in song order.
    All,
    /// The song's score view at this position (all tracks once it is gone).
    View(usize),
}

/// What a score shows, resolved against the song.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ShownScore {
    /// The tracks, top to bottom.
    pub(crate) tracks: Vec<usize>,
    /// The staves a track shows instead of its own.
    pub(crate) staves: Vec<(TrackIndex, TrackStaves)>,
    /// Whether empty staves are left out: the view's own choice; `None` for the program's
    /// setting (one track, or all tracks).
    pub(crate) hide_empty_staves: Option<bool>,
}

impl ScoreTracks {
    /// What the score shows for `song` with the cursor on `cursor`. With `cursor_shown`
    /// (on screen, where it must stay editable) the cursor's track is added where a view
    /// leaves it out; printed, the view is exactly as built.
    pub(crate) fn resolve(&self, song: &Song, cursor: usize, cursor_shown: bool) -> ShownScore {
        let count = song.track_count();
        let all = || -> Vec<usize> {
            song.tracks()
                .filter(|track| track.is_visible())
                .map(|track| track.index().get())
                .collect()
        };
        let mut shown = match self {
            ScoreTracks::Cursor => ShownScore {
                tracks: vec![cursor],
                ..ShownScore::default()
            },
            ScoreTracks::All => ShownScore {
                tracks: all(),
                ..ShownScore::default()
            },
            ScoreTracks::View(index) => match song.score_views().into_iter().nth(*index) {
                Some(view) => ShownScore {
                    tracks: view.tracks.iter().map(|entry| entry.track.get()).collect(),
                    staves: view
                        .tracks
                        .iter()
                        .map(|entry| (entry.track, entry.staves))
                        .collect(),
                    hide_empty_staves: Some(view.hide_empty_staves),
                },
                None => ShownScore {
                    tracks: all(),
                    ..ShownScore::default()
                },
            },
        };
        shown.tracks.retain(|&track| track < count);
        let missing = !shown.tracks.contains(&cursor);
        if (cursor_shown || shown.tracks.is_empty()) && missing && cursor < count {
            // Among all tracks, the cursor's goes in song order; a view gets it last.
            shown.tracks.push(cursor);
            if !matches!(self, ScoreTracks::View(_)) {
                shown.tracks.sort_unstable();
            }
        }
        if shown.tracks.is_empty() && count > 0 {
            shown.tracks.push(0);
        }
        shown
    }

    /// The tracks to lay out on screen, top to bottom (the cursor's among them).
    pub(crate) fn tracks(&self, song: &Song, cursor: usize) -> Vec<usize> {
        self.resolve(song, cursor, true).tracks
    }

    /// All tracks, or back to the cursor's alone (the View menu's "All tracks" key).
    pub(crate) fn toggle_all(&mut self) {
        *self = match self {
            ScoreTracks::Cursor => ScoreTracks::All,
            ScoreTracks::All | ScoreTracks::View(_) => ScoreTracks::Cursor,
        };
    }

    /// The track shown above (`step` -1) or below (`step` 1) `cursor`'s, if there is one:
    /// where Up past the top string and Down past the bottom one go.
    pub(crate) fn neighbour(&self, song: &Song, cursor: usize, step: isize) -> Option<usize> {
        let tracks = self.tracks(song, cursor);
        let at = tracks.iter().position(|&track| track == cursor)?;
        let next = at.checked_add_signed(step)?;
        tracks.get(next).copied()
    }

    /// The name the toolbar and menus show for this choice.
    pub(crate) fn label(&self, song: &Song) -> String {
        match self {
            ScoreTracks::Cursor => "One track".to_owned(),
            ScoreTracks::All => "All tracks".to_owned(),
            ScoreTracks::View(index) => song
                .score_views()
                .into_iter()
                .nth(*index)
                .map_or_else(|| "All tracks".to_owned(), |view| view.name),
        }
    }
}

/// The width of the tab bar's choice of score view.
pub(super) const SCORE_VIEW_WIDTH: f32 = 150.0;
/// Room before the view's name for the eye (and the list's indentation).
const EYE_ROOM: f32 = 20.0;

impl super::GuitarScoreApp {
    /// Shows `choice` in the active tab. A view that leaves the cursor's track out takes
    /// the cursor to its first track, at the same place in the music.
    pub(super) fn choose_score_tracks(&mut self, choice: ScoreTracks) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.score = choice;
        let song = tab.document.song();
        let shown = choice.resolve(song, tab.track, false);
        if !shown.tracks.contains(&tab.track)
            && let Some(&first) = shown.tracks.first()
        {
            let from = tab.document.editor().cursor();
            let to = cursor_in_track(song, from, first, true);
            tab.track = first;
            tab.document.editor_mut().set_cursor(to);
        }
    }

    /// Which tracks the score shows: one track, all tracks, or one of the song's score
    /// views; and the dialog that builds the views.
    pub(super) fn score_view_choice(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let current = tab.score;
        let label = current.label(song);
        let mut entries = vec![
            (ScoreTracks::Cursor, "One track".to_owned()),
            (ScoreTracks::All, "All tracks".to_owned()),
        ];
        entries.extend(
            song.score_views()
                .into_iter()
                .enumerate()
                .map(|(index, view)| (ScoreTracks::View(index), view.name)),
        );
        let mut chosen = None;
        let mut edit = false;
        let view = &mut self.view;
        // The button shows an eye before the view's name; the list's rows are indented to
        // line up with the name.
        let font = egui::TextStyle::Button.resolve(ui.style());
        let mut job = egui::text::LayoutJob::simple_singleline(
            label,
            font,
            ui.visuals().widgets.inactive.text_color(),
        );
        if let Some(section) = job.sections.first_mut() {
            section.leading_space = EYE_ROOM;
        }
        let combo = egui::ComboBox::from_id_salt("score_view_choice")
            .width(SCORE_VIEW_WIDTH)
            .selected_text(job)
            // The switches stay open while ticked; a view chosen closes the list.
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                for (choice, name) in &entries {
                    let clicked = ui
                        .horizontal(|ui| {
                            ui.add_space(EYE_ROOM);
                            ui.selectable_label(*choice == current, name).clicked()
                        })
                        .inner;
                    if clicked {
                        chosen = Some(*choice);
                        ui.close();
                    }
                }
                ui.separator();
                ui.checkbox(&mut view.hide_empty_staves, "Hide empty staves")
                    .on_hover_text(
                        "With all tracks: leave a track out of a line where it plays nothing \
                         (each score view has its own choice)",
                    );
                ui.checkbox(&mut view.multi_bar_rests, "Multi-bar rests")
                    .on_hover_text(
                        "Write a run of bars where every shown track rests as one bar with \
                         the number of bars above it",
                    );
                ui.separator();
                if ui.button("Edit score views…").clicked() {
                    edit = true;
                    ui.close();
                }
            });
        let button = combo.response.rect;
        let eye = egui::Rect::from_center_size(
            egui::pos2(
                button.left() + ui.spacing().button_padding.x + EYE_ROOM / 2.0 - 2.0,
                button.center().y,
            ),
            egui::vec2(14.0, 14.0),
        );
        super::icons::paint(
            ui.painter(),
            eye,
            ui.visuals().widgets.inactive.text_color(),
            super::icons::Icon::Eye,
        );
        ui.ctx().accesskit_node_builder(combo.response.id, |node| {
            node.set_label("Score view");
        });
        combo
            .response
            .on_hover_text("Which tracks the score shows: one, all, or a view of the score");
        if let Some(choice) = chosen {
            self.choose_score_tracks(choice);
        }
        if edit {
            self.open_score_views();
        }
    }

    /// The View menu's choice of tracks: the cursor's track, all tracks, or one of the
    /// song's score views; and the dialog that builds the views.
    pub(super) fn score_view_menu(&mut self, ui: &mut egui::Ui) {
        let hint = super::toolbar::key_hint(&self.keymap, ActionId::ToggleMultitrack);
        let Some(tab) = self.tabs.get(self.active) else {
            ui.add_enabled(false, egui::Button::new("All tracks"));
            return;
        };
        let current = tab.score;
        let views: Vec<String> = tab
            .document
            .song()
            .score_views()
            .into_iter()
            .map(|view| view.name)
            .collect();
        let mut chosen = None;
        if ui
            .radio(current == ScoreTracks::Cursor, "One track")
            .on_hover_text("Only the track the cursor is on")
            .clicked()
        {
            chosen = Some(ScoreTracks::Cursor);
        }
        if ui
            .radio(current == ScoreTracks::All, "All tracks")
            .on_hover_text(format!(
                "Every track in one score, their beats lined up (the tracks Guitar Pro \
                 shows together){hint}"
            ))
            .clicked()
        {
            chosen = Some(ScoreTracks::All);
        }
        for (index, name) in views.iter().enumerate() {
            if ui
                .radio(current == ScoreTracks::View(index), name)
                .on_hover_text("A score view of this song")
                .clicked()
            {
                chosen = Some(ScoreTracks::View(index));
            }
        }
        if let Some(choice) = chosen {
            self.choose_score_tracks(choice);
            ui.close();
        }
        ui.checkbox(&mut self.view.hide_empty_staves, "Hide empty staves")
            .on_hover_text(
                "With all tracks: leave a track out of a line where it plays nothing (the \
                 first line and the cursor's track always show). Each score view has its own \
                 choice.",
            );
        ui.checkbox(&mut self.view.multi_bar_rests, "Multi-bar rests")
            .on_hover_text(
                "Write a run of bars where every shown track rests as one bar with the \
                 number of bars above it, as parts are printed",
            );
        if ui
            .button("Score views…")
            .on_hover_text(
                "Build views of the score: which tracks, in what order, with which staves",
            )
            .clicked()
        {
            self.open_score_views();
            ui.close();
        }
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

    use gscore_model::{ScoreView, ViewTrack};

    /// A song of `count` guitar tracks.
    fn song(count: usize) -> Song {
        let tunings = vec![gscore_model::testing::GUITAR_6; count];
        gscore_model::testing::song(&tunings, 1).unwrap()
    }

    fn view(tracks: &[usize], hide: bool) -> ScoreView {
        ScoreView {
            name: "Part".to_owned(),
            tracks: tracks
                .iter()
                .map(|&track| ViewTrack {
                    track: TrackIndex::new(track),
                    staves: TrackStaves::TAB_ONLY,
                })
                .collect(),
            hide_empty_staves: hide,
        }
    }

    #[test]
    fn all_tracks_are_those_guitar_pro_shows_and_the_cursors() {
        let mut song = song(4);
        assert_eq!(ScoreTracks::Cursor.tracks(&song, 2), [2]);
        assert_eq!(ScoreTracks::All.tracks(&song, 2), [0, 1, 2, 3]);
        let _ = song.set_track_visible(TrackIndex::new(1), false).unwrap();
        let _ = song.set_track_visible(TrackIndex::new(2), false).unwrap();
        assert_eq!(ScoreTracks::All.tracks(&song, 0), [0, 3]);
        // On screen the cursor's hidden track stays, in song order; printed it does not.
        assert_eq!(ScoreTracks::All.tracks(&song, 2), [0, 2, 3]);
        assert_eq!(ScoreTracks::All.resolve(&song, 2, false).tracks, [0, 3]);
    }

    #[test]
    fn a_view_shows_its_tracks_in_its_order_with_its_staves() {
        let mut song = song(4);
        let _ = song.set_score_views(&[view(&[3, 1], true)]).unwrap();
        let shown = ScoreTracks::View(0).resolve(&song, 3, true);
        assert_eq!(shown.tracks, [3, 1]);
        assert_eq!(
            shown.staves,
            [
                (TrackIndex::new(3), TrackStaves::TAB_ONLY),
                (TrackIndex::new(1), TrackStaves::TAB_ONLY)
            ]
        );
        assert_eq!(shown.hide_empty_staves, Some(true));
        // The cursor elsewhere: last on screen, left out when printed.
        assert_eq!(ScoreTracks::View(0).tracks(&song, 0), [3, 1, 0]);
        assert_eq!(ScoreTracks::View(0).resolve(&song, 0, false).tracks, [3, 1]);
        // A view that is gone shows all tracks.
        assert_eq!(ScoreTracks::View(4).tracks(&song, 0), [0, 1, 2, 3]);
        assert_eq!(ScoreTracks::View(0).label(&song), "Part");
    }

    #[test]
    fn neighbours_are_the_shown_tracks_above_and_below() {
        let mut song = song(6);
        let _ = song.set_score_views(&[view(&[5, 0, 2], false)]).unwrap();
        let shown = ScoreTracks::View(0);
        assert_eq!(shown.neighbour(&song, 0, -1), Some(5));
        assert_eq!(shown.neighbour(&song, 0, 1), Some(2));
        assert_eq!(shown.neighbour(&song, 5, -1), None);
        assert_eq!(shown.neighbour(&song, 2, 1), None);
        assert_eq!(ScoreTracks::Cursor.neighbour(&song, 1, 1), None);
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
