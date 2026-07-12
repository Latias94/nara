//! Shared engine primitives that are independent from ECS and rendering.

pub mod format;
pub mod limits;
#[cfg(feature = "serde")]
pub mod serde_shape;

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

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }
}
