//! Effects on beats and notes: strokes, slap and tapping, slides, bends, harmonics, and the
//! simple on/off techniques (ghost notes, accents, palm mute, let ring, ...).
//!
//! The types are this crate's own; the accessors extend [`BeatView`] and [`NoteView`].

use gscore_gp::{
    BeatStrokeDirection, BendEffect as GpBendEffect, BendPoint as GpBendPoint, BendType,
    HarmonicEffect as GpHarmonicEffect, HarmonicType, LegatoEffect, SlapEffect, SlideType,
};

use crate::error::ModelError;
use crate::grace::Grace;
use crate::techniques::{self, Fade, Finger, Hand, TremoloSpeed, Trill};
use crate::views::{BeatView, NoteView};

/// The direction of a pick or brush stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeDirection {
    /// Towards the higher strings' side (an upstroke).
    Up,
    /// Towards the lower strings' side (a downstroke).
    Down,
}

/// Which kind of spread chord: a brush (strum) or a rolled chord (arpeggio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeKind {
    /// A brush (strum).
    Brush,
    /// A rolled chord (arpeggio).
    Arpeggio,
}

/// A brush or arpeggio: its direction, how long the chord is spread and when it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    /// Which way the strings are struck.
    pub direction: StrokeDirection,
    /// How quick the spread is: the denominator of a note value (8 = over an eighth note).
    pub speed: u8,
    /// The first string starts on the beat (`true`), or the last one lands on it
    /// (`false`: the spread falls before the beat).
    pub on_beat: bool,
}

impl Stroke {
    /// The default speed of a brush stroke (a 32nd note).
    pub const BRUSH_SPEED: u8 = 32;
    /// The default speed of an arpeggio (an eighth note).
    pub const ARPEGGIO_SPEED: u8 = 8;

    /// A brush with the default speed and timing.
    pub fn brush(direction: StrokeDirection) -> Self {
        Stroke {
            direction,
            speed: Self::BRUSH_SPEED,
            on_beat: true,
        }
    }

    /// An arpeggio with the default speed and timing.
    pub fn arpeggio(direction: StrokeDirection) -> Self {
        Stroke {
            direction,
            speed: Self::ARPEGGIO_SPEED,
            on_beat: true,
        }
    }

    /// Checks that the speed is a note value this editor knows.
    pub fn validate(&self) -> Result<(), ModelError> {
        if matches!(self.speed, 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128) {
            Ok(())
        } else {
            Err(ModelError::Invalid(format!(
                "{} is not a note value a stroke can take",
                self.speed
            )))
        }
    }
}

/// A tapping or slap technique played on a whole beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlapKind {
    /// Tapping (right-hand hammer-on).
    Tapping,
    /// Slap.
    Slapping,
    /// Pop.
    Popping,
}

/// How a slide gets to or from its note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideKind {
    /// Slide in from a higher note.
    IntoFromAbove,
    /// Slide in from a lower note.
    IntoFromBelow,
    /// Shift slide to the next note.
    ShiftSlideTo,
    /// Legato slide to the next note.
    LegatoSlideTo,
    /// Slide out downwards, to no particular note.
    OutDownwards,
    /// Slide out upwards, to no particular note.
    OutUpwards,
    /// A pick slide (pick scrape) down the strings.
    PickSlideDown,
    /// A pick slide (pick scrape) up the strings.
    PickSlideUp,
}

/// What a bend does to the pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BendKind {
    /// Bend the string up (including a prebend and a bend that is released again).
    Up,
    /// Release a bend back down.
    Release,
}

/// A bend on a note and how far it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bend {
    /// Whether the bend goes up or releases back down.
    pub kind: BendKind,
    /// How far the bend reaches at its highest point, in quarter tones: a half-step bend
    /// is 2, a full (whole-step) bend 4. Zero when the file stores no bend points.
    pub quarter_tones: i16,
}

/// How strongly a note is accented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentKind {
    /// A normal accent (`>`).
    Normal,
    /// A heavy accent.
    Heavy,
}

/// How wide a note's vibrato is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vibrato {
    /// A slight, narrow vibrato.
    Slight,
    /// A wide vibrato.
    Wide,
}

