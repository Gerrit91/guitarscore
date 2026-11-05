//! The staff half of the edit palette: tiles for a bar's repeat signs, double barline,
//! line and page breaks, tempo, time and key signature, section marker and bars per line.
//!
//! Each tile opens a small dialog. OK turns the dialog's drafts into one ordinary command,
//! so every change is undoable and recorded in the recovery journal.

use egui::{Align, Layout, Margin, RichText, vec2};
use gscore_edit::{ActionId, Command};
use gscore_model::{
    Clef, Feel, Fermata, MeasureIndex, Repeat, Section, SystemBreak, TrackColor, TrackIndex,
};

use super::edit_palette::PaletteSnapshot;
use super::edit_staff_dialogs::{body, title, width};
use super::metronome;
use super::style;
use super::tiles::{self, Face};

/// The dialog a staff tile opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dialog {
    Tempo,
    Time,
    Key,
    Clef,
    FreeTime,
    Section,
    RepeatStart,
    RepeatEnd,
    Endings,
    Feel,
    Fermata,
    Barline,
    Break,
    Bars,
}

/// The bar (and track) a dialog was opened on.
#[derive(Debug, Clone, Copy)]
pub(super) struct Target {
    pub(super) dialog: Dialog,
    pub(super) measure: MeasureIndex,
    pub(super) track: TrackIndex,
}

/// What the staff tiles keep between frames.
#[derive(Debug)]
pub(super) struct StaffState {
    /// The open dialog and where it applies.
    open: Option<Target>,
    /// Where the dialog window sits (remembered while it is open).
    pos: Option<egui::Pos2>,
    pub(super) tempo: u32,
    pub(super) numerator: u8,
    pub(super) denominator: u16,
    pub(super) key: i8,
    pub(super) minor: bool,
    pub(super) clef: Clef,
    pub(super) free_time: bool,
    pub(super) section_title: String,
    pub(super) section_color: TrackColor,
    pub(super) repeat_start: bool,
    pub(super) repeat_end: bool,
    pub(super) repeat_count: u8,
    pub(super) endings: u8,
    pub(super) double_bar: bool,
    pub(super) system_break: SystemBreak,
    pub(super) bars_fixed: bool,
    pub(super) bars: u8,
    pub(super) feel: Feel,
    pub(super) fermata: Option<Fermata>,
    /// When "Tap tempo" was clicked (seconds), the last few clicks.
    taps: Vec<f64>,
    /// When the tempo dialog's metronome was started (seconds), while it swings.
    pub(super) swinging: Option<f64>,
    /// Why OK is disabled, if the drafts cannot become a command.
    pub(super) error: Option<String>,
}

impl Default for StaffState {
    fn default() -> Self {
        StaffState {
            open: None,
            pos: None,
            tempo: 120,
            numerator: 4,
            denominator: 4,
            key: 0,
            minor: false,
            clef: Clef::Treble,
            free_time: false,
            section_title: String::new(),
            section_color: Section::DEFAULT_COLOR,
            repeat_start: false,
            repeat_end: false,
            repeat_count: 1,
            endings: 0,
            double_bar: false,
            system_break: SystemBreak::None,
            bars_fixed: false,
            bars: 4,
            feel: Feel::None,
            fermata: None,
            taps: Vec::new(),
            swinging: None,
            error: None,
        }
    }
}

impl StaffState {
    /// Records a tap-tempo click at `time` (seconds); see [`metronome::tap_tempo`].
    pub(super) fn tap(&mut self, time: f64) -> Option<u32> {
        metronome::tap_tempo(&mut self.taps, time)
    }

