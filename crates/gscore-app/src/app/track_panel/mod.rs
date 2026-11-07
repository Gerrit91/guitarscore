//! The track panel (the right panel): a track's name, instrument, sound, color, staves,
//! tuning, capo and string count. Every change is a [`Command`] with undo, and sound changes
//! are heard at once (the sound commands go to the player while it plays).

pub(super) mod look;
mod tuning;

use egui::RichText;
use gscore_edit::Command;
use gscore_model::{
    InstrumentKind, InstrumentSymbol, Song, StringTuning, TrackColor, TrackIndex, TrackStaves,
    TrackView,
};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::instrument;
use super::style;
use crate::chain_settings::Tone;

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
    /// The Sound section's picker (its filter and scroll position).
    pub(super) sound_picker: super::tone_picker::TonePickerState,
}

impl TrackPanelState {
    /// Whether the tuning dialog is open.
    pub(super) fn tuning_open(&self) -> bool {
        self.tuning.is_some()
    }
}

/// A snapshot of the shown track, taken before drawing so the song is not borrowed while
/// the widgets run.
struct TrackPanel {
    name: String,
    color: TrackColor,
    kind: InstrumentKind,
    instrument_name: Option<String>,
    tuning: Vec<StringTuning>,
    /// Whether the notes have pitches to write as standard notation (also a voice's).
    has_pitches: bool,
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
            tuning: track.tuning().ok().flatten().unwrap_or_default(),
            has_pitches: track.note_tuning().ok().flatten().is_some(),
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
        let Some(index) = self.tabs.get(self.active).map(|tab| tab.track) else {
            return;
        };
        // The sound's player, tone and picker rows are app state: read them before the
        // document is borrowed for the widgets.
        let sound_panel = self.sound_panel(index);
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &tab.document;
        let Some(track) = document.song().track(TrackIndex::new(index)) else {
            return;
        };
        let snapshot = TrackPanel::of(document.song(), &track);
        if tab.track_panel.track != Some(index) {
            tab.track_panel.track = Some(index);
            tab.track_panel.name.clone_from(&snapshot.name);
            tab.track_panel.editing = false;
        }
        // Taken out so the widgets (which borrow the app for their commands) do not hold
        // the tab; it goes back below.
        let mut sound_picker = std::mem::take(&mut tab.track_panel.sound_picker);

