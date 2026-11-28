//! Fixed-step 2D physics for Nara's ECS. Distances and velocities use scene units and seconds.
//!
//! Install [`Physics2dPlugin`] and spawn [`RigidBody2d`] and either [`Collider2d`] or
//! [`CompoundCollider2d`] components. Systems before [`Physics2dSet::Step`] can author kinematic
//! targets and dynamic inputs. Systems after it can read collision messages, queries, scene
//! transforms, and dynamic velocities from the latest step.
//!
//! ```
//! use std::time::Duration;
//!
//! use nara_app::App;
//! use nara_core::Vec2;
//! use nara_physics2d::{Collider2d, Physics2dPlugin, RigidBody2d};
//! use nara_scene::spatial::Transform2d;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut app = App::new();
//! app.add_plugins(Physics2dPlugin)?;
//! app.world_mut()?.spawn((
//!     Transform2d::from_xy(0.0, -0.5),
//!     Collider2d::Box { half_extents: Vec2::new(8.0, 0.5) },
//! ));
//! let ball = app.world_mut()?.spawn((
//!     Transform2d::from_xy(0.0, 3.0),
//!     RigidBody2d::Dynamic,
//!     Collider2d::Circle { radius: 0.5 },
//! )).id();
//! app.step_fixed(Duration::from_millis(16))?;
//! assert!(app.world().get::<Transform2d>(ball).unwrap().translation.y < 3.0);
//! # Ok(())
//! # }
//! ```

mod character;
mod compound;
mod dynamics;
mod platformer;
mod query;
mod simulation;
#[cfg(feature = "scene-runtime")]
mod tilemap;

pub mod stateless;

use bevy_ecs::{
    entity::Entity,
    message::Messages,
    prelude::{Component, IntoScheduleConfigs, Message, SystemSet},
};
use nara_app::{
    App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
    PluginId,
};
use nara_core::Vec2;
use nara_reflect::Schema;
use nara_scene::spatial::{SpatialPropagation, Transform2d, propagate_spatial, register_spatial};
use rapier2d::{
    math::{Pose, Vector},
    prelude::{Group, InteractionGroups, InteractionTestMode, SharedShape},
};
use thiserror::Error;

pub use character::CharacterMovement;
pub use compound::{CompoundBox2d, CompoundCollider2d, GridRect, merge_solid_grid_rects};
pub use dynamics::{
    Damping2d, ExternalForce2d, ExternalImpulse2d, GravityScale2d, LockedAxes2d, PhysicsMaterial2d,
    Velocity2d,
};
pub use platformer::{
    PlatformerAutostep, PlatformerCharacter, PlatformerConfig, PlatformerJumpTimers,
    PlatformerMovement,
};
pub use query::{PhysicsQueryFilter2d, RayHit2d, ShapeHit2d, TopDownCharacter};
pub use simulation::{ColliderGeometry2d, CompoundColliderGeometry2d, PhysicsWorld2d};
#[cfg(feature = "scene-runtime")]
pub use tilemap::{TILEMAP_ONE_WAY_GROUP, TILEMAP_SOLID_GROUP, TilemapColliders2d};

/// Stable ID for this plugin in Nara plugin groups.
pub const PHYSICS_2D_PLUGIN_ID: PluginId = PluginId::new("nara.physics2d");
const PHYSICS_2D_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PHYSICS_2D_PLUGIN_ID, PluginCategory::Runtime);

/// The complete physics boundary within [`FixedUpdateSet::Simulate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum Physics2dSet {
    /// Synchronizes ECS inputs, steps physics, and publishes dynamic poses and velocities.
    Step,
}

/// Installs fixed-step physics and its [`Messages<CollisionEvent>`].
///
/// The plugin handles spatial propagation before and after its fixed step. Games that change
/// transforms in frame schedules can also run [`propagate_spatial`] in `PostUpdate`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Physics2dPlugin;

