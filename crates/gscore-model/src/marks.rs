//! Measure marks that Guitar Pro stores in its measure headers: the feel (triplet feel)
//! and fermatas. Guitar Pro's own field has only straight, eighth-note and sixteenth-note
//! triplet feel; dotted feels are a GuitarScore addition kept in `gsFeel`.

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
    /// Dotted-eighth feel (GuitarScore addition; GP7/GPX only).
    DottedEighth,
    /// Dotted-sixteenth feel (GuitarScore addition; GP7/GPX only).
    DottedSixteenth,
}

impl Feel {
    /// Every choice, in dialog order.
    pub const ALL: [Feel; 5] = [
        Feel::None,
        Feel::Eighth,
        Feel::Sixteenth,
        Feel::DottedEighth,
        Feel::DottedSixteenth,
    ];

    /// The id the command language and `gsFeel` use.
    pub fn id(self) -> &'static str {
        match self {
            Feel::None => "none",
            Feel::Eighth => "eighth",
            Feel::Sixteenth => "sixteenth",
            Feel::DottedEighth => "dotted_eighth",
            Feel::DottedSixteenth => "dotted_sixteenth",
        }
    }

    /// A label for menus and dialogs.
    pub fn label(self) -> &'static str {
        match self {
            Feel::None => "Straight",
            Feel::Eighth => "Eighth-note swing",
            Feel::Sixteenth => "Sixteenth-note swing",
            Feel::DottedEighth => "Dotted eighth",
            Feel::DottedSixteenth => "Dotted sixteenth",
        }
    }

    /// The choice named by `id`, if any.
    pub fn from_id(id: &str) -> Option<Self> {
        Feel::ALL.into_iter().find(|feel| feel.id() == id)
    }

    /// The feel a header stores: `gs_feel` wins, then Guitar Pro's triplet feel.
    pub(crate) fn from_gp(triplet: &GpTripletFeel, gs_feel: Option<&str>) -> Self {
        if let Some(feel) = gs_feel.and_then(Feel::from_id) {
            return feel;
        }
        match triplet {
            GpTripletFeel::None => Feel::None,
            GpTripletFeel::Eighth => Feel::Eighth,
            GpTripletFeel::Sixteenth => Feel::Sixteenth,
        }
    }

    /// Guitar Pro's triplet-feel field (dotted feels have none).
    pub(crate) fn to_gp(self) -> GpTripletFeel {
        match self {
            Feel::Eighth => GpTripletFeel::Eighth,
            Feel::Sixteenth => GpTripletFeel::Sixteenth,
            Feel::None | Feel::DottedEighth | Feel::DottedSixteenth => GpTripletFeel::None,
        }
    }

    /// The `gsFeel` value, or `None` for a straight measure.
    pub(crate) fn gs_id(self) -> Option<&'static str> {
        (self != Feel::None).then(|| self.id())
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
