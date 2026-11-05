//! Guitar Pro's drum articulations that are not General MIDI sounds.
//!
//! Guitar Pro 6 and 7 store a drum note as an articulation number. Most are the General
//! MIDI key they play; the rest (half-open hi-hat, ride edge, chokes, cowbell and conga
//! variants, ...) are numbers of Guitar Pro's own, which its drum kit maps to the nearest
//! General MIDI sound when it plays or exports MIDI. The table is that mapping, as written
//! in the `<Articulation>` elements of a Guitar Pro 7 drum kit (`testdata/basic/
//! multi_staff.gp`), so our playback and MIDI export sound like Guitar Pro's.

/// `(articulation, General MIDI key)` for every articulation that is not its own key.
const ARTICULATIONS: [(u8, u8); 44] = [
    (29, 59),  // Ride (choke)
    (30, 49),  // Reverse cymbal
    (31, 40),  // Snare (side stick)
    (33, 37),  // Metronome
    (34, 38),  // Metronome (bell)
    (82, 54),  // Shaker (hit)
    (83, 53),  // Jingle bell, tinkle bell
    (84, 53),  // Bell tree (hit)
    (91, 38),  // Snare (rim shot)
    (92, 46),  // Hi-hat (half)
    (93, 51),  // Ride (edge)
    (94, 51),  // Ride (choke)
    (95, 55),  // Splash (choke)
    (96, 52),  // China (choke)
    (97, 49),  // Crash high (choke)
    (98, 57),  // Crash medium (choke)
    (99, 56),  // Cowbell low (hit)
    (100, 56), // Cowbell low (tip)
    (101, 56), // Cowbell medium (tip)
    (102, 56), // Cowbell high (hit)
    (103, 56), // Cowbell high (tip)
    (104, 60), // Bongo high (mute)
    (105, 60), // Bongo high (slap)
    (106, 61), // Bongo low (mute)
    (107, 61), // Bongo low (slap)
    (108, 64), // Conga low (slap)
    (109, 64), // Conga low (mute)
    (110, 63), // Conga high (slap)
    (111, 54), // Tambourine (return)
    (112, 54), // Tambourine (roll)
    (113, 54), // Tambourine (hand)
    (114, 43), // Grancassa
    (115, 49), // Piatti (hit)
    (116, 49), // Piatti (hand)
    (117, 69), // Cabasa (return)
    (118, 70), // Left maraca (return)
    (119, 70), // Right maraca (hit)
    (120, 70), // Right maraca (return)
    (122, 54), // Shaker (return)
    (123, 53), // Bell tree (return)
    (124, 62), // Golpe (thumb)
    (125, 62), // Golpe (finger)
    (126, 59), // Ride (middle), second ride
    (127, 59), // Ride (bell), second ride
];

/// The General MIDI key a drum note with `articulation` plays.
pub(crate) fn drum_key(articulation: u8) -> u8 {
    ARTICULATIONS
        .iter()
        .find(|(input, _)| *input == articulation)
        .map_or(articulation, |(_, key)| *key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_midi_keys_stay_and_guitar_pros_own_articulations_map() {
        assert_eq!(drum_key(36), 36, "bass drum");
        assert_eq!(drum_key(42), 42, "closed hi-hat");
        assert_eq!(drum_key(92), 46, "half-open hi-hat");
        assert_eq!(drum_key(93), 51, "ride edge");
        let mut inputs: Vec<u8> = ARTICULATIONS.iter().map(|(input, _)| *input).collect();
        inputs.dedup();
        assert_eq!(inputs.len(), ARTICULATIONS.len(), "sorted, each once");
        assert!(ARTICULATIONS.iter().all(|(_, key)| (35..=81).contains(key)));
    }
}