/// A legato run written over one or more beats (not a slide or a hammer-on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Legato {
    /// This beat starts the run.
    pub origin: bool,
    /// This beat continues the run.
    pub destination: bool,
}

/// A whammy (tremolo) bar effect and how far it moves the pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Whammy {
    /// The furthest point of the shape, in quarter tones as the file stores them; negative
    /// means down (a dive). A whole-step bend is 4.
    pub quarter_tones: i16,
}

/// What kind of harmonic a note is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicKind {
    /// A natural harmonic, written on the fretted note.
    Natural,
    /// An artificial harmonic.
    Artificial,
    /// A tapped harmonic.
    Tapped,
    /// A pinch (squeal) harmonic.
    Pinch,
    /// A semi-harmonic.
    Semi,
}

/// A harmonic on a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Harmonic {
    /// The kind of harmonic.
    pub kind: HarmonicKind,
    /// The node's fret when the file stores the harmonic fret. The file stores it as a
    /// distance from the fretted note, so a pinch harmonic on fret 7 with a stored 12
    /// sounds the node on fret 19.
    pub node: Option<u8>,
}

/// The highest position of a bend or whammy shape: the points run from 0 to 12 over the
/// note's length, as Guitar Pro stores them.
pub const BEND_MAX_POSITION: u8 = 12;

/// The largest bend (or whammy) value in quarter tones: 48 is two octaves.
pub const BEND_MAX_VALUE: i8 = 48;

/// Most points a bend or whammy shape may have.
pub const MAX_BEND_POINTS: usize = 16;

/// Most slides one note may carry (there is one slide type per entry).
const MAX_SLIDES: usize = 8;

/// One point of a bend or whammy shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BendPoint {
    /// Where the point is, as a percentage of the note's length: 0 to 12.
    pub position: u8,
    /// How far the string is bent there, in quarter tones: 2 is a half step, 4 a whole
    /// step; negative means down (a dive).
    pub value: i8,
}

/// A note bend's full shape: its kind and every point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BendShape {
    /// Whether the bend goes up or releases back down.
    pub kind: BendKind,
    /// The points, in order; their positions must increase strictly.
    pub points: Vec<BendPoint>,
}

impl BendShape {
    /// A shape with the given kind and points, checked.
    pub fn new(kind: BendKind, points: Vec<BendPoint>) -> Result<Self, ModelError> {
        let shape = BendShape { kind, points };
        shape.validate()?;
        Ok(shape)
    }

    /// A simple bend that starts at 0 and reaches `quarter_tones` at the end.
    pub fn up(quarter_tones: i16) -> Result<Self, ModelError> {
        let value = i8::try_from(quarter_tones).map_err(|_| {
            ModelError::Invalid(format!(
                "bend {quarter_tones} quarter tones does not fit in a bend point"
            ))
        })?;
        BendShape::new(
            BendKind::Up,
            vec![
                BendPoint {
                    position: 0,
                    value: 0,
                },
                BendPoint {
                    position: BEND_MAX_POSITION,
                    value,
                },
            ],
        )
    }

    /// Checks that there is at least one point and at most [`MAX_BEND_POINTS`], that every
    /// position is at most [`BEND_MAX_POSITION`], that every value is within
    /// ±[`BEND_MAX_VALUE`] quarter tones, and that the positions increase strictly.
    pub fn validate(&self) -> Result<(), ModelError> {
        validate_points(&self.points, "bend")
    }
}

/// A whammy (tremolo) bar shape: the points only; the direction follows from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhammyShape {
    /// The points, in order; their positions must increase strictly.
    pub points: Vec<BendPoint>,
}

impl WhammyShape {
    /// A shape with the given points, checked.
    pub fn new(points: Vec<BendPoint>) -> Result<Self, ModelError> {
        let shape = WhammyShape { points };
        shape.validate()?;
        Ok(shape)
    }

    /// Checks the points like [`BendShape::validate`].
    pub fn validate(&self) -> Result<(), ModelError> {
        validate_points(&self.points, "whammy")
    }
}

