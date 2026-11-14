//! The engine's mixer: every track a strip (its source, its chain, then fader and pan, then
//! the reverb send), the buses into the master, and real meters. Played offline with the
//! test SoundFont (a sine), so levels can be compared exactly.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::sync::Arc;

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{Bus, ChainKnobs, SoundBank, SynthEngine};
use gscore_guitar::{
    BLOCKS, BlockKind, BlockSettings, ChainSpec, CompRatio, CompressorKnobs, ReverbKnobs,
};
use gscore_model::testing::{self, TestNote};
use gscore_model::{Song, TrackIndex};
use gscore_play::{Channel, MidiEvent, Sequence, controller};

const RATE: u32 = 48_000;

/// The chain's knobs at the old unity output, for the calls below.
fn unity(settings: [BlockSettings; BLOCKS]) -> ChainKnobs {
    ChainKnobs {
        settings,
        output_db: 0.0,
    }
}

fn bank() -> SoundBank {
    SoundBank::read(&mut sine_soundfont("Test Sine").as_slice()).unwrap()
}

/// `tracks` guitar tracks, each playing a whole note A3 (two seconds).
fn song(tracks: usize) -> Song {
    testing::song_with_notes_on_tracks(tracks, &[(1, &[TestNote::new(3, 2)])]).unwrap()
}

fn engine(sequence: &Sequence) -> SynthEngine {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    engine.load(Arc::new(sequence.clone()));
    engine.play();
    engine
}

/// `seconds` of left and right.
fn render(engine: &mut SynthEngine, seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let frames = (seconds * RATE as f32) as usize;
    let (mut left, mut right) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    engine.render(&mut left, &mut right);
    (left, right)
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// The steady part of a note (after its attack).
fn steady(samples: &[f32]) -> &[f32] {
    &samples[RATE as usize / 4..RATE as usize]
}

#[test]
fn a_tracks_fader_and_pan_act_as_the_synthesizers_did() {
    // Full volume against half: the square law (64/127)², as General MIDI synthesizers
    // (and RustySynth before the mixer) apply it.
    let level = |volume: u8, pan: u8| {
        let mut song = song(1);
        let _ = song.set_track_volume(TrackIndex::new(0), volume).unwrap();
        let _ = song.set_track_pan(TrackIndex::new(0), pan).unwrap();
        let (left, right) = render(&mut engine(&Sequence::new(&song, None).unwrap()), 1.0);
        (rms(steady(&left)), rms(steady(&right)))
    };
    let (full, _) = level(127, 64);
    let (half, _) = level(64, 64);
    let expected = (64.0_f32 / 127.0).powi(2);
    assert!(
        (half / full - expected).abs() < 0.01,
        "{} against {expected}",
        half / full
    );
    // Hard left: nothing on the right, and as loud on the left as both sides in the
    // middle together (equal power).
    let (left, right) = level(127, 0);
    assert!(right < 1e-3 * left, "{right}");
    let (middle, _) = level(127, 64);
    assert!(
        (left / middle - std::f32::consts::SQRT_2).abs() < 0.02,
        "{}",
        left / middle
    );
}

#[test]
fn a_soundfont_track_has_its_own_chain() {
    // Two tracks, the second through an amp turned all the way down: only its meter and
    // its share of the mix drop.
    let song = song(2);
    let sequence = Sequence::new(&song, None).unwrap();
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Amp);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].amp.level_db = -24.0;
    settings[0].amp.voice = gscore_guitar::AmpVoice::Clean;
    settings[0].amp.gain = 0.0;
    let mut plain = engine(&sequence);
    let (plain_left, _) = render(&mut plain, 1.0);
    let mut chained = engine(&sequence);
    let chain = gscore_audio::prepare_chain(&spec, &unity(settings), RATE);
    assert!(chained.set_chain(1, chain, unity(settings)).is_none());
    let (chained_left, _) = render(&mut chained, 1.0);
    let meters = chained.meters();
    let first = meters.track(0).left;
    let second = meters.track(1).left;
    assert!(first > 0.05, "the first track plays: {first}");
    assert!(
        second < 0.2 * first,
        "the second is turned down: {second} against {first}"
    );
    let ratio = rms(steady(&chained_left)) / rms(steady(&plain_left));
    assert!(
        (0.45..0.7).contains(&ratio),
        "about half the mix is left: {ratio}"
    );
}

