//! The note tools: a virtual fretboard, a piano keyboard and the drum map. Each is a
//! floating window that shows the notes of the cursor's beat; a click on the fretboard or
//! the keyboard puts a note into that beat (or takes it out again).

use egui::{Align2, Color32, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{BeatPosition, Fret, NoteValue, StringNumber};

use super::GuitarScoreApp;
use super::style;
use crate::chain_settings::DrumPart;

/// One of the note tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoteTool {
    /// The strings and frets of the shown track.
    Fretboard,
    /// A piano keyboard.
    Keyboard,
    /// Which General MIDI drum note is which sound, and the kit part it is mixed in.
    Drums,
}

impl NoteTool {
    /// Every tool, in toolbar and menu order.
    pub(super) const ALL: [NoteTool; 3] =
        [NoteTool::Fretboard, NoteTool::Keyboard, NoteTool::Drums];

    /// The window's title, also the toolbar button's and the menu entry's name.
    pub(super) fn title(self) -> &'static str {
        match self {
            NoteTool::Fretboard => "Fretboard",
            NoteTool::Keyboard => "Keyboard",
            NoteTool::Drums => "Drum map",
        }
    }

    /// What the tool is for, as a tooltip.
    pub(super) fn tip(self) -> &'static str {
        match self {
            NoteTool::Fretboard => {
                "Fretboard: the notes of the cursor's beat on the strings; click to enter one"
            }
            NoteTool::Keyboard => {
                "Keyboard: the notes of the cursor's beat on piano keys; click to enter one"
            }
            NoteTool::Drums => "Drum map: which MIDI note is which drum, and its kit part",
        }
    }

    fn index(self) -> usize {
        match self {
            NoteTool::Fretboard => 0,
            NoteTool::Keyboard => 1,
            NoteTool::Drums => 2,
        }
    }

    /// The window's [`egui::Area`] id.
    fn window_id(self) -> egui::Id {
        egui::Id::unique(("gscore_note_tool", self.index()))
    }
}

/// Which note tools are open, and where their windows sit.
#[derive(Debug, Default)]
pub(super) struct NoteTools {
    open: [bool; 3],
    pos: [Option<Pos2>; 3],
}

impl NoteTools {
    /// Whether `tool`'s window is open.
    pub(super) fn is_open(&self, tool: NoteTool) -> bool {
        self.open.get(tool.index()).copied().unwrap_or(false)
    }
}

/// Height of the line of fret numbers below the fretboard.
const NUMBERS: f32 = 16.0;
/// How many frets the fretboard shows.
const FRETS: u8 = 24;
/// The keyboard's lowest and highest key (C1 to C7).
const LOWEST_KEY: u8 = 24;
const HIGHEST_KEY: u8 = 96;
/// The General MIDI drum notes the map lists.
const DRUM_KEYS: std::ops::RangeInclusive<u8> = 35..=81;

/// A note of the cursor's beat.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShownNote {
    string: StringNumber,
    /// The fret, or the drum's MIDI note on a drum track.
    fret: u8,
    /// The pitch it sounds at (MIDI), with its name; none on a drum track.
    pitch: Option<(u8, String)>,
}

/// The cursor's beat, as the tools show it.
#[derive(Debug, Clone)]
struct BeatNotes {
    at: BeatPosition,
    percussion: bool,
    /// The staff's strings from string 1 down, each with the pitch it sounds open.
    strings: Vec<(StringNumber, u8)>,
    /// How many strings the staff has (a drum kit's notes sit on strings too).
    string_count: usize,
    /// The cursor's string.
    cursor_string: Option<StringNumber>,
    /// The note value of the beat to make first, when the cursor's bar has no beat yet (an
    /// empty Guitar Pro 7 bar): a note entered there needs its beat, as a typed fret does.
    new_beat: Option<NoteValue>,
    notes: Vec<ShownNote>,
}