    /// Whether a staff dialog is open.
    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Takes the drafts from `snapshot` and opens `dialog` on the cursor's bar.
    fn open_dialog(&mut self, dialog: Dialog, snapshot: &PaletteSnapshot) {
        self.open = Some(Target {
            dialog,
            measure: snapshot.measure,
            track: snapshot.cursor.track,
        });
        self.pos = None;
        self.error = None;
        self.taps.clear();
        self.tempo = snapshot.tempo.unwrap_or(120);
        let (numerator, denominator) = snapshot.time_signature.map_or((4, 4), |signature| {
            (signature.numerator(), signature.denominator())
        });
        self.numerator = numerator;
        self.denominator = denominator;
        let (key, minor) = snapshot
            .key
            .map_or((0, false), |signature| (signature.key, signature.minor));
        self.key = key;
        self.minor = minor;
        self.clef = snapshot.clef;
        self.free_time = snapshot.free_time;
        self.section_title = snapshot
            .section
            .as_ref()
            .map_or_else(String::new, |section| section.title.clone());
        self.section_color = snapshot
            .section
            .as_ref()
            .map_or(Section::DEFAULT_COLOR, |section| section.color);
        self.repeat_start = snapshot.repeat.start;
        self.repeat_end = snapshot.repeat.end_repeats.is_some();
        self.repeat_count = snapshot.repeat.end_repeats.unwrap_or(1).max(1);
        self.endings = snapshot.repeat.endings;
        self.double_bar = snapshot.double_bar;
        // The first bar always starts the score, so it cannot carry a break.
        self.system_break = if snapshot.measure.get() == 0 {
            SystemBreak::None
        } else {
            snapshot.system_break
        };
        self.bars_fixed = snapshot.bars_per_system.is_some();
        self.bars = snapshot.bars_per_system.unwrap_or(4);
        self.feel = snapshot.feel;
        self.fermata = snapshot.fermata;
    }

    /// Draws the dialog while one is open; OK pushes one command.
    pub(super) fn show_dialog(&mut self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        let Some(target) = self.open else {
            return;
        };
        let title = title(target.dialog);
        let content_width = width(target.dialog);
        let size = vec2(content_width + 32.0, 300.0);
        let pos = *self
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut self.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(content_width);
                        body(ui, self, target);
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 30.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                let button =
                                    |text: &str| egui::Button::new(text).min_size(vec2(84.0, 28.0));
                                if ui.add(button("Cancel")).clicked() {
                                    outcome = Outcome::Cancel;
                                }
                                let tokens = style::tokens(ui);
                                let ok =
                                    egui::Button::new(RichText::new("OK").color(tokens.on_accent))
                                        .fill(tokens.accent)
                                        .min_size(vec2(84.0, 28.0));
                                if ui.add_enabled(self.error.is_none(), ok).clicked() {
                                    outcome = Outcome::Ok;
                                }
                            },
                        );
                        if outcome == Outcome::Open {
                            ui.input(|input| {
                                if input.key_pressed(egui::Key::Escape) {
                                    outcome = Outcome::Cancel;
                                } else if input.key_pressed(egui::Key::Enter)
                                    && self.error.is_none()
                                {
                                    outcome = Outcome::Ok;
                                }
                            });
                        }
                    });
            });
        match outcome {
            Outcome::Ok => {
                if let Some(command) = self.command(target) {
                    commands.push(command);
                }
                self.open = None;
            }
            Outcome::Cancel => self.open = None,
            Outcome::Open => {}
        }
    }

    /// The command the dialog's drafts describe.
    fn command(&self, target: Target) -> Option<Command> {
        Some(match target.dialog {
            Dialog::Tempo => Command::SetMeasureTempo {
                measure: target.measure,
                tempo: self.tempo,
            },
            Dialog::Time => Command::SetTimeSignature {
                measure: target.measure,
                numerator: self.numerator,
                denominator: self.denominator,
            },
            Dialog::Key => Command::SetKeySignature {
                measure: target.measure,
                key: self.key,
                minor: self.minor,
            },
            Dialog::Clef => Command::SetClef {
                track: target.track,
                measure: target.measure,
                clef: self.clef,
            },
            Dialog::FreeTime => Command::SetFreeTime {
                measure: target.measure,
                free_time: self.free_time,
            },
            Dialog::Section => {
                let title = self.section_title.trim();
                if title.is_empty() {
                    Command::ClearSection {
                        measure: target.measure,
                    }
                } else {
                    Command::SetSection {
                        measure: target.measure,
                        title: title.to_owned(),
                        color: self.section_color,
                    }
                }
            }
            Dialog::RepeatStart | Dialog::RepeatEnd | Dialog::Endings => Command::SetRepeat {
                measure: target.measure,
                repeat: Repeat {
                    start: self.repeat_start,
                    end_repeats: self.repeat_end.then_some(self.repeat_count),
                    endings: self.endings,
                },
            },
            Dialog::Feel => Command::SetFeel {
                measure: target.measure,
                feel: self.feel,
            },
            Dialog::Fermata => Command::SetFermata {
                measure: target.measure,
                fermata: self.fermata,
            },
            Dialog::Barline => Command::SetDoubleBar {
                measure: target.measure,
                double_bar: self.double_bar,
            },
            Dialog::Break => Command::SetSystemBreak {
                track: target.track,
                measure: target.measure,
                kind: self.system_break,
            },
            Dialog::Bars => Command::SetBarsPerSystem {
                track: target.track,
                bars: self.bars_fixed.then_some(self.bars),
            },
        })
    }
}