#[test]
fn the_meters_show_the_real_levels() {
    let mut song = song(2);
    let _ = song.set_track_volume(TrackIndex::new(1), 0).unwrap();
    let mut engine = engine(&Sequence::new(&song, None).unwrap());
    let (left, right) = render(&mut engine, 0.5);
    let meters = engine.meters();
    let (first_left, first_right) = (meters.track(0).left, meters.track(0).right);
    assert!(first_left > 0.05 && (first_left - first_right).abs() < 1e-3);
    assert_eq!(
        meters.track(1),
        gscore_audio::Level::default(),
        "a track at volume 0 shows nothing"
    );
    // The master's bar holds the loudest sample of the last 0.3 s of the mix.
    let master = meters.master().left;
    let tail = left[left.len() - 14_400..]
        .iter()
        .fold(0.0_f32, |peak, x| peak.max(x.abs()));
    assert!(
        (master - tail).abs() < 0.05 * tail,
        "{master} against {tail}"
    );
    assert!(right.iter().all(|x| x.is_finite()));
    // Stopped, the meters fall back toward silence.
    engine.stop();
    let _ = render(&mut engine, 4.0);
    let after = engine.meters().track(0).mark_left;
    assert!(after < 0.01 * first_left, "{after}");
}

#[test]
fn a_soundfont_tracks_reverb_send_reaches_the_bus() {
    // The track sends to the reverb bus with its Reverb knob (MIDI CC 91): the synth's
    // own reverb is off now, so without the bus nothing rings on after the note.
    let mut song = testing::song_with_notes(&[(4, &[TestNote::new(3, 2)]), (2, &[])]).unwrap();
    let _ = song.set_track_reverb(TrackIndex::new(0), 120).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Reverb);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].reverb = ReverbKnobs {
        size: 0.8,
        decay: 4.0,
        mix: 1.0,
        ..ReverbKnobs::default()
    };
    let tail = |bus: bool| {
        let mut engine = engine(&sequence);
        if bus {
            engine.set_reverb_bus(Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0))));
            engine.set_reverb_bus_settings(unity(settings), 127, false);
        }
        let (left, _) = render(&mut engine, 2.0);
        (
            rms(&left[RATE as usize + RATE as usize / 2..]),
            engine.meters().reverb().left,
        )
    };
    let (dry, _) = tail(false);
    let (wet, bus_meter) = tail(true);
    assert!(
        wet > 3.0 * dry.max(1e-6),
        "the bus rings on: {wet} against {dry}"
    );
    assert!(bus_meter > 0.0, "the bus's meter moves");
}

#[test]
fn a_live_fader_move_is_heard_at_once() {
    let song = song(1);
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    let mut engine = engine(&sequence);
    let (before, _) = render(&mut engine, 0.5);
    engine.send_event(MidiEvent::Control {
        channel,
        controller: controller::VOLUME,
        value: 0,
    });
    let (after, _) = render(&mut engine, 0.3);
    assert!(rms(&before[12_000..]) > 0.01);
    assert!(
        rms(&after[4_800..]) < 1e-4,
        "the fader closed: {}",
        rms(&after[4_800..])
    );
}

#[test]
fn tracks_sharing_a_channel_are_one_strip() {
    // Guitar Pro lets two tracks share a MIDI channel: the synthesizer makes one signal of
    // them. The first track's strip carries it (both notes); the second's stays silent,
    // and either track's volume moves the shared strip.
    let song = song(2);
    let mut sequence = Sequence::new(&song, None).unwrap();
    let shared = sequence.tracks[0].channel;
    let other = sequence.tracks[1].channel;
    sequence.tracks[1].channel = shared;
    let retarget = |channel: &mut Channel| {
        if *channel == other {
            *channel = shared;
        }
    };
    for event in &mut sequence.events {
        match &mut event.event {
            MidiEvent::NoteOn { channel, .. }
            | MidiEvent::NoteOff { channel, .. }
            | MidiEvent::ProgramChange { channel, .. }
            | MidiEvent::Control { channel, .. }
            | MidiEvent::PitchBend { channel, .. } => retarget(channel),
        }
    }
    let mut engine = engine(&sequence);
    let (left, _) = render(&mut engine, 1.0);
    let meters = engine.meters();
    assert!(meters.track(0).left > 0.05, "the shared strip plays");
    assert_eq!(
        meters.track(1),
        gscore_audio::Level::default(),
        "the linked track has no signal of its own"
    );
    assert!(rms(steady(&left)) > 0.01);
}

