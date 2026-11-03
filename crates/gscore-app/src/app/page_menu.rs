//! The note, bar and track menus: in the score's right-click menu (after the edit
//! commands, before the panels) and in the menu bar. They act on the cursor (or the
//! selection); a right-click first moves the cursor to what was clicked, unless it is inside
//! the selection.

use gscore_edit::{ActionId, Command, Keymap};
use gscore_model::{MeasureIndex, TrackIndex};

use super::GuitarScoreApp;
use super::edit_effects::action_is_on;
use super::edit_palette::PaletteSnapshot;
use super::grid::GridAction;
use crate::state::{MeasureSelection, ViewState};

/// What was chosen in the menu, run after the page is drawn.
#[derive(Debug)]
pub(super) enum MenuChoice {
    /// An editor or palette action, as if its key was pressed.
    Action(ActionId),
    /// A track or bar command, or a track window, like the track grid's menu.
    Grid(GridAction),
}

/// One of the menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    /// The note (and beat) at the cursor.
    Note,
    /// The cursor's bar.
    Bar,
    /// The shown track.
    Track,
}

/// What the menu needs to know about the song at the cursor.
pub(super) struct MenuTarget<'a> {
    /// The palette's view of the cursor (or selection): its notes, beats and effects.
    pub(super) snapshot: &'a PaletteSnapshot,
    /// The shown track and how many there are.
    pub(super) track: usize,
    pub(super) tracks: usize,
    /// The cursor's bar and how many there are.
    pub(super) measure: usize,
    pub(super) measures: usize,
}

/// Note techniques, toggled like their palette tiles.
const TECHNIQUES: &[ActionId] = &[
    ActionId::ToggleDeadNote,
    ActionId::ToggleGhostNote,
    ActionId::ToggleHammer,
    ActionId::TogglePalmMute,
    ActionId::ToggleLetRing,
    ActionId::ToggleStaccato,
    ActionId::ToggleAccent,
    ActionId::ToggleHeavyAccent,
    ActionId::ToggleVibrato,
    ActionId::ToggleWideVibrato,
    ActionId::ToggleTrill,
    ActionId::ToggleTremoloPicking,
];

const SLIDES: &[ActionId] = &[
    ActionId::ToggleShiftSlide,
    ActionId::ToggleLegatoSlide,
    ActionId::ToggleSlideInBelow,
    ActionId::ToggleSlideInAbove,
    ActionId::ToggleSlideOutDown,
    ActionId::ToggleSlideOutUp,
];

const HARMONICS: &[ActionId] = &[
    ActionId::ToggleNaturalHarmonic,
    ActionId::ToggleArtificialHarmonic,
    ActionId::ToggleTappedHarmonic,
    ActionId::TogglePinchHarmonic,
    ActionId::ToggleSemiHarmonic,
];

/// The content of one menu.
pub(super) fn section(
    ui: &mut egui::Ui,
    section: Section,
    target: &MenuTarget<'_>,
    keymap: &Keymap,
    choices: &mut Vec<MenuChoice>,
) {
    let mut menu = Menu {
        keymap,
        snapshot: target.snapshot,
        choices,
    };
    match section {
        Section::Note => menu.note(ui),
        Section::Bar => menu.bar(ui, target),
        Section::Track => menu.track(ui, target),
    }
}

/// Runs `show` with the menu target at the document's cursor. The snapshot is only taken
/// here, while a menu is open.
pub(super) fn with_target<R>(
    document: &crate::document::Document,
    show: impl FnOnce(&MenuTarget<'_>) -> R,
) -> R {
    let editor = document.editor();
    let at = editor.cursor();
    let snapshot = PaletteSnapshot::of(document.song(), at, editor.selection());
    show(&MenuTarget {
        snapshot: &snapshot,
        track: at.track.get(),
        tracks: document.song().track_count(),
        measure: at.measure.get(),
        measures: document.song().measure_count(),
    })
}

impl GuitarScoreApp {
    /// Runs what was chosen in a menu, after drawing.
    pub(super) fn run_menu_choices(&mut self, choices: Vec<MenuChoice>, ctx: &egui::Context) {
        for choice in choices {
            match choice {
                MenuChoice::Action(action) => {
                    self.perform_action(gscore_edit::Action::Plain(action), ctx);
                }
                MenuChoice::Grid(action) => self.apply_grid_action(action),
            }
        }
    }
}

/// The score's right-click menu.
pub(super) fn show(
    ui: &mut egui::Ui,
    target: &MenuTarget<'_>,
    keymap: &Keymap,
    view: &mut ViewState,
    choices: &mut Vec<MenuChoice>,
) {
    let mut menu = Menu {
        keymap,
        snapshot: target.snapshot,
        choices,
    };
    for action in [ActionId::Cut, ActionId::Copy, ActionId::Paste] {
        menu.action(ui, action, None, true);
    }
    ui.separator();
    for (title, section) in [
        ("Note", Section::Note),
        ("Bar", Section::Bar),
        ("Track", Section::Track),
    ] {
        ui.menu_button(title, |ui| match section {
            Section::Note => menu.note(ui),
            Section::Bar => menu.bar(ui, target),
            Section::Track => menu.track(ui, target),
        });
    }
    ui.separator();
    ui.menu_button("Panels", |ui| {
        ui.checkbox(&mut view.show_track_grid, "Track grid");
        ui.checkbox(&mut view.show_track_panel, "Track panel");
        ui.checkbox(&mut view.show_edit_panel, "Edit palette");
    });
}

struct Menu<'a> {
    keymap: &'a Keymap,
    snapshot: &'a PaletteSnapshot,
    choices: &'a mut Vec<MenuChoice>,
}

