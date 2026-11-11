//! Shared engine primitives that are independent from ECS and rendering.

pub mod document;
pub mod format;
pub mod limits;
#[cfg(feature = "serde")]
pub mod serde_shape;
pub mod source;

pub use document::{
    DocumentContentState, DocumentHistoryStatus, DocumentRevision, DocumentSourceId,
};
pub use format::{
    EngineVersion, EngineVersionParseError, FormatGenerator, FormatGeneratorError, FormatKind,
    FormatKindError, FormatVersion, PersistentFileContract, PersistentFileContractError,
    PersistentFileDecodeError, PersistentFileEnvelope, PersistentFileHeader,
    decode_persistent_file, decode_persistent_file_with_preflight,
};
pub use glam::{Mat3, Vec2, Vec3};
pub use limits::{ByteLimit, DepthLimit, ItemLimit, TimeLimit};
#[cfg(feature = "serde")]
pub use serde_shape::{
    SerdeShapeError, SerdeShapeLimits, SerdeShapePreflightError, preflight_serde_shape,
};
pub use source::{SourceUniverseId, SourceUniverseIdError};

/// Linear-sRGB RGB channels with linear, unassociated alpha.
///
/// RGB uses sRGB/Rec.709 primaries and a linear transfer function. Numeric constructors do not
/// decode encoded sRGB picker or image values. Alpha is coverage/opacity and never receives an
/// RGB transfer function. Owning domains validate finite values and their supported range;
/// this representation does not apply exposure, tone mapping or a display profile.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self::rgba(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::rgba(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    /// Constructs linear RGB with straight alpha without encoding conversion or clamping.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Constructs opaque linear RGB without encoding conversion or clamping.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }
}
