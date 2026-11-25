//! Immutable Rapier2D collision queries and stateless top-down character movement.
//!
//! Callers own scene replacement, character poses, time, and application or ECS integration.
//! Native Rapier types and queries remain available through [`rapier`].
//!
//! Build a scene once for each set of static geometry. Queries and character movement borrow its
//! native acceleration structure. Supply an already integrated world displacement and its fixed
//! duration separately. Placement checks exclude sensors and do not recover initial penetration.
//!
//! ```
//! use std::time::Duration;
//!
//! use nara_core::{Mat3, Vec2};
//! use nara_physics2d::stateless::{
//!     StaticCollisionScene, TopDownCharacter, affine_box_collider,
//!     rapier::{math::{Pose, Vector}, prelude::SharedShape},
//! };
//!
//! # fn main() -> Result<(), nara_physics2d::stateless::CollisionError> {
//! let wall = affine_box_collider(
//!     Vec2::ZERO,
//!     Vec2::new(0.05, 4.0),
//!     Mat3::from_translation(Vec2::new(2.0, 0.0)),
//! )?;
//! let scene = StaticCollisionScene::new([wall])?;
//! let character = TopDownCharacter::new(SharedShape::ball(0.25), 0.01)?;
//! let start = Pose::IDENTITY;
//! assert!(!character.overlaps(&scene, start)?);
//!
//! let delta = Duration::from_millis(16);
//! let desired = Vec2::new(4.0, 1.0) * delta.as_secs_f32();
//! let movement = character.move_shape(&scene, start, desired, delta)?;
//! let offset = Vector::new(movement.translation.x, movement.translation.y);
//! assert!(!character.overlaps(&scene, start.append_translation(offset))?);
//! # Ok(())
//! # }
//! ```

mod character;
mod geometry;
mod scene;

pub use crate::CharacterMovement;
pub use character::TopDownCharacter;
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