/// Checks the points of a bend or whammy shape; `what` names the effect in errors.
fn validate_points(points: &[BendPoint], what: &str) -> Result<(), ModelError> {
    if points.is_empty() {
        return Err(ModelError::Invalid(format!(
            "a {what} needs at least one point"
        )));
    }
    if points.len() > MAX_BEND_POINTS {
        return Err(ModelError::Invalid(format!(
            "a {what} has {} points (supported: at most {MAX_BEND_POINTS})",
            points.len()
        )));
    }
    for point in points {
        if point.position > BEND_MAX_POSITION {
            return Err(ModelError::Invalid(format!(
                "{what} position {} is above {BEND_MAX_POSITION}",
                point.position
            )));
        }
        if point.value < -BEND_MAX_VALUE || point.value > BEND_MAX_VALUE {
            return Err(ModelError::Invalid(format!(
                "{what} value {} is outside -{BEND_MAX_VALUE}..={BEND_MAX_VALUE}",
                point.value
            )));
        }
    }
    for pair in points.windows(2) {
        let [a, b] = pair else { continue };
        if a.position >= b.position {
            return Err(ModelError::Invalid(format!(
                "{what} positions {} and {} do not increase strictly",
                a.position, b.position
            )));
        }
    }
    Ok(())
}

/// One effect of a note to set or clear; every other effect stays as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteEffect {
    /// The note starts a hammer-on or pull-off.
    Hammer(bool),
    /// The note's bend and its shape; `None` removes it.
    Bend(Option<BendShape>),
    /// Adds (`true`) or removes (`false`) one slide type, keeping the others.
    Slide(SlideKind, bool),
    /// The note's harmonic, including its kind and node; `None` removes it.
    Harmonic(Option<Harmonic>),
    /// A ghost note.
    Ghost(bool),
    /// The accent, normal or heavy; `None` removes it.
    Accent(Option<AccentKind>),
    /// A palm mute.
    PalmMute(bool),
    /// Let the note ring.
    LetRing(bool),
    /// A staccato mark.
    Staccato(bool),
    /// A trill with its other fret and speed; `None` removes it.
    Trill(Option<Trill>),
    /// Tremolo picking and its speed; `None` removes it.
    TremoloPicking(Option<TremoloSpeed>),
    /// A left-hand tap (the fretting hand hammers the note on, no pick).
    LeftHandTap(bool),
    /// Write the note's string number (a circled number).
    StringNumber(bool),
    /// The finger of one hand; `None` removes it.
    Finger(Hand, Option<Finger>),
    /// The note's vibrato width; `None` removes it.
    Vibrato(Option<Vibrato>),
    /// The grace note before (or on) the note; `None` removes it.
    Grace(Option<Grace>),
}

/// One effect of a beat to set or clear; every other effect stays as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatEffect {
    /// The pick stroke direction; `None` removes it.
    PickStroke(Option<StrokeDirection>),
    /// The brush (strum) direction; `None` removes it.
    BrushStroke(Option<Stroke>),
    /// Tapping, slap or pop; `None` removes it.
    Slap(Option<SlapKind>),
    /// A fade-in.
    FadeIn(bool),
    /// The whammy bar shape; `None` removes it.
    Whammy(Option<WhammyShape>),
    /// The beat's legato run flags; `None` removes them.
    Legato(Option<Legato>),
    /// The beat's own vibrato width; `None` removes it.
    Vibrato(Option<Vibrato>),
    /// A rolled chord (arpeggio) and its direction; `None` removes it.
    Arpeggio(Option<Stroke>),
    /// A dead slapped beat (a slap on muted strings instead of the notes).
    DeadSlapped(bool),
    /// A rasgueado.
    Rasgueado(bool),
    /// A fade out (switching it on removes a fade-in or volume swell).
    FadeOut(bool),
    /// A volume swell, in and out again (switching it on removes the other fades).
    VolumeSwell(bool),
}

impl NoteEffect {
    /// Checks the parts that can be out of range (a bend shape) before the effect is
    /// written.
    pub fn validate(&self) -> Result<(), ModelError> {
        match self {
            NoteEffect::Bend(Some(shape)) => shape.validate(),
            _ => Ok(()),
        }
    }

