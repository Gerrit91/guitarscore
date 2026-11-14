//! Guitar Pro's layout properties (`<XProperty>`).
//!
//! GP7 and later write the layout a user set up in the design mode as opaque key/value
//! pairs with numeric ids on a bar or master bar (Guitar Pro's own numbering). The model
//! keeps them verbatim, so a save gives Guitar Pro back exactly what it wrote, and reads
//! the one id it understands (a bar's width scale) through the helpers here.

/// The id of a bar's width scale (Guitar Pro's design mode; `Bar.displayScale`).
pub const BAR_SCALE: &str = "1124139520";

/// The id of a master bar's (song-wide) width scale.
pub const MASTER_BAR_SCALE: &str = "1124073984";

/// The width scale accepted from a file or an edit: a bar can be squeezed to a quarter of
/// its natural width or stretched to four times it. A hostile value outside this cannot
/// make a page impossible to lay out.
pub const SCALE_RANGE: (f32, f32) = (0.25, 4.0);

/// How far a beat's column may be moved from its natural place, in staff spaces.
pub const SPACING_RANGE: (f32, f32) = (-4.0, 4.0);

/// How many properties one bar or master bar keeps (Guitar Pro writes a handful).
pub const MAX_XPROPERTIES: usize = 64;

/// One opaque layout property, kept as the file had it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XProperty {
    /// Guitar Pro's numeric id, as written.
    pub id: String,
    /// The value's element: `Int`, `Double`, `Float` or `Text`.
    pub kind: String,
    /// The value as the file wrote it.
    pub value: String,
}

impl XProperty {
    /// The value as a number, for the ids that hold one.
    fn number(&self) -> Option<f32> {
        let value = self.value.trim().parse::<f64>().ok()?;
        if !value.is_finite() {
            return None;
        }
        // A finite f64 within f32's range converts exactly enough for a length.
        #[allow(clippy::cast_possible_truncation)]
        let value = value as f32;
        value.is_finite().then_some(value)
    }
}

/// The width scale `properties` gives `id`, clamped to [`SCALE_RANGE`].
pub fn property_scale(properties: &[XProperty], id: &str) -> Option<f32> {
    properties
        .iter()
        .find(|property| property.id == id)
        .and_then(XProperty::number)
        .map(|scale| scale.clamp(SCALE_RANGE.0, SCALE_RANGE.1))
}

/// Sets the width scale `id` of `properties` to `scale`, or removes it with `None`. A value
/// that is not a finite number removes it too.
pub fn set_property_scale(properties: &mut Vec<XProperty>, id: &str, scale: Option<f32>) {
    let scale = scale
        .filter(|scale| scale.is_finite())
        .map(|scale| scale.clamp(SCALE_RANGE.0, SCALE_RANGE.1));
    if let Some(property) = properties.iter_mut().find(|property| property.id == id) {
        match scale {
            Some(scale) => {
                property.kind = "Double".to_owned();
                property.value = format!("{scale:.4}");
            }
            None => properties.retain(|property| property.id != id),
        }
    } else if let Some(scale) = scale
        && properties.len() < MAX_XPROPERTIES
    {
        properties.push(XProperty {
            id: id.to_owned(),
            kind: "Double".to_owned(),
            value: format!("{scale:.4}"),
        });
    }
}

/// A beat spacing read from a file: clamped to [`SPACING_RANGE`], `None` when it is not a
/// usable number. A spacing of zero is kept out (it changes nothing).
pub fn beat_spacing(value: f64) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let value = value as f32;
    let value = if value.is_finite() {
        value.clamp(SPACING_RANGE.0, SPACING_RANGE.1)
    } else {
        0.0
    };
    // Anything below a thousandth of a staff space changes nothing worth keeping.
    (value.abs() >= 0.001).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scale_is_kept_and_removed_by_id() {
        let mut properties = Vec::new();
        set_property_scale(&mut properties, BAR_SCALE, Some(1.5));
        assert_eq!(property_scale(&properties, BAR_SCALE), Some(1.5));
        assert_eq!(property_scale(&properties, MASTER_BAR_SCALE), None);
        // Setting it again replaces the value instead of adding another property.
        set_property_scale(&mut properties, BAR_SCALE, Some(0.75));
        assert_eq!(properties.len(), 1);
        assert_eq!(property_scale(&properties, BAR_SCALE), Some(0.75));
        set_property_scale(&mut properties, BAR_SCALE, None);
        assert!(properties.is_empty());
        // A value that is not a number is never stored.
        set_property_scale(&mut properties, BAR_SCALE, Some(f32::NAN));
        assert!(properties.is_empty());
        set_property_scale(&mut properties, BAR_SCALE, Some(f32::INFINITY));
        assert!(properties.is_empty());
    }

    #[test]
    fn hostile_values_are_clamped_or_ignored() {
        let property = |value: &str| XProperty {
            id: BAR_SCALE.to_owned(),
            kind: "Double".to_owned(),
            value: value.to_owned(),
        };
        let properties = [property("1000")];
        assert_eq!(property_scale(&properties, BAR_SCALE), Some(SCALE_RANGE.1));
        let properties = [property("-3")];
        assert_eq!(property_scale(&properties, BAR_SCALE), Some(SCALE_RANGE.0));
        for value in ["nan", "NaN", "inf", "-inf", "1e400", "", "wide"] {
            let properties = [property(value)];
            assert_eq!(property_scale(&properties, BAR_SCALE), None, "{value}");
        }
    }

    #[test]
    fn the_properties_list_is_capped() {
        let mut properties = Vec::new();
        for index in 0..(MAX_XPROPERTIES.saturating_add(10)) {
            set_property_scale(&mut properties, &index.to_string(), Some(1.0));
        }
        assert_eq!(properties.len(), MAX_XPROPERTIES);
    }

    #[test]
    fn a_spacing_is_clamped_and_zero_is_none() {
        assert_eq!(beat_spacing(0.0), None);
        assert_eq!(beat_spacing(0.5), Some(0.5));
        assert_eq!(beat_spacing(99.0), Some(SPACING_RANGE.1));
        assert_eq!(beat_spacing(-99.0), Some(SPACING_RANGE.0));
        assert_eq!(beat_spacing(f64::NAN), None);
        assert_eq!(beat_spacing(f64::INFINITY), None);
        // A tiny value rounds to nothing worth keeping.
        assert_eq!(beat_spacing(1e-9), None);
    }
}
