//! Immutable Rapier2D collision queries and stateless top-down character movement.
//!
//! Callers own scene replacement, character poses, time, and application or ECS integration.
//! Native Rapier types and queries remain available through [`rapier`].

mod character;
mod geometry;
mod scene;

pub use character::{CharacterMovement, TopDownCharacter};
pub use geometry::affine_box_collider;
pub use rapier2d as rapier;
pub use scene::StaticCollisionScene;

use thiserror::Error;

/// An input cannot be represented by the collision scene or character controller.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CollisionError {
    /// Geometry is nonfinite, degenerate, or cannot retain its required native representation.
    #[error("Invalid collision geometry: {0}")]
    InvalidGeometry(&'static str),
    /// A supplied collider is still attached to a native rigid body.
    #[error("Static collision scenes require standalone colliders without a rigid-body parent")]
    AttachedCollider,
    /// The character's separation margin is not finite and strictly positive.
    #[error("Character margin must be finite and strictly positive")]
    InvalidMargin,
    /// The supplied character pose contains nonfinite values.
    #[error("Character pose must contain only finite values")]
    InvalidPose,
    /// The supplied character displacement or duration contains nonfinite values.
    #[error("Character displacement and duration must contain only finite values")]
    InvalidMovement,
}

fn pose_is_finite(pose: &rapier::math::Pose) -> bool {
    pose.translation.is_finite() && pose.rotation.is_finite()
}