impl Plugin for Physics2dPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PHYSICS_2D_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<PhysicsWorld2d>()?
            .init_resource::<Messages<CollisionEvent>>()?
            .configure_sets(
                CoreStage::FixedUpdate,
                Physics2dSet::Step.in_set(FixedUpdateSet::Simulate),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    propagate_physics_spatial,
                    #[cfg(feature = "scene-runtime")]
                    (
                        tilemap::hydrate_tilemap_colliders,
                        propagate_physics_spatial,
                    )
                        .chain()
                        .run_if(
                            bevy_ecs::schedule::common_conditions::resource_exists::<
                                nara_asset::Assets<nara_asset::tilemap::TileSetAsset>,
                            >,
                        ),
                    simulation::step_physics,
                    simulation::write_back_dynamic_state,
                    propagate_physics_spatial,
                )
                    .chain()
                    .in_set(Physics2dSet::Step),
            )?;
        register_spatial(app.world_mut()?);
        #[cfg(feature = "scene-runtime")]
        if app.has_plugin(nara_scene::SCENE_PLUGIN_ID) {
            register_scene_collisions(app)?;
        }
        Ok(())
    }

    #[cfg(feature = "scene-runtime")]
    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        if app
            .installed_plugins()
            .skip_while(|plugin| plugin.id != PHYSICS_2D_PLUGIN_ID)
            .any(|plugin| plugin.id == nara_scene::SCENE_PLUGIN_ID)
        {
            register_scene_collisions(app)?;
        }
        Ok(())
    }
}

#[cfg(feature = "scene-runtime")]
fn register_scene_collisions(app: &mut App) -> Result<(), PluginError> {
    use nara_scene::RegisterSceneComponent as _;

    app.register_scene_component::<CollisionGroups2d>()
        .map_err(|error| PluginError::SetupFailed {
            plugin: PHYSICS_2D_PLUGIN_ID,
            message: error.to_string(),
        })?;
    Ok(())
}

fn propagate_physics_spatial(spatial: SpatialPropagation) {
    propagate_spatial(spatial);
}

/// The authority for an entity's planar position and rotation.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[require(Transform2d)]
pub enum RigidBody2d {
    /// The scene supplies the pose. A collider without a body is also static.
    #[default]
    Static,
    /// The scene supplies the target pose every tick. Use a sweep to constrain authored motion.
    Kinematic,
    /// The scene supplies the initial pose. Physics then writes back position and rotation.
    /// A dynamic body belongs on a scene root. Its collider supplies its mass.
    Dynamic,
}

/// One collision shape centered on its entity. Sizes are explicit scene units.
///
/// Visual scale and draw depth do not resize or offset the shape. Parent transforms can move
/// static and kinematic colliders. Their orientation follows the transformed local X axis
/// (or Y if X is collapsed), excluding the collider entity's own visual scale.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(Transform2d)]
pub enum Collider2d {
    Box {
        half_extents: Vec2,
    },
    Circle {
        radius: f32,
    },
    /// `half_height` measures half the length between the two cap centers.
    CapsuleY {
        half_height: f32,
        radius: f32,
    },
}

impl Collider2d {
    fn native_shape(self) -> Result<SharedShape, PhysicsError> {
        let shape = match self {
            Self::Box { half_extents }
                if half_extents.is_finite() && half_extents.min_element() > 0.0 =>
            {
                SharedShape::cuboid(half_extents.x, half_extents.y)
            }
            Self::Circle { radius } if positive_finite(radius) => SharedShape::ball(radius),
            Self::CapsuleY {
                half_height,
                radius,
            } if positive_finite(half_height) && positive_finite(radius) => {
                SharedShape::capsule_y(half_height, radius)
            }
            _ => return Err(PhysicsError::InvalidCollider),
        };
        validate_local_shape(&shape)?;
        Ok(shape)
    }
}

/// Collider layer membership and interaction masks. A pair interacts when both masks agree.
/// A collider without this component uses [`Self::ALL`].
#[derive(Component, Schema, Clone, Copy, Debug, PartialEq, Eq)]
#[schema(
    crate = "nara_reflect",
    id = "nara.CollisionGroups2d",
    version = 1,
    default,
    alias = "Collision Groups 2D",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct CollisionGroups2d {
    /// Layers this collider belongs to, one bit per layer.
    pub memberships: u32,
    /// Layers this collider accepts, one bit per layer.
    pub filters: u32,
}

impl CollisionGroups2d {
    /// Participates in and accepts every layer.
    pub const ALL: Self = Self {
        memberships: u32::MAX,
        filters: u32::MAX,
    };

    fn native(self) -> InteractionGroups {
        InteractionGroups::new(
            Group::from_bits_retain(self.memberships),
            Group::from_bits_retain(self.filters),
            InteractionTestMode::And,
        )
    }
}

impl Default for CollisionGroups2d {
    fn default() -> Self {
        Self::ALL
    }
}