#[test]
fn every_drum_track_has_its_own_strip() {
    // Kick and snare as two drum tracks: each on a channel of its own, played as drums
    // there, each with its own meter, and a chain on one leaves the other alone.
    let mut song = song(2);
    let _ = song.set_track_drums(TrackIndex::new(0), true).unwrap();
    let _ = song.set_track_drums(TrackIndex::new(1), true).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_ne!(sequence.tracks[0].channel, sequence.tracks[1].channel);
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Amp);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].amp.voice = gscore_guitar::AmpVoice::Clean;
    settings[0].amp.gain = 0.0;
    settings[0].amp.level_db = -24.0;
    let mut engine = engine(&sequence);
    let chain = gscore_audio::prepare_chain(&spec, &unity(settings), RATE);
    assert!(engine.set_chain(1, chain, unity(settings)).is_none());
    let _ = render(&mut engine, 0.5);
    let meters = engine.meters();
    let (kick, snare) = (meters.track(0).left, meters.track(1).left);
    assert!(kick > 0.05, "the kick plays on its own strip: {kick}");
    assert!(
        snare > 0.0 && snare < 0.2 * kick,
        "the snare plays on its own, through its chain: {snare} against {kick}"
    );
}

#[test]
fn a_sequence_sent_to_the_browsers_worklet_keeps_its_strips() {
    // The worklet gets the sequence through the wire format: its tracks must arrive, or the
    // mixer has no strips and the faders do nothing there.
    let level = |volume: u8| {
        let mut song = song(1);
        let _ = song.set_track_volume(TrackIndex::new(0), volume).unwrap();
        let sequence = Sequence::new(&song, None).unwrap();
        let sent = gscore_play::decode_sequence(&gscore_play::encode_sequence(&sequence)).unwrap();
        let (left, _) = render(&mut engine(&sent), 1.0);
        rms(steady(&left))
    };
    let ratio = level(64) / level(127);
    let expected = (64.0_f32 / 127.0).powi(2);
    assert!(
        (ratio - expected).abs() < 0.01,
        "{ratio} against {expected}"
    );
}

/// A CA-76 driven hard and made up short: it turns the part down.
fn hard_compressor() -> CompressorKnobs {
    CompressorKnobs {
        input_db: 18.0,
        output_db: -18.0,
        attack_ms: 0.02,
        release_ms: 200.0,
        ratio: CompRatio::Twenty,
        mix: 1.0,
    }
}

#[test]
fn a_compressor_bus_is_fed_by_the_comp_sends() {
    // The bus runs a compressor; each track's Comp send feeds it after its fader.
    let song = song(2);
    let sequence = Sequence::new(&song, None).unwrap();
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Compressor);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].compressor = hard_compressor();
    let run = |send: u8, level: u8, muted: bool| {
        let mut engine = engine(&sequence);
        let _ = engine.set_bus(
            Bus::Compressor,
            Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0))),
        );
        engine.set_bus_settings(Bus::Compressor, unity(settings), level, muted);
        for track in 0..2 {
            engine.set_comp_send(track, send);
        }
        let _ = render(&mut engine, 1.0);
        let meters = engine.meters();
        (meters.compressor().left, meters.bus_reduction())
    };
    // Nothing sent: the bus has nothing to work on, so it is silent and its needle rests.
    assert_eq!(run(0, 127, false), (0.0, 0.0));
    // Sent: the bus works (its needle shows how far) and its meter moves.
    let (level, reduction) = run(127, 127, false);
    assert!(level > 0.0, "the bus's meter moves: {level}");
    assert!(reduction < -1.0, "the compressor works: {reduction} dB");
    // Its fader and its mute silence what comes back (it still works).
    let (quiet, quiet_reduction) = run(127, 0, false);
    assert_eq!(quiet, 0.0);
    assert!(quiet_reduction < -1.0, "{quiet_reduction}");
    assert_eq!(run(127, 127, true).0, 0.0);
}

