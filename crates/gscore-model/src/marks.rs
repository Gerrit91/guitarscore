//! Measure marks that Guitar Pro stores in its measure headers: the feel (triplet feel)
//! and fermatas. GP7/GPX keep every feel; GP3-5 only the triplet feels.

use gscore_gp::{DirectionSign as GpDirectionSign, TripletFeel as GpTripletFeel};

/// How a measure's notes are played rhythmically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Feel {
    /// Straight, as written.
    #[default]
    None,
    /// Eighth notes in a triplet feel (swing).
    Eighth,
    /// Sixteenth notes in a triplet feel.
    Sixteenth,
    /// Dotted-eighth feel: a dotted eighth and a sixteenth (GP7/GPX only).
    DottedEighth,
    /// Dotted-sixteenth feel (GP7/GPX only).
    DottedSixteenth,
    /// Scottish (Scotch snap) eighth feel: a sixteenth, then a dotted eighth (GP7/GPX only).
    ScottishEighth,
    /// Scottish sixteenth feel (GP7/GPX only).
    ScottishSixteenth,
}

impl Feel {
    /// Every choice, in dialog order.
    pub const ALL: [Feel; 7] = [
        Feel::None,
        Feel::Eighth,
        Feel::Sixteenth,
        Feel::DottedEighth,
        Feel::DottedSixteenth,
        Feel::ScottishEighth,
        Feel::ScottishSixteenth,
    ];

    /// The id the command language and `gsFeel` use.
    pub fn id(self) -> &'static str {
        match self {
            Feel::None => "none",
            Feel::Eighth => "eighth",
            Feel::Sixteenth => "sixteenth",
            Feel::DottedEighth => "dotted_eighth",
            Feel::DottedSixteenth => "dotted_sixteenth",
            Feel::ScottishEighth => "scottish_eighth",
            Feel::ScottishSixteenth => "scottish_sixteenth",
        }
    }

    /// A label for menus and dialogs.
    pub fn label(self) -> &'static str {
        match self {
            Feel::None => "Straight",
            Feel::Eighth => "Triplet eighths (swing)",
            Feel::Sixteenth => "Triplet sixteenths",
            Feel::DottedEighth => "Dotted eighths",
            Feel::DottedSixteenth => "Dotted sixteenths",
            Feel::ScottishEighth => "Scottish eighths",
            Feel::ScottishSixteenth => "Scottish sixteenths",
        }
    }

    /// The choice named by `id`, if any.
    pub fn from_id(id: &str) -> Option<Self> {
        Feel::ALL.into_iter().find(|feel| feel.id() == id)
    }

    /// The feel of a Guitar Pro measure header.
    pub(crate) fn from_gp(triplet: &GpTripletFeel) -> Self {
        match triplet {
            GpTripletFeel::None => Feel::None,
            GpTripletFeel::Eighth => Feel::Eighth,
            GpTripletFeel::Sixteenth => Feel::Sixteenth,
            GpTripletFeel::DottedEighth => Feel::DottedEighth,
            GpTripletFeel::DottedSixteenth => Feel::DottedSixteenth,
            GpTripletFeel::ScottishEighth => Feel::ScottishEighth,
            GpTripletFeel::ScottishSixteenth => Feel::ScottishSixteenth,
        }
    }

    /// Guitar Pro's triplet-feel field.
    pub(crate) fn to_gp(self) -> GpTripletFeel {
        match self {
            Feel::None => GpTripletFeel::None,
            Feel::Eighth => GpTripletFeel::Eighth,
            Feel::Sixteenth => GpTripletFeel::Sixteenth,
            Feel::DottedEighth => GpTripletFeel::DottedEighth,
            Feel::DottedSixteenth => GpTripletFeel::DottedSixteenth,
            Feel::ScottishEighth => GpTripletFeel::ScottishEighth,
            Feel::ScottishSixteenth => GpTripletFeel::ScottishSixteenth,
        }
    }

    /// Whether the swung pairs are sixteenths (otherwise eighths).
    pub fn is_sixteenth(self) -> bool {
        matches!(
            self,
            Feel::Sixteenth | Feel::DottedSixteenth | Feel::ScottishSixteenth
        )
    }

    /// The share of a swung pair the first note takes, as `(numerator, denominator)`, or
    /// `None` for straight: 2/3 for a triplet feel, 3/4 dotted, 1/4 Scottish.
    pub fn first_share(self) -> Option<(u8, u8)> {
        match self {
            Feel::None => None,
            Feel::Eighth | Feel::Sixteenth => Some((2, 3)),
            Feel::DottedEighth | Feel::DottedSixteenth => Some((3, 4)),
            Feel::ScottishEighth | Feel::ScottishSixteenth => Some((1, 4)),
        }
    }
}

/// A fermata's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fermata {
    /// Very short.
    VeryShort,
    /// Short.
    Short,
    /// The ordinary fermata.
    Medium,
    /// Long.
    Long,
    /// Very long.
    VeryLong,
}

