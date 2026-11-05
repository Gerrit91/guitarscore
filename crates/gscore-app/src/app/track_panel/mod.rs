//! The track panel (the right panel): a track's name, instrument, sound, color, staves,
//! tuning, capo and string count. Every change is a [`Command`] with undo, and sound changes
//! are heard at once (the sound commands go to the player while it plays).

mod look;
mod tuning;

use egui::RichText;
use gscore_edit::Command;
use gscore_model::{
    InstrumentKind, InstrumentSymbol, Song, StringTuning, TrackColor, TrackIndex, TrackSound,
    TrackStaves, TrackView, program_name,
};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::instrument;
use super::style;

/// What the track panel keeps between frames: which track it showed, and the name being
/// typed (so a rename is one undo step, not one per keystroke).
#[derive(Debug, Default)]
pub(super) struct TrackPanelState {
    /// The track the draft belongs to.
    track: Option<usize>,
    /// The name in the text field.
    name: String,
    /// Whether the name is being edited in place.
    editing: bool,
    /// The tuning dialog, while it is open.
    tuning: Option<tuning::TuningDialog>,
}

/// A snapshot of the shown track, taken before drawing so the song is not borrowed while
/// the widgets run.
struct TrackPanel {
    name: String,
    color: TrackColor,
    kind: InstrumentKind,
    instrument_name: Option<String>,
    sound: Option<TrackSound>,
    tuning: Vec<StringTuning>,
    tuning_name: Option<String>,
    capo: u8,
    string_count: usize,
    percussion: bool,
    staves: TrackStaves,
    instrument: InstrumentSymbol,
}

impl TrackPanel {
    fn of(song: &Song, track: &TrackView<'_>) -> Self {
        TrackPanel {
            name: track.name().to_owned(),
            color: track.color(),
            kind: track.instrument_kind(),
            instrument_name: track.instrument_name().map(str::to_owned),
            sound: song.track_sound(track.index()),
            tuning: track.tuning().ok().flatten().unwrap_or_default(),
            tuning_name: track.tuning_name().ok().flatten(),
            capo: track.capo().unwrap_or(0),
            string_count: track.string_count(),
            percussion: track.is_percussion(),
            staves: track.staves(),
            instrument: instrument::derived(track, song.track_sound(track.index())),
        }
    }
}

impl GuitarScoreApp {
    /// The right panel: the shown track's settings.
    pub(super) fn track_panel(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &tab.document;
        let index = tab.track;
        let Some(track) = document.song().track(TrackIndex::new(index)) else {
            return;
        };
        let snapshot = TrackPanel::of(document.song(), &track);
        if tab.track_panel.track != Some(index) {
            tab.track_panel.track = Some(index);
            tab.track_panel.name.clone_from(&snapshot.name);
            tab.track_panel.editing = false;
        }

        let mut commands: Vec<(Command, bool)> = Vec::new();
        let mut open_tuning = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                self.header_row(ui, index, &snapshot, &mut commands);
                style::section(ui, "track_instrument", "Instrument", |ui| {
                    instrument(ui, index, &snapshot, &mut commands);
                    sound(ui, index, &snapshot, &mut commands);
                    tuning::button(ui, &snapshot, &mut open_tuning);
                });
                style::section(ui, "track_look", "Appearance", |ui| {
                    look::color(ui, index, &snapshot, &mut commands);
                    look::staves(ui, index, &snapshot, &mut commands);
                });
            });
        for (command, merge) in commands {
            self.apply_command(command, merge);
        }
        if open_tuning && let Some(tab) = self.tabs.get_mut(self.active) {
            tab.track_panel.tuning = Some(tuning::TuningDialog::of(index, &snapshot));
        }
    }

    /// The tuning dialog, while it is open.
    pub(super) fn show_tuning_dialog(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(mut dialog) = tab.track_panel.tuning.take() else {
            return;
        };
        let mut outcome = dialog.show(ctx);
        if outcome == tuning::Outcome::Apply {
            let track = TrackIndex::new(dialog.track());
            let song = &tab.document.song();
            // The strings that would go must carry no notes; the model says whether.
            if dialog.strings() != song.track(track).map_or(0, |track| track.string_count())
                && let Err(error) = song.check_track_strings(track, dialog.strings())
            {
                dialog.set_error(Some(error.to_string()));
                outcome = tuning::Outcome::Open;
            } else {
                dialog.set_error(None);
            }
        }
        match outcome {
            tuning::Outcome::Apply => {
                let commands = dialog.commands();
                self.apply_commands(commands);
            }
            tuning::Outcome::Cancel => {}
            tuning::Outcome::Remove | tuning::Outcome::Listen => {
                tab.track_panel.tuning = Some(dialog);
            }
            tuning::Outcome::Open => {
                tab.track_panel.tuning = Some(dialog);
            }
        }
    }

    /// The first line: the track's color, its instrument and its name. A click on the
    /// name edits it in place; Enter (or clicking elsewhere) renames, Escape cancels.
    fn header_row(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        snapshot: &TrackPanel,
        commands: &mut Vec<(Command, bool)>,
    ) {
        let tokens = style::tokens(ui);
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let state = &mut tab.track_panel;
        let mut rename_requested = false;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (swatch, _) =
                        ui.allocate_exact_size(egui::vec2(6.0, 22.0), egui::Sense::hover());
                    let (red, green, blue) = snapshot.color.parts();
                    ui.painter().rect_filled(
                        swatch,
                        2.0,
                        egui::Color32::from_rgb(red, green, blue),
                    );
                    let (icon, response) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    instrument::paint(
                        ui.painter(),
                        icon.shrink(2.0),
                        snapshot.instrument,
                        tokens.text,
                    );
                    response.on_hover_text(snapshot.instrument.name());
                    if state.editing {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut state.name)
                                .hint_text("Track name")
                                .font(egui::TextStyle::Heading)
                                .desired_width(f32::INFINITY),
                        );
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_label("Name field");
                        });
                        if !response.has_focus() && !response.lost_focus() {
                            response.request_focus();
                        }
                        let cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
                        if cancel {
                            state.name.clone_from(&snapshot.name);
                            state.editing = false;
                        } else if response.lost_focus() {
                            state.editing = false;
                            if state.name != snapshot.name && !state.name.trim().is_empty() {
                                commands.push((
                                    Command::RenameTrack {
                                        track: TrackIndex::new(index),
                                        name: state.name.trim().to_owned(),
                                    },
                                    false,
                                ));
                            } else {
                                state.name.clone_from(&snapshot.name);
                            }
                        }
                    } else {
                        let response = ui
                            .add(
                                egui::Label::new(
                                    RichText::new(&snapshot.name)
                                        .text_style(egui::TextStyle::Heading)
                                        .color(tokens.text),
                                )
                                .truncate()
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("Click to rename")
                            .on_hover_cursor(egui::CursorIcon::Text);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, "Edit track name")
                        });
                        if response.clicked() {
                            state.name.clone_from(&snapshot.name);
                            state.editing = true;
                        }
                    }
                    if !state.editing
                        && icons::button(ui, Icon::Rename, "Rename track", "Rename this track")
                            .clicked()
                    {
                        rename_requested = true;
                    }
                });
            });
        if rename_requested && let Some(tab) = self.tabs.get_mut(self.active) {
            tab.renaming = Some(super::grid::RenameTrack {
                track: index,
                name: snapshot.name.clone(),
                focused: false,
                pos: None,
            });
        }
        let rect = ui.max_rect();
        ui.painter().hline(
            rect.x_range(),
            ui.cursor().top(),
            egui::Stroke::new(1.0, tokens.line),
        );
    }
}