    /// Applies this effect to `note`, leaving the note's other effects untouched.
    pub(crate) fn apply(&self, note: &mut gscore_gp::Note) -> Result<(), ModelError> {
        match self {
            NoteEffect::Hammer(on) => note.effect.hammer = *on,
            NoteEffect::Bend(shape) => {
                note.effect.bend = match shape {
                    Some(shape) => Some(bend_to_gp(shape)?),
                    None => None,
                };
            }
            NoteEffect::Slide(kind, on) => toggle_slide(&mut note.effect.slides, *kind, *on)?,
            NoteEffect::Harmonic(harmonic) => {
                note.effect.harmonic = match harmonic {
                    Some(harmonic) => Some(harmonic_to_gp(*harmonic, note.value)?),
                    None => None,
                };
            }
            NoteEffect::Ghost(on) => note.effect.ghost_note = *on,
            NoteEffect::Accent(accent) => {
                note.effect.accentuated_note = accent.is_some();
                note.effect.heavy_accentuated_note = matches!(accent, Some(AccentKind::Heavy));
            }
            NoteEffect::PalmMute(on) => note.effect.palm_mute = *on,
            NoteEffect::LetRing(on) => note.effect.let_ring = *on,
            NoteEffect::Staccato(on) => note.effect.staccato = *on,
            NoteEffect::Trill(trill) => {
                note.effect.trill = match trill {
                    Some(trill) => Some(techniques::trill_to_gp(*trill, note.value)?),
                    None => None,
                };
            }
            NoteEffect::TremoloPicking(speed) => {
                note.effect.tremolo_picking = speed.map(techniques::tremolo_to_gp);
            }
            NoteEffect::LeftHandTap(on) => note.effect.left_hand_tapped = *on,
            NoteEffect::StringNumber(on) => note.effect.show_string_number = *on,
            NoteEffect::Finger(hand, finger) => {
                let finger = techniques::finger_to_gp(*finger);
                match hand {
                    Hand::Left => note.effect.left_hand_finger = finger,
                    Hand::Right => note.effect.right_hand_finger = finger,
                }
            }
            NoteEffect::Vibrato(vibrato) => {
                note.effect.vibrato = vibrato.is_some();
                note.effect.wide_vibrato = matches!(vibrato, Some(Vibrato::Wide));
            }
            NoteEffect::Grace(grace) => Grace::apply(*grace, note)?,
        }
        Ok(())
    }
}

impl BeatEffect {
    /// Checks the parts that can be out of range (a whammy shape) before the effect is
    /// written.
    pub fn validate(&self) -> Result<(), ModelError> {
        match self {
            BeatEffect::Whammy(Some(shape)) => shape.validate(),
            BeatEffect::BrushStroke(Some(stroke)) | BeatEffect::Arpeggio(Some(stroke)) => {
                stroke.validate()
            }
            _ => Ok(()),
        }
    }

