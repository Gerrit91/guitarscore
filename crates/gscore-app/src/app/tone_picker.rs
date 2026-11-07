//! The sound picker the track panel and the Sound chain window share: a small, filterable
//! table of General MIDI programs (or drum kits) for the MIDI synth, or of our tones for
//! physically modelled strings. Rows are grouped by family, the track's current choice is
//! highlighted and scrolled into view, and a hover shows what a tone is.

// Wired into the track panel and the Sound chain window by the following steps; until
// then the table is exercised by its tests only.
#![allow(dead_code)]

use egui::{Align, Align2, FontId, Rect, RichText, ScrollArea, Sense, Stroke, pos2, vec2};

use super::style;
use crate::settings::ChainPreset;
use crate::tone_library::{self, ToneFamily};

/// What a row chooses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PickerChoice {
    /// A General MIDI program (0-127) for the internal synth or a hardware one.
    Program(u8),
    /// A drum kit: its program in the SoundFont's percussion bank.
    DrumKit(u8),
    /// One of our built-in tones, by name.
    BuiltIn(&'static str),
    /// One of the user's saved tones: its kind id and its name.
    Saved { kind: String, name: String },
}

impl PickerChoice {
    /// A stable id, for scrolling the current choice into view.
    pub(super) fn id(&self) -> String {
        match self {
            PickerChoice::Program(program) => format!("program:{program}"),
            PickerChoice::DrumKit(program) => format!("kit:{program}"),
            PickerChoice::BuiltIn(name) => format!("tone:{name}"),
            PickerChoice::Saved { kind, name } => format!("saved:{kind}:{name}"),
        }
    }
}

/// One line of the table, with the group header that introduces it.
#[derive(Debug, Clone)]
pub(super) struct PickerRow {
    /// The family header shown above this row, if this is the first row of a group.
    pub group: Option<&'static str>,
    pub choice: PickerChoice,
    pub label: String,
    /// More about the row, shown on hover and searched by the filter.
    pub hint: String,
    /// The track's current choice.
    pub selected: bool,
    /// The tone its preset starts on (a built-in tone), marked in the list.
    pub default: bool,
}

/// What the picker keeps between frames: the filter text and the row to reveal next.
#[derive(Debug, Default)]
pub(super) struct TonePickerState {
    filter: String,
    /// Whether the filter field is open over the table.
    filter_open: bool,
    /// The field should take the keyboard on this frame (just opened).
    filter_focus: bool,
    /// The groups folded shut. A filter shows its matches in every group regardless.
    closed_groups: std::collections::BTreeSet<&'static str>,
    /// The row to scroll into view on the next frame.
    reveal: Option<String>,
    /// The choice the last frame highlighted, so a new one is revealed once.
    seen: Option<String>,
}

impl TonePickerState {
    /// Shows nothing but the table's rows (for a fresh picker).
    pub(super) fn new() -> Self {
        TonePickerState::default()
    }
}

/// Whether a row matches `filter` (its name, its family or its hint; empty matches all).
fn matches(row: &PickerRow, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }
    row.label.to_lowercase().contains(&filter)
        || row.hint.to_lowercase().contains(&filter)
        || row
            .group
            .is_some_and(|group| group.to_lowercase().contains(&filter))
}

/// Shows the picker and returns the row clicked this frame, with the usual eight rows.
pub(super) fn picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    rows: &[PickerRow],
    state: &mut TonePickerState,
) -> Option<PickerChoice> {
    picker_rows(ui, id_salt, rows, state, VISIBLE_ROWS, None)
}