/// The instrument: what kind of track it is, and whether it is a drum kit.
fn instrument(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    let kind = match snapshot.kind {
        InstrumentKind::Strings => "String instrument (tablature)",
        InstrumentKind::Drums => "Drum kit",
        InstrumentKind::Other => "Other instrument",
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("Instrument").color(style::tokens(ui).text_weak));
        let label = ui.label(kind);
        if let Some(name) = &snapshot.instrument_name {
            label.on_hover_text(name);
        }
    });
    // The symbol shown for the track (the grid, the tab and this panel).
    let mut symbol = snapshot.instrument;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Symbol").color(style::tokens(ui).text_weak));
        let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
        instrument::paint(ui.painter(), rect, symbol, style::tokens(ui).text);
        let combo = egui::ComboBox::from_id_salt("track_symbol")
            .selected_text(symbol.name())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for candidate in InstrumentSymbol::ALL {
                    let chosen = ui
                        .horizontal(|ui| {
                            let (icon, _) = ui
                                .allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                            instrument::paint(
                                ui.painter(),
                                icon,
                                candidate,
                                ui.visuals().text_color(),
                            );
                            ui.selectable_value(&mut symbol, candidate, candidate.name())
                        })
                        .inner;
                    if chosen.clicked() {
                        ui.close();
                    }
                }
            });
        ui.ctx().accesskit_node_builder(combo.response.id, |node| {
            node.set_label("Instrument symbol");
        });
        combo.response.on_hover_text(
            "The symbol the track is shown with. It is only the symbol: the sound comes \
             from the preset.",
        );
    });
    if symbol != snapshot.instrument {
        commands.push((
            Command::SetTrackSymbol {
                track: TrackIndex::new(index),
                symbol,
            },
            false,
        ));
    }
    let mut drums = snapshot.percussion;
    if ui.checkbox(&mut drums, "Drum kit").changed() && drums != snapshot.percussion {
        commands.push((
            Command::SetTrackDrums {
                track: TrackIndex::new(index),
                drums,
            },
            false,
        ));
    }
}

/// The sound: the General MIDI program, which is the preset the loaded SoundFont plays.
fn sound(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    let mut program = snapshot.sound.map_or(25, |sound| sound.program);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Preset").color(style::tokens(ui).text_weak));
        ui.add_enabled_ui(!snapshot.percussion, |ui| {
            let combo = egui::ComboBox::from_id_salt("track_program")
                .selected_text(format!("{program}: {}", program_name(program)))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for candidate in 0..=127_u8 {
                        ui.selectable_value(
                            &mut program,
                            candidate,
                            format!("{candidate}: {}", program_name(candidate)),
                        );
                    }
                });
            ui.ctx().accesskit_node_builder(combo.response.id, |node| {
                node.set_label("SoundFont preset");
            });
            combo.response.on_hover_text(
                "The SoundFont preset (General MIDI program) the built-in synthesizer plays \
                 for this track; a drum kit plays the percussion bank",
            );
        });
    });
    if snapshot.percussion {
        ui.label(RichText::new("A drum kit plays the drum presets").weak());
    }
    if program != snapshot.sound.map_or(25, |sound| sound.program) {
        commands.push((
            Command::SetTrackProgram {
                track: TrackIndex::new(index),
                program,
            },
            false,
        ));
    }
}