/// What happened to a dialog this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Open,
    Ok,
    Cancel,
}

/// The Staff section: icon tiles, each opening a dialog. Returns the palette action of a
/// tile that has no staff dialog of its own (the beat text editor is one of the effects).
pub(super) fn section(
    ui: &mut egui::Ui,
    state: &mut StaffState,
    snapshot: &PaletteSnapshot,
    hint: &dyn Fn(ActionId) -> String,
) -> Option<ActionId> {
    let mut action = None;
    style::section(ui, "staff", "Staff", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
            let items = [
                (
                    Dialog::Tempo,
                    Face::Tempo,
                    "Tempo",
                    "Set the tempo this bar starts at",
                    snapshot.tempo.is_some(),
                ),
                (
                    Dialog::Time,
                    Face::TimeSig,
                    "Time signature",
                    "Set the bar's time signature",
                    false,
                ),
                (
                    Dialog::Key,
                    Face::KeySig,
                    "Key signature",
                    "Set the bar's key signature",
                    snapshot.key.is_some_and(|key| key.key != 0 || key.minor),
                ),
                (
                    Dialog::Clef,
                    Face::GlyphMore {
                        text: "\u{E050}",
                        size: 30.0,
                        baseline: 9.0,
                    },
                    "Clef",
                    "Set the clef of the notation staff",
                    snapshot.clef != Clef::Treble,
                ),
                (
                    Dialog::FreeTime,
                    Face::TextMore("free"),
                    "Free time",
                    "No metronome for this bar",
                    snapshot.free_time,
                ),
                (
                    Dialog::Section,
                    Face::Section,
                    "Section",
                    "Name the section that starts here",
                    snapshot.section.is_some(),
                ),
                (
                    Dialog::RepeatStart,
                    Face::RepeatStart,
                    "Start repeat sign",
                    "Start a repeat at this bar",
                    snapshot.repeat.start,
                ),
                (
                    Dialog::RepeatEnd,
                    Face::RepeatEnd,
                    "End repeat sign",
                    "End a repeat at this bar",
                    snapshot.repeat.end_repeats.is_some(),
                ),
                (
                    Dialog::Endings,
                    Face::Endings,
                    "Alternative endings",
                    "Mark this bar as an alternative ending",
                    snapshot.repeat.endings != 0,
                ),
                (
                    Dialog::Feel,
                    Face::Swing,
                    "Swing feel",
                    "Set the bar's swing or dotted feel",
                    snapshot.feel != Feel::None,
                ),
                (
                    Dialog::Fermata,
                    Face::Fermata,
                    "Fermata",
                    "Put a fermata on this bar",
                    snapshot.fermata.is_some(),
                ),
                (
                    Dialog::Barline,
                    Face::DoubleBar,
                    "Barline",
                    "End this bar with a double barline",
                    snapshot.double_bar,
                ),
                (
                    Dialog::Break,
                    Face::Break,
                    "Break",
                    "Start a new line or page here",
                    snapshot.system_break != SystemBreak::None,
                ),
                (
                    Dialog::Bars,
                    Face::Bars,
                    "Bars per line…",
                    "How many bars a line holds",
                    snapshot.bars_per_system.is_some(),
                ),
            ];
            for (dialog, face, label, tip, on) in items {
                if tiles::dialog_tile(ui, face, on, true, label, tip).clicked() {
                    state.open_dialog(dialog, snapshot);
                }
            }
            // Beat text has its own editor rather than a staff dialog, but it is a dialog
            // tile like the others (and lives here, next to the other bar-level marks).
            let tip = format!(
                "Beat text: text written above the beat, like Guitar Pro's beat text{}",
                hint(ActionId::EditBeatText)
            );
            let beat_text = tiles::dialog_tile(
                ui,
                Face::BeatText,
                !snapshot.beat_text.is_empty(),
                true,
                "Beat text…",
                &tip,
            );
            if beat_text.clicked() {
                action = Some(ActionId::EditBeatText);
            }
        });
    });
    action
}
