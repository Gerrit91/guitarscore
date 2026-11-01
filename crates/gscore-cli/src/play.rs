//! `midi`, `ports` and `play`: MIDI export and playback.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gscore_audio::{MidiOutPlayer, midi_output_ports};
use gscore_model::{Song, TrackIndex};
use gscore_play::{PlayState, Player, Sequence, to_smf};

use crate::CliError;

/// How often `play` checks whether playback has finished.
const POLL: Duration = Duration::from_millis(50);

/// The sequence of the 1-based `tracks` (all if empty).
fn sequence(song: &Song, tracks: &[usize]) -> Result<Sequence, CliError> {
    let selected: Option<Vec<TrackIndex>> = if tracks.is_empty() {
        None
    } else {
        Some(
            tracks
                .iter()
                .map(|&n| {
                    n.checked_sub(1)
                        .filter(|&i| i < song.track_count())
                        .map(TrackIndex::new)
                        .ok_or_else(|| {
                            CliError::Usage(format!(
                                "track {n} does not exist (the song has {})\n",
                                song.track_count()
                            ))
                        })
                })
                .collect::<Result<_, _>>()?,
        )
    };
    Sequence::new(song, selected.as_deref()).map_err(|e| CliError::Failed(e.to_string()))
}

/// Writes `input` as a Standard MIDI File to `output`.
pub(crate) fn export(input: &Path, output: &Path, tracks: &[usize]) -> Result<String, CliError> {
    if !output
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mid") || ext.eq_ignore_ascii_case("midi"))
    {
        return Err(CliError::Usage(format!(
            "{} must end in .mid\n",
            output.display()
        )));
    }
    let song = crate::input::read_song(input)?.song;
    let sequence = sequence(&song, tracks)?;
    let bytes = to_smf(&sequence, song.title()).map_err(|e| CliError::Failed(e.to_string()))?;
    std::fs::write(output, bytes)
        .map_err(|e| CliError::Failed(format!("cannot write {}: {e}", output.display())))?;
    let mut out = format!(
        "Wrote {} ({} track(s), {} measure(s) played).\n",
        output.display(),
        sequence.tracks.len(),
        sequence.measures.len()
    );
    if sequence.skipped_notes > 0 {
        out.push_str(&format!(
            "{} note(s) left out: their pitch is invalid (see `gscore-cli info`).\n",
            sequence.skipped_notes
        ));
    }
    if sequence.degraded_pitch_bends > 0 {
        out.push_str(&format!(
            "{} bend(s) or slide(s) left out: every MIDI channel was in use.\n",
            sequence.degraded_pitch_bends
        ));
    }
    Ok(out)
}

/// Lists the MIDI output ports.
pub(crate) fn ports() -> Result<String, CliError> {
    let ports = midi_output_ports().map_err(|e| CliError::Failed(e.to_string()))?;
    if ports.is_empty() {
        return Ok(
            "No MIDI output ports. Start a synthesizer, for example:\n  fluidsynth -a pipewire -m alsa_seq <soundfont.sf2>\n"
                .to_owned(),
        );
    }
    let mut out = String::from("MIDI output ports:\n");
    for (number, port) in ports.iter().enumerate() {
        out.push_str(&format!("  {}: {port}\n", number.saturating_add(1)));
    }
    Ok(out)
}

/// Plays `input` to a MIDI port and returns when it has finished.
pub(crate) fn play(
    input: &Path,
    port: Option<&str>,
    tracks: &[usize],
    speed: f32,
) -> Result<String, CliError> {
    let song = crate::input::read_song(input)?.song;
    let sequence = Arc::new(sequence(&song, tracks)?);
    let failed = |e: gscore_play::PlayError| CliError::Failed(e.to_string());
    let mut player = MidiOutPlayer::connect(port).map_err(failed)?;
    player.load(Arc::clone(&sequence)).map_err(failed)?;
    player.set_tempo_scale(speed).map_err(failed)?;
    eprintln!(
        "Playing {} on {} ...",
        if song.title().is_empty() {
            "the song"
        } else {
            song.title()
        },
        player.port()
    );
    player.play().map_err(failed)?;

    // Wait for the player thread to start, then for it to finish.
    let started = Instant::now();
    while player.state() != PlayState::Playing && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(POLL);
    }
    while player.state() == PlayState::Playing {
        std::thread::sleep(POLL);
    }
    if let Some(error) = player.error() {
        return Err(CliError::Failed(error));
    }
    let mut out = format!("Played {} measure(s).\n", sequence.measures.len());
    if sequence.degraded_pitch_bends > 0 {
        out.push_str(&format!(
            "{} bend(s) or slide(s) left out: every MIDI channel was in use.\n",
            sequence.degraded_pitch_bends
        ));
    }
    Ok(out)
}
