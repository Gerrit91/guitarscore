//! Measure marks that Guitar Pro stores in its measure headers: the feel (triplet feel)
//! and fermatas. GP7/GPX keep every feel; GP3-5 only the triplet feels.

use gscore_gp::TripletFeel as GpTripletFeel;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
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
