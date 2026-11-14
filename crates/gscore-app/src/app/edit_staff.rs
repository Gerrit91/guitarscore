//! The staff half of the edit palette: tiles for a bar's repeat signs, double barline,
//! line and page breaks, tempo, time and key signature, section marker and bars per line.
//!
//! Most tiles open a small dialog; the bar repetitions, double barline and free time are
//! switches. OK turns the dialog's drafts into one ordinary command,
//! so every change is undoable and recorded in the recovery journal.

use egui::{Align, Layout, Margin, RichText, vec2};
use gscore_edit::{ActionId, Command};
use gscore_model::{
    Clef, Feel, Fermata, MeasureIndex, Repeat, Section, SimileMark, SystemBreak, TrackColor,
    TrackIndex,
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
    Section,
    RepeatStart,
    RepeatEnd,
    Endings,
    Feel,
    Fermata,
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
    pub(super) section_title: String,
    pub(super) section_color: TrackColor,
    pub(super) repeat_start: bool,
    pub(super) repeat_end: bool,
    pub(super) repeat_count: u8,
    pub(super) endings: u8,
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
            section_title: String::new(),
            section_color: Section::DEFAULT_COLOR,
            repeat_start: false,
            repeat_end: false,
            repeat_count: 1,
            endings: 0,
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
    pub(super) fn open_dialog(&mut self, dialog: Dialog, snapshot: &PaletteSnapshot) {
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

/// Whether a bar repetition can start at `measure` of `count`: a one-bar repeat needs a
/// bar before it, a two-bar repeat two bars before it and one after.
fn repeat_fits(measure: usize, count: usize, double: bool) -> bool {
    if double {
        measure >= 2 && measure.saturating_add(1) < count
    } else {
        measure >= 1 && measure < count
    }
}

/// The measures a mark on `measure` belongs with: itself, and the other half of a pair.
fn repeat_group(marks: &[Option<SimileMark>], measure: usize) -> Vec<usize> {
    match marks.get(measure).copied().flatten() {
        Some(SimileMark::FirstOfDouble) => vec![measure, measure.saturating_add(1)],
        Some(SimileMark::SecondOfDouble) => vec![measure.saturating_sub(1), measure],
        Some(SimileMark::Simple) => vec![measure],
        None => Vec::new(),
    }
}

/// The commands a click on the one-bar (or, when `double`, the two-bar) repetition tile
/// at `measure` makes: switch it off when it is on, else set it and clear whatever marks
/// it overlaps whole, so no half of a pair is left behind.
fn repeat_commands(
    track: TrackIndex,
    marks: &[Option<SimileMark>],
    measure: usize,
    double: bool,
) -> Vec<Command> {
    let set = |measure: usize, mark| Command::SetSimileMark {
        track,
        measure: MeasureIndex::new(measure),
        mark,
    };
    let current = marks.get(measure).copied().flatten();
    let on = match current {
        Some(SimileMark::Simple) => !double,
        Some(_) => double,
        None => false,
    };
    if on {
        return repeat_group(marks, measure)
            .into_iter()
            .map(|measure| set(measure, None))
            .collect();
    }
    if !repeat_fits(measure, marks.len(), double) {
        return Vec::new();
    }
    let new: Vec<(usize, SimileMark)> = if double {
        vec![
            (measure, SimileMark::FirstOfDouble),
            (measure.saturating_add(1), SimileMark::SecondOfDouble),
        ]
    } else {
        vec![(measure, SimileMark::Simple)]
    };
    let mut cleared: Vec<usize> = new
        .iter()
        .flat_map(|&(measure, _)| repeat_group(marks, measure))
        .filter(|cleared| !new.iter().any(|&(measure, _)| measure == *cleared))
        .collect();
    cleared.sort_unstable();
    cleared.dedup();
    cleared
        .into_iter()
        .map(|measure| set(measure, None))
        .chain(
            new.into_iter()
                .map(|(measure, mark)| set(measure, Some(mark))),
        )
        .collect()
}

/// The clef tile: a treble clef.
const CLEF_FACE: Face<'static> = Face::GlyphMore {
    text: "\u{E050}",
    size: 17.0,
    baseline: 4.0,
};

/// Every staff tile's face, for the test that they fit their tiles.
#[cfg(test)]
pub(super) fn faces() -> Vec<Face<'static>> {
    vec![
        CLEF_FACE,
        Face::KeySig,
        Face::TimeSig,
        Face::Tempo,
        Face::Section,
        Face::RepeatStart,
        Face::RepeatEnd,
        Face::Endings,
        Face::Swing,
        Face::Fermata,
        Face::DoubleBar,
        Face::Break,
        Face::Design,
        Face::Bars,
        Face::SimileSingle,
        Face::SimileDouble,
        Face::FreeTime,
        Face::BeatText,
    ]
}