#[test]
fn a_compressor_in_a_tracks_chain_turns_it_down() {
    // The track's own chain holds a CA-76 driven hard: its level drops against a track
    // without one, and its gain-reduction needle says how far it works.
    let song = song(1);
    let sequence = Sequence::new(&song, None).unwrap();
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Compressor);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].compressor = hard_compressor();
    let mut compressed = engine(&sequence);
    let _ = compressed.set_chain(
        0,
        Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0))),
        unity(settings),
    );
    let mut dry = engine(&sequence);
    let (wet_left, _) = render(&mut compressed, 1.0);
    let (dry_left, _) = render(&mut dry, 1.0);
    let reduction = compressed.meters().reduction(0);
    assert!(reduction < -2.0, "{reduction} dB");
    assert!(
        rms(steady(&wet_left)) < 0.8 * rms(steady(&dry_left)),
        "{} against {}",
        rms(steady(&wet_left)),
        rms(steady(&dry_left))
    );
    // Let go, the needle returns to rest.
    compressed.stop();
    let _ = render(&mut compressed, 3.0);
    assert_eq!(compressed.meters().reduction(0), 0.0);
}

#[test]
fn a_drum_tracks_parts_have_their_own_chains() {
    use gscore_guitar::CompressorKnobs;

    // A drum track stays one strip, but its kit is split into parts the mixer can process
    // apart: a chain on the kick's part takes the kick and leaves the snare alone. (No
    // notes in the song: the test plays the drums itself.)
    let mut song = testing::song_with_notes_on_tracks(1, &[]).expect("a silent song");
    let _ = song
        .set_track_drums(TrackIndex::new(0), true)
        .expect("a drum track");
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    assert_eq!(
        channel,
        Channel::PERCUSSION,
        "the first drum track gets channel 10"
    );
    assert!(sequence.tracks[0].sound.percussion);
    // A compressor that turns its part right down (no make-up).
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Compressor);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].compressor = CompressorKnobs {
        input_db: 24.0,
        output_db: -24.0,
        ..CompressorKnobs::default()
    };
    // One drum, with a chain on `chain_part` (or none).
    let level = |key: u8, chain_part: Option<usize>| {
        let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
        let bank = SoundBank::read(&mut bytes.as_slice()).unwrap();
        let mut engine = SynthEngine::new(&bank, RATE).unwrap();
        engine.load(Arc::new(sequence.clone()));
        engine.play();
        if let Some(part) = chain_part {
            let _ = engine.set_part_chain(
                0,
                part,
                Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0))),
                unity(settings),
            );
        }
        engine.send_event(MidiEvent::NoteOn {
            channel,
            key,
            velocity: 100,
        });
        let (left, _) = render(&mut engine, 1.0);
        rms(steady(&left))
    };
    let kick = level(36, None);
    let snare = level(38, None);
    assert!(
        kick > 0.001 && snare > 0.001,
        "the drums sound: {kick} {snare}"
    );
    // The kick's own chain turns it right down...
    let kick_cut = level(36, Some(0));
    assert!(kick_cut < 0.2 * kick, "{kick_cut} against {kick}");
    // ...and leaves the snare alone, which is another part.
    let snare_alone = level(38, Some(0));
    assert!(
        (snare_alone - snare).abs() < 1e-6,
        "{snare_alone} against {snare}"
    );
    // A part the kit does not use has no sound, so its chain changes nothing either.
    assert!((level(36, Some(2)) - kick).abs() < 1e-6);
}

