//! The Sound chain window's Plugin block (M24, experimental): choosing one of the CLAP
//! effects installed on this computer, then its parameters as sliders in GuitarScore's
//! own style (with a filter for plugins with many). What the plugin thread knows is read
//! before the window is drawn ([`PluginPanel`]), so drawing never waits for it.

use egui::{RichText, vec2};

use crate::chain_settings::{BlockChoice, ChainSetting};
use crate::plugins::{Found, Installed, PluginView};

use super::GuitarScoreApp;
use super::style;

/// Above this many parameters, the view offers a filter.
const FILTER_FROM: usize = 8;
/// The height of the plugin list and of the parameters, scrolled beyond.
const LIST_HEIGHT: f32 = 260.0;
/// A parameter slider's width.
const SLIDER_WIDTH: f32 = 260.0;

/// What the window shows of the Plugin block in its selected place.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PluginPanel {
    pub(super) place: usize,
    /// Why it does not play, if it does not.
    problem: Option<String>,
    /// Its parameters (when a plugin is chosen and the list is not asked for).
    view: Option<Found<PluginView>>,
    /// The installed effects (while choosing).
    installed: Option<Found<Installed>>,
    /// Plugins are switched off in the settings.
    off: bool,
    /// Whether the plugin's own editor is open, or why it could not open.
    editor: Result<bool, String>,
}

impl PluginPanel {
    /// Whether an answer of the plugin thread is still to come (the window redraws until
    /// it is there).
    pub(super) fn waiting(&self) -> bool {
        matches!(self.view, Some(Found::Waiting))
            || matches!(self.installed, Some(Found::Waiting))
            // An editor asked for: its answer comes from the host.
            || self.editor == Ok(true)
    }
}

/// What the view changed.
#[derive(Debug, Default)]
pub(super) struct PluginEdit {
    /// The chain with the block changed.
    pub(super) chain: Option<ChainSetting>,
    /// Look for installed plugins again.
    pub(super) rescan: bool,
    /// Show the list of plugins (true) or the chosen one's parameters again (false).
    pub(super) choosing: Option<bool>,
    /// Start the plugin again (it crashed, hung or could not start).
    pub(super) restart: bool,
    /// Open (true) or close (false) the plugin's own editor.
    pub(super) editor: Option<bool>,
}

impl GuitarScoreApp {
    /// Settings changed in the plugins' own editors: each goes into its track's tone (one
    /// undo step), the running plugin kept as it is.
    pub(super) fn take_plugin_states(&mut self) {
        for ((track, place), state) in self.playback.plugins().take_states() {
            let Some(player) = self.track_player(track) else {
                continue;
            };
            let kind = match player.engine {
                gscore_play::TrackEngine::Modelled(kind) => kind,
                _ => player.preset_kind.unwrap_or(gscore_play::StringKind::Steel),
            };
            let mut tone = self.track_chain_tone(track, &player, kind);
            let Some(plugin) = tone
                .chain
                .blocks
                .get_mut(place)
                .and_then(|block| block.plugin.as_mut())
            else {
                continue;
            };
            let encoded = crate::base64::encode(&state);
            if plugin.state == encoded {
                continue;
            }
            // The settings hold every parameter: none is set apart any more.
            plugin.state.clone_from(&encoded);
            plugin.values.clear();
            self.playback.accept_plugin_state(track, place, &encoded);
            self.set_track_tone(track, tone);
        }
    }

    /// What the plugin thread knows of the Plugin block at `place` of `track`'s chain
    /// `chain`; `choosing`: the list of plugins is asked for. `None` if the place holds
    /// another block.
    pub(super) fn plugin_panel(
        &mut self,
        track: usize,
        chain: &ChainSetting,
        place: usize,
        choosing: bool,
    ) -> Option<PluginPanel> {
        let block = chain.blocks.get(place)?;
        if block.choice != BlockChoice::Plugin {
            return None;
        }
        let off = !self.playback.plugins_enabled();
        let plugins = self.playback.plugins();
        let choosing = choosing || block.plugin.is_none();
        let installed = (choosing && !off).then(|| plugins.installed().cloned_found());
        let view = match (&block.plugin, choosing || off) {
            (Some(plugin), false) => Some(plugins.view(plugin).cloned_found()),
            _ => None,
        };
        let problem = (!off).then(|| plugins.problem((track, place))).flatten();
        let editor = plugins.editor((track, place));
        Some(PluginPanel {
            place,
            problem,
            view,
            installed,
            off,
            editor,
        })
    }
}

