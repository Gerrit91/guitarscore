//! The palette's dynamics: ppp to fff as tiles. A click sets the dynamic of the selected
//! beats that have notes (one `set_dynamic` each, a batch for several); notes entered
//! afterwards take the dynamic in effect, as in Guitar Pro.

use gscore_edit::Command;
use gscore_model::{BeatPosition, Dynamic, Song};

use super::style;
use super::tiles::{self, Face};

/// What the dynamics tiles show, taken before the widgets run.
#[derive(Debug, Clone)]
pub(super) struct DynamicsSnapshot {
    /// The selected beats that have notes (a dynamic needs notes), with their dynamic.
    targets: Vec<(BeatPosition, Dynamic)>,
    /// Their dynamic, if they all share one.
    shared: Option<Dynamic>,
    /// The dynamic in effect at the cursor (its beat's own, or the last one before it).
    in_effect: Dynamic,
}

impl DynamicsSnapshot {
    /// The dynamics of `beats` (the selection, or the cursor's beat) and at `cursor`.
    pub(super) fn of(song: &Song, beats: &[BeatPosition], cursor: BeatPosition) -> Self {
        let targets: Vec<(BeatPosition, Dynamic)> = beats
            .iter()
            .filter_map(|at| Some((*at, song.beat(*at)?.dynamic()?)))
            .collect();
        let shared = targets
            .first()
            .map(|(_, first)| *first)
            .filter(|first| targets.iter().all(|(_, d)| d == first));
        DynamicsSnapshot {
            targets,
            shared,
            in_effect: song.dynamic_at(cursor),
        }
    }

    /// The commands that give every target `dynamic` (none for beats that have it).
    fn commands(&self, dynamic: Dynamic) -> Vec<Command> {
        self.targets
            .iter()
            .filter(|(_, current)| *current != dynamic)
            .map(|(at, _)| Command::SetDynamic { at: *at, dynamic })
            .collect()
    }
}

/// The SMuFL character of a dynamic mark.
fn glyph(dynamic: Dynamic) -> &'static str {
    match dynamic {
        Dynamic::Ppp => "\u{E52A}",
        Dynamic::Pp => "\u{E52B}",
        Dynamic::P => "\u{E520}",
        Dynamic::Mp => "\u{E52C}",
        Dynamic::Mf => "\u{E52D}",
        Dynamic::F => "\u{E522}",
        Dynamic::Ff => "\u{E52F}",
        Dynamic::Fff => "\u{E530}",
    }
}

/// The dynamics section: one tile per mark; the selected beats' mark is highlighted.
pub(super) fn section(ui: &mut egui::Ui, snapshot: &DynamicsSnapshot, commands: &mut Vec<Command>) {
    style::section(ui, "dynamics", "Dynamics", |ui| {
        let enabled = !snapshot.targets.is_empty();
        if !enabled {
            ui.label(egui::RichText::new("Put the cursor on a note").weak());
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 3.0);
            for dynamic in Dynamic::ALL {
                let face = Face::Glyph {
                    text: glyph(dynamic),
                    size: 20.0,
                    baseline: 3.0,
                };
                let label = format!("{} ({})", dynamic.id(), dynamic.name());
                let tip = format!(
                    "{label}: how loud the selected notes are played; the notes entered \
                     after them follow"
                );
                let on = enabled && snapshot.shared == Some(dynamic);
                if tiles::tile(ui, face, on, enabled, &label, &tip).clicked() {
                    commands.extend(snapshot.commands(dynamic));
                }
            }
        });
        let note = format!(
            "In effect here: {} ({})",
            snapshot.in_effect.id(),
            snapshot.in_effect.name()
        );
        ui.label(egui::RichText::new(note).color(style::tokens(ui).text_weak));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gscore_model::{BeatIndex, Fret, MeasureIndex, StringNumber, TrackIndex, VoiceIndex};

    fn at(beat: usize) -> BeatPosition {
        BeatPosition {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(beat),
        }
    }

    #[test]
    fn only_beats_with_notes_get_a_dynamic() {
        let mut song = gscore_model::testing::one_measure().unwrap();
        for beat in [0, 2] {
            let _ = song
                .set_note(
                    at(beat),
                    StringNumber::new(1).unwrap(),
                    Fret::new(3).unwrap(),
                )
                .unwrap();
        }
        let _ = song.set_dynamic(at(2), Dynamic::P).unwrap();
        let snapshot = DynamicsSnapshot::of(&song, &[at(0), at(1), at(2)], at(0));
        assert_eq!(
            snapshot.targets,
            [(at(0), Dynamic::Mf), (at(2), Dynamic::P)]
        );
        assert_eq!(snapshot.shared, None, "mf and p");
        assert_eq!(snapshot.in_effect, Dynamic::Mf);
        // The beat that is already piano needs no command.
        assert_eq!(
            snapshot.commands(Dynamic::P),
            [Command::SetDynamic {
                at: at(0),
                dynamic: Dynamic::P
            }]
        );
        let rest_only = DynamicsSnapshot::of(&song, &[at(1)], at(1));
        assert!(rest_only.targets.is_empty());
        assert_eq!(rest_only.in_effect, Dynamic::Mf);
    }
}