impl BeatNotes {
    /// The string a new drum note goes on: the first the beat does not use, else the
    /// cursor's (its note is replaced).
    fn free_kit_string(&self) -> Option<StringNumber> {
        (1..=self.string_count)
            .filter_map(|number| StringNumber::new(u8::try_from(number).ok()?).ok())
            .find(|string| !self.notes.iter().any(|note| note.string == *string))
            .or(self.cursor_string)
    }

    /// The fret and string that sound `pitch` and are easiest to reach: the lowest fret on
    /// a string the beat does not use yet.
    fn place_for(&self, pitch: u8) -> Option<(StringNumber, u8)> {
        self.strings
            .iter()
            .filter(|(string, _)| !self.notes.iter().any(|note| note.string == *string))
            .filter_map(|(string, open)| {
                let fret = pitch.checked_sub(*open)?;
                (fret <= FRETS).then_some((*string, fret))
            })
            .min_by_key(|(_, fret)| *fret)
    }
}

/// Where fret `fret`'s wire is between the nut (0) and the last fret (1), spaced as on a
/// real neck (each fret a semitone shorter).
fn fret_share(fret: u8) -> f32 {
    let length = |fret: u8| 1.0 - 0.5_f32.powf(f32::from(fret) / 12.0);
    length(fret) / length(FRETS)
}

/// The fretboard's geometry inside its rectangle: the open-string area left of the nut,
/// then the frets.
#[derive(Debug, Clone, Copy)]
struct Board {
    rect: Rect,
    strings: usize,
}

impl Board {
    /// Width of the area left of the nut, where open strings are marked.
    const OPEN: f32 = 34.0;

    fn nut(self) -> f32 {
        self.rect.left() + Self::OPEN
    }

    /// The x of fret `fret`'s wire (the nut for 0).
    fn wire(self, fret: u8) -> f32 {
        self.nut() + (self.rect.right() - 8.0 - self.nut()) * fret_share(fret)
    }

    /// The y of the string at `index` (0 is string 1, at the top).
    fn string_y(self, index: usize) -> f32 {
        let gap = self.rect.height() / self.strings.max(1) as f32;
        self.rect.top() + gap * (index as f32 + 0.5)
    }

    /// Where a note on `fret` of the string at `index` is marked: between its fret's wires,
    /// or left of the nut for an open string.
    fn marker(self, index: usize, fret: u8) -> Pos2 {
        let x = match fret.checked_sub(1) {
            None => self.rect.left() + Self::OPEN / 2.0,
            Some(before) => (self.wire(before) + self.wire(fret.min(FRETS))) / 2.0,
        };
        pos2(x, self.string_y(index))
    }

    /// The string (its index) and fret under `pos`, if it is on the board.
    fn hit(self, pos: Pos2) -> Option<(usize, u8)> {
        if !self.rect.contains(pos) || self.strings == 0 {
            return None;
        }
        let gap = self.rect.height() / self.strings as f32;
        let index = (((pos.y - self.rect.top()) / gap).floor().max(0.0) as usize)
            .min(self.strings.saturating_sub(1));
        let fret = (1..=FRETS)
            .find(|fret| pos.x <= self.wire(*fret))
            .filter(|_| pos.x > self.nut())
            .unwrap_or(if pos.x <= self.nut() { 0 } else { FRETS });
        Some((index, fret))
    }
}

/// Whether a MIDI pitch is a black key.
fn is_black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// The keyboard's keys inside `rect`: every key from [`LOWEST_KEY`] to [`HIGHEST_KEY`]
/// with its rectangle, white keys first (the black ones lie on top of them).
fn keys(rect: Rect) -> Vec<(u8, Rect)> {
    let whites = (LOWEST_KEY..=HIGHEST_KEY).filter(|p| !is_black(*p)).count();
    let width = rect.width() / whites.max(1) as f32;
    let mut white = Vec::new();
    let mut black = Vec::new();
    let mut x = rect.left();
    for pitch in LOWEST_KEY..=HIGHEST_KEY {
        if is_black(pitch) {
            black.push((
                pitch,
                Rect::from_min_size(
                    pos2(x - width * 0.3, rect.top()),
                    vec2(width * 0.6, rect.height() * 0.62),
                ),
            ));
        } else {
            white.push((
                pitch,
                Rect::from_min_size(pos2(x, rect.top()), vec2(width, rect.height())),
            ));
            x += width;
        }
    }
    white.extend(black);
    white
}