        let mut commands: Vec<(Command, bool)> = Vec::new();
        let mut open_tuning = false;
        let mut open_sound = false;
        let mut sound_actions = SoundActions::default();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                self.header_row(ui, index, &snapshot, &mut commands);
                style::section(ui, "track_instrument", "Instrument", |ui| {
                    instrument(ui, index, &snapshot, &mut commands);
                    tuning::button(ui, &snapshot, &mut open_tuning);
                });
                style::section(ui, "track_look", "Appearance", |ui| {
                    look::staves(ui, index, &snapshot, &mut commands);
                });
                style::section(ui, "track_sound", "Sound", |ui| {
                    sound(
                        ui,
                        index,
                        sound_panel.as_ref(),
                        &mut commands,
                        &mut sound_picker,
                        &mut open_sound,
                        &mut sound_actions,
                    );
                });
            });
        for (command, merge) in commands {
            self.apply_command(command, merge);
        }
        if let Some(engine) = sound_actions.engine {
            self.set_track_engine(index, engine);
            // Another table (programs or tones) is shown now: a filter typed for the old
            // one would hide everything.
            sound_picker = super::tone_picker::TonePickerState::new();
        }
        if let Some(route) = sound_actions.midi_out {
            self.set_midi_out(index, route);
        }
        if let Some(tone) = sound_actions.tone {
            self.choose_track_tone(index, tone);
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.track_panel.sound_picker = sound_picker;
        }
        if open_sound {
            self.open_sound_chain(index);
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
                    let (swatch, swatch_response) =
                        ui.allocate_exact_size(egui::vec2(8.0, 22.0), egui::Sense::click());
                    let (red, green, blue) = snapshot.color.parts();
                    ui.painter().rect_filled(
                        swatch,
                        2.0,
                        egui::Color32::from_rgb(red, green, blue),
                    );
                    ui.ctx().accesskit_node_builder(swatch_response.id, |node| {
                        node.set_label("Track color");
                        node.set_role(egui::Role::Button);
                    });
                    swatch_response
                        .clone()
                        .on_hover_text("Track color — click to change")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    egui::Popup::menu(&swatch_response).show(|ui| {
                        ui.set_min_width(230.0);
                        if look::color(ui, index, snapshot, commands) {
                            ui.close();
                        }
                    });

                    let (icon, icon_response) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
                    instrument::paint(
                        ui.painter(),
                        icon.shrink(2.0),
                        snapshot.instrument,
                        tokens.text,
                    );
                    ui.ctx().accesskit_node_builder(icon_response.id, |node| {
                        node.set_label("Instrument symbol");
                        node.set_role(egui::Role::Button);
                    });
                    icon_response
                        .clone()
                        .on_hover_text(format!(
                            "{} — click to choose the symbol",
                            snapshot.instrument.name()
                        ))
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    let mut chosen = None;
                    egui::Popup::menu(&icon_response).show(|ui| {
                        chosen = symbol_picker(ui, snapshot.instrument);
                        if chosen.is_some() {
                            ui.close();
                        }
                    });
                    if let Some(symbol) = chosen {
                        commands.push((
                            Command::SetTrackSymbol {
                                track: TrackIndex::new(index),
                                symbol,
                            },
                            false,
                        ));
                    }
                    if state.editing {
                        // The field fills the room left in the header, its own margin
                        // included: a wider field would widen the whole panel.
                        let margin = egui::Margin::symmetric(4, 2);
                        let width = (ui.available_width() - margin.sum().x).max(40.0);
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut state.name)
                                .hint_text("Track name")
                                .font(egui::TextStyle::Heading)
                                .margin(margin)
                                .desired_width(width),
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
                        // The rename button takes its place at the right first, so a long
                        // name is shortened to the room left instead of widening the panel.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icons::button(ui, Icon::Rename, "Rename track", "Rename this track")
                                .clicked()
                            {
                                rename_requested = true;
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
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
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            "Edit track name",
                                        )
                                    });
                                    if response.clicked() {
                                        state.name.clone_from(&snapshot.name);
                                        state.editing = true;
                                    }
                                },
                            );
                        });
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

/// The grid of instrument symbols, for the picker under the header's instrument icon.
fn symbol_picker(ui: &mut egui::Ui, current: InstrumentSymbol) -> Option<InstrumentSymbol> {
    let mut chosen = None;
    ui.set_min_width(230.0);
    ui.set_max_width(230.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 3.0);
        for candidate in InstrumentSymbol::ALL {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(32.0, 30.0), egui::Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    candidate == current,
                    candidate.name(),
                )
            });
            let tokens = style::tokens(ui);
            let on = candidate == current;
            if on {
                ui.painter().rect_filled(rect, 4.0, tokens.accent);
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 4.0, tokens.control_hover);
            }
            let ink = if on { tokens.on_accent } else { tokens.text };
            instrument::paint(ui.painter(), rect.shrink(4.0), candidate, ink);
            if response.on_hover_text(candidate.name()).clicked() {
                chosen = Some(candidate);
            }
        }
    });
    chosen
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

/// What the Sound section asked for (applied after the panel's widgets ran).
#[derive(Debug, Default)]
struct SoundActions {
    engine: Option<gscore_play::TrackEngine>,
    midi_out: Option<crate::playback::MidiOutRoute>,
    /// A tone chosen in the library, for this track alone.
    tone: Option<Tone>,
}

