//! GuitarScore addition: GP7's `Content/PartConfiguration`, which says per track whether
//! standard notation and tablature are shown.
//!
//! The layout (as Guitar Pro 7 writes it, and as alphaTab reads it): a big-endian `i32`
//! count of score views; per view a multi-rest byte, a big-endian `i32` count of track
//! groups and one flags byte per track (bit 0 standard notation, bit 1 tablature, bit 2
//! slash, bit 3 numbered); then a big-endian `i32` (1 in every file seen). The first view
//! is the multi-track one, followed by one view per track.

use crate::model::track::Track;

/// The file's name in the archive.
pub(crate) const PART_CONFIGURATION: &str = "Content/PartConfiguration";

const NOTATION: u8 = 0x01;
const TABLATURE: u8 = 0x02;

/// More views or tracks than this means a broken file: it is ignored.
const MAX_ENTRIES: usize = 4096;

/// The flags byte of each track in the multi-track view, or `None` if the data is not a
/// part configuration.
pub(crate) fn read_track_flags(data: &[u8]) -> Option<Vec<u8>> {
    let int = |at: usize| -> Option<usize> {
        let bytes: [u8; 4] = data.get(at..at.checked_add(4)?)?.try_into().ok()?;
        usize::try_from(u32::from_be_bytes(bytes)).ok()
    };
    let views = int(0)?;
    if views == 0 || views > MAX_ENTRIES {
        return None;
    }
    // Byte 4 is the first view's multi-rest flag; its track count follows.
    let groups = int(5)?;
    if groups > MAX_ENTRIES {
        return None;
    }
    data.get(9..groups.checked_add(9)?).map(<[u8]>::to_vec)
}

/// Shows notation and tablature on `track` as `flags` says. A track that would show
/// neither keeps the tablature, so it never disappears from the page.
pub(crate) fn apply_flags(track: &mut Track, flags: u8) {
    track.settings.notation = flags & NOTATION != 0;
    track.settings.tablature = flags & TABLATURE != 0 || !track.settings.notation;
}

/// The part configuration for `tracks`: the multi-track view, then one view per track.
pub(crate) fn write(tracks: &[Track]) -> Vec<u8> {
    let flags: Vec<u8> = tracks
        .iter()
        .map(|track| {
            let mut flags = 0;
            if track.settings.notation {
                flags |= NOTATION;
            }
            if track.settings.tablature || !track.settings.notation {
                flags |= TABLATURE;
            }
            flags
        })
        .collect();
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_be_bytes();
    let mut out = Vec::new();
    out.extend_from_slice(&count(flags.len().saturating_add(1)));
    out.push(0);
    out.extend_from_slice(&count(flags.len()));
    out.extend_from_slice(&flags);
    for flag in &flags {
        out.push(0);
        out.extend_from_slice(&count(1));
        out.push(*flag);
    }
    out.extend_from_slice(&count(1));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes of a one-track Guitar Pro 7 file (notation and tablature shown).
    const ONE_TRACK: [u8; 20] = [0, 0, 0, 2, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 1, 3, 0, 0, 0, 1];

    #[test]
    fn a_guitar_pro_file_reads_and_writes_back_the_same() {
        assert_eq!(read_track_flags(&ONE_TRACK), Some(vec![3]));
        let mut track = Track::default();
        apply_flags(&mut track, 3);
        assert_eq!(write(&[track]), ONE_TRACK.to_vec());
    }

    #[test]
    fn display_choices_survive_a_round_trip() {
        let mut tracks = vec![Track::default(), Track::default(), Track::default()];
        apply_flags(&mut tracks[0], NOTATION);
        apply_flags(&mut tracks[1], TABLATURE);
        apply_flags(&mut tracks[2], 0);
        assert_eq!(
            read_track_flags(&write(&tracks)),
            Some(vec![NOTATION, TABLATURE, TABLATURE]),
            "a track showing nothing keeps its tablature"
        );
    }

    #[test]
    fn broken_data_is_ignored() {
        assert_eq!(read_track_flags(&[]), None);
        assert_eq!(read_track_flags(&[0, 0, 0, 0]), None);
        assert_eq!(read_track_flags(&[0xff, 0xff, 0xff, 0xff, 0]), None);
        // Claims more tracks than there are bytes.
        assert_eq!(read_track_flags(&[0, 0, 0, 1, 0, 0, 0, 0, 9, 3]), None);
    }
}
