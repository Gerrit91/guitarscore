//! The sound picker the track panel and the Sound chain window share: a small, filterable
//! table of General MIDI programs (or drum kits) for the MIDI synth, or of our tones for
//! physically modelled strings. Rows are grouped by family, the track's current choice is
//! highlighted and scrolled into view, and a hover shows what a tone is.

// Wired into the track panel and the Sound chain window by the following steps; until
// then the table is exercised by its tests only.
#![allow(dead_code)]

use egui::{Align, RichText, ScrollArea};

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

/// Shows the picker and returns the row clicked this frame.
pub(super) fn picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    rows: &[PickerRow],
    state: &mut TonePickerState,
) -> Option<PickerChoice> {
    ui.horizontal(|ui| {
        let filter = ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text("Filter")
                .desired_width(160.0),
        );
        ui.ctx()
            .accesskit_node_builder(filter.id, |node| node.set_label("Filter sounds"));
        if !state.filter.is_empty() && ui.small_button("Clear").clicked() {
            state.filter.clear();
        }
    });
    // A new current choice is scrolled into view once, as is the first frame's.
    let selected = rows
        .iter()
        .find(|row| row.selected)
        .map(|row| row.choice.id());
    if state.seen != selected {
        state.reveal.clone_from(&selected);
        state.seen = selected;
    }
    let row_height = ui.spacing().interact_size.y.max(18.0);
    let visible = row_height * VISIBLE_ROWS as f32;
    let mut chosen = None;
    ScrollArea::vertical()
        .id_salt(id_salt)
        .max_height(visible)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let mut shown_group: Option<&str> = None;
            let mut shown_any = false;
            for row in rows.iter().filter(|row| matches(row, &state.filter)) {
                if let Some(group) = row.group
                    && shown_group != Some(group)
                {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(group)
                            .strong()
                            .color(style::tokens(ui).text_weak),
                    );
                    shown_group = Some(group);
                }
                let label = if row.default {
                    format!("{}  ·  default", row.label)
                } else {
                    row.label.clone()
                };
                let response = ui
                    .selectable_label(row.selected, label)
                    .on_hover_text(&row.hint);
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_role(egui::Role::Button));
                if state.reveal.as_deref() == Some(row.choice.id().as_str()) {
                    ui.scroll_to_rect(response.rect, Some(Align::Center));
                }
                if response.clicked() {
                    chosen = Some(row.choice.clone());
                }
                shown_any = true;
            }
            if !shown_any {
                ui.label(RichText::new("Nothing matches the filter").weak());
            }
        });
    state.reveal = None;
    chosen
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

/// The built-in tones (by family, then the user's saved ones), with the current tone
/// highlighted and the tone its preset starts on marked.
pub(super) fn tone_rows(
    selected: Option<&PickerChoice>,
    default_tone: Option<&str>,
    saved: &[ChainPreset],
) -> Vec<PickerRow> {
    let mut rows: Vec<PickerRow> = Vec::new();
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
            default: false,
            choice,
        });
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
    fn the_tone_rows_hold_every_built_in_tone_then_the_saved_ones() {
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
        assert_eq!(rows.last().and_then(|row| row.group), Some("Saved"));
        assert_eq!(rows.last().map(|row| row.label.as_str()), Some("My Lead"));
        // Exactly one row is current, and the preset's default is marked separately.
        assert_eq!(rows.iter().filter(|row| row.selected).count(), 1);
        assert_eq!(rows.iter().filter(|row| row.default).count(), 1);
        assert!(
            rows.iter()
                .find(|row| row.default)
                .is_some_and(|row| row.label == "Modern Metal Rhythm")
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
                "Clean",
                "Crunch",
                "Lead",
                "High gain",
                "Acoustic",
                "Bass",
                "Saved"
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
