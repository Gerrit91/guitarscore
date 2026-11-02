//! Errors of reading and writing Guitar Pro files, and checked number conversions.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use fraction::ToPrimitive;
use thiserror::Error;

/// Error type for Guitar Pro file parsing
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GpError {
    /// Reached end of binary data unexpectedly
    #[error("Unexpected EOF at offset {offset}, needed {needed} bytes")]
    UnexpectedEof { offset: usize, needed: usize },

    /// Invalid enum/flag value encountered during parsing
    #[error("Invalid value {value} for {context}")]
    InvalidValue { context: &'static str, value: i64 },

    /// String decoding failure
    #[error("String decode error at offset {offset}")]
    StringDecode { offset: usize },

    /// ZIP, XML, or format-level errors from GP6/GP7 parsing
    #[error("Format error: {0}")]
    FormatError(String),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Type conversion failed (value out of range for target type)
    #[error("Type conversion failed for {context}: {value} out of range")]
    TypeConversion { context: &'static str, value: i64 },

    /// Required state not set (e.g., current_track not initialized)
    #[error("Required state '{field}' not set")]
    MissingState { field: &'static str },

    /// A reference to an item (track, channel, measure, ...) that does not exist
    #[error("{what} {index} does not exist")]
    NoSuch { what: &'static str, index: usize },

    /// Write operation error
    #[error("Write error: {0}")]
    WriteError(String),

    /// MSCZ archive (ZIP/container) error
    #[error("MSCZ archive error: {0}")]
    MsczArchive(String),

    /// MSCX XML parse error
    #[error("MSCX XML error: {0}")]
    MsczXml(String),

    /// MuseScore version outside the supported range
    #[error("Unsupported MuseScore version {got:?} (supported: {supported})")]
    MsczUnsupported {
        got: String,
        supported: &'static str,
    },
}

/// Convenience type alias
pub type GpResult<T> = Result<T, GpError>;

impl From<String> for GpError {
    fn from(msg: String) -> Self {
        GpError::FormatError(msg)
    }
}

/// Extension trait for numeric conversions with context-aware errors.
/// Use this instead of `ToPrimitive::to_*().unwrap()` to get proper error messages.
pub(crate) trait ToPrimitiveGp {
    fn to_u8_gp(&self, ctx: &'static str) -> GpResult<u8>;
    fn to_i8_gp(&self, ctx: &'static str) -> GpResult<i8>;
    fn to_i16_gp(&self, ctx: &'static str) -> GpResult<i16>;
    fn to_u16_gp(&self, ctx: &'static str) -> GpResult<u16>;
    fn to_i32_gp(&self, ctx: &'static str) -> GpResult<i32>;
    fn to_i64_gp(&self, ctx: &'static str) -> GpResult<i64>;
    fn to_usize_gp(&self, ctx: &'static str) -> GpResult<usize>;
    fn to_f32_gp(&self, ctx: &'static str) -> GpResult<f32>;
    fn to_f64_gp(&self, ctx: &'static str) -> GpResult<f64>;
}

/// The value an error message shows: the number itself, or the nearest `i64` (`i64::MAX`
/// for NaN) when it does not fit.
fn shown<T: ToPrimitive>(value: &T) -> i64 {
    value
        .to_i64()
        .or_else(|| {
            value
                .to_f64()
                .map(|f| if f < 0.0 { i64::MIN } else { i64::MAX })
        })
        .unwrap_or(i64::MAX)
}

/// The conversion, or a [`GpError::TypeConversion`] naming `ctx` and the value.
fn convert<T: ToPrimitive, U>(value: &T, ctx: &'static str, to: Option<U>) -> GpResult<U> {
    to.ok_or(GpError::TypeConversion {
        context: ctx,
        value: shown(value),
    })
}

macro_rules! impl_to_primitive_gp {
    ($($t:ty),*) => {
        $(
            impl ToPrimitiveGp for $t {
                fn to_u8_gp(&self, ctx: &'static str) -> GpResult<u8> {
                    convert(self, ctx, self.to_u8())
                }
                fn to_i8_gp(&self, ctx: &'static str) -> GpResult<i8> {
                    convert(self, ctx, self.to_i8())
                }
                fn to_i16_gp(&self, ctx: &'static str) -> GpResult<i16> {
                    convert(self, ctx, self.to_i16())
                }
                fn to_u16_gp(&self, ctx: &'static str) -> GpResult<u16> {
                    convert(self, ctx, self.to_u16())
                }
                fn to_i32_gp(&self, ctx: &'static str) -> GpResult<i32> {
                    convert(self, ctx, self.to_i32())
                }
                fn to_i64_gp(&self, ctx: &'static str) -> GpResult<i64> {
                    convert(self, ctx, self.to_i64())
                }
                fn to_usize_gp(&self, ctx: &'static str) -> GpResult<usize> {
                    convert(self, ctx, self.to_usize())
                }
                fn to_f32_gp(&self, ctx: &'static str) -> GpResult<f32> {
                    convert(self, ctx, self.to_f32())
                }
                fn to_f64_gp(&self, ctx: &'static str) -> GpResult<f64> {
                    convert(self, ctx, self.to_f64())
                }
            }
        )*
    };
}

impl_to_primitive_gp!(i8, u8, i16, u16, i32, u32, i64, u64, isize, usize, f32, f64);
