//! Transform components and spatial primitives.

use nara_core::{Mat3, Vec2};
use nara_ecs::Component;

#[derive(Clone, Copy, Debug, PartialEq, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transform2d {
    pub translation: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
}

impl Transform2d {
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        rotation: 0.0,
        scale: Vec2::ONE,
    };

    pub fn from_translation(translation: Vec2) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub fn matrix(&self) -> Mat3 {
        Mat3::from_scale_angle_translation(self.scale, self.rotation, self.translation)
    }
}

impl Default for Transform2d {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct GlobalTransform2d(pub Mat3);

impl Default for GlobalTransform2d {
    fn default() -> Self {
        Self(Mat3::IDENTITY)
    }
}

pub mod prelude {
    pub use crate::{GlobalTransform2d, Transform2d};
}