impl Fermata {
    /// Every choice, in dialog order.
    pub const ALL: [Fermata; 5] = [
        Fermata::VeryShort,
        Fermata::Short,
        Fermata::Medium,
        Fermata::Long,
        Fermata::VeryLong,
    ];

    /// The id the command language uses.
    pub fn id(self) -> &'static str {
        match self {
            Fermata::VeryShort => "very_short",
            Fermata::Short => "short",
            Fermata::Medium => "medium",
            Fermata::Long => "long",
            Fermata::VeryLong => "very_long",
        }
    }

    /// A label for menus and dialogs.
    pub fn label(self) -> &'static str {
        match self {
            Fermata::VeryShort => "Very short",
            Fermata::Short => "Short",
            Fermata::Medium => "Medium",
            Fermata::Long => "Long",
            Fermata::VeryLong => "Very long",
        }
    }

    /// The choice named by `id`, if any.
    pub fn from_id(id: &str) -> Option<Self> {
        Fermata::ALL.into_iter().find(|fermata| fermata.id() == id)
    }

    /// The type Guitar Pro's GPIF stores.
    pub(crate) fn to_gp(self) -> &'static str {
        match self {
            Fermata::VeryShort => "VeryShort",
            Fermata::Short => "Short",
            Fermata::Medium => "Medium",
            Fermata::Long => "Long",
            Fermata::VeryLong => "VeryLong",
        }
    }

    /// The fermata for a GPIF type; unknown types read as the ordinary one.
    pub(crate) fn from_gp(value: &str) -> Self {
        match value {
            "VeryShort" => Fermata::VeryShort,
            "Short" => Fermata::Short,
            "Long" => Fermata::Long,
            "VeryLong" => Fermata::VeryLong,
            _ => Fermata::Medium,
        }
    }
}

impl Fermata {
    /// How much longer the fermata holds the end of its measure, in quarter notes
    /// (`numerator / denominator`): a beat for the ordinary one, more for the long ones.
    pub fn hold_quarters(self) -> (u8, u8) {
        match self {
            Fermata::VeryShort => (1, 4),
            Fermata::Short => (1, 2),
            Fermata::Medium => (1, 1),
            Fermata::Long => (2, 1),
            Fermata::VeryLong => (3, 1),
        }
    }
}

/// A navigation sign stored at a measure: a coda or segno to jump to, a fine to stop at, or
/// a direction to jump from (D.C., D.S., ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// The coda (𝄌).
    Coda,
    /// The second coda.
    DoubleCoda,
    /// The segno (𝄋).
    Segno,
    /// The second segno.
    SegnoSegno,
    /// Stop here when returning from a D.C. or D.S.
    Fine,
    /// Da capo: play again from the beginning.
    DaCapo,
    /// Da capo al coda.
    DaCapoAlCoda,
    /// Da capo al double coda.
    DaCapoAlDoubleCoda,
    /// Da capo al fine.
    DaCapoAlFine,
    /// Dal segno: play again from the segno.
    DaSegno,
    /// Dal segno al coda.
    DaSegnoAlCoda,
    /// Dal segno al double coda.
    DaSegnoAlDoubleCoda,
    /// Dal segno al fine.
    DaSegnoAlFine,
    /// Dal segno segno: play again from the second segno.
    DaSegnoSegno,
    /// Dal segno segno al coda.
    DaSegnoSegnoAlCoda,
    /// Dal segno segno al double coda.
    DaSegnoSegnoAlDoubleCoda,
    /// Dal segno segno al fine.
    DaSegnoSegnoAlFine,
    /// To coda: jump to the coda when returning.
    DaCoda,
    /// To double coda: jump to the second coda when returning.
    DaDoubleCoda,
}

impl Direction {
    /// Every direction sign, in GPIF order.
    pub const ALL: [Direction; 19] = [
        Direction::Coda,
        Direction::DoubleCoda,
        Direction::Segno,
        Direction::SegnoSegno,
        Direction::Fine,
        Direction::DaCapo,
        Direction::DaCapoAlCoda,
        Direction::DaCapoAlDoubleCoda,
        Direction::DaCapoAlFine,
        Direction::DaSegno,
        Direction::DaSegnoAlCoda,
        Direction::DaSegnoAlDoubleCoda,
        Direction::DaSegnoAlFine,
        Direction::DaSegnoSegno,
        Direction::DaSegnoSegnoAlCoda,
        Direction::DaSegnoSegnoAlDoubleCoda,
        Direction::DaSegnoSegnoAlFine,
        Direction::DaCoda,
        Direction::DaDoubleCoda,
    ];

