use bevy_ecs::{component::Component, query::QueryData};
use nara_core::Vec2;
use rapier2d::prelude::{LockedAxes, PhysicsWorld, RigidBody, RigidBodyHandle};

use crate::{PhysicsError, native_vector, nonnegative_finite, scene_vector};

/// Opt-in velocity input and readback for a dynamic body.
///
/// Physics reads the component before the step and writes the resulting velocity back after it.
/// A dynamic body without the component keeps its velocity internally.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity2d {
    /// World-space velocity in scene units per second.
    pub linear: Vec2,
    /// Counterclockwise angular velocity in radians per second.
    pub angular: f32,
}

impl Velocity2d {
    pub(super) fn from_native(body: &RigidBody) -> Self {
        Self {
            linear: scene_vector(body.linvel()),
            angular: body.angvel(),
        }
    }
}

/// Force at a dynamic body's center of mass and torque, applied every fixed step.
///
/// Remove this component or set both fields to zero to stop applying the force and torque.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ExternalForce2d {
    /// World-space force in mass · scene units / seconds².
    pub force: Vec2,
    /// Torque in mass · scene units² / seconds², positive counterclockwise.
    pub torque: f32,
}

/// A one-time impulse at a dynamic body's center of mass and an angular impulse.
///
/// Physics zeroes the fields after the fixed step that applies them. The values remain queued
/// while the entity is invalid, static, kinematic, or lacks a mass-supplying collider.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ExternalImpulse2d {
    /// World-space impulse in mass · scene units / seconds.
    pub impulse: Vec2,
    /// Angular impulse in mass · scene units² / seconds, positive counterclockwise.
    pub torque_impulse: f32,
}

/// Dimensionless, finite, nonnegative coefficients for an entity's collider.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PhysicsMaterial2d {
    /// Resistance to sliding.
    pub friction: f32,
    /// Resilience on impact. A value of zero is inelastic.
    pub restitution: f32,
}

impl Default for PhysicsMaterial2d {
    fn default() -> Self {
        Self {
            friction: 0.5,
            restitution: 0.0,
        }
    }
}

impl PhysicsMaterial2d {
    pub(super) fn validate(self) -> Result<(), PhysicsError> {
        if nonnegative_finite(self.friction) && nonnegative_finite(self.restitution) {
            Ok(())
        } else {
            Err(PhysicsError::InvalidMaterial)
        }
    }
}

/// Dynamic body velocity damping coefficients, in inverse seconds. Both default to zero.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Damping2d {
    pub linear: f32,
    pub angular: f32,
}

/// Finite multiplier on gravity for a dynamic body. Defaults to `1.0`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GravityScale2d(pub f32);

impl Default for GravityScale2d {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Locks world-space translation axes and/or rotation for a dynamic body.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockedAxes2d {
    pub lock_translation_x: bool,
    pub lock_translation_y: bool,
    pub lock_rotation: bool,
}

impl LockedAxes2d {
    fn native(self) -> LockedAxes {
        let mut axes = LockedAxes::empty();
        axes.set(LockedAxes::TRANSLATION_LOCKED_X, self.lock_translation_x);
        axes.set(LockedAxes::TRANSLATION_LOCKED_Y, self.lock_translation_y);
        axes.set(LockedAxes::ROTATION_LOCKED_Z, self.lock_rotation);
        axes
    }

    fn constrain_velocity(self, velocity: &mut Velocity2d) {
        if self.lock_translation_x {
            velocity.linear.x = 0.0;
        }
        if self.lock_translation_y {
            velocity.linear.y = 0.0;
        }
        if self.lock_rotation {
            velocity.angular = 0.0;
        }
    }
}

#[derive(QueryData)]
pub(super) struct DynamicsInput {
    velocity: Option<&'static Velocity2d>,
    force: Option<&'static ExternalForce2d>,
    impulse: Option<&'static ExternalImpulse2d>,
    damping: Option<&'static Damping2d>,
    gravity_scale: Option<&'static GravityScale2d>,
    locked_axes: Option<&'static LockedAxes2d>,
}

impl DynamicsInputItem<'_, '_> {
    pub(super) fn validate(&self) -> Result<(), PhysicsError> {
        if let Some(velocity) = self.velocity
            && (!velocity.linear.is_finite() || !velocity.angular.is_finite())
        {
            return Err(PhysicsError::InvalidVelocity);
        }
        if let Some(force) = self.force
            && (!force.force.is_finite() || !force.torque.is_finite())
        {
            return Err(PhysicsError::InvalidForce);
        }
        if let Some(impulse) = self.impulse
            && (!impulse.impulse.is_finite() || !impulse.torque_impulse.is_finite())
        {
            return Err(PhysicsError::InvalidImpulse);
        }
        if let Some(damping) = self.damping
            && (!nonnegative_finite(damping.linear) || !nonnegative_finite(damping.angular))
        {
            return Err(PhysicsError::InvalidDamping);
        }
        if let Some(scale) = self.gravity_scale
            && !scale.0.is_finite()
        {
            return Err(PhysicsError::InvalidGravityScale);
        }
        Ok(())
    }

    /// Returns whether the step has accepted a queued impulse for consumption.
    pub(super) fn synchronize(
        &self,
        native: &mut PhysicsWorld,
        handle: RigidBodyHandle,
        mass_changed: bool,
    ) -> bool {
        let body = &native.bodies[handle];
        if !body.is_dynamic() {
            return false;
        }

        let locks = self.locked_axes.copied().unwrap_or_default();
        let damping = self.damping.copied().unwrap_or_default();
        let gravity_scale = self.gravity_scale.copied().unwrap_or_default().0;
        let force = self.force.copied().unwrap_or_default();
        let impulse = self.impulse.copied().filter(|impulse| {
            *impulse != ExternalImpulse2d::default() && !body.colliders().is_empty()
        });
        let current_velocity = Velocity2d::from_native(body);
        let mut velocity = self.velocity.copied().unwrap_or(current_velocity);
        locks.constrain_velocity(&mut velocity);
        let axes = locks.native();
        let linear_force = native_vector(force.force);
        let force_changed = body.user_force() != linear_force;
        let torque_changed = body.user_torque() != force.torque;
        let gravity_changed = body.gravity_scale() != gravity_scale;
        if !force_changed
            && !torque_changed
            && !gravity_changed
            && impulse.is_none()
            && body.locked_axes() == axes
            && body.linear_damping() == damping.linear
            && body.angular_damping() == damping.angular
            && current_velocity == velocity
        {
            return false;
        }

        let body = &mut native.bodies[handle];
        body.set_locked_axes(axes, true);
        body.set_linvel(native_vector(velocity.linear), true);
        body.set_angvel(velocity.angular, true);
        body.set_linear_damping(damping.linear);
        body.set_angular_damping(damping.angular);
        if gravity_changed {
            body.set_gravity_scale(gravity_scale, true);
        }
        if force_changed {
            body.reset_forces(true);
            body.add_force(linear_force, true);
        }
        if torque_changed {
            body.reset_torques(true);
            body.add_torque(force.torque, true);
        }
        if let Some(impulse) = impulse {
            if mass_changed {
                // The impulse uses mass immediately, before Rapier refreshes it in the step.
                body.recompute_mass_properties_from_colliders(&native.colliders);
            }
            body.apply_impulse(native_vector(impulse.impulse), true);
            body.apply_torque_impulse(impulse.torque_impulse, true);
            return true;
        }
        false
    }
}