#[test]
fn the_input_trim_sets_how_hard_the_chain_is_fed() {
    // A trim of -12 dB is a quarter of the power at the chain's input (and so at the
    // output, a track without a chain); the input meter shows it before the chain.
    let sequence = Sequence::new(&song(1), None).unwrap();
    let level = |trim: f32| {
        let mut engine = engine(&sequence);
        engine.set_trim(0, trim);
        let (left, _) = render(&mut engine, 1.0);
        let meters = engine.meters();
        (rms(steady(&left)), meters.input(0).left)
    };
    let (plain, plain_input) = level(0.0);
    let (down, down_input) = level(-12.0);
    assert!((down / plain - 0.251).abs() < 0.02, "{down} {plain}");
    assert!(
        (down_input / plain_input - 0.251).abs() < 0.05,
        "{down_input} {plain_input}"
    );
    let (up, up_input) = level(6.0);
    assert!(up > plain * 1.8, "{up} {plain}");
    assert!(up_input > plain_input * 1.8, "{up_input} {plain_input}");
}

#[test]
fn a_kits_room_carries_its_parts_sends() {
    // The kit's one short room: a part's Room send reaches it, its return sounds after
    // the part itself has died away, and more send is more room.
    let mut song = testing::song_with_notes_on_tracks(1, &[]).expect("a silent song");
    let _ = song
        .set_track_drums(TrackIndex::new(0), true)
        .expect("a drum track");
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    let tail = |send: u8| {
        let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
        let bank = SoundBank::read(&mut bytes.as_slice()).unwrap();
        let mut engine = SynthEngine::new(&bank, RATE).unwrap();
        engine.load(Arc::new(sequence.clone()));
        engine.play();
        engine.set_track_room(0, gscore_audio::prepare_drum_room(RATE));
        engine.set_part_room(0, 0, send);
        engine.send_event(MidiEvent::NoteOn {
            channel,
            key: 36,
            velocity: 100,
        });
        let _ = render(&mut engine, 0.2);
        engine.send_event(MidiEvent::NoteOff { channel, key: 36 });
        let (left, _) = render(&mut engine, 1.5);
        // The hit has died away after half a second; what is left is the room's tail.
        rms(&left[RATE as usize / 2..])
    };
    let dry = tail(0);
    let half = tail(64);
    let wet = tail(127);
    assert!(wet > half && half > dry * 3.0, "{dry} {half} {wet}");
}

#[test]
fn a_drum_tracks_parts_have_their_own_levels() {
    // Each part of a drum track has a mixing level of its own: turning the kick down
    // leaves the snare as it is, and the level scales the part after its chain.
    let mut song = testing::song_with_notes_on_tracks(1, &[]).expect("a silent song");
    let _ = song
        .set_track_drums(TrackIndex::new(0), true)
        .expect("a drum track");
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    let level = |key: u8, part: usize, part_level: u8| {
        let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
        let bank = SoundBank::read(&mut bytes.as_slice()).unwrap();
        let mut engine = SynthEngine::new(&bank, RATE).unwrap();
        engine.load(Arc::new(sequence.clone()));
        engine.play();
        engine.set_part_level(0, part, part_level);
        engine.send_event(MidiEvent::NoteOn {
            channel,
            key,
            velocity: 100,
        });
        let (left, _) = render(&mut engine, 1.0);
        rms(steady(&left))
    };
    let kick = level(36, 0, 127);
    let snare = level(38, 1, 127);
    assert!(kick > 0.001 && snare > 0.001, "the drums sound");
    // Half the level is about half the amplitude, and only the kick's.
    let kick_half = level(36, 0, 64);
    assert!(
        kick_half > 0.4 * kick && kick_half < 0.6 * kick,
        "{kick_half} against {kick}"
    );
    let snare_with_kick_down = level(38, 0, 64);
    assert!((snare_with_kick_down - snare).abs() < 1e-6);
    // The snare's own level moves only the snare.
    let snare_half = level(38, 1, 64);
    assert!(snare_half < 0.6 * snare, "{snare_half} against {snare}");
}

