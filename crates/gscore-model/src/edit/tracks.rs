//! Track edits: adding, deleting, duplicating, reordering and renaming tracks, their color,
//! and their mixer settings (volume, pan, mute and solo).
//!
//! Every track has its own MIDI channel when a mixer value changes: Guitar Pro files can
//! share a channel between tracks, but then a volume change would change both. A shared
//! channel is cloned for the track first, so the slider moves only that track.
//!
//! Adding, deleting, duplicating or moving a track changes every measure's layout, so
//! [`Song::edit_tracks`] renews all measure stamps; renaming and the mixer values only touch
//! one track and use [`Song::edit_track_settings`].

use gscore_gp::{
    Measure as GpMeasure, MeasureHeader as GpMeasureHeader, MidiChannel, Song as GpSong,
    Track as GpTrack, Voice as GpVoice,
};

use super::{Change, ChangeKind, track_mut};
use crate::error::ModelError;
use crate::position::TrackIndex;
use crate::song::Song;
use crate::style::TrackColor;

/// Most tracks a song may have. Files with more are left unedited instead of allocating
/// without bound.
pub const MAX_TRACKS: usize = 128;

/// Most MIDI channels the table is padded to when a track names a far one.
const MAX_CHANNELS: usize = 64;

/// Longest track name accepted (characters). Keeps a hostile file from making the UI
/// allocate without bound.
pub const MAX_TRACK_NAME_CHARS: usize = 256;

/// Which mixer flag an edit changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MixFlag {
    /// Guitar Pro's mute.
    Mute,
    /// Guitar Pro's solo.
    Solo,
}

impl Song {
    /// Inserts a new, empty track before `at`; `at` may be one past the last track to
    /// append. The new track copies the tuning and instrument of its neighbour and gets its
    /// own MIDI channel. Its measures are filled with rests.
    pub fn insert_track(&mut self, at: TrackIndex) -> Result<Change, ModelError> {
        insert(self, at)
    }

    /// Removes the track at `at`. The last remaining track cannot be removed.
    pub fn delete_track(&mut self, at: TrackIndex) -> Result<Change, ModelError> {
        delete(self, at)
    }

    /// Copies the track at `at` (notes, tuning, color and sound) and inserts the copy
    /// directly after it.
    pub fn duplicate_track(&mut self, at: TrackIndex) -> Result<Change, ModelError> {
        duplicate(self, at)
    }

    /// Moves the track at `from` to index `to` in the resulting order.
    pub fn move_track(&mut self, from: TrackIndex, to: TrackIndex) -> Result<Change, ModelError> {
        move_to(self, from, to)
    }

    /// Renames the track at `at`.
    pub fn rename_track(&mut self, at: TrackIndex, name: &str) -> Result<Change, ModelError> {
        rename(self, at, name)
    }

    /// Sets the track's color.
    pub fn set_track_color(
        &mut self,
        at: TrackIndex,
        color: TrackColor,
    ) -> Result<Change, ModelError> {
        set_color(self, at, color)
    }

    /// Sets the track's channel volume (0-127).
    pub fn set_track_volume(&mut self, at: TrackIndex, volume: u8) -> Result<Change, ModelError> {
        set_volume(self, at, volume)
    }

    /// Sets the track's pan: 0 is left, 64 the middle, 127 right.
    pub fn set_track_pan(&mut self, at: TrackIndex, pan: u8) -> Result<Change, ModelError> {
        set_pan(self, at, pan)
    }

    /// Mutes or unmutes the track.
    pub fn set_track_mute(&mut self, at: TrackIndex, mute: bool) -> Result<Change, ModelError> {
        set_flag(self, at, MixFlag::Mute, mute)
    }

    /// Solos or unsolos the track.
    pub fn set_track_solo(&mut self, at: TrackIndex, solo: bool) -> Result<Change, ModelError> {
        set_flag(self, at, MixFlag::Solo, solo)
    }
}

pub(super) fn insert(song: &mut Song, at: TrackIndex) -> Result<Change, ModelError> {
    song.edit_tracks(|inner| {
        let count = inner.tracks.len();
        if at.get() > count {
            return Err(ModelError::Invalid(format!(
                "{at}: the song has {count} track(s)"
            )));
        }
        if count >= MAX_TRACKS {
            return Err(ModelError::Invalid(format!(
                "the song already has {MAX_TRACKS} tracks"
            )));
        }
        // A neighbour lends its tuning and instrument to the new track: the track before
        // `at` (the one the user added this after), else the one being pushed aside.
        let template = at
            .get()
            .checked_sub(1)
            .and_then(|index| inner.tracks.get(index))
            .or_else(|| inner.tracks.get(at.get()))
            .or_else(|| inner.tracks.last())
            .cloned();
        let channel = append_channel(inner, template.as_ref().map(|t| t.channel_index));
        let name = unique_name(inner, "Track");
        let track = fresh_track(inner, template.as_ref(), name, channel)?;
        inner.tracks.insert(at.get(), track);
        renumber(inner)?;
        Ok(Change {
            kind: ChangeKind::TrackInserted {
                at,
                channel: Some(channel),
            },
        })
    })
}