/// Shows the picker with `visible` rows at once and returns the row clicked this frame.
/// `reveal` asks for a row to be scrolled into view even when it is not the current choice
/// (the saved tone being worked on, which may have moved since it was saved).
pub(super) fn picker_rows(
    ui: &mut egui::Ui,
    id_salt: &str,
    rows: &[PickerRow],
    state: &mut TonePickerState,
    visible: usize,
    reveal: Option<&PickerChoice>,
) -> Option<PickerChoice> {
    // A new current choice is scrolled into view once, as is the first frame's; a tone
    // being worked on is revealed even when its knobs moved away from it.
    let selected = reveal.map(PickerChoice::id).or_else(|| {
        rows.iter()
            .find(|row| row.selected)
            .map(|row| row.choice.id())
    });
    if state.seen != selected {
        state.reveal.clone_from(&selected);
        state.seen = selected;
    }
    let tokens = style::tokens(ui);
    let row_height = ui.spacing().interact_size.y.max(18.0);
    let visible = row_height * visible.max(1) as f32;
    // The filter is a corner icon over the table, not a row of its own.
    let top = ui.cursor().min;
    let filter_rect = Rect::from_min_size(
        pos2(ui.max_rect().right() - 26.0, top.y + 1.0),
        vec2(24.0, 24.0),
    );
    let mut chosen = None;
    // One dark surface behind the whole table, not a dark box per row with the panel
    // showing through the gaps.
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(3.0)
        .inner_margin(2)
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt(id_salt)
                .max_height(visible)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let width = ui.available_width();
                    let filtering = !state.filter.trim().is_empty();
                    let shown: Vec<&PickerRow> = rows
                        .iter()
                        .filter(|row| matches(row, &state.filter))
                        .collect();
                    if shown.is_empty() {
                        ui.label(RichText::new("Nothing matches the filter").weak());
                        return;
                    }
                    let mut index = 0;
                    while index < shown.len() {
                        let Some(first) = shown.get(index) else {
                            break;
                        };
                        let group = first.group;
                        let end = group.map_or(index.saturating_add(1), |group| {
                            shown
                                .get(index..)
                                .unwrap_or_default()
                                .iter()
                                .position(|row| row.group != Some(group))
                                .map_or(shown.len(), |offset| index.saturating_add(offset))
                        });
                        if let Some(group) = group {
                            let open = filtering || !state.closed_groups.contains(group);
                            if group_header(ui, group, open, width, row_height, tokens)
                                && !filtering
                            {
                                if open {
                                    state.closed_groups.insert(group);
                                } else {
                                    state.closed_groups.remove(group);
                                }
                            }
                            if open {
                                for row in shown.get(index..end).unwrap_or_default() {
                                    if table_row(
                                        ui,
                                        row,
                                        width,
                                        row_height,
                                        tokens,
                                        state.reveal.as_deref(),
                                    ) {
                                        chosen = Some(row.choice.clone());
                                    }
                                }
                            }
                        } else if let Some(row) = shown.get(index)
                            && table_row(
                                ui,
                                row,
                                width,
                                row_height,
                                tokens,
                                state.reveal.as_deref(),
                            )
                        {
                            chosen = Some(row.choice.clone());
                        }
                        index = end.max(index.saturating_add(1));
                    }
                });
        });
    state.reveal = None;
    // The funnel sits over the table's top-right corner and opens the filter field.
    let filter = ui.interact(
        filter_rect,
        ui.scope_id().with("tone_filter"),
        Sense::click(),
    );
    ui.ctx().accesskit_node_builder(filter.id, |node| {
        node.set_role(egui::Role::Button);
        node.set_label("Filter the sounds");
    });
    let filter_on = !state.filter.is_empty();
    if filter.hovered() || filter_on {
        ui.painter().rect_filled(
            filter_rect,
            3.0,
            if filter_on {
                tokens.accent.gamma_multiply(0.45)
            } else {
                tokens.control_hover
            },
        );
    }
    super::icons::paint(
        ui.painter(),
        filter_rect.shrink(2.0),
        tokens.text_weak,
        super::icons::Icon::Filter,
    );
    let filter = filter.on_hover_text("Filter the sounds by name or family");
    if filter.clicked() {
        state.filter_open = !state.filter_open;
        state.filter_focus = state.filter_open;
    }
    if state.filter_open {
        // A field beside the funnel, over the table: it has no row of its own and does not
        // hold up clicks on the rows around it.
        let at = pos2(
            (filter_rect.left() - 206.0).max(ui.max_rect().left()),
            filter_rect.top(),
        );
        egui::Area::new(ui.scope_id().with("tone_filter_field"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(4)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut state.filter)
                                    .hint_text("Filter")
                                    .desired_width(160.0),
                            );
                            ui.ctx().accesskit_node_builder(edit.id, |node| {
                                node.set_label("Filter sounds")
                            });
                            if state.filter_focus {
                                edit.request_focus();
                                state.filter_focus = false;
                            }
                            if !state.filter.is_empty() && ui.small_button("×").clicked() {
                                state.filter.clear();
                            }
                        });
                    });
            });
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            state.filter_open = false;
        }
    }
    chosen
}