#[test]
fn a_drum_part_pans_its_whole_hit_and_only_its_own() {
    // A part's pan moves its stereo image (the far side folds into the near one), centre
    // leaves it as it is, and the other parts stay where they are.
    let mut song = testing::song_with_notes_on_tracks(1, &[]).expect("a silent song");
    let _ = song
        .set_track_drums(TrackIndex::new(0), true)
        .expect("a drum track");
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    let sides = |key: u8, part: usize, pan: u8| {
        let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
        let bank = SoundBank::read(&mut bytes.as_slice()).unwrap();
        let mut engine = SynthEngine::new(&bank, RATE).unwrap();
        engine.load(Arc::new(sequence.clone()));
        engine.play();
        engine.set_part_pan(0, part, pan);
        engine.send_event(MidiEvent::NoteOn {
            channel,
            key,
            velocity: 100,
        });
        let (left, right) = render(&mut engine, 1.0);
        (rms(steady(&left)), rms(steady(&right)))
    };
    let (left, right) = sides(42, 2, 64);
    // (The track's own pan law puts 64 a hair right of the middle.)
    assert!(
        left > 0.001 && (left / right - 1.0).abs() < 0.02,
        "centred: {left} {right}"
    );
    let (hard_left, silent) = sides(42, 2, 0);
    assert!(silent < 1e-6, "nothing on the right: {silent}");
    assert!(
        hard_left > left && hard_left < 2.0 * left,
        "the whole hit on the left, about as loud: {hard_left} against {left}"
    );
    let (silent, hard_right) = sides(42, 2, 127);
    assert!(silent < 1e-6 && hard_right > left, "and on the right");
    // The hi-hat's pan leaves the snare in the middle.
    let (snare_left, snare_right) = sides(38, 2, 0);
    assert!((snare_left / snare_right - 1.0).abs() < 0.02);
}

#[test]
fn a_tracks_chorus_value_reaches_the_chorus_bus() {
    // The track's Chorus knob (MIDI CC 93) is a send into the shared chorus, after its
    // fader: the bus runs all wet and only the chorused signal comes back.
    let source = song(1);
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Chorus);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].chorus = gscore_guitar::ChorusKnobs {
        rate_hz: 1.0,
        depth_ms: 4.0,
        mix: 1.0,
    };
    let run = |send: u8, level: u8, muted: bool| {
        let mut song = source.clone();
        let _ = song.set_track_chorus(TrackIndex::new(0), send).unwrap();
        let sequence = Sequence::new(&song, None).unwrap();
        let mut engine = engine(&sequence);
        engine.set_chorus_bus(Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0))));
        engine.set_chorus_bus_settings(unity(settings), level, muted);
        let (left, _) = render(&mut engine, 1.0);
        (engine.meters().chorus().left, left)
    };
    let (meter, wet) = run(127, 127, false);
    assert!(meter > 0.0, "the bus's meter moves: {meter}");
    let (dry_meter, dry) = run(0, 127, false);
    assert_eq!(dry_meter, 0.0, "no send, no chorus");
    assert!(
        rms(steady(&wet)) > 0.0 && (rms(steady(&wet)) - rms(steady(&dry))).abs() > 1e-4,
        "the bus is heard under the dry track"
    );
    // Its fader and mute silence what comes back.
    assert_eq!(run(127, 0, false).0, 0.0);
    assert_eq!(run(127, 127, true).0, 0.0);
}

#[test]
fn the_master_chain_processes_the_sum_but_not_the_click() {
    // A master chain after the tracks and buses: an amp turned right down takes the whole
    // mix with it. The metronome's click is added after it, so practising stays audible.
    let source = song(1);
    let sequence = Sequence::new(&source, None).unwrap();
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Amp);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].amp.voice = gscore_guitar::AmpVoice::Clean;
    settings[0].amp.gain = 0.0;
    settings[0].amp.level_db = -30.0;
    let (dry, quiet) = {
        let mut dry = engine(&sequence);
        let (dry_left, _) = render(&mut dry, 1.0);
        let mut quiet = engine(&sequence);
        let _ = quiet.set_master_chain(
            Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0))),
            unity(settings),
        );
        let (quiet_left, _) = render(&mut quiet, 1.0);
        (rms(steady(&dry_left)), rms(steady(&quiet_left)))
    };
    assert!(quiet < 0.5 * dry, "{quiet} against {dry}");

    // A click on the metronome's percussion keys: it sounds the same with the master chain
    // as without it.
    let click = |chained: bool| {
        let mut song = source.clone();
        let _ = song.set_track_mute(TrackIndex::new(0), true).unwrap();
        let sequence = Sequence::new(&song, None).unwrap();
        let mut engine = engine(&sequence);
        if chained {
            let _ = engine.set_master_chain(
                Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0))),
                unity(settings),
            );
        }
        engine.send_event(MidiEvent::NoteOn {
            channel: Channel::PERCUSSION,
            key: 34,
            velocity: 120,
        });
        let (left, _) = render(&mut engine, 0.4);
        rms(&left)
    };
    let (plain, chained) = (click(false), click(true));
    assert!(plain > 0.001, "the click plays: {plain}");
    assert!(
        (plain - chained).abs() < 1e-6,
        "the master chain does not touch the click: {chained} against {plain}"
    );
}

