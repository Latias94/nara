use rapier2d::prelude::{
    BroadPhaseBvh, Collider, ColliderSet, CollisionPipeline, IslandManager, NarrowPhase,
    QueryFilter, QueryPipeline, RigidBodySet,
};

use crate::{CollisionError, pose_is_finite};

/// An immutable native query scene containing standalone static colliders.
///
/// Construction refreshes Rapier's BVH once through collision detection without advancing
/// dynamics or application time. Every subsequent query borrows the same native scene.
/// Sensors and disabled colliders retain their native behavior. Character operations exclude
/// sensors; callers of [`Self::queries`] choose their own native filter.
pub struct StaticCollisionScene {
    colliders: ColliderSet,
    bodies: RigidBodySet,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
}

impl StaticCollisionScene {
    /// Takes ownership of standalone colliders and builds their native query acceleration.
    ///
    /// Empty scenes are valid. Rejects attached colliders, nonfinite poses, and invalid native
    /// world AABBs before running the collision pipeline.
    pub fn new(colliders: impl IntoIterator<Item = Collider>) -> Result<Self, CollisionError> {
        let mut scene = Self {
            colliders: ColliderSet::new(),
            bodies: RigidBodySet::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
        };
        for collider in colliders {
            if collider.parent().is_some() {
                return Err(CollisionError::AttachedCollider);
            }
            if !pose_is_finite(collider.position()) {
                return Err(CollisionError::InvalidGeometry(
                    "Static collider pose must contain only finite values",
                ));
            }
            let aabb = collider.compute_aabb();
            if !aabb.mins.is_finite()
                || !aabb.maxs.is_finite()
                || !aabb.extents().is_finite()
                || aabb.extents().min_element() < 0.0
            {
                return Err(CollisionError::InvalidGeometry(
                    "Static collider must have a finite native world AABB with nonnegative extents",
                ));
            }
            scene.colliders.insert(collider);
        }
        CollisionPipeline::new().step(
            0.0,
            &mut IslandManager::new(),
            &mut scene.broad_phase,
            &mut scene.narrow_phase,
            &mut scene.bodies,
            &mut scene.colliders,
            &(),
            &(),
        );
        Ok(scene)
    }

    /// Returns the number of owned colliders, including sensors and disabled colliders.
    pub fn collider_count(&self) -> usize {
        self.colliders.len()
    }

    /// Borrows the original native BVH, collider set, body set, and query dispatcher.
    pub fn queries<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a> {
        self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        )
    }
}