    /// Applies this effect to `beat`, leaving the beat's other effects untouched.
    pub(crate) fn apply(&self, beat: &mut gscore_gp::Beat) -> Result<(), ModelError> {
        let effects = &mut beat.effect;
        match self {
            BeatEffect::PickStroke(direction) => {
                effects.pick_stroke = stroke_to_gp(*direction);
            }
            BeatEffect::BrushStroke(stroke) => match stroke {
                Some(stroke) => {
                    effects.stroke.direction = stroke_to_gp(Some(stroke.direction));
                    effects.stroke.value = u16::from(stroke.speed);
                    effects.stroke.on_beat = stroke.on_beat;
                }
                None => effects.stroke = gscore_gp::BeatStroke::default(),
            },
            BeatEffect::Slap(slap) => {
                effects.slap_effect = match slap {
                    Some(SlapKind::Tapping) => SlapEffect::Tapping,
                    Some(SlapKind::Slapping) => SlapEffect::Slapping,
                    Some(SlapKind::Popping) => SlapEffect::Popping,
                    None => SlapEffect::None,
                };
            }
            BeatEffect::FadeIn(on) => techniques::set_fade(effects, Fade::In, *on),
            BeatEffect::FadeOut(on) => techniques::set_fade(effects, Fade::Out, *on),
            BeatEffect::VolumeSwell(on) => techniques::set_fade(effects, Fade::Swell, *on),
            BeatEffect::Arpeggio(stroke) => match stroke {
                Some(stroke) => {
                    effects.arpeggio = stroke_to_gp(Some(stroke.direction));
                    effects.arpeggio_speed = u16::from(stroke.speed);
                    effects.arpeggio_on_beat = stroke.on_beat;
                }
                None => {
                    effects.arpeggio = BeatStrokeDirection::None;
                    effects.arpeggio_speed = 0;
                    effects.arpeggio_on_beat = true;
                }
            },
            BeatEffect::DeadSlapped(on) => effects.dead_slapped = *on,
            BeatEffect::Rasgueado(on) => effects.has_rasgueado = *on,
            BeatEffect::Whammy(shape) => {
                effects.tremolo_bar = match shape {
                    Some(shape) => Some(whammy_to_gp(shape)?),
                    None => None,
                };
            }
            BeatEffect::Legato(legato) => {
                effects.legato = match legato {
                    Some(legato) => LegatoEffect {
                        origin: legato.origin,
                        destination: legato.destination,
                    },
                    None => LegatoEffect::default(),
                };
            }
            BeatEffect::Vibrato(vibrato) => {
                effects.vibrato = vibrato.is_some();
                effects.wide_vibrato = matches!(vibrato, Some(Vibrato::Wide));
            }
        }
        Ok(())
    }
}

/// The stroke direction as Guitar Pro stores it.
fn stroke_to_gp(direction: Option<StrokeDirection>) -> BeatStrokeDirection {
    match direction {
        Some(StrokeDirection::Up) => BeatStrokeDirection::Up,
        Some(StrokeDirection::Down) => BeatStrokeDirection::Down,
        None => BeatStrokeDirection::None,
    }
}

/// A stored stroke speed as a note value, or `default` when the file had none or a value
/// this editor does not know.
fn valid_speed(value: u16, default: u8) -> u8 {
    u8::try_from(value)
        .ok()
        .filter(|value| matches!(value, 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128))
        .unwrap_or(default)
}

/// Adds or removes one slide type, keeping the others, and checks the resulting list.
fn toggle_slide(slides: &mut Vec<SlideType>, kind: SlideKind, on: bool) -> Result<(), ModelError> {
    let slide = match kind {
        SlideKind::IntoFromAbove => SlideType::IntoFromAbove,
        SlideKind::IntoFromBelow => SlideType::IntoFromBelow,
        SlideKind::ShiftSlideTo => SlideType::ShiftSlideTo,
        SlideKind::LegatoSlideTo => SlideType::LegatoSlideTo,
        SlideKind::OutDownwards => SlideType::OutDownwards,
        SlideKind::OutUpwards => SlideType::OutUpWards,
        SlideKind::PickSlideDown => SlideType::PickSlideDown,
        SlideKind::PickSlideUp => SlideType::PickSlideUp,
    };
    if on {
        if !slides.contains(&slide) {
            slides.push(slide);
        }
    } else {
        slides.retain(|existing| *existing != slide);
    }
    if slides.len() > MAX_SLIDES {
        return Err(ModelError::Invalid(format!(
            "a note has {} slides (supported: at most {MAX_SLIDES})",
            slides.len()
        )));
    }
    Ok(())
}

/// The bend shape as Guitar Pro stores it. `value` and `max_value` are summary fields the
/// editors display; they follow the shape's furthest point.
fn bend_to_gp(shape: &BendShape) -> Result<GpBendEffect, ModelError> {
    shape.validate()?;
    let kind = match shape.kind {
        BendKind::Up => BendType::Bend,
        BendKind::Release => BendType::ReleaseUp,
    };
    let mut bend = GpBendEffect {
        kind,
        points: shape
            .points
            .iter()
            .map(|point| GpBendPoint {
                position: point.position,
                value: point.value,
                vibrato: false,
            })
            .collect(),
        ..GpBendEffect::default()
    };
    let extreme = extreme_value(&shape.points);
    bend.value = extreme;
    bend.max_value = u8::try_from(extreme.unsigned_abs()).map_err(|_| {
        ModelError::Invalid(format!("bend value {extreme} does not fit its summary"))
    })?;
    Ok(bend)
}

