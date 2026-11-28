use std::time::Duration;

use bevy_ecs::entity::Entity;
use nara_core::Vec2;
use rapier2d::{
    control::KinematicCharacterController,
    math::Pose,
    parry::query::ShapeCastOptions,
    prelude::{QueryFilter, QueryPipeline, Ray, SharedShape},
};

use crate::{
    CharacterMovement, Collider2d, CollisionGroups2d, PhysicsError, PhysicsPose2d, PhysicsWorld2d,
    character::{sweep_character, top_down_controller},
    native_vector, positive_finite, scene_vector, validate_shape_pose,
};

/// Selects colliders from the latest fixed-step state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicsQueryFilter2d {
    /// Omit the collider on this entity, for example when casting from a moving character.
    pub exclude: Option<Entity>,
    /// Include sensor colliders alongside solid shapes. The default includes only solid shapes.
    pub include_sensors: bool,
    /// Apply bilateral collision groups. `None` queries every group.
    pub groups: Option<CollisionGroups2d>,
}

/// The nearest point where a ray hits a collider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit2d {
    pub entity: Entity,
    pub distance: f32,
    pub point: Vec2,
    pub normal: Vec2,
}

/// The first collider hit while translating a shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeHit2d {
    pub entity: Entity,
    /// Fraction of the requested displacement traveled, in `[0, 1]`.
    pub fraction: f32,
}

/// A reusable native character shape with top-down sliding and an absolute separation margin.
///
/// Pose and movement history belong to the caller. Check placement with a shape overlap
/// before moving if the character could start in penetration.
#[derive(Clone, Debug)]
pub struct TopDownCharacter {
    shape: SharedShape,
    controller: KinematicCharacterController,
}

impl TopDownCharacter {
    /// Builds the shape and validates its separation margin once.
    pub fn new(shape: Collider2d, margin: f32) -> Result<Self, PhysicsError> {
        if !positive_finite(margin) {
            return Err(PhysicsError::InvalidMargin);
        }
        Ok(Self {
            shape: shape.native_shape()?,
            controller: top_down_controller(margin),
        })
    }
}

impl PhysicsWorld2d {
    /// Casts a solid ray, normalizing `direction` so `max_distance` is in scene units.
    ///
    /// Rays starting inside a collider hit at distance zero.
    pub fn cast_ray(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        filter: PhysicsQueryFilter2d,
    ) -> Result<Option<RayHit2d>, PhysicsError> {
        let direction = direction.try_normalize().ok_or(PhysicsError::InvalidRay)?;
        if !origin.is_finite()
            || !max_distance.is_finite()
            || max_distance < 0.0
            || !(origin + direction * max_distance).is_finite()
        {
            return Err(PhysicsError::InvalidRay);
        }
        let ray = Ray::new(native_vector(origin), native_vector(direction));
        let hit = self
            .queries(filter)
            .cast_ray_and_get_normal(&ray, max_distance, true);
        Ok(hit.and_then(|(handle, hit)| {
            let entity = self.collider_entity(handle)?;
            Some(RayHit2d {
                entity,
                distance: hit.time_of_impact,
                point: origin + direction * hit.time_of_impact,
                normal: scene_vector(hit.normal),
            })
        }))
    }

    /// Sweeps a shape along a world displacement at a constant angle. An initial overlap hits at zero.
    pub fn cast_shape(
        &self,
        shape: Collider2d,
        pose: PhysicsPose2d,
        displacement: Vec2,
        filter: PhysicsQueryFilter2d,
    ) -> Result<Option<ShapeHit2d>, PhysicsError> {
        let shape = shape.native_shape()?;
        let pose = validate_sweep(&shape, pose, displacement)?;
        let hit = self.queries(filter).cast_shape(
            &pose,
            native_vector(displacement),
            shape.as_ref(),
            ShapeCastOptions::with_max_time_of_impact(1.0),
        );
        Ok(hit.and_then(|(handle, hit)| {
            let entity = self.collider_entity(handle)?;
            Some(ShapeHit2d {
                entity,
                fraction: hit.time_of_impact,
            })
        }))
    }

