//! The track panel's Song tab: the song's text information and the list of its sections.
//!
//! The settings window's Song tab has every field; this keeps the ones a song is usually
//! identified by at hand while editing. A section's row selects all its bars in every
//! track, ready for copying, deleting or looping, and puts the cursor on its first bar so
//! the score follows.

use egui::{Color32, RichText};
use gscore_edit::Command;
use gscore_model::{MAX_SONG_TEXT_CHARS, Section, SongInfo, SongTextField};

use super::super::GuitarScoreApp;
use super::super::grid::GridAction;
use super::super::style;
use super::super::table;
use crate::document::GridData;
use crate::state::MeasureSelection;

/// The fields the Song tab shows, in order. The settings window edits all of them.
const FIELDS: &[SongTextField] = &[
    SongTextField::Title,
    SongTextField::Subtitle,
    SongTextField::Artist,
    SongTextField::Album,
    SongTextField::Composer,
    SongTextField::Lyricist,
    SongTextField::Copyright,
];

/// What the Song tab keeps between frames: its text fields as typed, one entry per field
/// in [`FIELDS`], and the song's values they were last synced from. Keeping the buffers
/// (rather than drawing them from the song every frame) keeps an input method's
/// composition intact while it is being typed.
#[derive(Debug, Default)]
pub(in crate::app) struct SongPanelState {
    /// The fields as typed.
    draft: Vec<String>,
    /// The song's values the draft was synced from, to notice outside edits (undo).
    base: Vec<String>,
}

impl SongPanelState {
    /// Takes the song's values in, except the ones the user has typed over and not yet
    /// written (a command in flight, or an input method's composition).
    fn sync(&mut self, info: &SongInfo) {
        let song: Vec<String> = FIELDS.iter().map(|field| field.get(info)).collect();
        if song == self.base && self.draft.len() == song.len() {
            return;
        }
        if self.draft == self.base || self.draft.len() != song.len() {
            self.draft.clone_from(&song);
        }
        self.base = song;
    }
}

/// One section as the list shows it: where it starts, where its bars end (the bar before
/// the next section, or the song's last bar), and what it is called.
struct SectionRow<'a> {
    start: usize,
    end: usize,
    section: &'a Section,
}

impl GuitarScoreApp {
    /// The Song tab: the song's text fields and its sections.
    pub(super) fn song_tab(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let info = tab.document.song().info();
        let measure_count = tab.document.song().measure_count();
        let grid = tab.document.grid();
        // What the grid shows as selected, to mark the section whose bars are.
        let selection = tab.loop_measures();
        let track = tab.track;
        let state = &mut tab.track_panel.song;
        state.sync(&info);
        let mut commands: Vec<Command> = Vec::new();
        let mut break_merge = false;
        let mut select = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                style::section(ui, "song_information", "Song", |ui| {
                    for (index, field) in FIELDS.iter().enumerate() {
                        // A field that took the keyboard starts its own undo step, so each
                        // edit of a field is one step however long it is typed.
                        break_merge |= song_field(ui, *field, index, state, &info, &mut commands);
                    }
                });
                style::section(ui, "song_sections", "Sections", |ui| {
                    sections(ui, &grid, measure_count, selection, &mut select);
                });
            });
        if break_merge && let Some(tab) = self.tabs.get_mut(self.active) {
            tab.document.editor_mut().break_merge();
        }
        // Typing continues the undo step it started; the commands carry the field's text.
        for command in commands {
            self.apply_command(command, true);
        }
        // The drafts now follow what the song says (a command may have been refused).
        let info = self.document().map(|document| document.song().info());
        if let (Some(info), Some(tab)) = (&info, self.tabs.get_mut(self.active)) {
            tab.track_panel.song.sync(info);
        }
        if let Some(selection) = select {
            self.apply_grid_action(GridAction::Select(selection));
            // The score follows to the section's first bar; the selection itself covers
            // every track, as the grid ruler's does.
            self.apply_grid_action(GridAction::MoveCursor {
                track,
                measure: selection.start(),
            });
        }
    }
}

