use std::{collections::HashMap, sync::mpsc};

use bevy_ecs::{
    entity::EntityHashMap,
    prelude::{Commands, Entity, Has, Local, Messages, Or, Query, Res, ResMut, Resource, With},
    query::QueryData,
};
use nara_app::FixedTime;
use nara_core::Vec2;
use nara_scene::spatial::{ChildOf, GlobalTransform2d, Transform2d};
use rapier2d::prelude::{
    ActiveCollisionTypes, ActiveEvents, ChannelEventCollector, ColliderBuilder, ColliderHandle,
    ColliderSet, CollisionEvent as RapierCollisionEvent, InteractionGroups, PhysicsWorld,
    RigidBodyBuilder, RigidBodyHandle, RigidBodyType, SharedShape,
};

use crate::{
    Collider2d, CollisionEvent, CollisionGroups2d, CompoundBox2d, CompoundCollider2d,
    ExternalImpulse2d, PhysicsError, PhysicsMaterial2d, PhysicsPose2d, RigidBody2d, Sensor2d,
    Teleport2d, Velocity2d, dynamics::DynamicsInput, native_vector, scene_vector,
    validate_shape_pose,
};

/// The latest committed fixed-step physics state. Queries reuse the native acceleration structure.
///
/// Changes in ECS become queryable when the next [`crate::Physics2dSet::Step`] completes. Queries
/// made before that boundary use the previous step, or an empty world before the first step.
#[derive(Resource, Default)]
pub struct PhysicsWorld2d {
    pub(super) native: PhysicsWorld,
    bodies: EntityHashMap<BodyEntry>,
    collider_entities: HashMap<ColliderHandle, Entity>,
    removed_colliders: Vec<ColliderHandle>,
    errors: EntityHashMap<PhysicsError>,
}

/// Primitive shape data for drawing world-space debug outlines from the last completed step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColliderGeometry2d {
    pub entity: Entity,
    pub shape: Collider2d,
    pub pose: PhysicsPose2d,
    pub sensor: bool,
}

/// A compound collider's world pose and local boxes from the last completed step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompoundColliderGeometry2d<'a> {
    pub entity: Entity,
    pub pose: PhysicsPose2d,
    pub sensor: bool,
    boxes: &'a [CompoundBox2d],
}

impl<'a> CompoundColliderGeometry2d<'a> {
    /// Borrows the local boxes. Apply `pose` to draw them in world space.
    #[must_use]
    pub fn boxes(&self) -> &'a [CompoundBox2d] {
        self.boxes
    }
}

impl PhysicsWorld2d {
    /// The default is `(0, -9.81)` scene units per second squared.
    #[must_use]
    pub fn gravity(&self) -> Vec2 {
        scene_vector(self.native.gravity)
    }

    /// Sets finite gravity for the next tick and wakes sleeping bodies when it changes.
    pub fn set_gravity(&mut self, gravity: Vec2) -> Result<(), PhysicsError> {
        if !gravity.is_finite() {
            return Err(PhysicsError::InvalidGravity);
        }
        let gravity = native_vector(gravity);
        if self.native.gravity != gravity {
            self.native.gravity = gravity;
            self.native.wake_up_all(true);
        }
        Ok(())
    }

    /// Reports why the last step excluded this entity. Repairing ECS inputs admits it on a later tick.
    #[must_use]
    pub fn body_error(&self, entity: Entity) -> Option<PhysicsError> {
        self.errors.get(&entity).copied()
    }