/// One tile of the Staff section.
enum StaffTile<'a> {
    /// A tile that opens a staff dialog.
    Dialog {
        dialog: Dialog,
        face: Face<'a>,
        label: &'static str,
        tip: &'static str,
        on: bool,
    },
    BeatText,
    Lyrics,
    DoubleBar,
    FreeTime,
    /// Guitar Pro's design mode: a switch on the page's layout handles.
    Design,
    BarRepeat {
        double: bool,
    },
}

/// The Staff section: icon tiles, each opening a dialog. Returns the palette action of a
/// tile that has no staff dialog of its own (the beat text editor is one of the effects).
pub(super) fn section(
    ui: &mut egui::Ui,
    state: &mut StaffState,
    snapshot: &PaletteSnapshot,
    hint: &dyn Fn(ActionId) -> String,
    commands: &mut Vec<Command>,
    design_mode: &mut bool,
) -> Option<ActionId> {
    let mut action = None;
    style::section(ui, "staff", "Staff", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
            // The tiles in the order a bar is set up: its signs, its text, its barlines and
            // repeats, then the page.
            let dialog = |dialog, face, label, tip, on| StaffTile::Dialog {
                dialog,
                face,
                label,
                tip,
                on,
            };
            let order = [
                dialog(
                    Dialog::Clef,
                    CLEF_FACE,
                    "Clef",
                    "Set the clef of the notation staff",
                    snapshot.clef != Clef::Treble,
                ),
                dialog(
                    Dialog::Key,
                    Face::KeySig,
                    "Key signature",
                    "Set the bar's key signature",
                    snapshot.key.is_some_and(|key| key.key != 0 || key.minor),
                ),
                dialog(
                    Dialog::Time,
                    Face::TimeSig,
                    "Time signature",
                    "Set the bar's time signature",
                    false,
                ),
                dialog(
                    Dialog::Tempo,
                    Face::Tempo,
                    "Tempo",
                    "Set the tempo this bar starts at",
                    snapshot.tempo.is_some(),
                ),
                dialog(
                    Dialog::Feel,
                    Face::Swing,
                    "Swing feel",
                    "Set the bar's swing or dotted feel",
                    snapshot.feel != Feel::None,
                ),
                dialog(
                    Dialog::Section,
                    Face::Section,
                    "Section",
                    "Name the section that starts here",
                    snapshot.section.is_some(),
                ),
                StaffTile::BeatText,
                StaffTile::Lyrics,
                StaffTile::DoubleBar,
                StaffTile::FreeTime,
                dialog(
                    Dialog::RepeatStart,
                    Face::RepeatStart,
                    "Start repeat sign",
                    "Start a repeat at this bar",
                    snapshot.repeat.start,
                ),
                dialog(
                    Dialog::RepeatEnd,
                    Face::RepeatEnd,
                    "End repeat sign",
                    "End a repeat at this bar",
                    snapshot.repeat.end_repeats.is_some(),
                ),
                dialog(
                    Dialog::Endings,
                    Face::Endings,
                    "Alternative endings",
                    "Mark this bar as an alternative ending",
                    snapshot.repeat.endings != 0,
                ),
                StaffTile::BarRepeat { double: false },
                StaffTile::BarRepeat { double: true },
                dialog(
                    Dialog::Break,
                    Face::Break,
                    "Break",
                    "Start a new line or page here",
                    snapshot.system_break != SystemBreak::None,
                ),
                dialog(
                    Dialog::Bars,
                    Face::Bars,
                    "Bars per line…",
                    "How many bars a line holds",
                    snapshot.bars_per_system.is_some(),
                ),
                // Guitar Pro's design mode closes the row: the line break and the bars per
                // line are one pair, and the design mode is where a line is laid out by
                // hand.
                StaffTile::Design,
            ];
            let measure = snapshot.measure.get();
            let mark = snapshot.similes.get(measure).copied().flatten();
            for tile in order {
                match tile {
                    StaffTile::Dialog {
                        dialog,
                        face,
                        label,
                        tip,
                        on,
                    } => {
                        if tiles::dialog_tile(ui, face, on, true, label, tip).clicked() {
                            state.open_dialog(dialog, snapshot);
                        }
                    }
                    StaffTile::BeatText => {
                        // Beat text has its own editor rather than a staff dialog.
                        let tip = format!(
                            "Beat text: text written above the beat, like Guitar Pro's beat \
                             text{}",
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
                    }
                    StaffTile::Lyrics => {
                        // Lyrics belong to the song, not the bar; the tile sits with the
                        // other text.
                        let tip = format!(
                            "Lyrics: five lines of syllables under the staff{}",
                            hint(ActionId::EditLyrics)
                        );
                        let lyrics = tiles::dialog_tile(
                            ui,
                            Face::TextMore("Ly"),
                            !snapshot.lyric_lines.is_empty(),
                            true,
                            "Lyrics…",
                            &tip,
                        );
                        if lyrics.clicked() {
                            action = Some(ActionId::EditLyrics);
                        }
                    }
                    StaffTile::DoubleBar => {
                        // A switch: one click doubles this bar's end barline (or makes it
                        // single again).
                        if tiles::tile(
                            ui,
                            Face::DoubleBar,
                            snapshot.double_bar,
                            true,
                            "Double barline",
                            "End this bar with a double barline",
                        )
                        .clicked()
                        {
                            commands.push(Command::SetDoubleBar {
                                measure: snapshot.measure,
                                double_bar: !snapshot.double_bar,
                            });
                        }
                    }
                    StaffTile::Design => {
                        // A switch: the page shows the layout's handles (and the bar's own
                        // width and moved beats can be dragged).
                        if tiles::tile(
                            ui,
                            Face::Design,
                            *design_mode,
                            true,
                            "Design mode",
                            "Design mode: drag a beat or a bar line above the staff to \
                             fine-tune the layout; + and - at the end of a line fit fewer or \
                             more bars, and x puts a line's layout back",
                        )
                        .clicked()
                        {
                            *design_mode = !*design_mode;
                        }
                    }
                    StaffTile::FreeTime => {
                        // A switch: one click turns the metronome off (or back on) here.
                        if tiles::tile(
                            ui,
                            Face::FreeTime,
                            snapshot.free_time,
                            true,
                            "Free time",
                            "Free time",
                        )
                        .clicked()
                        {
                            commands.push(Command::SetFreeTime {
                                measure: snapshot.measure,
                                free_time: !snapshot.free_time,
                            });
                        }
                    }
                    StaffTile::BarRepeat { double } => {
                        // A one-bar repeat, or a two-bar repeat set and cleared as one pair.
                        let (face, label, tip) = if double {
                            (
                                Face::SimileDouble,
                                "Two-bar repeat",
                                "This bar and the next repeat the two bars before them",
                            )
                        } else {
                            (
                                Face::SimileSingle,
                                "Bar repeat",
                                "This bar repeats the bar before it",
                            )
                        };
                        let on = match mark {
                            Some(SimileMark::Simple) => !double,
                            Some(_) => double,
                            None => false,
                        };
                        let possible = repeat_fits(measure, snapshot.similes.len(), double);
                        if tiles::tile(ui, face, on, on || possible, label, tip).clicked() {
                            let parts = repeat_commands(
                                snapshot.cursor.track,
                                &snapshot.similes,
                                measure,
                                double,
                            );
                            if !parts.is_empty() {
                                commands.push(Command::Batch { commands: parts });
                            }
                        }
                    }
                }
            }
        });
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `marks` after `commands`.
    fn apply(marks: &[Option<SimileMark>], commands: &[Command]) -> Vec<Option<SimileMark>> {
        let mut marks = marks.to_vec();
        for command in commands {
            if let Command::SetSimileMark { measure, mark, .. } = command {
                marks[measure.get()] = *mark;
            }
        }
        marks
    }

    #[test]
    fn a_two_bar_repeat_over_other_marks_clears_them_whole() {
        use SimileMark::{FirstOfDouble as First, SecondOfDouble as Second, Simple};
        let track = TrackIndex::new(0);
        // Bar 4 repeats once, bars 5 and 6 as a pair; a new pair on bars 4 and 5 overlaps
        // both, so bar 6 loses its first half and is cleared too.
        let before = [
            None,
            None,
            None,
            Some(Simple),
            Some(First),
            Some(Second),
            None,
        ];
        let after = apply(&before, &repeat_commands(track, &before, 3, true));
        assert_eq!(
            after,
            [None, None, None, Some(First), Some(Second), None, None]
        );
        // Clicking either half of the new pair clears it whole.
        let cleared = apply(&after, &repeat_commands(track, &after, 4, true));
        assert_eq!(cleared, [None; 7]);
    }

    #[test]
    fn a_repeat_needs_bars_before_it_to_repeat() {
        let track = TrackIndex::new(0);
        let none = [None; 4];
        assert!(repeat_commands(track, &none, 0, false).is_empty());
        assert!(repeat_commands(track, &none, 1, true).is_empty());
        assert!(
            repeat_commands(track, &none, 3, true).is_empty(),
            "no bar after it"
        );
        assert_eq!(repeat_commands(track, &none, 2, true).len(), 2);
    }
}
