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

/// The symbol the app draws for a track: which instrument the user picked for it.
///
/// It is a display choice, not the played sound (the General MIDI program stays separate),
/// and it is kept in the file as the track's `GuitarScoreSymbol` property. Without one,
/// the app derives a symbol from the instrument type and the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InstrumentSymbol {
    /// An electric guitar.
    ElectricGuitar,
    /// An acoustic (steel-string) guitar.
    AcousticGuitar,
    /// An electric or acoustic bass.
    Bass,
    /// A drum kit.
    Drums,
    /// A piano.
    Piano,
    /// A pipe or drawbar organ.
    Organ,
    /// A synthesizer.
    Synth,
    /// A singer's microphone.
    Voice,
    /// A violin (or another bowed string instrument).
    Violin,
    /// A trumpet (or another brass instrument).
    Trumpet,
    /// A saxophone.
    Saxophone,
    /// A flute (or another wind instrument).
    Flute,
    /// A banjo.
    Banjo,
    /// A ukulele.
    Ukulele,
    /// Anything else.
    Other,
}

impl InstrumentSymbol {
    /// Every symbol the track panel offers, in menu order.
    pub const ALL: [InstrumentSymbol; 15] = [
        InstrumentSymbol::ElectricGuitar,
        InstrumentSymbol::AcousticGuitar,
        InstrumentSymbol::Bass,
        InstrumentSymbol::Drums,
        InstrumentSymbol::Piano,
        InstrumentSymbol::Organ,
        InstrumentSymbol::Synth,
        InstrumentSymbol::Voice,
        InstrumentSymbol::Violin,
        InstrumentSymbol::Trumpet,
        InstrumentSymbol::Saxophone,
        InstrumentSymbol::Flute,
        InstrumentSymbol::Banjo,
        InstrumentSymbol::Ukulele,
        InstrumentSymbol::Other,
    ];

    /// The id stored in the file (the GP7 instrument type where one fits).
    pub fn id(self) -> &'static str {
        match self {
            InstrumentSymbol::ElectricGuitar => "electricGuitar",
            InstrumentSymbol::AcousticGuitar => "acousticGuitar",
            InstrumentSymbol::Bass => "electricBass",
            InstrumentSymbol::Drums => "drumKit",
            InstrumentSymbol::Piano => "acousticPiano",
            InstrumentSymbol::Organ => "organ",
            InstrumentSymbol::Synth => "padSynthesizer",
            InstrumentSymbol::Voice => "voice",
            InstrumentSymbol::Violin => "violin",
            InstrumentSymbol::Trumpet => "trumpet",
            InstrumentSymbol::Saxophone => "saxophone",
            InstrumentSymbol::Flute => "flute",
            InstrumentSymbol::Banjo => "banjo",
            InstrumentSymbol::Ukulele => "ukulele",
            InstrumentSymbol::Other => "other",
        }
    }

    /// The symbol stored under `id`, if this version knows it.
    pub fn from_id(id: &str) -> Option<InstrumentSymbol> {
        InstrumentSymbol::ALL
            .into_iter()
            .find(|symbol| symbol.id() == id)
    }

    /// What the track panel and screen readers call it.
    pub fn name(self) -> &'static str {
        match self {
            InstrumentSymbol::ElectricGuitar => "Electric guitar",
            InstrumentSymbol::AcousticGuitar => "Acoustic guitar",
            InstrumentSymbol::Bass => "Bass",
            InstrumentSymbol::Drums => "Drums",
            InstrumentSymbol::Piano => "Piano",
            InstrumentSymbol::Organ => "Organ",
            InstrumentSymbol::Synth => "Synthesizer",
            InstrumentSymbol::Voice => "Voice",
            InstrumentSymbol::Violin => "Violin",
            InstrumentSymbol::Trumpet => "Trumpet",
            InstrumentSymbol::Saxophone => "Saxophone",
            InstrumentSymbol::Flute => "Flute",
            InstrumentSymbol::Banjo => "Banjo",
            InstrumentSymbol::Ukulele => "Ukulele",
            InstrumentSymbol::Other => "Other instrument",
        }
    }

    /// The broad kind the symbol belongs to, for deciding whether a tuning applies.
    pub fn kind(self) -> InstrumentKind {
        match self {
            InstrumentSymbol::ElectricGuitar
            | InstrumentSymbol::AcousticGuitar
            | InstrumentSymbol::Bass
            | InstrumentSymbol::Violin
            | InstrumentSymbol::Banjo
            | InstrumentSymbol::Ukulele => InstrumentKind::Strings,
            InstrumentSymbol::Drums => InstrumentKind::Drums,
            _ => InstrumentKind::Other,
        }
    }
}

/// Last words of GP7 instrument type ids that mean a string instrument
/// (`electricGuitar`, `acousticBass`, `banjo`, ...).
const STRING_INSTRUMENTS: &[&str] = &["guitar", "bass", "banjo", "ukulele", "mandolin"];

/// The standard General MIDI name of a program (0 is Acoustic Grand Piano, 25 Steel
/// String Guitar), as shown to the user. Values above 127 have no name.
pub fn program_name(program: u8) -> &'static str {
    gscore_gp::CHANNEL_DEFAULT_NAMES
        .get(usize::from(program))
        .copied()
        .unwrap_or("Unknown")
}

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

    #[test]
    fn every_symbol_id_is_unique_and_reads_back() {
        let mut ids = Vec::new();
        for symbol in InstrumentSymbol::ALL {
            assert!(!ids.contains(&symbol.id()), "{symbol:?}");
            ids.push(symbol.id());
            assert_eq!(InstrumentSymbol::from_id(symbol.id()), Some(symbol));
        }
        assert_eq!(InstrumentSymbol::from_id("not a symbol"), None);
    }
}