impl Menu<'_> {
    /// An entry for `action` with its keys; checked when it is a technique that is on.
    fn action(&mut self, ui: &mut egui::Ui, action: ActionId, label: Option<&str>, enabled: bool) {
        let label = label.unwrap_or(action.label());
        let keys: Vec<String> = self
            .keymap
            .keys_for(action)
            .iter()
            .map(ToString::to_string)
            .collect();
        let button = match action_is_on(action, self.snapshot) {
            Some(on) => egui::Button::selectable(on, label),
            None => egui::Button::new(label),
        }
        .shortcut_text(keys.join(", "));
        if ui.add_enabled(enabled, button).clicked() {
            self.choices.push(MenuChoice::Action(action));
            ui.close();
        }
    }

    /// An entry that runs a grid action.
    fn grid(&mut self, ui: &mut egui::Ui, label: &str, enabled: bool, action: GridAction) {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            self.choices.push(MenuChoice::Grid(action));
            ui.close();
        }
    }

    fn note(&mut self, ui: &mut egui::Ui) {
        let notes = !self.snapshot.note_targets.is_empty();
        let beats = !self.snapshot.beats.is_empty();
        self.action(ui, ActionId::ToggleBend, Some("Bend…"), notes);
        self.action(ui, ActionId::ToggleWhammy, Some("Whammy bar…"), beats);
        ui.menu_button("Techniques", |ui| {
            for &action in TECHNIQUES {
                self.action(ui, action, None, notes);
            }
        });
        ui.menu_button("Slides", |ui| {
            for &action in SLIDES {
                self.action(ui, action, None, notes);
            }
        });
        ui.menu_button("Harmonics", |ui| {
            for &action in HARMONICS {
                self.action(ui, action, None, notes);
            }
        });
        ui.separator();
        self.action(ui, ActionId::ToggleTie, None, beats);
        self.action(ui, ActionId::MakeRest, Some("Make rest"), beats);
        self.action(ui, ActionId::ClearNote, Some("Delete note"), notes);
        ui.separator();
        self.action(ui, ActionId::DurationShorter, None, beats);
        self.action(ui, ActionId::DurationLonger, None, beats);
        self.action(ui, ActionId::InsertBeat, None, true);
        self.action(ui, ActionId::DeleteBeat, None, beats);
    }

    fn bar(&mut self, ui: &mut egui::Ui, target: &MenuTarget<'_>) {
        self.action(ui, ActionId::InsertMeasure, Some("Insert bar before"), true);
        self.grid(
            ui,
            "Insert bar after",
            true,
            GridAction::Command {
                command: Command::InsertMeasure {
                    measure: MeasureIndex::new(target.measure.saturating_add(1)),
                },
                merge: false,
            },
        );
        self.action(
            ui,
            ActionId::DeleteMeasure,
            Some("Delete bar"),
            target.measures > 1,
        );
        ui.separator();
        self.grid(
            ui,
            "Select bar",
            true,
            GridAction::Select(MeasureSelection::new(target.measure)),
        );
    }

    fn track(&mut self, ui: &mut egui::Ui, target: &MenuTarget<'_>) {
        let track = target.track;
        let index = TrackIndex::new(track);
        self.grid(ui, "Edit track…", true, GridAction::ShowTrack { track });
        self.grid(ui, "Rename…", true, GridAction::Rename { track });
        ui.separator();
        self.grid(
            ui,
            "Add track above…",
            true,
            GridAction::NewTrack { at: track },
        );
        self.grid(
            ui,
            "Add track below…",
            true,
            GridAction::NewTrack {
                at: track.saturating_add(1),
            },
        );
        self.grid(
            ui,
            "Duplicate track",
            true,
            GridAction::CommandShowing {
                command: Command::DuplicateTrack { track: index },
                track: track.saturating_add(1),
            },
        );
        ui.separator();
        self.grid(
            ui,
            "Move up",
            track > 0,
            GridAction::CommandShowing {
                command: Command::MoveTrack {
                    from: index,
                    to: TrackIndex::new(track.saturating_sub(1)),
                },
                track: track.saturating_sub(1),
            },
        );
        self.grid(
            ui,
            "Move down",
            track.saturating_add(1) < target.tracks,
            GridAction::CommandShowing {
                command: Command::MoveTrack {
                    from: index,
                    to: TrackIndex::new(track.saturating_add(1)),
                },
                track: track.saturating_add(1),
            },
        );
        ui.separator();
        self.grid(
            ui,
            "Delete track",
            target.tracks > 1,
            GridAction::Command {
                command: Command::DeleteTrack { track: index },
                merge: false,
            },
        );
    }
}