/// Makes an entity's collider detect intersections without applying contact forces.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Sensor2d;

/// Use the authored [`Transform2d`] as an immediate pose once, before the next physics step.
///
/// Insert alongside a transform edit. Physics removes the marker when it accepts the pose.
/// Dynamic velocity is preserved, and the object can move again during the same step.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Teleport2d;

/// World-space translation and counterclockwise angle in radians.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicsPose2d {
    pub translation: Vec2,
    pub rotation: f32,
}

impl PhysicsPose2d {
    #[must_use]
    pub const fn new(translation: Vec2, rotation: f32) -> Self {
        Self {
            translation,
            rotation,
        }
    }

    fn native(self) -> Result<Pose, PhysicsError> {
        if !self.translation.is_finite() || !self.rotation.is_finite() {
            return Err(PhysicsError::InvalidPose);
        }
        Ok(Pose::new(native_vector(self.translation), self.rotation))
    }

    fn from_native(pose: &Pose) -> Self {
        Self::new(scene_vector(pose.translation), pose.rotation.angle())
    }
}

/// A collider pair began or ended contact or sensor overlap, as reported by the fixed solver.
///
/// The entities in each pair are ordered by [`Entity`]. A stopped pair can include despawned
/// entities. Two static colliders do not produce messages. Contacts reached during integration
/// can be reported at the next fixed tick. Read with Bevy ECS's `MessageReader<CollisionEvent>` after
/// [`Physics2dSet::Step`]. The plugin retains messages from the current and previous frame with
/// fixed ticks. A headless `App::step_fixed` counts as one such frame.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionEvent {
    Started(Entity, Entity),
    Stopped(Entity, Entity),
}

/// An invalid simulation input or spatial query.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsError {
    #[error("a dynamic rigid body must be a scene root")]
    ParentedDynamic,
    #[error("the planar pose must have a finite translation and rotation")]
    InvalidPose,
    #[error("the collider must have positive, representable dimensions and bounds")]
    InvalidCollider,
    #[error("an entity can have either a primitive or a compound collider")]
    ConflictingColliders,
    #[error("gravity must be finite")]
    InvalidGravity,
    #[error("linear and angular velocity must be finite")]
    InvalidVelocity,
    #[error("external force and torque must be finite")]
    InvalidForce,
    #[error("external impulse and angular impulse must be finite")]
    InvalidImpulse,
    #[error("friction and restitution must be finite and nonnegative")]
    InvalidMaterial,
    #[error("linear and angular damping must be finite and nonnegative")]
    InvalidDamping,
    #[error("gravity scale must be finite")]
    InvalidGravityScale,
    #[error(
        "a ray needs a finite origin, direction, and endpoint, a nonzero direction, and a nonnegative range"
    )]
    InvalidRay,
    #[error("the shape displacement and its endpoint must be finite")]
    InvalidDisplacement,
    #[error("the character's separation margin must be finite and positive")]
    InvalidMargin,
    #[error("the character's up vector must be finite and nonzero")]
    InvalidCharacterUp,
    #[error("slope angles must be finite and in [0, pi/2] radians")]
    InvalidSlopeAngle,
    #[error(
        "ground snap and autostep distances must be finite and nonnegative, with positive step height"
    )]
    InvalidCharacterDistance,
}

fn positive_finite(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn nonnegative_finite(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

fn validate_local_shape(shape: &SharedShape) -> Result<(), PhysicsError> {
    validate_shape_pose(shape, &Pose::IDENTITY)?;
    let mass = shape.mass_properties(1.0);
    if positive_finite(mass.mass()) && positive_finite(mass.principal_inertia()) {
        Ok(())
    } else {
        Err(PhysicsError::InvalidCollider)
    }
}

fn validate_shape_pose(shape: &SharedShape, pose: &Pose) -> Result<(), PhysicsError> {
    let bounds = shape.compute_aabb(pose);
    let extents = bounds.extents();
    if bounds.mins.is_finite()
        && bounds.maxs.is_finite()
        && extents.is_finite()
        && extents.min_element() > 0.0
    {
        Ok(())
    } else {
        Err(PhysicsError::InvalidCollider)
    }
}

fn native_vector(vector: Vec2) -> Vector {
    Vector::new(vector.x, vector.y)
}

fn scene_vector(vector: Vector) -> Vec2 {
    Vec2::new(vector.x, vector.y)
}