/// The whammy shape as Guitar Pro stores it. Its kind follows the shape: back to where it
/// started is a dip, lower is a dive, higher is an upward release.
fn whammy_to_gp(shape: &WhammyShape) -> Result<GpBendEffect, ModelError> {
    shape.validate()?;
    let first = shape.points.first().map_or(0, |point| point.value);
    let last = shape.points.last().map_or(0, |point| point.value);
    let kind = if last == first {
        BendType::Dip
    } else if last < first {
        BendType::Dive
    } else {
        BendType::ReleaseUp
    };
    let mut bar = GpBendEffect {
        kind,
        points: shape
            .points
            .iter()
            .map(|point| GpBendPoint {
                position: point.position,
                value: point.value,
                vibrato: false,
            })
            .collect(),
        ..GpBendEffect::default()
    };
    let extreme = extreme_value(&shape.points);
    bar.value = extreme;
    bar.max_value = u8::try_from(extreme.unsigned_abs()).map_err(|_| {
        ModelError::Invalid(format!("whammy value {extreme} does not fit its summary"))
    })?;
    Ok(bar)
}

/// A stored point as this crate's type; whether it is a valid shape is up to
/// [`BendShape::validate`] or [`WhammyShape::validate`], so reading does not drop data.
fn model_point(point: &GpBendPoint) -> BendPoint {
    BendPoint {
        position: point.position,
        value: point.value,
    }
}

/// The value of the point furthest from zero, signed (empty shapes have none, but their
/// validation rejects them first).
fn extreme_value(points: &[BendPoint]) -> i16 {
    points
        .iter()
        .map(|point| i16::from(point.value))
        .max_by_key(|value| value.unsigned_abs())
        .unwrap_or(0)
}

/// The harmonic as Guitar Pro stores it: the kind plus the node as an offset from the
/// fretted note.
fn harmonic_to_gp(harmonic: Harmonic, fret: i16) -> Result<GpHarmonicEffect, ModelError> {
    let kind = match harmonic.kind {
        HarmonicKind::Natural => HarmonicType::Natural,
        HarmonicKind::Artificial => HarmonicType::Artificial,
        HarmonicKind::Tapped => HarmonicType::Tapped,
        HarmonicKind::Pinch => HarmonicType::Pinch,
        HarmonicKind::Semi => HarmonicType::Semi,
    };
    let node = harmonic.node.map(i32::from);
    let offset = match node {
        Some(node) => {
            let offset = node
                .checked_sub(i32::from(fret))
                .ok_or_else(|| ModelError::Invalid(format!("harmonic node {node} does not fit")))?;
            Some(i8::try_from(offset).map_err(|_| {
                ModelError::Invalid(format!("harmonic node {node} is too far from fret {fret}"))
            })?)
        }
        None => None,
    };
    Ok(GpHarmonicEffect {
        kind,
        fret: offset,
        ..GpHarmonicEffect::default()
    })
}

