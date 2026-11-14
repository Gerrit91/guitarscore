//! A delay block's knobs in the Sound chain window: its time (a note value synced to the
//! song's tempo, or free in milliseconds), feedback, echo level, cuts and ping-pong.

use gscore_guitar::{DelayNote, knob_ranges};

use super::string_sound::round_knob;
use crate::chain_settings::{BlockSetting, DelayNoteSetting};

/// What a note value means, for its tooltip.
fn note_tip(note: DelayNote) -> String {
    let kind = match note {
        DelayNote::DottedQuarter | DelayNote::DottedEighth | DelayNote::DottedSixteenth => {
            "dotted: the classic rhythmic echo between the beats"
        }
        DelayNote::QuarterTriplet | DelayNote::EighthTriplet | DelayNote::SixteenthTriplet => {
            "a triplet"
        }
        _ => "straight",
    };
    format!(
        "Echoes every {} of a quarter note at the song's tempo ({kind})",
        match note {
            DelayNote::Half => "2",
            DelayNote::DottedQuarter => "1.5",
            DelayNote::Quarter => "1",
            DelayNote::QuarterTriplet => "2/3",
            DelayNote::DottedEighth => "3/4",
            DelayNote::Eighth => "1/2",
            DelayNote::EighthTriplet => "1/3",
            DelayNote::DottedSixteenth => "3/8",
            DelayNote::Sixteenth => "1/4",
            DelayNote::SixteenthTriplet => "1/6",
        }
    )
}

/// The delay's time: Sync or Free, then the note values or the milliseconds.
fn time(ui: &mut egui::Ui, block: &mut BlockSetting) -> bool {
    let mut changed = false;
    let items = [
        (
            "Sync",
            "Sync",
            "The time is a note value at the song's tempo: it follows tempo changes and the \
             playback speed",
            true,
        ),
        ("Free", "Free", "The time in milliseconds", true),
    ];
    let chosen = usize::from(!block.delay_sync);
    if let Some(index) = super::tiles::segments_with_tips_vertical(ui, &items, chosen) {
        let sync = index == 0;
        if sync != block.delay_sync {
            block.delay_sync = sync;
            changed = true;
        }
    }
    ui.add_space(6.0);
    if block.delay_sync {
        // Two rows: the longer values, then the shorter ones.
        for row in DelayNote::ALL.chunks(5) {
            let tips: Vec<String> = row.iter().map(|note| note_tip(*note)).collect();
            let items: Vec<(&str, &str, &str, bool)> = row
                .iter()
                .zip(&tips)
                .map(|(note, tip)| (note.label(), note.label(), tip.as_str(), true))
                .collect();
            let chosen = row
                .iter()
                .position(|note| block.delay_note == DelayNoteSetting::from(*note))
                .unwrap_or(0);
            if let Some(index) = super::tiles::segments_with_tips_enabled(ui, &items, chosen)
                && let Some(note) = row.get(index)
            {
                block.delay_note = DelayNoteSetting::from(*note);
                changed = true;
            }
            ui.add_space(2.0);
        }
        ui.add_space(4.0);
    } else {
        changed |= round_knob(
            ui,
            &mut block.delay_ms,
            knob_ranges::DELAY_MS,
            "Time",
            " ms",
            350.0,
            true,
            "The time between echoes, in milliseconds.",
        )
        .changed;
    }
    changed
}

/// A delay's knobs; whether anything changed.
pub(super) fn delay_knobs(ui: &mut egui::Ui, block: &mut BlockSetting) -> bool {
    let mut changed = time(ui, block);
    for (value, range, name, unit, default, log, tip) in [
        (
            &mut block.delay_feedback,
            knob_ranges::FEEDBACK,
            "Feedback",
            "",
            0.35,
            false,
            "How much of each echo comes back as the next: 0 a single echo, high a long \
             trail.",
        ),
        (
            &mut block.mix,
            knob_ranges::MIX,
            "Level",
            "",
            0.35,
            false,
            "How loud the echoes are under the guitar (the guitar itself stays as it is).",
        ),
        (
            &mut block.low_cut,
            knob_ranges::LOW_CUT,
            "Low cut",
            " Hz",
            120.0,
            true,
            "The echoes lose their lows below this, more with every repeat: they stay out of \
             the bass's way.",
        ),
        (
            &mut block.high_cut,
            knob_ranges::HIGH_CUT,
            "High cut",
            " Hz",
            6_000.0,
            true,
            "The echoes lose their highs above this, more with every repeat: lower is \
             darker and sits further back.",
        ),
        (
            &mut block.width,
            knob_ranges::WIDTH,
            "Ping-pong",
            "",
            0.0,
            false,
            "0: the echoes stay in the middle; 1: they bounce from left to right.",
        ),
    ] {
        changed |= round_knob(ui, value, range, name, unit, default, log, tip).changed;
    }
    changed
}