    /// Returns intersecting collider entities in entity order.
    pub fn overlap_shape(
        &self,
        shape: Collider2d,
        pose: PhysicsPose2d,
        filter: PhysicsQueryFilter2d,
    ) -> Result<Vec<Entity>, PhysicsError> {
        let shape = shape.native_shape()?;
        let pose = pose.native()?;
        validate_shape_pose(&shape, &pose)?;
        let mut entities: Vec<_> = self
            .queries(filter)
            .intersect_shape(pose, shape.as_ref())
            .filter_map(|(handle, _)| self.collider_entity(handle))
            .collect();
        entities.sort_unstable();
        Ok(entities)
    }

    /// Returns collider entities intersecting a world-space circle, in entity order.
    pub fn overlap_circle(
        &self,
        center: Vec2,
        radius: f32,
        filter: PhysicsQueryFilter2d,
    ) -> Result<Vec<Entity>, PhysicsError> {
        self.overlap_shape(
            Collider2d::Circle { radius },
            PhysicsPose2d::new(center, 0.0),
            filter,
        )
    }

    /// Sweeps and slides a character against solid colliders without changing ECS or physics state.
    ///
    /// Pass the already integrated displacement for `delta`. Apply the effective translation
    /// once to the character's authored pose. Pass its entity as `exclude` if it has an ECS collider.
    pub fn move_top_down_character(
        &self,
        character: &TopDownCharacter,
        pose: PhysicsPose2d,
        displacement: Vec2,
        delta: Duration,
        exclude: Option<Entity>,
    ) -> Result<CharacterMovement, PhysicsError> {
        self.move_top_down_character_with_filter(
            character,
            pose,
            displacement,
            delta,
            PhysicsQueryFilter2d {
                exclude,
                ..Default::default()
            },
        )
    }

    /// Sweeps and slides a character against colliders matching the filter.
    ///
    /// Pass a world-space displacement already integrated over `delta` and apply the returned
    /// translation once. Use `filter.exclude` for the character's own entity.
    pub fn move_top_down_character_with_filter(
        &self,
        character: &TopDownCharacter,
        pose: PhysicsPose2d,
        displacement: Vec2,
        delta: Duration,
        filter: PhysicsQueryFilter2d,
    ) -> Result<CharacterMovement, PhysicsError> {
        let pose = validate_sweep(&character.shape, pose, displacement)?;
        let queries = self.queries(filter);
        Ok(sweep_character(
            &character.controller,
            &queries,
            character.shape.as_ref(),
            &pose,
            displacement,
            delta.as_secs_f32(),
        ))
    }

    pub(super) fn queries(&self, filter: PhysicsQueryFilter2d) -> QueryPipeline<'_> {
        let mut native = QueryFilter::default();
        if !filter.include_sensors {
            native = native.exclude_sensors();
        }
        if let Some(body) = filter.exclude.and_then(|entity| self.body_handle(entity)) {
            native = native.exclude_rigid_body(body);
        }
        if let Some(groups) = filter.groups {
            native = native.groups(groups.native());
        }
        self.native.query_pipeline_with_filter(native)
    }
}

pub(super) fn validate_sweep(
    shape: &SharedShape,
    origin: PhysicsPose2d,
    displacement: Vec2,
) -> Result<Pose, PhysicsError> {
    let pose = origin.native()?;
    let destination = origin.translation + displacement;
    if !displacement.is_finite() || !destination.is_finite() {
        return Err(PhysicsError::InvalidDisplacement);
    }
    validate_shape_pose(shape, &pose)?;
    validate_shape_pose(
        shape,
        &Pose::new(native_vector(destination), origin.rotation),
    )?;
    Ok(pose)
}