impl BeatView<'_> {
    /// The pick stroke on this beat, if any.
    pub fn pick_stroke(&self) -> Option<StrokeDirection> {
        match self.inner.effect.pick_stroke {
            BeatStrokeDirection::Up => Some(StrokeDirection::Up),
            BeatStrokeDirection::Down => Some(StrokeDirection::Down),
            BeatStrokeDirection::None => None,
        }
    }

    /// The brush stroke (strum direction) on this beat, if any.
    pub fn brush_stroke(&self) -> Option<StrokeDirection> {
        self.stroke(StrokeKind::Brush)
            .map(|stroke| stroke.direction)
    }

    /// The brush or arpeggio with its direction, speed and timing, if the beat has one.
    pub fn stroke(&self, kind: StrokeKind) -> Option<Stroke> {
        let direction = match kind {
            StrokeKind::Brush => {
                techniques::direction_from_gp(&self.inner.effect.stroke.direction)?
            }
            StrokeKind::Arpeggio => techniques::direction_from_gp(&self.inner.effect.arpeggio)?,
        };
        let (speed, default, on_beat) = match kind {
            StrokeKind::Brush => (
                self.inner.effect.stroke.value,
                Stroke::BRUSH_SPEED,
                self.inner.effect.stroke.on_beat,
            ),
            StrokeKind::Arpeggio => (
                self.inner.effect.arpeggio_speed,
                Stroke::ARPEGGIO_SPEED,
                self.inner.effect.arpeggio_on_beat,
            ),
        };
        Some(Stroke {
            direction,
            speed: valid_speed(speed, default),
            on_beat,
        })
    }

    /// The tapping or slap technique on this beat, if any.
    pub fn slap(&self) -> Option<SlapKind> {
        match self.inner.effect.slap_effect {
            SlapEffect::Tapping => Some(SlapKind::Tapping),
            SlapEffect::Slapping => Some(SlapKind::Slapping),
            SlapEffect::Popping => Some(SlapKind::Popping),
            SlapEffect::None => None,
        }
    }

    /// True if the beat fades in.
    pub fn has_fade_in(&self) -> bool {
        self.inner.effect.fade_in
    }

    /// True if the beat uses the whammy bar (tremolo bar).
    pub fn has_whammy_bar(&self) -> bool {
        self.inner.effect.tremolo_bar.is_some()
    }

    /// How wide the beat's own vibrato is, if it has one. This is separate from a note's
    /// vibrato: Guitar Pro writes it as the `VibratoWTremBar` beat property.
    pub fn vibrato(&self) -> Option<Vibrato> {
        if !self.inner.effect.vibrato {
            return None;
        }
        Some(if self.inner.effect.wide_vibrato {
            Vibrato::Wide
        } else {
            Vibrato::Slight
        })
    }

    /// What the whammy bar does, if the beat uses it.
    pub fn whammy_bar(&self) -> Option<Whammy> {
        let bar = self.inner.effect.tremolo_bar.as_ref()?;
        let quarter_tones = bar
            .points
            .iter()
            .map(|point| i16::from(point.value))
            .max_by_key(|value| value.unsigned_abs())
            .unwrap_or(0);
        Some(Whammy { quarter_tones })
    }

    /// The whammy bar's full shape, if the beat uses it. The stored points are returned as
    /// they are; [`WhammyShape::validate`] checks them.
    pub fn whammy_shape(&self) -> Option<WhammyShape> {
        let bar = self.inner.effect.tremolo_bar.as_ref()?;
        Some(WhammyShape {
            points: bar.points.iter().map(model_point).collect(),
        })
    }

    /// The beat-level legato run this beat is part of, if any.
    pub fn legato(&self) -> Option<Legato> {
        let legato = self.inner.effect.legato;
        (legato.origin || legato.destination).then_some(Legato {
            origin: legato.origin,
            destination: legato.destination,
        })
    }
}

