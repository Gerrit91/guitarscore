//! GuitarScore addition: GP7's `Content/LayoutConfiguration`, which says per track whether
//! it is shown when all tracks are shown together (the eye in Guitar Pro's track list).
//!
//! The layout, worked out from Guitar Pro 7 saves of one song with all tracks shown and
//! with six of them hidden: a big-endian `u32` (4 to 7 in the files seen) and two bytes
//! (`01 00` or `03 00`); one byte per track, `ff` shown or `00` hidden; a second byte per
//! track whose meaning is unknown (`ff` in every file seen); a big-endian `u32` that looks
//! like the selected track. Only the visibility is understood; the rest is kept as read.

use crate::model::song::LayoutConfiguration;
use crate::model::track::Track;

/// The file's name in the archive.
pub(crate) const LAYOUT_CONFIGURATION: &str = "Content/LayoutConfiguration";

/// A track's byte when it is shown, and when it is hidden.
const SHOWN: u8 = 0xff;
const HIDDEN: u8 = 0x00;

/// The bytes before and after the per-track arrays.
const HEAD: usize = 6;
const TAIL: usize = 4;

/// The configuration's header and trailer, and per track whether it is shown and its
/// second byte; `None` if the data does not fit `tracks` tracks (the file is then ignored,
/// as a broken or unknown variant).
pub(crate) fn read(data: &[u8], tracks: usize) -> Option<(LayoutConfiguration, Vec<(bool, u8)>)> {
    let arrays = tracks.checked_mul(2)?;
    if data.len() != HEAD.checked_add(arrays)?.checked_add(TAIL)? {
        return None;
    }
    let int = |at: usize| -> Option<u32> {
        let bytes: [u8; 4] = data.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(u32::from_be_bytes(bytes))
    };
    let shown = data.get(HEAD..HEAD.checked_add(tracks)?)?;
    let second = data.get(HEAD.checked_add(tracks)?..HEAD.checked_add(arrays)?)?;
    let config = LayoutConfiguration {
        kind: int(0)?,
        flags: [*data.get(4)?, *data.get(5)?],
        selected: int(HEAD.checked_add(arrays)?)?,
    };
    let per_track = shown
        .iter()
        .zip(second)
        .map(|(&shown, &second)| (shown != HIDDEN, second))
        .collect();
    Some((config, per_track))
}

/// The bytes for `tracks`, with the header and trailer of `config` (the file's own, or
/// what Guitar Pro writes for a new song). A selected track past the last becomes the
/// first.
pub(crate) fn write(config: Option<&LayoutConfiguration>, tracks: &[Track]) -> Vec<u8> {
    let config = config.cloned().unwrap_or_default();
    let selected = usize::try_from(config.selected)
        .ok()
        .filter(|&selected| selected < tracks.len())
        .map_or(0, |_| config.selected);
    let mut out = Vec::with_capacity(
        HEAD.saturating_add(tracks.len().saturating_mul(2))
            .saturating_add(TAIL),
    );
    out.extend_from_slice(&config.kind.to_be_bytes());
    out.extend_from_slice(&config.flags);
    out.extend(
        tracks
            .iter()
            .map(|track| if track.visible { SHOWN } else { HIDDEN }),
    );
    out.extend(tracks.iter().map(|track| track.layout_second));
    out.extend_from_slice(&selected.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guitar Pro 7's file for a nine-track song with tracks 2 to 7 hidden (the eye in
    /// the track list), the eighth track selected.
    const SIX_HIDDEN: [u8; 28] = [
        0, 0, 0, 4, 1, 0, // header
        0xff, 0, 0, 0, 0, 0, 0, 0xff, 0xff, // shown per track
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // second byte per track
        0, 0, 0, 7, // selected track
    ];

    fn tracks(shown: &[(bool, u8)]) -> Vec<Track> {
        shown
            .iter()
            .map(|&(visible, layout_second)| Track {
                visible,
                layout_second,
                ..Track::default()
            })
            .collect()
    }

    #[test]
    fn guitar_pros_hidden_tracks_are_read_and_written_back_the_same() {
        let (config, per_track) = read(&SIX_HIDDEN, 9).expect("a layout configuration");
        let shown: Vec<bool> = per_track.iter().map(|&(shown, _)| shown).collect();
        assert_eq!(
            shown,
            [true, false, false, false, false, false, false, true, true]
        );
        assert_eq!(config.selected, 7);
        assert_eq!(
            write(Some(&config), &tracks(&per_track)),
            SIX_HIDDEN.to_vec()
        );
    }

    #[test]
    fn data_that_does_not_fit_the_tracks_is_ignored() {
        assert!(read(&SIX_HIDDEN, 8).is_none());
        assert!(read(&SIX_HIDDEN, 10).is_none());
        assert!(read(&[], 0).is_none());
        assert!(read(&SIX_HIDDEN[..20], 9).is_none());
    }

    #[test]
    fn a_new_song_gets_guitar_pros_header_and_a_selected_track_that_exists() {
        let two = tracks(&[(true, 0xff), (false, 0xff)]);
        assert_eq!(
            write(None, &two),
            [0, 0, 0, 4, 1, 0, 0xff, 0, 0xff, 0xff, 0, 0, 0, 0]
        );
        // The selected track was removed: the first is selected instead.
        let config = LayoutConfiguration {
            selected: 5,
            ..LayoutConfiguration::default()
        };
        assert_eq!(write(Some(&config), &two)[10..], [0, 0, 0, 0]);
    }

    /// A whole GP7 file: the tracks hidden in GuitarScore stay hidden when it is saved
    /// and read again (the configuration used to be dropped on saving).
    #[test]
    fn hidden_tracks_survive_saving_a_guitar_pro_7_file() {
        const GP7: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/basic/multi_staff.gp"
        ));
        let mut song = crate::model::Song::default();
        song.read_gp(GP7).expect("multi_staff.gp");
        assert!(song.tracks.iter().all(|track| track.visible));
        song.tracks[1].visible = false;
        song.tracks[3].visible = false;
        let mut reread = crate::model::Song::default();
        reread
            .read_gp(&song.write_gp().expect("written"))
            .expect("read back");
        let shown: Vec<bool> = reread.tracks.iter().map(|track| track.visible).collect();
        assert_eq!(shown, [true, false, true, false, true, true]);
        assert_eq!(reread.layout_configuration, song.layout_configuration);
    }
}