/// One text field of the Song tab: its label, then the field filling the row. A change is
/// a [`Command`], so it can be undone. Returns whether the field took the keyboard this
/// frame.
fn song_field(
    ui: &mut egui::Ui,
    field: SongTextField,
    index: usize,
    state: &mut SongPanelState,
    info: &SongInfo,
    commands: &mut Vec<Command>,
) -> bool {
    let tokens = style::tokens(ui);
    let (changed, gained_focus) = ui
        .horizontal(|ui| {
            // The labels line up on the right, as a property sheet's do; a long one
            // shortens rather than widening the panel.
            let label_width = 64.0_f32.min(ui.available_width() * 0.4);
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 18.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(field.label()).small().color(tokens.text_weak),
                        )
                        .truncate(),
                    );
                },
            );
            let Some(text) = state.draft.get_mut(index) else {
                return (false, false);
            };
            let response = ui.add(
                egui::TextEdit::singleline(text)
                    .hint_text(field.label())
                    .char_limit(MAX_SONG_TEXT_CHARS)
                    .desired_width((ui.available_width() - 2.0).max(40.0)),
            );
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(format!("{} field", field.label()));
            });
            (response.changed(), response.gained_focus())
        })
        .inner;
    if changed {
        let typed = state.draft.get(index).cloned().unwrap_or_default();
        if typed != field.get(info) {
            commands.push(Command::SetSongText { field, text: typed });
        }
    }
    gained_focus
}

/// One section as the table lists it: its bars, its name, its color, and whether the grid
/// shows those bars selected now.
struct SectionLine {
    start: usize,
    end: usize,
    title: String,
    bars: String,
    color: Color32,
    chosen: bool,
}

/// The Sections category: a table like the track's tone presets, one row per section with
/// its color, its name and its bars; a click selects those bars in every track.
fn sections(
    ui: &mut egui::Ui,
    grid: &GridData,
    measure_count: usize,
    selection: Option<MeasureSelection>,
    select: &mut Option<MeasureSelection>,
) {
    let lines: Vec<SectionLine> = section_rows(grid, measure_count)
        .into_iter()
        .map(|row| {
            let title = if row.section.title.trim().is_empty() {
                "(untitled section)".to_owned()
            } else {
                row.section.title.clone()
            };
            let (first, last) = (row.start.saturating_add(1), row.end.saturating_add(1));
            let bars = if row.start == row.end {
                format!("bar {first}")
            } else {
                format!("bars {first}–{last}")
            };
            let (red, green, blue) = row.section.color.parts();
            SectionLine {
                start: row.start,
                end: row.end,
                title,
                bars,
                color: Color32::from_rgb(red, green, blue),
                chosen: selection
                    .is_some_and(|shown| shown.start() == row.start && shown.end() == row.end),
            }
        })
        .collect();
    if lines.is_empty() {
        ui.label(RichText::new("This song has no sections yet.").weak());
        ui.label(
            RichText::new(
                "Name a bar with the Section tool in the edit palette; it then lists here, \
                 and a click selects all its bars.",
            )
            .small()
            .weak(),
        );
        return;
    }
    let mut clicked = None;
    table::surface(ui, "song_sections_table", table::VISIBLE_ROWS, |ui| {
        for (index, line) in lines.iter().enumerate() {
            let accessible = format!("Select the bars of section {}", line.title);
            let hint = format!("Select {} of \"{}\" in every track", line.bars, line.title);
            let (_, response) = table::row(
                ui,
                &table::Row {
                    label: &line.title,
                    detail: Some(&line.bars),
                    swatch: Some(line.color),
                    selected: line.chosen,
                    accessible: &accessible,
                    hint: &hint,
                },
            );
            if response.clicked() {
                clicked = Some(index);
            }
        }
    });
    if let Some(line) = clicked.and_then(|index| lines.get(index)) {
        *select = Some(MeasureSelection {
            anchor: line.start,
            focus: line.end,
            track: None,
        });
    }
}

/// The sections in `grid`, in order: each lasts from its bar to the bar before the next
/// section, or the song's last bar (as the grid's ruler shows them).
fn section_rows<'a>(grid: &'a GridData, measure_count: usize) -> Vec<SectionRow<'a>> {
    let starts: Vec<usize> = (0..measure_count)
        .filter(|measure| grid.sections.get(*measure).is_some_and(Option::is_some))
        .collect();
    starts
        .iter()
        .enumerate()
        .filter_map(|(index, &start)| {
            let Some(Some(section)) = grid.sections.get(start) else {
                return None;
            };
            let next = starts
                .get(index.saturating_add(1))
                .copied()
                .unwrap_or(measure_count);
            Some(SectionRow {
                start,
                end: next.saturating_sub(1).max(start),
                section,
            })
        })
        .collect()
}