/// A group's header: a disclosure triangle and its name. Folded groups show only this.
/// True when clicked.
fn group_header(
    ui: &mut egui::Ui,
    group: &str,
    open: bool,
    width: f32,
    row_height: f32,
    tokens: &style::Tokens,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, row_height), Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::Role::Button);
        node.set_label(format!("Group: {group}"));
    });
    if response.hovered() {
        ui.painter().rect_filled(rect, 3.0, tokens.control_hover);
    }
    let center = pos2(rect.left() + 9.0, rect.center().y);
    let points = if open {
        vec![
            center + vec2(-3.0, -1.5),
            center + vec2(3.0, -1.5),
            center + vec2(0.0, 2.5),
        ]
    } else {
        vec![
            center + vec2(-1.5, -3.0),
            center + vec2(-1.5, 3.0),
            center + vec2(2.5, 0.0),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        tokens.text_weak,
        Stroke::NONE,
    ));
    ui.painter()
        .with_clip_rect(rect.shrink2(vec2(18.0, 0.0)))
        .text(
            pos2(rect.left() + 18.0, rect.center().y),
            Align2::LEFT_CENTER,
            group,
            FontId::proportional(12.0),
            tokens.text_weak,
        );
    // A soft line under the header, like the ones between the items.
    let line = rect.bottom() - 0.5;
    ui.painter().line_segment(
        [
            pos2(rect.left() + 4.0, line),
            pos2(rect.right() - 4.0, line),
        ],
        Stroke::new(1.0, tokens.line.gamma_multiply(0.65)),
    );
    response
        .on_hover_text(if open {
            "Fold this group shut"
        } else {
            "Open this group"
        })
        .clicked()
}

/// One row of the table: its background (when chosen or hovered), its name, and a soft
/// line under it. True when clicked.
fn table_row(
    ui: &mut egui::Ui,
    row: &PickerRow,
    width: f32,
    row_height: f32,
    tokens: &style::Tokens,
    reveal: Option<&str>,
) -> bool {
    let label = if row.default {
        format!("{}  ·  default", row.label)
    } else {
        row.label.clone()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, row_height), Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::Role::Button);
        node.set_label(&label);
    });
    let fill = if row.selected {
        tokens.accent.gamma_multiply(0.35)
    } else if response.hovered() {
        tokens.control_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 3.0, fill);
    }
    if row.selected {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
            1.0,
            tokens.accent,
        );
    }
    ui.painter()
        .with_clip_rect(rect.shrink2(vec2(7.0, 0.0)))
        .text(
            pos2(rect.left() + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            &label,
            FontId::proportional(12.0),
            tokens.text,
        );
    let line = rect.bottom() - 0.5;
    ui.painter().line_segment(
        [
            pos2(rect.left() + 7.0, line),
            pos2(rect.right() - 7.0, line),
        ],
        Stroke::new(1.0, tokens.line.gamma_multiply(0.55)),
    );
    if reveal == Some(row.choice.id().as_str()) {
        ui.scroll_to_rect(rect, Some(Align::Center));
    }
    response.on_hover_text(&row.hint).clicked()
}

/// How many rows the picker shows at once (about eight).
pub(super) const VISIBLE_ROWS: usize = 8;

/// The standard General MIDI families and the programs they span.
const FAMILIES: [(&str, u8, u8); 16] = [
    ("Piano", 0, 7),
    ("Chromatic percussion", 8, 15),
    ("Organ", 16, 23),
    ("Guitar", 24, 31),
    ("Bass", 32, 39),
    ("Strings", 40, 47),
    ("Ensemble", 48, 55),
    ("Brass", 56, 63),
    ("Reed", 64, 71),
    ("Pipe", 72, 79),
    ("Synth lead", 80, 87),
    ("Synth pad", 88, 95),
    ("Synth effects", 96, 103),
    ("Ethnic", 104, 111),
    ("Percussive", 112, 119),
    ("Sound effects", 120, 127),
];

/// The General MIDI drum kits SoundFonts put in the percussion bank (bank 128).
const DRUM_KITS: [(u8, &str); 9] = [
    (0, "Standard"),
    (8, "Room"),
    (16, "Power"),
    (24, "Electronic"),
    (25, "TR-808"),
    (32, "Jazz"),
    (40, "Brush"),
    (48, "Orchestra"),
    (56, "Sound effects"),
];

/// The built-in or saved tone that exactly matches `tone`, if one does.
pub(super) fn current_tone_choice(
    tone: &crate::chain_settings::Tone,
    saved: &[ChainPreset],
) -> Option<PickerChoice> {
    if let Some(built_in) = tone_library::all().iter().find(|built| built.tone == *tone) {
        return Some(PickerChoice::BuiltIn(built_in.name));
    }
    saved
        .iter()
        .find(|preset| {
            preset.kind == crate::settings::kind_id(tone.kind)
                && preset.sound == tone.sound
                && preset.chain == tone.chain
        })
        .map(|preset| PickerChoice::Saved {
            kind: preset.kind.clone(),
            name: preset.name.clone(),
        })
}