    /// Iterates committed primitive shapes, including sensors. See [`Self::compound_colliders`]
    /// for compound shapes.
    pub fn colliders(&self) -> impl Iterator<Item = ColliderGeometry2d> + '_ {
        self.bodies.iter().filter_map(|(&entity, entry)| {
            let entry = entry.collider.as_ref()?;
            let ColliderSpec::Primitive(shape) = &entry.spec else {
                return None;
            };
            let collider = self.native.colliders.get(entry.handle)?;
            Some(ColliderGeometry2d {
                entity,
                shape: *shape,
                pose: PhysicsPose2d::from_native(collider.position()),
                sensor: collider.is_sensor(),
            })
        })
    }

    /// Iterates committed compound geometry, including sensors.
    pub fn compound_colliders(&self) -> impl Iterator<Item = CompoundColliderGeometry2d<'_>> + '_ {
        self.bodies.iter().filter_map(|(&entity, entry)| {
            let entry = entry.collider.as_ref()?;
            let ColliderSpec::Compound(spec) = &entry.spec else {
                return None;
            };
            let collider = self.native.colliders.get(entry.handle)?;
            Some(CompoundColliderGeometry2d {
                entity,
                pose: PhysicsPose2d::from_native(collider.position()),
                sensor: collider.is_sensor(),
                boxes: spec.boxes(),
            })
        })
    }

    pub(super) fn collider_entity(&self, handle: ColliderHandle) -> Option<Entity> {
        self.collider_entities.get(&handle).copied()
    }

    pub(super) fn body_handle(&self, entity: Entity) -> Option<RigidBodyHandle> {
        self.bodies.get(&entity).map(|entry| entry.handle)
    }

    fn synchronize(&mut self, input: BodyInputItem<'_, '_>) -> Result<bool, PhysicsError> {
        let entity = input.entity;
        let body_type = match input.kind.copied().unwrap_or_default() {
            RigidBody2d::Static => RigidBodyType::Fixed,
            RigidBody2d::Kinematic => RigidBodyType::KinematicPositionBased,
            RigidBody2d::Dynamic if input.parented => return Err(PhysicsError::ParentedDynamic),
            RigidBody2d::Dynamic => RigidBodyType::Dynamic,
        };

        input.dynamics.validate()?;
        let material = input.material.copied().unwrap_or_default();
        material.validate()?;
        let settings = ColliderSettings {
            sensor: input.sensor,
            groups: input.groups.copied().unwrap_or_default().native(),
            material,
        };

        let prior = self.bodies.get(&entity);
        let prior_body = prior.map(|entry| &self.native.bodies[entry.handle]);
        let pose = match prior_body {
            Some(body) if body.is_dynamic() && body_type.is_dynamic() && !input.teleport => {
                *body.position()
            }
            _ => input.authored_pose()?.native()?,
        };

        let prior_collider = prior.and_then(|entry| entry.collider.as_ref());
        let prior_spec = prior_collider.map(|entry| &entry.spec);
        let has_collider = input.collider.is_some() || input.compound.is_some();
        let replacement = match (input.collider, input.compound) {
            (Some(_), Some(_)) => return Err(PhysicsError::ConflictingColliders),
            (Some(spec), None) if !matches!(prior_spec, Some(ColliderSpec::Primitive(previous)) if spec == previous) => {
                Some((ColliderSpec::Primitive(*spec), spec.native_shape()?))
            }
            (None, Some(spec)) if !matches!(prior_spec, Some(ColliderSpec::Compound(previous)) if spec == previous) => {
                Some((ColliderSpec::Compound(spec.clone()), spec.shape.clone()))
            }
            _ => None,
        };
        if let Some((_, shape)) = &replacement {
            validate_shape_pose(shape, &pose)?;
        } else if has_collider
            && let Some(collider) = prior_collider
            && prior_body.is_some_and(|body| body.position() != &pose)
        {
            validate_shape_pose(self.native.colliders[collider.handle].shared_shape(), &pose)?;
        }

        let entry = self.bodies.entry(entity).or_insert_with(|| BodyEntry {
            handle: self
                .native
                .insert_body(RigidBodyBuilder::new(body_type).pose(pose)),
            collider: None,
        });
        let body = &self.native.bodies[entry.handle];
        let type_changed = body.body_type() != body_type;
        if type_changed || input.teleport {
            let body = &mut self.native.bodies[entry.handle];
            body.set_body_type(body_type, true);
            body.set_position(pose, true);
        } else if body.is_kinematic() && body.next_position() != &pose {
            self.native.bodies[entry.handle].set_next_kinematic_position(pose);
        } else if body.is_fixed() && body.position() != &pose {
            self.native.bodies[entry.handle].set_position(pose, false);
        }

        let mass_changed = type_changed || replacement.is_some();
        if let Some((spec, shape)) = replacement {
            let handle = if let Some(collider) = &entry.collider {
                self.native.colliders[collider.handle].set_shape(shape);
                collider.handle
            } else {
                let handle = settings.insert(&mut self.native, entry.handle, shape);
                self.collider_entities.insert(handle, entity);
                handle
            };
            entry.collider = Some(BodyCollider { spec, handle });
        } else if !has_collider && let Some(collider) = entry.collider.take() {
            self.native.remove_collider(collider.handle);
            self.removed_colliders.push(collider.handle);
        }
        if let Some(collider) = &entry.collider {
            settings.update(&mut self.native.colliders, collider.handle);
        }
        Ok(input
            .dynamics
            .synchronize(&mut self.native, entry.handle, mass_changed))
    }

    fn collision_event(&self, event: RapierCollisionEvent) -> Option<CollisionEvent> {
        let first = self.collider_entity(event.collider1())?;
        let second = self.collider_entity(event.collider2())?;
        let (first, second) = (first.min(second), first.max(second));
        Some(if event.started() {
            CollisionEvent::Started(first, second)
        } else {
            CollisionEvent::Stopped(first, second)
        })
    }

    fn forget_removed_colliders(&mut self) {
        for handle in self.removed_colliders.drain(..) {
            self.collider_entities.remove(&handle);
        }
    }
}