pub(super) fn delete(song: &mut Song, at: TrackIndex) -> Result<Change, ModelError> {
    song.edit_tracks(|inner| {
        let count = inner.tracks.len();
        if count <= 1 {
            return Err(ModelError::Invalid(
                "cannot delete the only track".to_owned(),
            ));
        }
        if at.get() >= count {
            return Err(ModelError::Invalid(format!(
                "{at}: the song has {count} track(s)"
            )));
        }
        let track = inner.tracks.remove(at.get());
        renumber(inner)?;
        Ok(Change {
            kind: ChangeKind::TrackRemoved {
                at,
                track: Box::new(track),
            },
        })
    })
}

pub(super) fn duplicate(song: &mut Song, at: TrackIndex) -> Result<Change, ModelError> {
    song.edit_tracks(|inner| {
        let count = inner.tracks.len();
        let source = inner
            .tracks
            .get(at.get())
            .ok_or_else(|| ModelError::Invalid(format!("{at}: the song has {count} track(s)")))?
            .clone();
        if count >= MAX_TRACKS {
            return Err(ModelError::Invalid(format!(
                "the song already has {MAX_TRACKS} tracks"
            )));
        }
        let channel = append_channel(inner, Some(source.channel_index));
        let mut copy = source.clone();
        copy.name = unique_name(inner, &source.name);
        copy.short_name = String::new();
        copy.channel_index = channel;
        let to = TrackIndex::new(at.get().saturating_add(1));
        inner.tracks.insert(to.get(), copy);
        renumber(inner)?;
        Ok(Change {
            kind: ChangeKind::TrackInserted {
                at: to,
                channel: Some(channel),
            },
        })
    })
}

pub(super) fn move_to(
    song: &mut Song,
    from: TrackIndex,
    to: TrackIndex,
) -> Result<Change, ModelError> {
    song.edit_tracks(|inner| {
        let count = inner.tracks.len();
        if from.get() >= count {
            return Err(ModelError::Invalid(format!(
                "{from}: the song has {count} track(s)"
            )));
        }
        if to.get() >= count {
            return Err(ModelError::Invalid(format!(
                "{to}: the song has {count} track(s)"
            )));
        }
        if from == to {
            return Err(ModelError::Invalid(format!(
                "{from} is already at that position"
            )));
        }
        let track = inner.tracks.remove(from.get());
        inner.tracks.insert(to.get(), track);
        renumber(inner)?;
        Ok(Change {
            kind: ChangeKind::TrackMoved { from, to },
        })
    })
}

pub(super) fn rename(song: &mut Song, at: TrackIndex, name: &str) -> Result<Change, ModelError> {
    if name.chars().count() > MAX_TRACK_NAME_CHARS {
        return Err(ModelError::Invalid(format!(
            "the track name is longer than {MAX_TRACK_NAME_CHARS} characters"
        )));
    }
    song.edit_track_settings(|inner| {
        let track = track_mut(inner, at)?;
        let previous = std::mem::replace(&mut track.name, name.to_owned());
        Ok(Change {
            kind: ChangeKind::TrackRenamed { at, previous },
        })
    })
}

pub(super) fn set_color(
    song: &mut Song,
    at: TrackIndex,
    color: TrackColor,
) -> Result<Change, ModelError> {
    song.edit_track_settings(|inner| {
        let track = track_mut(inner, at)?;
        let previous = track.color;
        track.color = color.rgb() as i32;
        Ok(Change {
            kind: ChangeKind::TrackColor { at, previous },
        })
    })
}

pub(super) fn set_volume(
    song: &mut Song,
    at: TrackIndex,
    volume: u8,
) -> Result<Change, ModelError> {
    set_mix(song, at, |channel| {
        channel.volume = i8::try_from(volume.min(127)).unwrap_or(127);
    })
}

pub(super) fn set_pan(song: &mut Song, at: TrackIndex, pan: u8) -> Result<Change, ModelError> {
    set_mix(song, at, |channel| {
        channel.balance = i8::try_from(pan.min(127)).unwrap_or(127);
    })
}