impl NoteView<'_> {
    /// True if this note is the origin of a hammer-on or pull-off.
    pub fn is_hammer(&self) -> bool {
        self.inner.effect.hammer
    }

    /// What the note's bend does, if it has one.
    pub fn bend(&self) -> Option<Bend> {
        let bend = self.inner.effect.bend.as_ref()?;
        let kind = match bend.kind {
            BendType::Bend
            | BendType::BendRelease
            | BendType::BendReleaseBend
            | BendType::Prebend
            | BendType::PrebendRelease => BendKind::Up,
            BendType::ReleaseUp | BendType::ReleaseDown => BendKind::Release,
            BendType::None
            | BendType::Dip
            | BendType::Dive
            | BendType::InvertedDip
            | BendType::Return => return None,
        };
        // Bend points are stored in quarter tones. The highest point is how far the bend
        // reaches, also for a bend that is released again (whose last point is 0).
        let quarter_tones = bend
            .points
            .iter()
            .map(|point| i16::from(point.value))
            .max_by_key(|value| value.unsigned_abs())
            .unwrap_or(0);
        Some(Bend {
            kind,
            quarter_tones,
        })
    }

    /// The bend's full shape, if the note has a bend. The stored points are returned as
    /// they are; [`BendShape::validate`] checks them.
    pub fn bend_shape(&self) -> Option<BendShape> {
        let bend = self.inner.effect.bend.as_ref()?;
        let kind = match bend.kind {
            BendType::Bend
            | BendType::BendRelease
            | BendType::BendReleaseBend
            | BendType::Prebend
            | BendType::PrebendRelease => BendKind::Up,
            BendType::ReleaseUp | BendType::ReleaseDown => BendKind::Release,
            BendType::None
            | BendType::Dip
            | BendType::Dive
            | BendType::InvertedDip
            | BendType::Return => return None,
        };
        Some(BendShape {
            kind,
            points: bend.points.iter().map(model_point).collect(),
        })
    }

    /// The slides on this note, in the order the file stores them.
    pub fn slides(&self) -> impl Iterator<Item = SlideKind> + '_ {
        self.inner
            .effect
            .slides
            .iter()
            .filter_map(|slide| match slide {
                SlideType::IntoFromAbove => Some(SlideKind::IntoFromAbove),
                SlideType::IntoFromBelow => Some(SlideKind::IntoFromBelow),
                SlideType::ShiftSlideTo => Some(SlideKind::ShiftSlideTo),
                SlideType::LegatoSlideTo => Some(SlideKind::LegatoSlideTo),
                SlideType::OutDownwards => Some(SlideKind::OutDownwards),
                SlideType::OutUpWards => Some(SlideKind::OutUpwards),
                SlideType::PickSlideDown => Some(SlideKind::PickSlideDown),
                SlideType::PickSlideUp => Some(SlideKind::PickSlideUp),
                SlideType::None => None,
            })
    }

    /// The harmonic on this note, with its kind and node.
    pub fn harmonic(&self) -> Option<Harmonic> {
        let harmonic = self.inner.effect.harmonic.as_ref()?;
        let kind = match harmonic.kind {
            HarmonicType::Natural => HarmonicKind::Natural,
            HarmonicType::Artificial => HarmonicKind::Artificial,
            HarmonicType::Tapped => HarmonicKind::Tapped,
            HarmonicType::Pinch => HarmonicKind::Pinch,
            HarmonicType::Semi => HarmonicKind::Semi,
        };
        let node = harmonic.fret.and_then(|offset| {
            i32::from(self.inner.value)
                .checked_add(i32::from(offset))
                .and_then(|node| u8::try_from(node).ok())
        });
        Some(Harmonic { kind, node })
    }

    /// True if the note is a ghost note.
    pub fn is_ghost(&self) -> bool {
        self.inner.effect.ghost_note
    }

    /// True if the note has a normal accent.
    pub fn is_accented(&self) -> bool {
        self.inner.effect.accentuated_note
    }

    /// How the note is accented, if it is.
    pub fn accent(&self) -> Option<AccentKind> {
        if self.inner.effect.heavy_accentuated_note {
            Some(AccentKind::Heavy)
        } else if self.inner.effect.accentuated_note {
            Some(AccentKind::Normal)
        } else {
            None
        }
    }

    /// True if the note is palm muted.
    pub fn is_palm_muted(&self) -> bool {
        self.inner.effect.palm_mute
    }

    /// True if the note should let ring.
    pub fn is_let_ring(&self) -> bool {
        self.inner.effect.let_ring
    }

    /// True if the note is staccato.
    pub fn is_staccato(&self) -> bool {
        self.inner.effect.staccato
    }

    /// True if the note has a trill.
    pub fn has_trill(&self) -> bool {
        self.inner.effect.trill.is_some()
    }

    /// True if the note is tremolo picked.
    pub fn has_tremolo_picking(&self) -> bool {
        self.inner.effect.tremolo_picking.is_some()
    }

    /// True if the note has vibrato (slight or wide).
    pub fn has_vibrato(&self) -> bool {
        self.inner.effect.vibrato
    }

    /// How wide the note's vibrato is, if it has one.
    pub fn vibrato(&self) -> Option<Vibrato> {
        if !self.inner.effect.vibrato {
            return None;
        }
        Some(if self.inner.effect.wide_vibrato {
            Vibrato::Wide
        } else {
            Vibrato::Slight
        })
    }
}
