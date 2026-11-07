//! Which MIDI channel each track plays on (AGENTS.md section 5, decision 10).

use crate::event::Channel;

/// Hands out MIDI channels: the first drum track gets the percussion channel (10), every
/// other track - a further drum track too - its own channel, skipping 10. A drum track on
/// another channel than 10 is marked percussion by the built-in synthesizer (each drum
/// track then has its own mixer strip); a MIDI port and a MIDI file get it on 10, as
/// General MIDI devices expect ([`Sequence::general_midi_channel`]). With more tracks
/// than channels they share instead of failing: melodic tracks round-robin (never on a
/// drum track's channel), further drum tracks on 10.
///
/// [`Sequence::general_midi_channel`]: crate::Sequence::general_midi_channel
///
/// Channels left over after the tracks are the ones a note with a bend or slide can borrow
/// temporarily ([`ChannelAllocator::free_channels`]); the sequencer uses them so a bend on
/// one note does not bend the rest of the chord or track (AGENTS.md section 5, decision 10).
#[derive(Debug, Clone, Default)]
pub struct ChannelAllocator {
    next: u8,
    /// Whether a drum track has channel 10 already.
    percussion_taken: bool,
    /// The channels further drum tracks were given (other than 10).
    drum_channels: Vec<Channel>,
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
            if !self.percussion_taken {
                self.percussion_taken = true;
                return Channel::PERCUSSION;
            }
            // A further drum track: a fresh channel of its own while there is one.
            if usize::from(self.next) >= MELODIC.len() {
                self.shared = true;
                return Channel::PERCUSSION;
            }
            let channel = self.take_melodic();
            self.drum_channels.push(channel);
            return channel;
        }
        // Round-robin once every channel is taken, but never onto a drum track's channel
        // (a melodic track there would play drums).
        for _ in 0..=MELODIC.len() {
            if usize::from(self.next) >= MELODIC.len() {
                self.shared = true;
            }
            let channel = self.take_melodic();
            if !self.drum_channels.contains(&channel) {
                return channel;
            }
        }
        // Every channel plays drums (no song has that many drum tracks): the first.
        MELODIC
            .first()
            .copied()
            .and_then(Channel::new)
            .unwrap_or(Channel::PERCUSSION)
    }

    /// The next melodic channel in turn (wrapping around), marked used.
    fn take_melodic(&mut self) -> Channel {
        let slot = usize::from(self.next)
            .checked_rem(MELODIC.len())
            .unwrap_or(0);
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
    fn every_drum_track_gets_a_channel_of_its_own() {
        // Kick, snare and toms as three drum tracks between two guitars: the first drums
        // on 10, the others on fresh channels, which nobody borrows.
        let mut channels = ChannelAllocator::default();
        let kick = channels.allocate(true);
        let guitar = channels.allocate(false);
        let snare = channels.allocate(true);
        let toms = channels.allocate(true);
        let bass = channels.allocate(false);
        assert_eq!(kick, Channel::PERCUSSION);
        let all = [kick, guitar, snare, toms, bass];
        for (index, channel) in all.iter().enumerate() {
            assert!(!all[index + 1..].contains(channel), "{all:?}");
        }
        assert!(!channels.free_channels().contains(&snare));
        assert!(!channels.shared());
    }

    #[test]
    fn with_too_many_tracks_melodic_ones_never_land_on_a_drum_channel() {
        let mut channels = ChannelAllocator::default();
        let _kick = channels.allocate(true);
        let snare = channels.allocate(true);
        let melodic: Vec<Channel> = (0..30).map(|_| channels.allocate(false)).collect();
        assert!(!melodic.contains(&snare), "{melodic:?}");
        assert!(!melodic.contains(&Channel::PERCUSSION));
        assert!(channels.shared());
        // Drum tracks past the channels share 10.
        assert_eq!(channels.allocate(true), Channel::PERCUSSION);
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