/// The sound: what plays the track (one setting, shared with the Sound chain window) and,
/// below it, the matching picker: the General MIDI programs or drum kits for the MIDI
/// synth, our tone library for modelled strings, and the General MIDI programs again for a
/// hardware synthesizer (with its port and channel). The stomp box opens the Sound chain
/// window.
fn sound(
    ui: &mut egui::Ui,
    index: usize,
    panel: Option<&super::track_engine::SoundPanel>,
    commands: &mut Vec<(Command, bool)>,
    picker: &mut super::tone_picker::TonePickerState,
    open: &mut bool,
    actions: &mut SoundActions,
) {
    let Some(panel) = panel else {
        ui.label(RichText::new("This track has no sound settings").weak());
        return;
    };
    let player = &panel.player;
    ui.horizontal(|ui| {
        let pm = match player.engine {
            gscore_play::TrackEngine::Modelled(kind) => gscore_play::TrackEngine::Modelled(kind),
            gscore_play::TrackEngine::SoundFont | gscore_play::TrackEngine::MidiOut => {
                gscore_play::TrackEngine::Modelled(
                    player.preset_kind.unwrap_or(gscore_play::StringKind::Steel),
                )
            }
        };
        if let Some(engine) =
            super::tone_picker::source_switch(ui, player.engine, pm, player.strings)
        {
            actions.engine = Some(engine);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = icons::large_button(
                ui,
                Icon::StompBox,
                "Sound chain",
                "Sound chain: what plays this track, the strings' sound and the effects after \
                 them",
            );
            if button.clicked() {
                *open = true;
            }
        });
    });
    match player.engine {
        gscore_play::TrackEngine::Modelled(_) => {
            if !player.strings {
                ui.label(
                    RichText::new("Only string instruments can be played by modelled strings.")
                        .weak(),
                );
                return;
            }
            let rows = super::tone_picker::tone_rows(
                panel.current.as_ref(),
                Some(panel.default_tone.as_str()),
                &panel.saved,
            );
            if let Some(choice) = super::tone_picker::picker(ui, "track_tone_picker", &rows, picker)
                && let Some(tone) = tone_of_choice(choice, &panel.saved)
            {
                actions.tone = Some(tone);
            }
        }
        gscore_play::TrackEngine::SoundFont | gscore_play::TrackEngine::MidiOut => {
            if player.engine == gscore_play::TrackEngine::MidiOut
                && let Some(route) = super::sound_chain::midi_out_controls(ui, player, &panel.ports)
            {
                actions.midi_out = Some(route);
            }
            let rows = super::tone_picker::general_midi_rows(player.program, player.drums);
            if let Some(choice) =
                super::tone_picker::picker(ui, "track_program_picker", &rows, picker)
                && let Some(program) = choice_program(&choice)
            {
                commands.push((
                    Command::SetTrackProgram {
                        track: TrackIndex::new(index),
                        program,
                    },
                    false,
                ));
            }
        }
    }
}

/// The program a General MIDI or drum-kit row chooses.
fn choice_program(choice: &super::tone_picker::PickerChoice) -> Option<u8> {
    use super::tone_picker::PickerChoice;
    match choice {
        PickerChoice::Program(program) | PickerChoice::DrumKit(program) => Some(*program),
        PickerChoice::BuiltIn(_) | PickerChoice::Saved { .. } => None,
    }
}

/// The tone a picker row chooses, from the built-in library or the user's saved tones.
fn tone_of_choice(
    choice: super::tone_picker::PickerChoice,
    saved: &[crate::settings::ChainPreset],
) -> Option<Tone> {
    use super::tone_picker::PickerChoice;
    match choice {
        PickerChoice::BuiltIn(name) => {
            crate::tone_library::find(name).map(|tone| tone.tone.clone())
        }
        PickerChoice::Saved { kind, name } => {
            let preset = saved
                .iter()
                .find(|preset| preset.kind == kind && preset.name == name)?;
            Some(Tone {
                kind: crate::settings::kind_from_id(&preset.kind)?,
                sound: preset.sound,
                chain: preset.chain.clone(),
            })
        }
        PickerChoice::Program(_) | PickerChoice::DrumKit(_) => None,
    }
}