/// A borrowed answer as an owned one (the panel outlives this frame's borrow).
trait ClonedFound<T> {
    fn cloned_found(self) -> Found<T>;
}

impl<T: Clone> ClonedFound<T> for Found<&T> {
    fn cloned_found(self) -> Found<T> {
        match self {
            Found::Waiting => Found::Waiting,
            Found::Ready(value) => Found::Ready(value.clone()),
            Found::Failed(error) => Found::Failed(error),
        }
    }
}

/// The Plugin block's view: the list to choose from, or the chosen plugin's parameters.
/// `filter` is the text typed into the filter (kept by the window).
pub(super) fn show(
    ui: &mut egui::Ui,
    chain: &ChainSetting,
    panel: &PluginPanel,
    filter: &mut String,
) -> PluginEdit {
    let mut edit = PluginEdit::default();
    let Some(block) = chain.blocks.get(panel.place) else {
        return edit;
    };
    let tokens = style::tokens(ui);
    if panel.off {
        ui.label(RichText::new(block.plugin.as_ref().map_or("Plugin", |p| &p.name)).strong());
        ui.label(
            RichText::new(
                "Plugins are off: this place passes the sound unchanged. Switch on \"CLAP \
                 plugins (experimental)\" in Settings › Audio I/O to hear it.",
            )
            .weak(),
        );
        return edit;
    }
    if let Some(installed) = &panel.installed {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Choose a plugin").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if block.plugin.is_some() && ui.small_button("Back").clicked() {
                    edit.choosing = Some(false);
                }
                if ui
                    .small_button("Look again")
                    .on_hover_text("Look for installed plugins again (after installing one)")
                    .clicked()
                {
                    edit.rescan = true;
                }
            });
        });
        filter_field(ui, filter, "Filter plugins");
        match installed {
            Found::Waiting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Looking for installed plugins…").weak());
                });
            }
            Found::Failed(error) => {
                ui.label(RichText::new(error).color(tokens.mute));
            }
            Found::Ready(installed) => {
                if let Some(listed) = plugin_list(ui, installed, filter) {
                    let mut new_chain = chain.clone();
                    if let Some(slot) = new_chain.blocks.get_mut(panel.place) {
                        slot.plugin = Some(Box::new(listed.block()));
                    }
                    edit.chain = Some(new_chain);
                    edit.choosing = Some(false);
                    filter.clear();
                }
            }
        }
        return edit;
    }
    let Some(plugin) = &block.plugin else {
        return edit;
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(&plugin.name).strong());
        if !plugin.vendor.is_empty() {
            ui.label(RichText::new(&plugin.vendor).weak());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Change…")
                .on_hover_text("Put another plugin in this place")
                .clicked()
            {
                edit.choosing = Some(true);
                filter.clear();
            }
            // The plugin's own editor, in a window of its own beside GuitarScore's.
            let open = matches!(panel.editor, Ok(true));
            let label = if open { "Close editor" } else { "Open editor" };
            if ui
                .button(label)
                .on_hover_text(
                    "The plugin's own window, with everything it offers (models, impulse \
                     responses, presets). What you set there is kept with the tone.",
                )
                .clicked()
            {
                edit.editor = Some(!open);
            }
            if !plugin.values.is_empty()
                && ui
                    .small_button("Reset")
                    .on_hover_text("Every parameter back to the plugin's saved settings")
                    .clicked()
            {
                let mut new_chain = chain.clone();
                if let Some(slot) = new_chain.blocks.get_mut(panel.place)
                    && let Some(plugin) = &mut slot.plugin
                {
                    plugin.values.clear();
                }
                edit.chain = Some(new_chain);
            }
        });
    });
    if let Err(why) = &panel.editor {
        ui.label(RichText::new(format!("Its editor could not open: {why}")).color(tokens.mute));
    }
    if let Some(problem) = &panel.problem {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(problem).color(tokens.mute));
            if ui
                .small_button("Restart")
                .on_hover_text("Start the plugin again (its settings as saved)")
                .clicked()
            {
                edit.restart = true;
            }
        });
    }
    match &panel.view {
        None | Some(Found::Waiting) => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Asking the plugin for its parameters…").weak());
            });
        }
        Some(Found::Failed(error)) => {
            ui.label(RichText::new(error).color(tokens.mute));
        }
        Some(Found::Ready(view)) => {
            if view.latency > 0 {
                ui.label(
                    RichText::new(format!("Latency {} samples", view.latency))
                        .small()
                        .weak(),
                );
            }
            if view.params.is_empty() {
                ui.label(RichText::new("This plugin has no parameters to set here.").weak());
                return edit;
            }
            if view.params.len() > FILTER_FROM {
                filter_field(ui, filter, "Filter parameters");
            }
            let mut values = plugin.clone();
            if params(ui, view, &mut values, filter) {
                let mut new_chain = chain.clone();
                if let Some(slot) = new_chain.blocks.get_mut(panel.place) {
                    slot.plugin = Some(values);
                }
                edit.chain = Some(new_chain);
            }
        }
    }
    edit
}