    /// The id the command language uses.
    pub fn id(self) -> &'static str {
        match self {
            Direction::Coda => "coda",
            Direction::DoubleCoda => "double_coda",
            Direction::Segno => "segno",
            Direction::SegnoSegno => "segno_segno",
            Direction::Fine => "fine",
            Direction::DaCapo => "da_capo",
            Direction::DaCapoAlCoda => "da_capo_al_coda",
            Direction::DaCapoAlDoubleCoda => "da_capo_al_double_coda",
            Direction::DaCapoAlFine => "da_capo_al_fine",
            Direction::DaSegno => "da_segno",
            Direction::DaSegnoAlCoda => "da_segno_al_coda",
            Direction::DaSegnoAlDoubleCoda => "da_segno_al_double_coda",
            Direction::DaSegnoAlFine => "da_segno_al_fine",
            Direction::DaSegnoSegno => "da_segno_segno",
            Direction::DaSegnoSegnoAlCoda => "da_segno_segno_al_coda",
            Direction::DaSegnoSegnoAlDoubleCoda => "da_segno_segno_al_double_coda",
            Direction::DaSegnoSegnoAlFine => "da_segno_segno_al_fine",
            Direction::DaCoda => "da_coda",
            Direction::DaDoubleCoda => "da_double_coda",
        }
    }

    /// A label for menus and dialogs.
    pub fn label(self) -> &'static str {
        match self {
            Direction::Coda => "Coda",
            Direction::DoubleCoda => "Double coda",
            Direction::Segno => "Segno",
            Direction::SegnoSegno => "Segno segno",
            Direction::Fine => "Fine",
            Direction::DaCapo => "Da capo (D.C.)",
            Direction::DaCapoAlCoda => "Da capo al coda",
            Direction::DaCapoAlDoubleCoda => "Da capo al double coda",
            Direction::DaCapoAlFine => "Da capo al fine",
            Direction::DaSegno => "Dal segno (D.S.)",
            Direction::DaSegnoAlCoda => "Dal segno al coda",
            Direction::DaSegnoAlDoubleCoda => "Dal segno al double coda",
            Direction::DaSegnoAlFine => "Dal segno al fine",
            Direction::DaSegnoSegno => "Dal segno segno",
            Direction::DaSegnoSegnoAlCoda => "Dal segno segno al coda",
            Direction::DaSegnoSegnoAlDoubleCoda => "Dal segno segno al double coda",
            Direction::DaSegnoSegnoAlFine => "Dal segno segno al fine",
            Direction::DaCoda => "To coda",
            Direction::DaDoubleCoda => "To double coda",
        }
    }

    /// The choice named by `id`, if any.
    pub fn from_id(id: &str) -> Option<Self> {
        Direction::ALL
            .into_iter()
            .find(|direction| direction.id() == id)
    }

    /// The direction a Guitar Pro measure header stores.
    pub(crate) fn from_gp(sign: &GpDirectionSign) -> Self {
        match sign {
            GpDirectionSign::Coda => Direction::Coda,
            GpDirectionSign::DoubleCoda => Direction::DoubleCoda,
            GpDirectionSign::Segno => Direction::Segno,
            GpDirectionSign::SegnoSegno => Direction::SegnoSegno,
            GpDirectionSign::Fine => Direction::Fine,
            GpDirectionSign::DaCapo => Direction::DaCapo,
            GpDirectionSign::DaCapoAlCoda => Direction::DaCapoAlCoda,
            GpDirectionSign::DaCapoAlDoubleCoda => Direction::DaCapoAlDoubleCoda,
            GpDirectionSign::DaCapoAlFine => Direction::DaCapoAlFine,
            GpDirectionSign::DaSegno => Direction::DaSegno,
            GpDirectionSign::DaSegnoAlCoda => Direction::DaSegnoAlCoda,
            GpDirectionSign::DaSegnoAlDoubleCoda => Direction::DaSegnoAlDoubleCoda,
            GpDirectionSign::DaSegnoAlFine => Direction::DaSegnoAlFine,
            GpDirectionSign::DaSegnoSegno => Direction::DaSegnoSegno,
            GpDirectionSign::DaSegnoSegnoAlCoda => Direction::DaSegnoSegnoAlCoda,
            GpDirectionSign::DaSegnoSegnoAlDoubleCoda => Direction::DaSegnoSegnoAlDoubleCoda,
            GpDirectionSign::DaSegnoSegnoAlFine => Direction::DaSegnoSegnoAlFine,
            GpDirectionSign::DaCoda => Direction::DaCoda,
            GpDirectionSign::DaDoubleCoda => Direction::DaDoubleCoda,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for direction in Direction::ALL {
            assert_eq!(Direction::from_id(direction.id()), Some(direction));
        }
        for feel in Feel::ALL {
            assert_eq!(Feel::from_id(feel.id()), Some(feel));
        }
        for fermata in Fermata::ALL {
            assert_eq!(Fermata::from_id(fermata.id()), Some(fermata));
            assert_eq!(Fermata::from_gp(fermata.to_gp()), fermata);
        }
        assert_eq!(Feel::from_id("swing"), None);
        assert_eq!(Fermata::from_id(""), None);
    }
}
