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
    CollisionEvent as RapierCollisionEvent, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle,
    RigidBodyType, SharedShape,
};

use crate::{
    Collider2d, CollisionEvent, PhysicsError, PhysicsPose2d, RigidBody2d, Sensor2d, Teleport2d,
    native_vector, scene_vector, validate_shape_pose,
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

/// Shape data for drawing debug outlines in world space using the last completed step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColliderGeometry2d {
    pub entity: Entity,
    pub shape: Collider2d,
    pub pose: PhysicsPose2d,
    pub sensor: bool,
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

    /// Iterates the shapes currently registered with physics, including sensors.
    pub fn colliders(&self) -> impl Iterator<Item = ColliderGeometry2d> + '_ {
        self.bodies.iter().filter_map(|(&entity, entry)| {
            let (spec, handle) = entry.collider?;
            let collider = self.native.colliders.get(handle)?;
            Some(ColliderGeometry2d {
                entity,
                shape: spec,
                pose: PhysicsPose2d::from_native(collider.position()),
                sensor: collider.is_sensor(),
            })
        })
    }

    pub(super) fn collider_entity(&self, handle: ColliderHandle) -> Option<Entity> {
        self.collider_entities.get(&handle).copied()
    }

    pub(super) fn body_handle(&self, entity: Entity) -> Option<RigidBodyHandle> {
        self.bodies.get(&entity).map(|entry| entry.handle)
    }

    fn synchronize(&mut self, input: BodyInputItem<'_, '_>) -> Result<(), PhysicsError> {
        let entity = input.entity;
        let body_type = match input.kind.copied().unwrap_or_default() {
            RigidBody2d::Static => RigidBodyType::Fixed,
            RigidBody2d::Kinematic => RigidBodyType::KinematicPositionBased,
            RigidBody2d::Dynamic if input.parented => return Err(PhysicsError::ParentedDynamic),
            RigidBody2d::Dynamic => RigidBodyType::Dynamic,
        };

        let prior = self.bodies.get(&entity).copied();
        let prior_body = prior.map(|entry| &self.native.bodies[entry.handle]);
        let pose = match prior_body {
            Some(body) if body.is_dynamic() && body_type.is_dynamic() && !input.teleport => {
                *body.position()
            }
            _ => input.authored_pose()?.native()?,
        };

        let prior_collider = prior.and_then(|entry| entry.collider);
        let replacement = match (input.collider, prior_collider) {
            (Some(spec), Some((previous, handle))) if *spec == previous => {
                if prior_body.is_some_and(|body| body.position() != &pose) {
                    validate_shape_pose(self.native.colliders[handle].shared_shape(), &pose)?;
                }
                None
            }
            (Some(spec), _) => {
                let shape = spec.native_shape()?;
                validate_shape_pose(&shape, &pose)?;
                Some((*spec, shape))
            }
            (None, _) => None,
        };

        let entry = self.bodies.entry(entity).or_insert_with(|| BodyEntry {
            handle: self
                .native
                .insert_body(RigidBodyBuilder::new(body_type).pose(pose)),
            collider: None,
        });
        let body = &self.native.bodies[entry.handle];
        if body.body_type() != body_type || input.teleport {
            let body = &mut self.native.bodies[entry.handle];
            body.set_body_type(body_type, true);
            body.set_position(pose, true);
        } else if body.is_kinematic() && body.next_position() != &pose {
            self.native.bodies[entry.handle].set_next_kinematic_position(pose);
        } else if body.is_fixed() && body.position() != &pose {
            self.native.bodies[entry.handle].set_position(pose, false);
        }

        match (entry.collider, replacement) {
            (Some((_, handle)), Some((spec, shape))) => {
                let collider = &mut self.native.colliders[handle];
                collider.set_shape(shape);
                collider.set_sensor(input.sensor);
                entry.collider = Some((spec, handle));
            }
            (None, Some((spec, shape))) => {
                let handle = insert_collider(&mut self.native, entry.handle, shape, input.sensor);
                self.collider_entities.insert(handle, entity);
                entry.collider = Some((spec, handle));
            }
            (Some((_, handle)), None) if input.collider.is_none() => {
                self.native.remove_collider(handle);
                self.removed_colliders.push(handle);
                entry.collider = None;
            }
            (Some((_, handle)), None)
                if self.native.colliders[handle].is_sensor() != input.sensor =>
            {
                self.native.colliders[handle].set_sensor(input.sensor);
            }
            _ => {}
        }
        Ok(())
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

#[derive(Clone, Copy)]
struct BodyEntry {
    handle: RigidBodyHandle,
    collider: Option<(Collider2d, ColliderHandle)>,
}

impl BodyEntry {
    fn retire(self, native: &mut PhysicsWorld, removed_colliders: &mut Vec<ColliderHandle>) {
        native.remove_body(self.handle);
        if let Some((_, handle)) = self.collider {
            removed_colliders.push(handle);
        }
    }
}

#[derive(QueryData)]
pub(super) struct BodyInput {
    entity: Entity,
    local: &'static Transform2d,
    global: &'static GlobalTransform2d,
    kind: Option<&'static RigidBody2d>,
    collider: Option<&'static Collider2d>,
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

fn insert_collider(
    native: &mut PhysicsWorld,
    parent: RigidBodyHandle,
    shape: SharedShape,
    sensor: bool,
) -> ColliderHandle {
    let pair_types = ActiveCollisionTypes::default()
        | ActiveCollisionTypes::KINEMATIC_FIXED
        | ActiveCollisionTypes::KINEMATIC_KINEMATIC;
    let builder = ColliderBuilder::new(shape)
        .sensor(sensor)
        .active_collision_types(pair_types)
        .active_events(ActiveEvents::COLLISION_EVENTS);
    native.insert_collider(builder, Some(parent))
}

type BodyFilter = Or<(With<RigidBody2d>, With<Collider2d>)>;

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
        if let Err(error) = physics.synchronize(input) {
            if let Some(entry) = physics.bodies.remove(&entity) {
                entry.retire(&mut physics.native, &mut physics.removed_colliders);
            }
            physics.errors.insert(entity, error);
        } else if teleport {
            commands.entity(entity).remove::<Teleport2d>();
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

pub(super) fn write_back_dynamic_poses(
    physics: Res<PhysicsWorld2d>,
    mut transforms: Query<&mut Transform2d, With<RigidBody2d>>,
) {
    for (&entity, entry) in &physics.bodies {
        let body = &physics.native.bodies[entry.handle];
        if body.is_dynamic()
            && let Ok(mut transform) = transforms.get_mut(entity)
        {
            let pose = PhysicsPose2d::from_native(body.position());
            if transform.translation != pose.translation || transform.rotation != pose.rotation {
                transform.translation = pose.translation;
                transform.rotation = pose.rotation;
            }
        }
    }
}