/// A one-line filter with a clear button.
fn filter_field(ui: &mut egui::Ui, filter: &mut String, label: &str) {
    ui.horizontal(|ui| {
        let edit = ui.add(
            egui::TextEdit::singleline(filter)
                .hint_text("Filter")
                .desired_width(200.0),
        );
        ui.ctx()
            .accesskit_node_builder(edit.id, |node| node.set_label(label));
        if !filter.is_empty() && ui.small_button("×").clicked() {
            filter.clear();
        }
    });
}

/// Whether `text` matches what was typed into the filter (every word, any case).
fn matches(filter: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    filter
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

/// The installed effects as a list; the one clicked.
fn plugin_list<'a>(
    ui: &mut egui::Ui,
    installed: &'a Installed,
    filter: &str,
) -> Option<&'a crate::plugins::Listed> {
    let mut chosen = None;
    if installed.effects.is_empty() {
        ui.label(
            RichText::new(
                "No CLAP effects are installed: put .clap files in ~/.clap or /usr/lib/clap \
                 (or a folder named in CLAP_PATH), then Look again.",
            )
            .weak(),
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("plugin list")
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for listed in &installed.effects {
                let text = format!(
                    "{} {} {}",
                    listed.name,
                    listed.vendor,
                    listed.features.join(" ")
                );
                if !matches(filter, &text) {
                    continue;
                }
                let row = ui
                    .add(
                        egui::Button::new(RichText::new(&listed.name).strong())
                            .right_text(RichText::new(&listed.vendor).weak())
                            .min_size(vec2(ui.available_width(), 0.0))
                            .frame(false),
                    )
                    .on_hover_text(format!(
                        "{}\n{}\n{}",
                        listed.id,
                        listed.features.join(", "),
                        listed.library.display()
                    ));
                if row.clicked() {
                    chosen = Some(listed);
                }
            }
        });
    for problem in &installed.problems {
        ui.label(RichText::new(problem).small().weak());
    }
    chosen
}

/// The parameters as sliders, each over its own range. True if one moved.
fn params(
    ui: &mut egui::Ui,
    view: &PluginView,
    plugin: &mut crate::chain_settings::PluginBlock,
    filter: &str,
) -> bool {
    let mut moved = false;
    egui::ScrollArea::vertical()
        .id_salt("plugin parameters")
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("plugin parameter grid")
                .num_columns(2)
                .spacing(vec2(12.0, 4.0))
                .show(ui, |ui| {
                    ui.spacing_mut().slider_width = SLIDER_WIDTH;
                    for param in &view.params {
                        // Plugins use the module (a group, "Filter/Low") inconsistently,
                        // some for an internal name: it is in the tooltip and the filter.
                        if !matches(filter, &format!("{} {}", param.name, param.module)) {
                            continue;
                        }
                        let label = ui.label(&param.name);
                        if !param.module.is_empty() {
                            label.on_hover_text(&param.module);
                        }
                        let mut value = plugin.value(param.id).unwrap_or(param.value);
                        let slider = egui::Slider::new(&mut value, param.min..=param.max)
                            .clamping(egui::SliderClamping::Always);
                        let slider = if param.stepped {
                            slider.step_by(1.0).fixed_decimals(0)
                        } else {
                            slider
                        };
                        let response = ui
                            .add(slider)
                            .on_hover_text("Double-click: back to the plugin's default");
                        if response.double_clicked() {
                            plugin.set_value(param.id, param.default);
                            moved = true;
                        } else if response.changed() {
                            plugin.set_value(param.id, value);
                            moved = true;
                        }
                        ui.end_row();
                    }
                });
        });
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_matches_every_word_in_any_case() {
        assert!(matches("", "Room Reverb"));
        assert!(matches("room rev", "Dragonfly Room Reverb"));
        assert!(!matches("hall", "Dragonfly Room Reverb"));
    }
}
