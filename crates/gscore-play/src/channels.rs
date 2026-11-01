//! Which MIDI channel each track plays on (AGENTS.md section 5, decision 10).

use crate::event::Channel;

/// Hands out MIDI channels: drum tracks share the percussion channel (10), every other
/// track gets its own channel, skipping 10. With more melodic tracks than channels, they
/// share channels round-robin instead of failing (a shared channel uses the first track's
/// instrument). Borrowing free channels for pitch bends comes with bend playback.
#[derive(Debug, Clone, Default)]
pub struct ChannelAllocator {
    next: u8,
    /// Whether channels had to be shared.
    shared: bool,
}

/// The melodic channels in allocation order (all but the percussion channel).
const MELODIC: [u8; 15] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];

impl ChannelAllocator {
    /// The channel for the next track.
    pub fn allocate(&mut self, percussion: bool) -> Channel {
        if percussion {
            return Channel::PERCUSSION;
        }
        let slot = usize::from(self.next)
            .checked_rem(MELODIC.len())
            .unwrap_or(0);
        if usize::from(self.next) >= MELODIC.len() {
            self.shared = true;
        }
        self.next = self.next.saturating_add(1);
        MELODIC
            .get(slot)
            .copied()
            .and_then(Channel::new)
            .unwrap_or(Channel::PERCUSSION)
    }

    /// Whether some melodic tracks had to share a channel.
    pub fn shared(&self) -> bool {
        self.shared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drums_get_channel_ten_and_others_skip_it() {
        let mut channels = ChannelAllocator::default();
        let melodic: Vec<u8> = (0..10).map(|_| channels.allocate(false).get()).collect();
        assert_eq!(melodic, [0, 1, 2, 3, 4, 5, 6, 7, 8, 10]);
        assert_eq!(channels.allocate(true), Channel::PERCUSSION);
        assert!(!channels.shared());
    }

    #[test]
    fn many_tracks_share_channels_instead_of_failing() {
        let mut channels = ChannelAllocator::default();
        let all: Vec<u8> = (0..20).map(|_| channels.allocate(false).get()).collect();
        assert!(all.iter().all(|&c| c != 9 && c < 16));
        assert!(channels.shared());
        assert_eq!(all[15], 0, "wraps around");
    }
}