/// The sound source switch the track panel and the Sound chain window share (MIDI synth,
/// PM synthesis, or a hardware synthesizer). `pm` is the PM engine to choose (the track's
/// kind, or the one its preset stands for). Returns the engine clicked.
pub(super) fn source_switch(
    ui: &mut egui::Ui,
    engine: gscore_play::TrackEngine,
    pm: gscore_play::TrackEngine,
    strings: bool,
) -> Option<gscore_play::TrackEngine> {
    use gscore_play::TrackEngine;
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        let mut choice = |ui: &mut egui::Ui, candidate: TrackEngine, label: &str, enabled: bool| {
            let selected = match (engine, candidate) {
                (TrackEngine::Modelled(_), TrackEngine::Modelled(_)) => true,
                _ => engine == candidate,
            };
            let response = ui
                .add_enabled(enabled, egui::Button::selectable(selected, label))
                .on_hover_text(match candidate {
                    TrackEngine::SoundFont => {
                        "The internal synth plays the track with the SoundFont (General MIDI)"
                    }
                    TrackEngine::Modelled(_) => {
                        "Physical modelling: the strings are synthesized and played through \
                         the chain"
                    }
                    TrackEngine::MidiOut => {
                        "A hardware synthesizer on a MIDI output port plays the track; its \
                         sound does not come back through the mixer"
                    }
                });
            if !enabled {
                response.clone().on_hover_text(
                    "Only string instruments that are not drum kits can be played by \
                     modelled strings",
                );
            }
            if response.clicked() {
                chosen = Some(candidate);
            }
        };
        choice(ui, TrackEngine::SoundFont, "MIDI synth", true);
        choice(ui, pm, "PM Synthesis", strings);
        #[cfg(not(target_arch = "wasm32"))]
        choice(ui, TrackEngine::MidiOut, "MIDI out", true);
    });
    chosen
}

/// The General MIDI programs (or, for a drum track, the percussion bank's kits), grouped by
/// family, with `program` highlighted.
pub(super) fn general_midi_rows(program: u8, drums: bool) -> Vec<PickerRow> {
    if drums {
        return drum_kit_rows(program);
    }
    let mut rows = Vec::new();
    for (family, first, last) in FAMILIES {
        for candidate in first..=last {
            rows.push(PickerRow {
                group: Some(family),
                choice: PickerChoice::Program(candidate),
                label: format!("{candidate}: {}", gscore_model::program_name(candidate)),
                hint: format!(
                    "General MIDI program {candidate}, {}",
                    family.to_lowercase()
                ),
                selected: candidate == program,
                default: false,
            });
        }
    }
    rows
}

/// The percussion bank's kits, grouped as one family.
fn drum_kit_rows(program: u8) -> Vec<PickerRow> {
    DRUM_KITS
        .iter()
        .map(|&(kit, name)| PickerRow {
            group: Some("Drum kits"),
            choice: PickerChoice::DrumKit(kit),
            label: format!("{kit}: {name}"),
            hint: format!("General MIDI drum kit {name} (the percussion bank)"),
            selected: kit == program,
            default: false,
        })
        .collect()
}

