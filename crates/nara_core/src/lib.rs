//! Shared engine primitives that are independent from ECS and rendering.

pub mod diagnostic;
pub mod identity;
pub mod limits;
pub mod source;
pub mod vfs;

pub use diagnostic::{Diagnostic, DiagnosticSeverity, SourceLoc};
pub use glam::{Affine2, Affine3A, IVec2, Mat3, Mat4, Quat, UVec2, Vec2, Vec3, Vec4};
pub use identity::{AssetId, DocumentId, EntityUuid, UuidIdError};
pub use limits::{ByteLimit, DepthLimit, ItemLimit, TimeLimit};
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
