//! Building a new track: its channel, tuning, name and rest-filled measures.

use gscore_gp::{
    Measure as GpMeasure, MeasureHeader as GpMeasureHeader, Song as GpSong, Track as GpTrack,
    Voice as GpVoice,
};

use crate::error::ModelError;
use crate::style::TrackColor;

/// Appends a channel for a new track, copied from `template` (or a default one), and
/// returns its index.
pub(super) fn append_channel(song: &mut GpSong, template: Option<usize>) -> usize {
    let mut channel = template
        .and_then(|index| song.channels.get(index))
        .copied()
        .unwrap_or_default();
    channel.channel = u8::try_from(song.channels.len() % 16).unwrap_or(0);
    song.channels.push(channel);
    song.channels.len().saturating_sub(1)
}

/// A track with `name`, `channel` and rest-filled measures matching the song.
pub(super) fn fresh_track(
    song: &GpSong,
    template: Option<&GpTrack>,
    name: String,
    channel: usize,
) -> Result<GpTrack, ModelError> {
    let mut track = template.cloned().unwrap_or_default();
    track.name = name;
    track.short_name = String::new();
    track.number = 1; // renumbered
    track.solo = false;
    track.mute = false;
    track.visible = true;
    track.channel_index = channel;
    track.color = TrackColor::DEFAULT.rgb() as i32;
    // A new track starts with its kind's default sound, not the neighbour's tone.
    track.sound_state = None;
    // Nor the neighbour's GP7 sound (Guitar Pro would play it for the new instrument).
    track.gpif_sounds = None;
    track.gpif_audio_engine = None;
    track.measures = empty_measures(song)?;
    Ok(track)
}

/// One rest-filled measure per measure header.
fn empty_measures(song: &GpSong) -> Result<Vec<GpMeasure>, ModelError> {
    song.measure_headers
        .iter()
        .enumerate()
        .map(|(index, header)| empty_measure(header, index))
        .collect()
}

fn empty_measure(header: &GpMeasureHeader, index: usize) -> Result<GpMeasure, ModelError> {
    // A signature that cannot be filled with rests (only hostile or unwritable ones) still
    // gets a valid, empty measure: the editor can add beats to it.
    let rests = crate::edit::measures::rests_for(&header.time_signature).unwrap_or_default();
    let voices = if rests.is_empty() {
        Vec::new()
    } else {
        vec![GpVoice {
            beats: rests,
            ..GpVoice::default()
        }]
    };
    Ok(GpMeasure {
        number: index.saturating_add(1),
        track_index: 0, // renumbered
        header_index: index,
        time_signature: header.time_signature.clone(),
        key_signature: header.key_signature.clone(),
        voices,
        ..GpMeasure::default()
    })
}

/// `base`, or `base 2`, `base 3`, ...: the first name no track has yet.
pub(super) fn unique_name(song: &GpSong, base: &str) -> String {
    if !song.tracks.iter().any(|track| track.name == base) {
        return base.to_owned();
    }
    (2_usize..)
        .map(|number| format!("{base} {number}"))
        .find(|candidate| !song.tracks.iter().any(|track| &track.name == candidate))
        // The loop stops at the first free number; `MAX_TRACKS` bounds how long it runs.
        .unwrap_or_else(|| format!("{base} {}", song.tracks.len().saturating_add(1)))
}