/// The user's saved tones first (they are the defaults), then the built-in library by
/// family; the current tone is highlighted and the tone its preset starts on marked.
pub(super) fn tone_rows(
    selected: Option<&PickerChoice>,
    default_tone: Option<&str>,
    saved: &[ChainPreset],
) -> Vec<PickerRow> {
    let mut rows: Vec<PickerRow> = Vec::new();
    let mut saved: Vec<&ChainPreset> = saved.iter().collect();
    saved.sort_by_cached_key(|preset| preset.name.to_lowercase());
    for preset in saved {
        let choice = PickerChoice::Saved {
            kind: preset.kind.clone(),
            name: preset.name.clone(),
        };
        rows.push(PickerRow {
            group: Some("Saved"),
            label: preset.name.clone(),
            hint: format!(
                "Your saved tone \"{}\" ({} strings)",
                preset.name,
                preset.kind.as_str()
            ),
            selected: selected == Some(&choice),
            default: default_tone == Some(preset.name.as_str()),
            choice,
        });
    }
    for family in ToneFamily::ALL {
        for tone in tone_library::all()
            .iter()
            .filter(|tone| tone.family == family)
        {
            let choice = PickerChoice::BuiltIn(tone.name);
            rows.push(PickerRow {
                group: Some(family.label()),
                label: tone.name.to_owned(),
                hint: tone.description.to_owned(),
                selected: selected == Some(&choice),
                default: default_tone == Some(tone.name),
                choice,
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::settings::StringSound;

    #[test]
    fn the_programs_are_grouped_by_family_and_the_current_one_is_highlighted() {
        let rows = general_midi_rows(27, false);
        assert_eq!(rows.len(), 128);
        assert_eq!(rows.first().and_then(|row| row.group), Some("Piano"));
        let guitar = rows
            .iter()
            .find(|row| row.choice == PickerChoice::Program(24))
            .expect("the first guitar");
        assert_eq!(guitar.group, Some("Guitar"));
        assert_eq!(guitar.label, "24: Nylon Str Guitar");
        assert!(rows.iter().filter(|row| row.selected).count() == 1);
        assert!(
            rows.iter()
                .find(|row| row.selected)
                .is_some_and(|row| row.choice == PickerChoice::Program(27))
        );
        // Every family's first row carries its header exactly once.
        let mut headers: Vec<&str> = Vec::new();
        for row in &rows {
            if let Some(group) = row.group
                && headers.last() != Some(&group)
            {
                headers.push(group);
            }
        }
        assert_eq!(headers.len(), FAMILIES.len());
        assert_eq!(headers.first(), Some(&"Piano"));
        assert_eq!(headers.last(), Some(&"Sound effects"));
    }

    #[test]
    fn a_drum_track_lists_the_bands_kits() {
        let rows = general_midi_rows(8, true);
        assert_eq!(rows.len(), DRUM_KITS.len());
        assert_eq!(rows[0].label, "0: Standard");
        assert_eq!(rows[1].group, Some("Drum kits"));
        assert!(rows[1].selected, "Room is chosen");
        assert!(
            rows.iter()
                .all(|row| matches!(row.choice, PickerChoice::DrumKit(_)))
        );
    }

    #[test]
    fn the_tone_rows_hold_the_saved_ones_first_then_every_built_in_tone() {
        let saved = ChainPreset {
            name: "My Lead".to_owned(),
            kind: "electric".to_owned(),
            sound: StringSound::default(),
            chain: crate::chain_settings::ChainSetting::default(),
            engine: String::new(),
        };
        let selected = PickerChoice::BuiltIn("Djent Tight");
        let rows = tone_rows(
            Some(&selected),
            Some("Modern Metal Rhythm"),
            std::slice::from_ref(&saved),
        );
        let built_in = tone_library::all().len();
        assert_eq!(rows.len(), built_in + 1);
        assert_eq!(rows.first().and_then(|row| row.group), Some("Saved"));
        assert_eq!(rows.first().map(|row| row.label.as_str()), Some("My Lead"));
        // Exactly one row is current, and the preset's default is marked separately.
        assert_eq!(rows.iter().filter(|row| row.selected).count(), 1);
        assert_eq!(rows.iter().filter(|row| row.default).count(), 1);
        assert!(
            rows.iter()
                .find(|row| row.default)
                .is_some_and(|row| row.label == "Modern Metal Rhythm")
        );
        // A saved default is marked on its own row.
        let saved_default = tone_rows(
            Some(&PickerChoice::Saved {
                kind: "electric".to_owned(),
                name: "My Lead".to_owned(),
            }),
            Some("My Lead"),
            std::slice::from_ref(&saved),
        );
        assert!(
            saved_default
                .first()
                .is_some_and(|row| row.default && row.selected)
        );
        // The families appear in their own order, each as one header per group.
        let groups: Vec<&str> = rows.iter().filter_map(|row| row.group).collect();
        let mut unique: Vec<&str> = Vec::new();
        for group in groups {
            if unique.last() != Some(&group) {
                unique.push(group);
            }
        }
        assert_eq!(
            unique,
            vec![
                "Saved",
                "Clean",
                "Crunch",
                "Lead",
                "High gain",
                "Acoustic",
                "Bass"
            ]
        );
    }

    #[test]
    fn the_filter_searches_names_hints_and_groups() {
        let rows = general_midi_rows(27, false);
        let matching = |filter: &str| {
            rows.iter()
                .filter(|row| matches(row, filter))
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(matching("").len(), 128);
        assert_eq!(matching("nylon"), vec!["24: Nylon Str Guitar"]);
        assert!(matching("guitar").len() > 1, "the family is searched");
        assert!(matching("no such sound").is_empty());
        assert_eq!(matching("  NYLON  "), vec!["24: Nylon Str Guitar"]);
        let tones = tone_rows(None, None, &[]);
        let found = tones
            .iter()
            .filter(|row| matches(row, "djent"))
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(found, vec!["Djent Tight"]);
    }
}