/// The key under `pos`: a black key wins over the white ones it lies on.
fn key_at(rect: Rect, pos: Pos2) -> Option<u8> {
    keys(rect)
        .into_iter()
        .rev()
        .find(|(_, key)| key.contains(pos))
        .map(|(pitch, _)| pitch)
}

/// The General MIDI name of drum note `key`.
fn drum_name(key: u8) -> &'static str {
    const NAMES: [&str; 47] = [
        "Acoustic bass drum",
        "Bass drum 1",
        "Side stick",
        "Acoustic snare",
        "Hand clap",
        "Electric snare",
        "Low floor tom",
        "Closed hi-hat",
        "High floor tom",
        "Pedal hi-hat",
        "Low tom",
        "Open hi-hat",
        "Low-mid tom",
        "Hi-mid tom",
        "Crash cymbal 1",
        "High tom",
        "Ride cymbal 1",
        "Chinese cymbal",
        "Ride bell",
        "Tambourine",
        "Splash cymbal",
        "Cowbell",
        "Crash cymbal 2",
        "Vibraslap",
        "Ride cymbal 2",
        "Hi bongo",
        "Low bongo",
        "Mute hi conga",
        "Open hi conga",
        "Low conga",
        "High timbale",
        "Low timbale",
        "High agogo",
        "Low agogo",
        "Cabasa",
        "Maracas",
        "Short whistle",
        "Long whistle",
        "Short guiro",
        "Long guiro",
        "Claves",
        "Hi wood block",
        "Low wood block",
        "Mute cuica",
        "Open cuica",
        "Mute triangle",
        "Open triangle",
    ];
    key.checked_sub(*DRUM_KEYS.start())
        .and_then(|index| NAMES.get(usize::from(index)))
        .copied()
        .unwrap_or("Unnamed")
}

/// The kit part drum note `key` is mixed in.
fn drum_part(key: u8) -> DrumPart {
    DrumPart::at(gscore_audio::drum_part(key)).unwrap_or(DrumPart::Percussion)
}

/// What a click in a tool asks for.
enum Click {
    /// A fret on a string (toggled: a click on the note that is there removes it).
    Fret(StringNumber, u8),
    /// A drum of the map (toggled likewise), by its MIDI note.
    Drum(u8),
    /// A key of the keyboard (toggled likewise).
    Key(u8),
}

impl GuitarScoreApp {
    /// Opens `tool`'s window, or closes it when it is open (the toolbar buttons toggle).
    pub(super) fn toggle_note_tool(&mut self, tool: NoteTool) {
        if let Some(open) = self.note_tools.open.get_mut(tool.index()) {
            *open = !*open;
        }
    }