/// Changes one mixer value on the track's own channel.
fn set_mix(
    song: &mut Song,
    at: TrackIndex,
    edit: impl FnOnce(&mut MidiChannel),
) -> Result<Change, ModelError> {
    song.edit_track_settings(|inner| {
        let previous_channel_count = inner.channels.len();
        let (channel_index, added) = own_channel(inner, at)?;
        let channel = inner
            .channels
            .get_mut(channel_index)
            .ok_or_else(|| ModelError::Invalid(format!("{at}: no MIDI channel")))?;
        let previous = *channel;
        edit(channel);
        Ok(Change {
            kind: ChangeKind::TrackMix {
                at,
                previous_channel: channel_index,
                previous,
                added,
                previous_channel_count,
            },
        })
    })
}

pub(super) fn set_flag(
    song: &mut Song,
    at: TrackIndex,
    flag: MixFlag,
    value: bool,
) -> Result<Change, ModelError> {
    song.edit_track_settings(|inner| {
        let track = track_mut(inner, at)?;
        let previous = match flag {
            MixFlag::Mute => std::mem::replace(&mut track.mute, value),
            MixFlag::Solo => std::mem::replace(&mut track.solo, value),
        };
        Ok(Change {
            kind: ChangeKind::TrackFlag { at, flag, previous },
        })
    })
}

/// The channel this track's mixer value lives on. If other tracks share the channel, a
/// copy is appended and the track points at it, so only this track changes.
fn own_channel(song: &mut GpSong, at: TrackIndex) -> Result<(usize, Option<usize>), ModelError> {
    let count = song.tracks.len();
    let index = song
        .tracks
        .get(at.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the song has {count} track(s)")))?
        .channel_index;
    let shared = song
        .tracks
        .iter()
        .filter(|track| track.channel_index == index)
        .count();
    if shared <= 1 && index < song.channels.len() {
        return Ok((index, None));
    }
    if index >= MAX_CHANNELS {
        // A file may name a channel far past the table; give the track a fresh one
        // instead of padding the table up to a hostile index.
        let added = append_channel(song, None);
        if let Some(track) = song.tracks.get_mut(at.get()) {
            track.channel_index = added;
        }
        return Ok((added, Some(added)));
    }
    while song.channels.len() <= index {
        song.channels.push(MidiChannel::default());
    }
    let mut channel = song.channels.get(index).copied().unwrap_or_default();
    channel.channel = u8::try_from(song.channels.len() % 16).unwrap_or(0);
    song.channels.push(channel);
    let added = song.channels.len().saturating_sub(1);
    if let Some(track) = song.tracks.get_mut(at.get()) {
        track.channel_index = added;
    }
    Ok((added, Some(added)))
}

/// Appends a channel for a new track, copied from `template` (or a default one), and
/// returns its index.
fn append_channel(song: &mut GpSong, template: Option<usize>) -> usize {
    let mut channel = template
        .and_then(|index| song.channels.get(index))
        .copied()
        .unwrap_or_default();
    channel.channel = u8::try_from(song.channels.len() % 16).unwrap_or(0);
    song.channels.push(channel);
    song.channels.len().saturating_sub(1)
}

/// A track with `name`, `channel` and rest-filled measures matching the song.
fn fresh_track(
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
    let rests = super::measures::rests_for(&header.time_signature).unwrap_or_default();
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
fn unique_name(song: &GpSong, base: &str) -> String {
    if !song.tracks.iter().any(|track| track.name == base) {
        return base.to_owned();
    }
    (2_usize..)
        .map(|number| format!("{base} {number}"))
        .find(|candidate| !song.tracks.iter().any(|track| &track.name == candidate))
        // The loop stops at the first free number; `MAX_TRACKS` bounds how long it runs.
        .unwrap_or_else(|| format!("{base} {}", song.tracks.len().saturating_add(1)))
}

/// Numbers tracks and their measures again after the order changed.
pub(super) fn renumber(song: &mut GpSong) -> Result<(), ModelError> {
    for (index, track) in song.tracks.iter_mut().enumerate() {
        track.number = i32::try_from(index.saturating_add(1))
            .map_err(|_| ModelError::Invalid("too many tracks".to_owned()))?;
        for (measure_index, measure) in track.measures.iter_mut().enumerate() {
            measure.track_index = index;
            measure.header_index = measure_index;
            measure.number = measure_index.saturating_add(1);
        }
    }
    Ok(())
}