struct BodyEntry {
    handle: RigidBodyHandle,
    collider: Option<BodyCollider>,
}

impl BodyEntry {
    fn retire(&self, native: &mut PhysicsWorld, removed_colliders: &mut Vec<ColliderHandle>) {
        native.remove_body(self.handle);
        if let Some(collider) = &self.collider {
            removed_colliders.push(collider.handle);
        }
    }
}

struct BodyCollider {
    spec: ColliderSpec,
    handle: ColliderHandle,
}

enum ColliderSpec {
    Primitive(Collider2d),
    Compound(CompoundCollider2d),
}

#[derive(QueryData)]
pub(super) struct BodyInput {
    entity: Entity,
    local: &'static Transform2d,
    global: &'static GlobalTransform2d,
    kind: Option<&'static RigidBody2d>,
    collider: Option<&'static Collider2d>,
    compound: Option<&'static CompoundCollider2d>,
    groups: Option<&'static CollisionGroups2d>,
    material: Option<&'static PhysicsMaterial2d>,
    dynamics: DynamicsInput,
    parented: Has<ChildOf>,
    sensor: Has<Sensor2d>,
    teleport: Has<Teleport2d>,
}

impl BodyInputItem<'_, '_> {
    fn authored_pose(&self) -> Result<PhysicsPose2d, PhysicsError> {
        if !self.parented {
            return Ok(PhysicsPose2d::new(
                self.local.translation,
                self.local.rotation,
            ));
        }
        if !self.local.translation.is_finite() || !self.local.rotation.is_finite() {
            return Err(PhysicsError::InvalidPose);
        }
        let axes = self.global.affine().matrix2;
        let right = axes.x_axis * self.local.scale.x.signum();
        let up = axes.y_axis * self.local.scale.y.signum();
        let rotation = if right.is_finite() && right != Vec2::ZERO {
            right.y.atan2(right.x)
        } else if up.is_finite() && up != Vec2::ZERO {
            (-up.x).atan2(up.y)
        } else {
            return Err(PhysicsError::InvalidPose);
        };
        Ok(PhysicsPose2d::new(self.global.translation(), rotation))
    }
}

struct ColliderSettings {
    sensor: bool,
    groups: InteractionGroups,
    material: PhysicsMaterial2d,
}

