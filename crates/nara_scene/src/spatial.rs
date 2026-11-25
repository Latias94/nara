//! Local 2D spatial components and their derived world-space values.

mod codec;
mod plugin;
mod propagation;
#[cfg(test)]
mod tests;

pub use bevy_ecs::hierarchy::{ChildOf, Children};
pub use codec::{
    TRANSFORM_COMPONENTS_PLUGIN_DECLARATION, TRANSFORM_COMPONENTS_PLUGIN_ID,
    TRANSFORM_SCHEMA_OWNER_ID, TRANSFORM_SCHEMA_PROVIDER, TRANSFORM_SCHEMA_PROVIDER_ID,
    TransformComponentsPlugin, register_transform_components,
};
pub use plugin::{SPATIAL_PLUGIN_ID, SpatialPlugin, SpatialSet};
pub use propagation::{SpatialPropagation, propagate_spatial, register_spatial};

use bevy_ecs::component::Component;
use nara_core::{Affine2, Mat3, Vec2};

/// Editable transform relative to the parent, or to world space on a root.
///
/// The required components are derived values. An omitted local [`Visibility`] inherits without
/// creating a persistent visibility component.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(summary_tick)]
#[require(GlobalTransform2d, InheritedVisibility)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transform2d {
    pub translation: Vec2,
    /// Counterclockwise rotation in radians.
    pub rotation: f32,
    pub scale: Vec2,
    /// Draw depth relative to the parent. This has no effect on planar positions.
    #[cfg_attr(feature = "serde", serde(default))]
    pub z: f32,
}

impl Transform2d {
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        rotation: 0.0,
        scale: Vec2::ONE,
        z: 0.0,
    };

    #[must_use]
    pub const fn from_translation(translation: Vec2) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    #[must_use]
    pub const fn from_xy(x: f32, y: f32) -> Self {
        Self::from_translation(Vec2::new(x, y))
    }

    #[must_use]
    pub fn affine(&self) -> Affine2 {
        Affine2::from_scale_angle_translation(self.scale, self.rotation, self.translation)
    }

    #[must_use]
    pub fn matrix(&self) -> Mat3 {
        self.affine().into()
    }
}

impl Default for Transform2d {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// World-space affine transform and accumulated draw depth, maintained by [`propagate_spatial`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GlobalTransform2d(Affine2, f32);

impl GlobalTransform2d {
    pub const IDENTITY: Self = Self(Affine2::IDENTITY, 0.0);

    #[must_use]
    pub const fn affine(&self) -> Affine2 {
        self.0
    }

    #[must_use]
    pub fn matrix(&self) -> Mat3 {
        self.0.into()
    }

    #[must_use]
    pub const fn translation(&self) -> Vec2 {
        self.0.translation
    }

    #[must_use]
    pub const fn z(&self) -> f32 {
        self.1
    }
}

impl Default for GlobalTransform2d {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// An entity's local visibility policy.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[component(summary_tick)]
#[require(InheritedVisibility)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Visibility {
    /// Use the parent's effective visibility, or visible for a root.
    #[default]
    Inherited,
    /// Make the entity visible regardless of its parent's effective visibility.
    Visible,
    /// Hide the entity regardless of its parent's effective visibility.
    Hidden,
}

/// Effective visibility through the hierarchy, maintained by [`propagate_spatial`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InheritedVisibility(bool);

impl InheritedVisibility {
    pub const VISIBLE: Self = Self(true);
    pub const HIDDEN: Self = Self(false);

    #[must_use]
    pub const fn get(self) -> bool {
        self.0
    }
}

impl Default for InheritedVisibility {
    fn default() -> Self {
        Self::VISIBLE
    }
}