#[test]
fn a_drum_tracks_parts_have_mutes_and_solos() {
    // A part's mute and solo work inside the kit: muting the kick leaves the snare, and a
    // soloed part silences the others.
    let mut song = testing::song_with_notes_on_tracks(1, &[]).expect("a silent song");
    let _ = song
        .set_track_drums(TrackIndex::new(0), true)
        .expect("a drum track");
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    let play = |key: u8, mute_kick: bool, solo_snare: bool| {
        let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
        let bank = SoundBank::read(&mut bytes.as_slice()).unwrap();
        let mut engine = SynthEngine::new(&bank, RATE).unwrap();
        engine.load(Arc::new(sequence.clone()));
        engine.play();
        engine.set_part_mute(0, 0, mute_kick);
        engine.set_part_solo(0, 1, solo_snare);
        engine.send_event(MidiEvent::NoteOn {
            channel,
            key,
            velocity: 100,
        });
        let (left, _) = render(&mut engine, 1.0);
        (
            rms(steady(&left)),
            engine.meters().part(0, 0).left,
            engine.meters().part(0, 1).left,
        )
    };
    let (kick, kick_meter, _) = play(36, false, false);
    let (snare, _, snare_meter) = play(38, false, false);
    assert!(kick > 0.001 && kick_meter > 0.0, "the kick's meter moves");
    assert!(
        snare > 0.001 && snare_meter > 0.0,
        "the snare's meter moves"
    );
    // The muted kick is silent and its meter rests; the snare is unchanged.
    let (muted_kick, muted_meter, _) = play(36, true, false);
    assert_eq!(muted_kick, 0.0);
    assert_eq!(muted_meter, 0.0);
    assert!((play(38, true, false).0 - snare).abs() < 1e-6);
    // A soloed snare: the kick is silent, the snare plays.
    assert_eq!(play(36, false, true).0, 0.0);
    assert!((play(38, false, true).0 - snare).abs() < 1e-6);
}

#[test]
fn a_drum_tracks_part_chains_follow_it_when_a_track_before_it_is_muted() {
    use gscore_guitar::CompressorKnobs;

    // The drums are the song's second track, the first is muted: in the sequence the
    // drums are the first (and only) track, but their parts' chains belong to the song's
    // track 1 (as every chain is the song's track's).
    let mut song = testing::song_with_notes_on_tracks(2, &[]).expect("a silent song");
    let _ = song.set_track_drums(TrackIndex::new(1), true).unwrap();
    let _ = song.set_track_mute(TrackIndex::new(0), true).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.tracks.len(), 1);
    assert_eq!(sequence.tracks[0].track, TrackIndex::new(1));
    let channel = sequence.tracks[0].channel;
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Compressor);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].compressor = CompressorKnobs {
        input_db: 24.0,
        output_db: -24.0,
        ..CompressorKnobs::default()
    };
    let kick = |chained: bool| {
        let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
        let bank = SoundBank::read(&mut bytes.as_slice()).unwrap();
        let mut engine = SynthEngine::new(&bank, RATE).unwrap();
        engine.load(Arc::new(sequence.clone()));
        engine.play();
        if chained {
            let chain = Some(Box::new(spec.prepare(RATE).instance(&settings, 0.0)));
            let _ = engine.set_part_chain(1, 0, chain, unity(settings));
        }
        engine.send_event(MidiEvent::NoteOn {
            channel,
            key: 36,
            velocity: 100,
        });
        let (left, _) = render(&mut engine, 1.0);
        rms(steady(&left))
    };
    let (plain, chained) = (kick(false), kick(true));
    assert!(plain > 0.001);
    assert!(
        chained < 0.2 * plain,
        "the kick's chain applies: {chained} against {plain}"
    );
}
