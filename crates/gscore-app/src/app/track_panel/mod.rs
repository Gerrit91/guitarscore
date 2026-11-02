//! The track panel (the right panel): a bar of track actions at the top, the shown track's
//! name, instrument, sound, color, staves, tuning, capo and string count, and the master
//! volume and pan at the bottom. Every change is a [`Command`] with undo, and sound changes
//! are heard at once (the sound commands go to the player while it plays).

mod look;
mod mix;
mod tuning;

use egui::RichText;
use gscore_edit::Command;
use gscore_model::{
    InstrumentKind, MasterMix, Song, StringTuning, TrackColor, TrackIndex, TrackSound, TrackStaves,
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
    staves: TrackStaves,
    /// How many tracks the song has.
    tracks: usize,
    /// Whether every track is muted, and whether every track is soloed.
    all_muted: bool,
    all_soloed: bool,
    master: MasterMix,
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
            tracks: song.track_count(),
            all_muted: song.tracks().all(|track| track.is_muted()),
            all_soloed: song.tracks().all(|track| track.is_soloed()),
            master: song.master_mix(),
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
        let mut actions = mix::BarActions::default();
        egui::Panel::top("track_panel_bar").show(ui, |ui| {
            mix::action_bar(ui, index, &snapshot, &mut actions);
        });
        egui::Panel::bottom("track_panel_master").show(ui, |ui| {
            mix::master_row(ui, &snapshot, &mut actions);
        });
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.heading(format!("Track {}", index.saturating_add(1)));
                self.name_editor(ui, index, &snapshot, &mut commands);
                instrument(ui, index, &snapshot, &mut commands);
                sound(ui, index, &snapshot, &mut commands);
                look::color(ui, index, &snapshot, &mut commands);
                look::staves(ui, index, &snapshot, &mut commands);
                tuning::tuning(ui, index, &snapshot, &mut commands);
            });
        for (command, merge) in commands {
            self.apply_command(command, merge);
        }
        self.apply_bar_actions(actions);
    }

    /// Runs what the action bar and the master row asked for. A track that was added or
    /// moved is shown afterwards.
    fn apply_bar_actions(&mut self, actions: mix::BarActions) {
        if actions.break_merge
            && let Some(document) = &mut self.document
        {
            document.editor_mut().break_merge();
        }
        let Some((command, merge)) = actions.command else {
            return;
        };
        let revision = self.document.as_ref().map(|d| d.editor().revision());
        self.apply_command(command, merge);
        let applied = self.document.as_ref().map(|d| d.editor().revision()) != revision;
        if applied && let Some(track) = actions.show_track {
            self.view.track = track;
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
