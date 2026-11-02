//! The track panel (the right panel): a track's name, instrument, sound, color, tuning,
//! capo and string count. Every change is a [`Command`] with undo, and sound changes are
//! heard at once (the sound commands go to the player while it plays).

use egui::RichText;
use gscore_edit::Command;
use gscore_model::{
    InstrumentKind, MAX_STRINGS, Pitch, Song, StringTuning, TrackColor, TrackIndex, TrackSound,
    TrackView, program_name,
};

use super::GuitarScoreApp;

/// What the track panel keeps between frames: which track it showed, and the name being
/// typed (so a rename is one undo step, not one per keystroke).
#[derive(Debug, Default)]
pub(super) struct TrackPanelState {
    /// The track the draft belongs to.
    track: Option<usize>,
    /// The name in the text field.
    name: String,
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
        }
    }
}

impl GuitarScoreApp {
    /// The right panel: the shown track's settings.
    pub(super) fn track_panel(&mut self, ui: &mut egui::Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let index = self.view.track;
        let Some(track) = document.song().track(TrackIndex::new(index)) else {
            return;
        };
        let snapshot = TrackPanel::of(document.song(), &track);
        if self.track_panel.track != Some(index) {
            self.track_panel.track = Some(index);
            self.track_panel.name.clone_from(&snapshot.name);
        }

        let mut commands: Vec<(Command, bool)> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.heading(format!("Track {}", index.saturating_add(1)));
                self.name_editor(ui, index, &snapshot, &mut commands);
                instrument(ui, index, &snapshot, &mut commands);
                sound(ui, index, &snapshot, &mut commands);
                color(ui, index, &snapshot, &mut commands);
                tuning(ui, index, &snapshot, &mut commands);
            });
        for (command, merge) in commands {
            self.apply_command(command, merge);
        }
    }

    /// The name field and its Rename button.
    fn name_editor(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        snapshot: &TrackPanel,
        commands: &mut Vec<(Command, bool)>,
    ) {
        let mut entered = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.track_panel.name)
                    .hint_text("Track name")
                    .desired_width(f32::INFINITY),
            );
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Name field"));
            entered =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        });
        let changed = self.track_panel.name != snapshot.name && !self.track_panel.name.is_empty();
        if ui
            .add_enabled(changed, egui::Button::new("Rename track"))
            .clicked()
            || (changed && entered)
        {
            commands.push((
                Command::RenameTrack {
                    track: TrackIndex::new(index),
                    name: self.track_panel.name.clone(),
                },
                false,
            ));
        }
    }
}

/// The instrument: what kind of track it is, and whether it is a drum kit.
fn instrument(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    ui.separator();
    let kind = match snapshot.kind {
        InstrumentKind::Strings => "String instrument (tablature)",
        InstrumentKind::Drums => "Drum kit",
        InstrumentKind::Other => "Other instrument",
    };
    ui.horizontal(|ui| {
        ui.strong("Instrument");
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

/// The sound: the General MIDI program, which is the preset the loaded SoundFont plays.
fn sound(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    ui.separator();
    let mut program = snapshot.sound.map_or(25, |sound| sound.program);
    ui.horizontal(|ui| {
        ui.strong("Sound");
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

/// The track color.
fn color(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    ui.separator();
    let (red, green, blue) = snapshot.color.parts();
    let mut rgb = [red, green, blue];
    let (mut changed, mut dragged) = (false, false);
    ui.horizontal(|ui| {
        ui.strong("Color");
        let response = ui.color_edit_button_srgb(&mut rgb);
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Track color"));
        changed = response.changed();
        dragged = response.dragged();
    });
    let [red, green, blue] = rgb;
    let color =
        TrackColor::new(u32::from(red) * 0x1_0000 + u32::from(green) * 0x100 + u32::from(blue));
    if changed && color != snapshot.color {
        // A color-picker drag is one undo step, like a slider drag.
        commands.push((
            Command::SetTrackColor {
                track: TrackIndex::new(index),
                color,
            },
            dragged,
        ));
    }
}

/// The tuning: string count, capo, and each string's open pitch.
fn tuning(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.strong("Tuning");
        if let Some(name) = &snapshot.tuning_name {
            ui.label(RichText::new(name).weak());
        }
    });
    let mut strings = snapshot.string_count;
    let mut capo = snapshot.capo;
    ui.horizontal(|ui| {
        ui.label("Strings");
        let response = ui.add(
            egui::DragValue::new(&mut strings)
                .range(1..=MAX_STRINGS)
                .speed(1)
                .update_while_editing(false),
        );
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Strings"));
        ui.add_space(8.0);
        ui.label("Capo");
        let response = ui.add(
            egui::DragValue::new(&mut capo)
                .range(0..=127)
                .speed(1)
                .update_while_editing(false),
        );
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Capo"));
        if capo == 0 {
            ui.label(RichText::new("none").weak());
        }
    });
    if strings != snapshot.string_count {
        commands.push((
            Command::SetTrackStrings {
                track: TrackIndex::new(index),
                strings,
            },
            false,
        ));
    }
    if capo != snapshot.capo {
        commands.push((
            Command::SetTrackCapo {
                track: TrackIndex::new(index),
                capo,
            },
            false,
        ));
    }
    if snapshot.tuning.is_empty() {
        ui.label(RichText::new("This track has no tuning").weak());
    }
    for string in &snapshot.tuning {
        let mut pitch = string.pitch.get();
        ui.horizontal(|ui| {
            ui.label(format!("String {}", string.string.get()));
            let response = ui.add(
                egui::DragValue::new(&mut pitch)
                    .range(0..=127)
                    .speed(1)
                    .update_while_editing(false),
            );
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(format!("String {} pitch", string.string.get()));
            });
            ui.label(pitch_name(pitch));
        });
        if pitch != string.pitch.get()
            && let Ok(pitch) = Pitch::new(pitch)
        {
            commands.push((
                Command::SetTrackStringPitch {
                    track: TrackIndex::new(index),
                    string: string.string,
                    pitch,
                },
                false,
            ));
        }
    }
}

/// The name of a MIDI pitch, or the number if it is somehow out of range.
fn pitch_name(pitch: u8) -> String {
    Pitch::new(pitch).map_or_else(|_| pitch.to_string(), |pitch| pitch.to_string())
}