    /// The notes of the cursor's beat in the open document.
    fn beat_notes(&self) -> Option<BeatNotes> {
        let tab = self.tabs.get(self.active)?;
        let song = tab.document.song();
        let cursor = tab.document.editor().cursor();
        let track = song.staff(cursor.track, cursor.staff)?;
        let percussion = track.is_percussion();
        let tuning = track
            .tuning()
            .ok()
            .flatten()
            .or_else(|| track.note_tuning().ok().flatten())
            .unwrap_or_default();
        let mut strings: Vec<(StringNumber, u8)> = tuning
            .iter()
            .filter_map(|string| {
                let open = track.sounding_pitch_at(string.string, Fret::OPEN).ok()?;
                Some((string.string, open.get()))
            })
            .collect();
        strings.sort_by_key(|(string, _)| string.get());
        let at = cursor.position();
        let notes = song
            .measure_at(at)
            .and_then(|measure| measure.voice(cursor.voice))
            .and_then(|voice| voice.beat(cursor.beat))
            .map(|beat| {
                beat.notes()
                    .filter_map(|note| {
                        let pitch = if percussion {
                            None
                        } else {
                            let pitch = track.sounding_pitch(&note).ok()?;
                            Some((pitch.get(), pitch.note_name().to_owned()))
                        };
                        Some(ShownNote {
                            string: note.string().ok()?,
                            fret: note.fret().ok()?.get(),
                            pitch,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(BeatNotes {
            at,
            percussion,
            strings,
            string_count: track.string_count(),
            cursor_string: cursor.string_number(),
            new_beat: (song.beat(at).is_none() && cursor.beat.get() == 0)
                .then(|| tab.document.editor().new_beat_value()),
            notes,
        })
    }

    /// The note tools' windows, while they are open.
    pub(super) fn show_note_tools(&mut self, ctx: &egui::Context) {
        if !self.note_tools.open.iter().any(|open| *open) {
            return;
        }
        let beat = self.beat_notes();
        let mut click = None;
        for tool in NoteTool::ALL {
            if !self.note_tools.is_open(tool) {
                continue;
            }
            let index = tool.index();
            let size = match tool {
                NoteTool::Fretboard => vec2(820.0, 190.0),
                NoteTool::Keyboard => vec2(820.0, 150.0),
                NoteTool::Drums => vec2(380.0, 440.0),
            };
            let mut pos = self.note_tools.pos.get(index).copied().flatten();
            let at = *pos.get_or_insert_with(|| {
                // The fretboard along the bottom, the keyboard above it and the drum map
                // at the right, so the three do not cover each other or the page's top.
                let screen = ctx.content_rect();
                let centered = (screen.center().x - size.x / 2.0).max(screen.left() + 8.0);
                let bottom = screen.bottom() - 130.0;
                let (x, y) = match tool {
                    NoteTool::Fretboard => (centered, bottom - 190.0),
                    NoteTool::Keyboard => (centered, bottom - 190.0 - 160.0),
                    NoteTool::Drums => (screen.right() - size.x - 24.0, screen.top() + 90.0),
                };
                pos2(x.max(screen.left() + 8.0), y.max(screen.top() + 80.0))
            });
            let mut close = false;
            egui::Window::new(tool.title())
                .id(tool.window_id())
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .current_pos(at)
                .frame(style::window_frame(ctx))
                .show(ctx, |ui| {
                    if style::window_title_bar(ui, tool.title(), &mut pos) {
                        close = true;
                    }
                    egui::Frame::new()
                        .inner_margin(Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.set_width(size.x - 24.0);
                            match (tool, &beat) {
                                (_, None) => {
                                    ui.label(RichText::new("Open a song to use this.").weak());
                                }
                                (NoteTool::Fretboard, Some(beat)) => {
                                    click = fretboard(ui, beat).or(click.take());
                                }
                                (NoteTool::Keyboard, Some(beat)) => {
                                    click = keyboard(ui, beat).or(click.take());
                                }
                                (NoteTool::Drums, Some(beat)) => {
                                    click = drum_map(ui, beat).or(click.take());
                                }
                            }
                        });
                });
            if let Some(slot) = self.note_tools.pos.get_mut(index) {
                *slot = pos;
            }
            // Escape closes the window the user is looking at.
            close |= style::escape_closes(ctx, tool.window_id());
            if close && let Some(open) = self.note_tools.open.get_mut(index) {
                *open = false;
            }
        }
        let (Some(click), Some(beat)) = (click, beat) else {
            return;
        };
        let place = match click {
            Click::Fret(string, fret) => Some((string, fret)),
            Click::Drum(key) => {
                // A drum that is hit already goes out again; else it goes on a string of
                // the kit the beat does not use yet (the cursor's, when all are used).
                let hit = beat.notes.iter().find(|note| note.fret == key);
                match hit {
                    Some(note) => Some((note.string, note.fret)),
                    None => beat.free_kit_string().map(|string| (string, key)),
                }
            }
            Click::Key(pitch) => {
                // A key that sounds already goes out again; else it goes where it is
                // easiest to reach.
                let sounding = beat
                    .notes
                    .iter()
                    .find(|note| note.pitch.as_ref().is_some_and(|(p, _)| *p == pitch));
                match sounding {
                    Some(note) => Some((note.string, note.fret)),
                    None => {
                        let place = beat.place_for(pitch);
                        if place.is_none() {
                            self.notice =
                                Some("No free string of this track can play that key.".to_owned());
                        }
                        place
                    }
                }
            }
        };
        let Some((string, fret)) = place else {
            return;
        };
        let there = beat
            .notes
            .iter()
            .any(|note| note.string == string && note.fret == fret);
        let command = if there {
            Command::ClearNote {
                at: beat.at,
                string,
            }
        } else {
            let Ok(fret) = Fret::new(fret) else {
                return;
            };
            let enter = Command::SetFret {
                at: beat.at,
                string,
                fret,
            };
            match beat.new_beat {
                // A bar without a beat gets the note's beat first, as one undo step.
                Some(duration) => Command::Batch {
                    commands: vec![
                        Command::InsertBeat {
                            at: beat.at,
                            duration,
                        },
                        enter,
                    ],
                },
                None => enter,
            }
        };
        self.apply_command(command, false);
        // A note put in is heard, like a typed fret; taking one out is silent.
        if !there {
            self.audition(beat.at, string, ctx);
        }
    }
}

/// A weak line saying what a tool cannot show for this track.
fn not_for(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(style::tokens(ui).text_weak));
}

/// The fretboard: the strings from string 1 at the top, the frets, and a circle with the
/// note's name on every note of the beat. Returns the place that was clicked.
fn fretboard(ui: &mut egui::Ui, beat: &BeatNotes) -> Option<Click> {
    if beat.percussion || beat.strings.is_empty() {
        not_for(
            ui,
            "The shown track has no strings (see the drum map or the keyboard).",
        );
        return None;
    }
    let tokens = style::tokens(ui);
    let height = (beat.strings.len() as f32 * 20.0).max(80.0);
    // The board, and a line below it for the fret numbers.
    let (whole, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height + NUMBERS), Sense::click());
    let rect = Rect::from_min_max(whole.min, pos2(whole.right(), whole.bottom() - NUMBERS));
    let board = Board {
        rect,
        strings: beat.strings.len(),
    };
    let painter = ui.painter_at(whole);
    let wood = tokens.control;
    painter.rect_filled(
        Rect::from_min_max(pos2(board.nut(), rect.top()), rect.max),
        3.0,
        wood,
    );
    // Position marks, as on a neck: one dot, two at the octaves.
    for fret in [3_u8, 5, 7, 9, 12, 15, 17, 19, 21, 24] {
        let x = (board.wire(fret.saturating_sub(1)) + board.wire(fret)) / 2.0;
        let dots: &[f32] = if fret % 12 == 0 { &[0.3, 0.7] } else { &[0.5] };
        for share in dots {
            painter.circle_filled(
                pos2(x, rect.top() + rect.height() * share),
                3.5,
                tokens.text_weak.gamma_multiply(0.45),
            );
        }
        painter.text(
            pos2(x, rect.bottom() + NUMBERS / 2.0),
            Align2::CENTER_CENTER,
            fret.to_string(),
            FontId::proportional(10.0),
            tokens.text_weak,
        );
    }
    for fret in 0..=FRETS {
        let x = board.wire(fret);
        let width = if fret == 0 { 4.0 } else { 1.5 };
        painter.line_segment(
            [pos2(x, rect.top() + 2.0), pos2(x, rect.bottom() - 2.0)],
            Stroke::new(width, tokens.text_weak),
        );
    }
    for index in 0..beat.strings.len() {
        let y = board.string_y(index);
        painter.line_segment(
            [pos2(rect.left() + 4.0, y), pos2(rect.right() - 4.0, y)],
            Stroke::new(1.0 + index as f32 * 0.25, tokens.text),
        );
    }
    let mut summary = Vec::new();
    for note in &beat.notes {
        let Some(index) = beat
            .strings
            .iter()
            .position(|(string, _)| *string == note.string)
        else {
            continue;
        };
        let name = note.pitch.as_ref().map_or("", |(_, name)| name.as_str());
        let center = board.marker(index, note.fret);
        painter.circle_filled(center, 9.0, tokens.accent);
        painter.text(
            center,
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(10.0),
            tokens.on_accent,
        );
        summary.push(format!(
            "{name} on string {} fret {}",
            note.string.get(),
            note.fret
        ));
    }
    let label = if summary.is_empty() {
        "Fretboard: no notes".to_owned()
    } else {
        format!("Fretboard: {}", summary.join(", "))
    };
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(label));
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let (index, fret) = response
        .clicked()
        .then(|| response.interact_pointer_pos())
        .flatten()
        .and_then(|pos| board.hit(pos))?;
    let (string, _) = beat.strings.get(index)?;
    Some(Click::Fret(*string, fret))
}

/// The keyboard: the keys of the beat's notes in the accent color. Returns the key that
/// was clicked.
fn keyboard(ui: &mut egui::Ui, beat: &BeatNotes) -> Option<Click> {
    if beat.percussion {
        not_for(ui, "The shown track is a drum kit (see the drum map).");
        return None;
    }
    let tokens = style::tokens(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::click());
    let painter = ui.painter_at(rect);
    let sounding: Vec<u8> = beat
        .notes
        .iter()
        .filter_map(|note| note.pitch.as_ref().map(|(pitch, _)| *pitch))
        .collect();
    for (pitch, key) in keys(rect) {
        let fill = if sounding.contains(&pitch) {
            tokens.accent
        } else if is_black(pitch) {
            Color32::from_gray(28)
        } else {
            Color32::from_gray(238)
        };
        painter.rect_filled(key.shrink(0.5), 2.0, fill);
        painter.rect_stroke(
            key.shrink(0.5),
            2.0,
            Stroke::new(1.0, Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
        // Every C is named, so the octaves can be told apart.
        if pitch % 12 == 0 {
            painter.text(
                pos2(key.center().x, key.bottom() - 9.0),
                Align2::CENTER_CENTER,
                format!("C{}", i16::from(pitch / 12) - 1),
                FontId::proportional(8.5),
                Color32::from_gray(90),
            );
        }
    }
    let mut names: Vec<&str> = beat
        .notes
        .iter()
        .filter_map(|note| note.pitch.as_ref().map(|(_, name)| name.as_str()))
        .collect();
    names.dedup();
    let label = if names.is_empty() {
        "Keyboard: no notes".to_owned()
    } else {
        format!("Keyboard: {}", names.join(", "))
    };
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(label));
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let pitch = response
        .clicked()
        .then(|| response.interact_pointer_pos())
        .flatten()
        .and_then(|pos| key_at(rect, pos))?;
    Some(Click::Key(pitch))
}

/// The drum map: every General MIDI drum note with its name and the kit part it is mixed
/// in; the notes of the cursor's beat are marked. On a drum track a click on a drum's name
/// puts it into the beat (or takes it out); returns that click.
fn drum_map(ui: &mut egui::Ui, beat: &BeatNotes) -> Option<Click> {
    let tokens = style::tokens(ui);
    let hit: Vec<u8> = if beat.percussion {
        beat.notes.iter().map(|note| note.fret).collect()
    } else {
        Vec::new()
    };
    if !beat.percussion {
        not_for(
            ui,
            "The shown track is not a drum kit; this is the map a drum track uses.",
        );
        ui.add_space(4.0);
    }
    let mut click = None;
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("drum_map")
                .num_columns(3)
                .striped(true)
                .spacing([16.0, 3.0])
                .show(ui, |ui| {
                    for title in ["MIDI note", "Drum", "Kit part"] {
                        ui.label(RichText::new(title).strong());
                    }
                    ui.end_row();
                    for key in DRUM_KEYS {
                        let on = hit.contains(&key);
                        let ink = if on { tokens.accent } else { tokens.text };
                        let text = |text: String| {
                            let text = RichText::new(text).color(ink);
                            if on { text.strong() } else { text }
                        };
                        ui.label(text(key.to_string()));
                        if beat.percussion {
                            // The name is the row's button.
                            let name = ui
                                .add(egui::Button::selectable(on, drum_name(key)))
                                .on_hover_text(if on {
                                    "Take this drum out of the beat"
                                } else {
                                    "Put this drum into the beat"
                                });
                            if name.clicked() {
                                click = Some(Click::Drum(key));
                            }
                        } else {
                            ui.label(text(drum_name(key).to_owned()));
                        }
                        ui.label(text(drum_part(key).label().to_owned()));
                        ui.end_row();
                    }
                });
        });
    click
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn board() -> Board {
        Board {
            rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 120.0)),
            strings: 6,
        }
    }

    #[test]
    fn a_marker_is_found_again_by_a_click_on_it() {
        let board = board();
        for index in 0..6 {
            for fret in 0..=FRETS {
                assert_eq!(board.hit(board.marker(index, fret)), Some((index, fret)));
            }
        }
        assert_eq!(board.hit(pos2(0.0, 0.0)), None);
    }

    #[test]
    fn frets_get_narrower_up_the_neck() {
        assert_eq!(fret_share(0), 0.0);
        assert!((fret_share(FRETS) - 1.0).abs() < 1e-6);
        let width = |fret: u8| fret_share(fret) - fret_share(fret - 1);
        assert!(width(1) > width(12) && width(12) > width(24));
    }

    #[test]
    fn the_keyboard_finds_black_keys_on_top_of_white_ones() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(860.0, 100.0));
        for (pitch, key) in keys(rect) {
            let probe = pos2(key.center().x, key.top() + 10.0);
            assert_eq!(key_at(rect, probe), Some(pitch), "key {pitch}");
        }
        // Low on a white key, beside the black ones.
        assert_eq!(key_at(rect, pos2(1.0, 95.0)), Some(LOWEST_KEY));
        assert!(is_black(61) && !is_black(60));
    }

    #[test]
    fn the_drum_map_names_the_kit() {
        assert_eq!(drum_name(36), "Bass drum 1");
        assert_eq!(drum_name(81), "Open triangle");
        assert_eq!(drum_name(20), "Unnamed");
        assert_eq!(drum_part(36), DrumPart::Kick);
        assert_eq!(drum_part(38), DrumPart::Snare);
        assert_eq!(drum_part(42), DrumPart::HiHat);
        assert_eq!(drum_part(56), DrumPart::Percussion);
    }

    #[test]
    fn a_key_goes_to_the_lowest_fret_on_a_free_string() {
        let string = |number: u8| StringNumber::new(number).unwrap();
        let beat = BeatNotes {
            at: BeatPosition {
                track: gscore_model::TrackIndex::new(0),
                staff: gscore_model::StaffIndex::new(0),
                measure: gscore_model::MeasureIndex::new(0),
                voice: gscore_model::VoiceIndex::new(0),
                beat: gscore_model::BeatIndex::new(0),
            },
            percussion: false,
            string_count: 6,
            cursor_string: Some(string(6)),
            new_beat: None,
            // Standard tuning, string 1 (E4) first.
            strings: vec![
                (string(1), 64),
                (string(2), 59),
                (string(3), 55),
                (string(4), 50),
                (string(5), 45),
                (string(6), 40),
            ],
            notes: vec![ShownNote {
                string: string(1),
                fret: 0,
                pitch: Some((64, "E".to_owned())),
            }],
        };
        // E4 is open on string 1, which is taken: fret 5 on string 2.
        assert_eq!(beat.place_for(64), Some((string(2), 5)));
        // A2 is open on string 5.
        assert_eq!(beat.place_for(45), Some((string(5), 0)));
        // Below the lowest string: nowhere.
        assert_eq!(beat.place_for(30), None);
    }
}
