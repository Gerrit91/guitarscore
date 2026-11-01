//! Which MIDI channel each track plays on (AGENTS.md section 5, decision 10).

use crate::event::Channel;

/// Hands out MIDI channels: drum tracks share the percussion channel (10), every other
/// track gets its own channel, skipping 10. With more melodic tracks than channels, they
/// share channels round-robin instead of failing (a shared channel uses the first track's
/// instrument).
///
/// Channels left over after the tracks are the ones a note with a bend or slide can borrow
/// temporarily ([`ChannelAllocator::free_channels`]); the sequencer uses them so a bend on
/// one note does not bend the rest of the chord or track (AGENTS.md section 5, decision 10).
#[derive(Debug, Clone, Default)]
pub struct ChannelAllocator {
    next: u8,
    /// Channels handed to tracks so far (a shared channel appears once).
    used: Vec<Channel>,
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
        let channel = MELODIC
            .get(slot)
            .copied()
            .and_then(Channel::new)
            .unwrap_or(Channel::PERCUSSION);
        if !self.used.contains(&channel) {
            self.used.push(channel);
        }
        channel
    }

    /// Whether some melodic tracks had to share a channel.
    pub fn shared(&self) -> bool {
        self.shared
    }

    /// The melodic channels no track uses, in ascending order. They are available for notes
    /// that need a pitch bend of their own.
    pub fn free_channels(&self) -> Vec<Channel> {
        MELODIC
            .iter()
            .copied()
            .filter_map(Channel::new)
            .filter(|channel| !self.used.contains(channel))
            .collect()
    }

    /// The channels tracks were given, in allocation order (shared channels once).
    pub fn used_channels(&self) -> &[Channel] {
        &self.used
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
        assert!(channels.free_channels().is_empty());
    }

    #[test]
    fn unused_channels_stay_free_for_borrowing() {
        let mut channels = ChannelAllocator::default();
        assert_eq!(channels.free_channels().len(), 15);
        let first = channels.allocate(false);
        let _drums = channels.allocate(true);
        let second = channels.allocate(false);
        assert_eq!(channels.used_channels(), [first, second]);
        let free = channels.free_channels();
        assert_eq!(free.len(), 13);
        assert!(!free.contains(&first));
        assert!(!free.contains(&second));
        assert!(!free.contains(&Channel::PERCUSSION));
    }
}
