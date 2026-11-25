use std::time::Duration;

use nara_core::Vec2;
use rapier2d::{
    control::KinematicCharacterController,
    math::Pose,
    prelude::{QueryFilter, Shape, SharedShape},
};

use crate::character::{sweep_character, top_down_controller};

use super::{CharacterMovement, CollisionError, StaticCollisionScene, pose_is_finite};

/// A reusable native shape and top-down Rapier character-controller preset.
///
/// No pose, velocity, grounded status, or movement history is retained. The character is never
/// inserted into a scene. Call [`Self::overlaps`] to admit source, checkpoint, or resume poses:
/// initial penetration is not automatically recovered by [`Self::move_shape`].
#[derive(Clone, Debug)]
pub struct TopDownCharacter {
    shape: SharedShape,
    controller: KinematicCharacterController,
}

impl TopDownCharacter {
    /// Admits the native shape's finite, strictly positive local AABB and separation margin once.
    ///
    /// Uses native sliding, no autostep or ground snap, fixed unit Y up, unrestricted slope
    /// angles, an absolute margin, and Rapier's default normal nudge. Shape validity beyond
    /// the AABB and supported query combinations remain the native shape owner's contract.
    pub fn new(shape: SharedShape, margin: f32) -> Result<Self, CollisionError> {
        if !margin.is_finite() || margin <= 0.0 {
            return Err(CollisionError::InvalidMargin);
        }
        let aabb = shape.compute_local_aabb();
        if !aabb.mins.is_finite()
            || !aabb.maxs.is_finite()
            || !aabb.extents().is_finite()
            || aabb.extents().min_element() <= 0.0
        {
            return Err(CollisionError::InvalidGeometry(
                "Character shape must have a finite native local AABB with strictly positive extents",
            ));
        }
        Ok(Self {
            shape,
            controller: top_down_controller(margin),
        })
    }

    /// Sweeps the native shape and returns Rapier's effective displacement without changing it.
    ///
    /// `desired` is a world displacement, already integrated over `delta` by the caller. It is
    /// passed unchanged to Rapier, with `delta` supplied separately in seconds. Zero duration
    /// is valid and does not rescale `desired`. Sensors are excluded. Geometry and the scene
    /// are not revalidated or rebuilt. Initial overlaps must be admitted separately.
    pub fn move_shape(
        &self,
        scene: &StaticCollisionScene,
        pose: Pose,
        desired: Vec2,
        delta: Duration,
    ) -> Result<CharacterMovement, CollisionError> {
        if !pose_is_finite(&pose) {
            return Err(CollisionError::InvalidPose);
        }
        let seconds = delta.as_secs_f32();
        if !desired.is_finite() || !seconds.is_finite() {
            return Err(CollisionError::InvalidMovement);
        }
        let queries = scene.queries(QueryFilter::default().exclude_sensors());
        Ok(sweep_character(
            &self.controller,
            &queries,
            self.shape.as_ref(),
            &pose,
            desired,
            seconds,
        ))
    }

    /// Tests native intersection at a proposed world pose, excluding sensors.
    ///
    /// This checks actual shape intersection, not the controller's separation margin. It does
    /// not move or recover the character, and does not insert it into the scene.
    pub fn overlaps(
        &self,
        scene: &StaticCollisionScene,
        pose: Pose,
    ) -> Result<bool, CollisionError> {
        if !pose_is_finite(&pose) {
            return Err(CollisionError::InvalidPose);
        }
        let queries = scene.queries(QueryFilter::default().exclude_sensors());
        let overlaps = queries
            .intersect_shape(pose, self.shape.as_ref())
            .next()
            .is_some();
        Ok(overlaps)
    }

    /// Borrows the same native shape used by movement and overlap queries.
    pub fn shape(&self) -> &dyn Shape {
        self.shape.as_ref()
    }
}
