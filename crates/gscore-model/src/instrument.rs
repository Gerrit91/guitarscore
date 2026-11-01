//! What kind of instrument a track is, which decides whether a tuning and tablature apply.

/// The broad kind of a track's instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstrumentKind {
    /// A string instrument written as tablature: guitar, bass, banjo, ukulele, mandolin.
    Strings,
    /// A drum kit or other percussion; "frets" are MIDI drum notes.
    Drums,
    /// Anything else (voice, piano, synthesizer, brass, ...): no tuning, no real tab.
    Other,
}

/// Last words of GP7 instrument type ids that mean a string instrument
/// (`electricGuitar`, `acousticBass`, `banjo`, ...).
const STRING_INSTRUMENTS: &[&str] = &["guitar", "bass", "banjo", "ukulele", "mandolin"];

/// Classifies a track from its GP7 instrument type id (if the format stores one) and its
/// percussion flag.
pub(crate) fn classify(instrument_type: Option<&str>, percussion: bool) -> InstrumentKind {
    if percussion {
        return InstrumentKind::Drums;
    }
    let Some(type_id) = instrument_type else {
        // GP3-GP5 store no instrument type; their non-percussion tracks are tab tracks.
        return InstrumentKind::Strings;
    };
    // The instrument is the last camelCase word: `electricBass` is a bass, but
    // `bassClarinet` is a clarinet.
    let last_word_start = type_id.rfind(char::is_uppercase).unwrap_or(0);
    let last_word = type_id
        .get(last_word_start..)
        .unwrap_or(type_id)
        .to_lowercase();
    if STRING_INSTRUMENTS.contains(&last_word.as_str()) {
        InstrumentKind::Strings
    } else {
        InstrumentKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_gp7_instrument_types() {
        let cases = [
            ("electricGuitar", InstrumentKind::Strings),
            ("steelGuitar", InstrumentKind::Strings),
            ("electricBass", InstrumentKind::Strings),
            ("banjo", InstrumentKind::Strings),
            ("ukulele", InstrumentKind::Strings),
            ("voice", InstrumentKind::Other),
            ("acousticPiano", InstrumentKind::Other),
            ("padSynthesizer", InstrumentKind::Other),
            ("trumpet", InstrumentKind::Other),
            ("bassClarinet", InstrumentKind::Other),
            ("bassoon", InstrumentKind::Other),
            ("", InstrumentKind::Other),
        ];
        for (type_id, expected) in cases {
            assert_eq!(classify(Some(type_id), false), expected, "{type_id}");
        }
    }

    #[test]
    fn percussion_flag_and_missing_type() {
        assert_eq!(classify(Some("drumKit"), true), InstrumentKind::Drums);
        assert_eq!(classify(None, true), InstrumentKind::Drums);
        assert_eq!(classify(None, false), InstrumentKind::Strings);
    }
}
