//! Latency compensation (M24): a track whose chain hands its sound back late (a plugin's
//! latency) is not heard late. The other tracks are held back by as much, so notes played
//! together come out together, and the meters say how much the mix is held back.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::collections::VecDeque;
use std::sync::Arc;

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{ChainKnobs, SoundBank, SynthEngine};
use gscore_guitar::{
    BLOCKS, BlockKind, BlockSettings, ChainSpec, External, ExternalBlock, ExternalSource,
};
use gscore_model::TrackIndex;
use gscore_model::testing::{self, TestNote};
use gscore_play::Sequence;

const RATE: u32 = 48_000;

/// A plugin stand-in: hands its sound back `frames` late, and says so.
struct Late {
    frames: usize,
    line: VecDeque<(f32, f32)>,
}

impl ExternalBlock for Late {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.line.push_back((*l, *r));
            let (out_l, out_r) = if self.line.len() > self.frames {
                self.line.pop_front().unwrap()
            } else {
                (0.0, 0.0)
            };
            *l = out_l;
            *r = out_r;
        }
    }

    fn reset(&mut self) {
        self.line.clear();
    }

    fn latency(&self) -> u32 {
        self.frames as u32
    }
}

struct LateSource(usize);

impl ExternalSource for LateSource {
    fn key(&self) -> &str {
        "late"
    }

    fn instance(&self, _rate: u32) -> Option<Box<dyn ExternalBlock>> {
        Some(Box::new(Late {
            frames: self.0,
            line: VecDeque::with_capacity(self.0 + 4096),
        }))
    }
}

/// The first frame of `samples` that is not silent.
fn onset(samples: &[f32]) -> usize {
    samples.iter().position(|s| s.abs() > 1e-4).unwrap()
}

/// Two tracks playing a note together, the first panned left, the second right; the
/// second's chain hands its sound back `late` frames late (bypassed when `bypass`).
fn onsets(late: usize, bypass: bool) -> (usize, usize, u32) {
    let mut song = testing::song_with_notes_on_tracks(2, &[(1, &[TestNote::new(3, 2)])]).unwrap();
    let _ = song.set_track_pan(TrackIndex::new(0), 0).unwrap();
    let _ = song.set_track_pan(TrackIndex::new(1), 127).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let bank = SoundBank::read(&mut sine_soundfont("Test Sine").as_slice()).unwrap();
    let mut engine = SynthEngine::new(&bank, RATE).unwrap();
    engine.load(Arc::new(sequence));
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::External(External(Arc::new(LateSource(late)))));
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].bypass = bypass;
    let knobs = ChainKnobs {
        settings,
        output_db: 0.0,
    };
    let chain = gscore_audio::prepare_chain(&spec, &knobs, RATE);
    assert!(engine.set_chain(1, chain, knobs).is_none());
    engine.play();
    let frames = RATE as usize / 2;
    let (mut left, mut right) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    engine.render(&mut left, &mut right);
    (onset(&left), onset(&right), engine.meters().compensation())
}

#[test]
fn a_late_track_and_the_others_come_out_together() {
    let (left, right, held) = onsets(300, false);
    assert_eq!(held, 300, "the mix is held back by the plugin's latency");
    assert!(
        left.abs_diff(right) <= 2,
        "the notes start together: left {left}, right {right}"
    );
    // Without a late block the notes start at once.
    let (plain_left, plain_right, plain_held) = onsets(0, false);
    assert_eq!(plain_held, 0);
    assert!(plain_left.abs_diff(plain_right) <= 2);
    assert!(
        left.abs_diff(plain_left + 300) <= 2,
        "the mix comes 300 frames later: {left} against {plain_left}"
    );
}

#[test]
fn a_bypassed_late_block_holds_nothing_back() {
    let (left, right, held) = onsets(300, true);
    assert_eq!(held, 0);
    assert!(left.abs_diff(right) <= 2, "left {left}, right {right}");
}