impl ColliderSettings {
    fn insert(
        &self,
        native: &mut PhysicsWorld,
        parent: RigidBodyHandle,
        shape: SharedShape,
    ) -> ColliderHandle {
        let pair_types = ActiveCollisionTypes::default()
            | ActiveCollisionTypes::KINEMATIC_FIXED
            | ActiveCollisionTypes::KINEMATIC_KINEMATIC;
        let builder = ColliderBuilder::new(shape)
            .sensor(self.sensor)
            .collision_groups(self.groups)
            .friction(self.material.friction)
            .restitution(self.material.restitution)
            .active_collision_types(pair_types)
            .active_events(ActiveEvents::COLLISION_EVENTS);
        native.insert_collider(builder, Some(parent))
    }

    fn update(&self, colliders: &mut ColliderSet, handle: ColliderHandle) {
        let collider = &colliders[handle];
        if collider.is_sensor() != self.sensor
            || collider.collision_groups() != self.groups
            || collider.friction() != self.material.friction
            || collider.restitution() != self.material.restitution
        {
            let collider = &mut colliders[handle];
            collider.set_sensor(self.sensor);
            collider.set_collision_groups(self.groups);
            collider.set_friction(self.material.friction);
            collider.set_restitution(self.material.restitution);
        }
    }
}

type BodyFilter = Or<(
    With<RigidBody2d>,
    With<Collider2d>,
    With<CompoundCollider2d>,
)>;

pub(super) struct RapierEvents {
    collector: ChannelEventCollector,
    collisions: mpsc::Receiver<RapierCollisionEvent>,
}

impl Default for RapierEvents {
    fn default() -> Self {
        let (collision_tx, collisions) = mpsc::channel();
        let (force_tx, _) = mpsc::channel();
        Self {
            collector: ChannelEventCollector::new(collision_tx, force_tx),
            collisions,
        }
    }
}

pub(super) fn step_physics(
    time: Res<FixedTime>,
    mut physics: ResMut<PhysicsWorld2d>,
    bodies: Query<BodyInput, BodyFilter>,
    mut collisions: ResMut<Messages<CollisionEvent>>,
    mut commands: Commands,
    events: Local<RapierEvents>,
) {
    let physics = &mut *physics;
    physics.errors.clear();
    physics.bodies.retain(|entity, entry| {
        if bodies.contains(*entity) {
            true
        } else {
            entry.retire(&mut physics.native, &mut physics.removed_colliders);
            false
        }
    });
    for input in &bodies {
        let (entity, teleport) = (input.entity, input.teleport);
        match physics.synchronize(input) {
            Ok(consumed_impulse) => {
                if teleport {
                    commands.entity(entity).remove::<Teleport2d>();
                }
                if consumed_impulse {
                    commands.entity(entity).insert(ExternalImpulse2d::default());
                }
            }
            Err(error) => {
                if let Some(entry) = physics.bodies.remove(&entity) {
                    entry.retire(&mut physics.native, &mut physics.removed_colliders);
                }
                physics.errors.insert(entity, error);
            }
        }
    }

    if time.steps_this_frame() == 1 {
        collisions.update();
    }
    physics.native.integration_parameters.dt = time.delta().as_secs_f32();
    physics.native.step_with_events(&(), &events.collector);
    collisions.write_batch(
        events
            .collisions
            .try_iter()
            .filter_map(|event| physics.collision_event(event)),
    );
    physics.forget_removed_colliders();
}

pub(super) fn write_back_dynamic_state(
    physics: Res<PhysicsWorld2d>,
    mut state: Query<(&mut Transform2d, Option<&mut Velocity2d>), With<RigidBody2d>>,
) {
    for (&entity, entry) in &physics.bodies {
        let body = &physics.native.bodies[entry.handle];
        if body.is_dynamic()
            && let Ok((mut transform, velocity)) = state.get_mut(entity)
        {
            let pose = PhysicsPose2d::from_native(body.position());
            if transform.translation != pose.translation || transform.rotation != pose.rotation {
                transform.translation = pose.translation;
                transform.rotation = pose.rotation;
            }
            if let Some(mut velocity) = velocity {
                let result = Velocity2d::from_native(body);
                if *velocity != result {
                    *velocity = result;
                }
            }
        }
    }
}
