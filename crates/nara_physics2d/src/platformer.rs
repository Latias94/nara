use std::{
    f32::consts::{FRAC_PI_2, FRAC_PI_4},
    time::Duration,
};

use nara_core::Vec2;
use rapier2d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    math::{Pose, Vector},
    parry::{
        bounding_volume::BoundingVolume,
        query::{ShapeCastHit, ShapeCastOptions, cast_shapes},
    },
    prelude::{Collider, QueryPipeline, Shape, SharedShape},
};

use crate::{
    Collider2d, CollisionGroups2d, PhysicsError, PhysicsPose2d, PhysicsQueryFilter2d,
    PhysicsWorld2d, native_vector, nonnegative_finite, positive_finite, query::validate_sweep,
    scene_vector,
};

// Match the controller's minimum upward component for supporting normals.
const GROUND_DOT: f32 = 1.0e-3;

/// Optional automatic traversal of small steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlatformerAutostep {
    /// Maximum step height in scene units, greater than zero.
    pub max_height: f32,
    /// Minimum clear width on top of the step, in scene units.
    pub min_width: f32,
    /// Allow stepping onto dynamic bodies.
    pub include_dynamic_bodies: bool,
}

/// Movement configuration for a [`PlatformerCharacter`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlatformerConfig {
    /// World-space up direction. The constructor normalizes this vector.
    pub up: Vec2,
    /// Separation from obstacles in scene units, greater than zero.
    pub margin: f32,
    /// Maximum climb angle measured from up to the surface normal, in radians.
    pub max_slope_climb_angle: f32,
    /// Slope angle at which descending starts to slide along the surface, in radians.
    pub min_slope_slide_angle: f32,
    /// Maximum downward snap distance in scene units when moving from support.
    pub snap_to_ground: Option<f32>,
    /// Automatic step traversal, disabled by default.
    pub autostep: Option<PlatformerAutostep>,
    /// Membership bits marking one-way platforms. Wildcard `ALL` memberships count as solid.
    pub one_way_layer: Option<u32>,
}

impl Default for PlatformerConfig {
    fn default() -> Self {
        Self {
            up: Vec2::Y,
            margin: 0.01,
            max_slope_climb_angle: FRAC_PI_4,
            min_slope_slide_angle: FRAC_PI_4,
            snap_to_ground: None,
            autostep: None,
            one_way_layer: None,
        }
    }
}

/// A reusable shape and configuration for sweeps with slope, step, and ground handling.
///
/// The caller owns the character's pose, gravity, and velocity. Use a shape overlap query to
/// check placement when starting inside geometry is possible.
#[derive(Clone, Debug)]
pub struct PlatformerCharacter {
    shape: SharedShape,
    config: PlatformerConfig,
}

impl PlatformerCharacter {
    /// Validates the character shape and settings. Slope angles are in `[0, π/2]`.
    pub fn new(shape: Collider2d, mut config: PlatformerConfig) -> Result<Self, PhysicsError> {
        if !positive_finite(config.margin) {
            return Err(PhysicsError::InvalidMargin);
        }
        config.up = (config.up / config.up.abs().max_element())
            .try_normalize()
            .ok_or(PhysicsError::InvalidCharacterUp)?;
        if !(0.0..=FRAC_PI_2).contains(&config.max_slope_climb_angle)
            || !(0.0..=FRAC_PI_2).contains(&config.min_slope_slide_angle)
        {
            return Err(PhysicsError::InvalidSlopeAngle);
        }
        if config
            .snap_to_ground
            .is_some_and(|distance| !nonnegative_finite(distance))
            || config.autostep.is_some_and(|step| {
                !positive_finite(step.max_height) || !nonnegative_finite(step.min_width)
            })
        {
            return Err(PhysicsError::InvalidCharacterDistance);
        }
        Ok(Self {
            shape: shape.native_shape()?,
            config,
        })
    }

    fn controller(&self) -> KinematicCharacterController {
        KinematicCharacterController {
            up: native_vector(self.config.up),
            offset: CharacterLength::Absolute(self.config.margin),
            max_slope_climb_angle: self.config.max_slope_climb_angle,
            min_slope_slide_angle: self.config.min_slope_slide_angle,
            autostep: self.config.autostep.map(|step| CharacterAutostep {
                max_height: CharacterLength::Absolute(step.max_height),
                min_width: CharacterLength::Absolute(step.min_width),
                include_dynamic_bodies: step.include_dynamic_bodies,
            }),
            // Probe both solid and one-way geometry before picking a surface to snap to.
            snap_to_ground: None,
            ..Default::default()
        }
    }
}

/// The outcome of a platformer movement query.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlatformerMovement {
    /// World displacement in scene units. Apply once to the authored pose.
    pub translation: Vec2,
    /// The character is supported at the returned position.
    pub grounded: bool,
    /// A contact during movement had a downward-facing normal.
    pub hit_ceiling: bool,
    /// Normal of the first wall or unclimbable slope hit during movement.
    pub wall_normal: Option<Vec2>,
    /// Angle of the supporting surface, in radians from the configured up direction.
    pub slope_angle: Option<f32>,
    /// Counts solid movement contacts and one-way landings. Repeated hits may occur.
    pub collision_count: u32,
}

impl PlatformerMovement {
    fn record_solid(&mut self, normal: Vector, controller: &KinematicCharacterController) {
        self.collision_count += 1;
        let facing = normal.dot(controller.up);
        if facing < -GROUND_DOT {
            self.hit_ceiling = true;
        } else {
            let angle = facing.clamp(-1.0, 1.0).acos();
            if self.wall_normal.is_none()
                && angle > 0.0
                && angle >= controller.max_slope_climb_angle
            {
                self.wall_normal = Some(scene_vector(normal));
            }
        }
    }
}

impl PhysicsWorld2d {
    /// Sweeps a platformer against the latest committed colliders.
    ///
    /// `displacement` is a world displacement already integrated over `delta`. Apply the result
    /// once. The filter can exclude the character's entity and select compatible groups. One-way
    /// shapes support descent and ground probes from above each part's top along `up`.
    pub fn move_platformer(
        &self,
        character: &PlatformerCharacter,
        pose: PhysicsPose2d,
        displacement: Vec2,
        delta: Duration,
        filter: PhysicsQueryFilter2d,
    ) -> Result<PlatformerMovement, PhysicsError> {
        let pose = validate_sweep(&character.shape, pose, displacement)?;
        let queries = self.queries(filter);
        let layers = character.config.one_way_layer.unwrap_or(0);
        let is_one_way = |_, collider: &Collider| {
            let memberships = collider.collision_groups().memberships.bits();
            memberships != CollisionGroups2d::ALL.memberships && memberships & layers != 0
        };
        let is_solid = |handle, collider: &Collider| !is_one_way(handle, collider);
        let solids = if layers == 0 {
            queries
        } else {
            queries.with_filter(queries.filter.predicate(&is_solid))
        };
        let one_way = (layers != 0 && displacement.dot(character.config.up) <= 0.0)
            .then(|| queries.with_filter(queries.filter.predicate(&is_one_way)));
        let sweep = PlatformerSweep {
            character,
            solids,
            one_way,
        };
        Ok(sweep.move_shape(pose, native_vector(displacement), delta.as_secs_f32()))
    }
}

struct PlatformerSweep<'a> {
    character: &'a PlatformerCharacter,
    solids: QueryPipeline<'a>,
    one_way: Option<QueryPipeline<'a>>,
}

impl PlatformerSweep<'_> {
    fn move_shape(&self, pose: Pose, desired: Vector, seconds: f32) -> PlatformerMovement {
        let controller = self.character.controller();
        let shape = self.character.shape.as_ref();
        let mut result = PlatformerMovement::default();
        let mut previous = Vector::ZERO;
        let mut landing = None;
        let descending = self.one_way.is_some() && desired.dot(controller.up) < 0.0;
        let movement = controller.move_shape(seconds, &self.solids, shape, &pose, desired, |hit| {
            if descending && landing.is_none() {
                landing = self.one_way_landing(pose, previous, hit.translation_applied);
                previous = hit.translation_applied;
            }
            if landing.is_none() {
                result.record_solid(hit.hit.normal1, &controller);
            }
        });
        if descending && landing.is_none() {
            landing = self.one_way_landing(pose, previous, movement.translation);
        }

        // Land along the path resolved against solid obstacles, then give the remaining lateral
        // motion back to the controller. One-way geometry supplies support without side walls.
        let (mut translation, grounded_on_solid) = if let Some(landing) = landing {
            result.collision_count += 1;
            let remaining = desired - landing;
            let lateral = remaining - controller.up * remaining.dot(controller.up);
            let landing_pose = Pose::from_translation(landing) * pose;
            let slide = controller.move_shape(
                seconds,
                &self.solids,
                shape,
                &landing_pose,
                lateral,
                |hit| result.record_solid(hit.hit.normal1, &controller),
            );
            (landing + slide.translation, slide.grounded)
        } else {
            (movement.translation, movement.grounded)
        };

        let margin = self.character.config.margin;
        let nudge = controller.normal_nudge_factor;
        let probe = cast_options(margin + nudge, nudge);
        let end = Pose::from_translation(translation) * pose;
        let mut ground = self.ground_hit(&end, probe);
        if ground.is_none()
            && desired.dot(controller.up) <= 0.0
            && translation.dot(controller.up) <= 0.0
            && let Some(snap) = self.character.config.snap_to_ground
            && self.ground_hit(&pose, probe).is_some()
            && let Some(hit) = self.ground_hit(&end, cast_options(margin, snap))
        {
            translation -= controller.up * hit.time_of_impact;
            ground = Some(hit);
        }
        result.translation = scene_vector(translation);
        result.grounded = grounded_on_solid || ground.is_some();
        result.slope_angle =
            ground.map(|hit| hit.normal1.dot(controller.up).clamp(0.0, 1.0).acos());
        result
    }

    fn one_way_landing(&self, origin: Pose, from: Vector, to: Vector) -> Option<Vector> {
        let displacement = to - from;
        if displacement.dot(native_vector(self.character.config.up)) >= 0.0 {
            return None;
        }
        let pose = Pose::from_translation(from) * origin;
        self.cast_one_way(
            &pose,
            displacement,
            cast_options(self.character.config.margin, 1.0),
        )
        .map(|hit| from + displacement * hit.time_of_impact)
    }

    fn ground_hit(&self, pose: &Pose, options: ShapeCastOptions) -> Option<ShapeCastHit> {
        let up = native_vector(self.character.config.up);
        let solid = self
            .solids
            .cast_shape(pose, -up, self.character.shape.as_ref(), options)
            .map(|(_, hit)| hit)
            .filter(|hit| hit.normal1.dot(up) >= GROUND_DOT);
        let one_way = self.cast_one_way(pose, -up, options);
        match (solid, one_way) {
            (Some(solid), Some(one_way)) if solid.time_of_impact <= one_way.time_of_impact => {
                Some(solid)
            }
            (solid, one_way) => one_way.or(solid),
        }
    }

    fn cast_one_way(
        &self,
        pose: &Pose,
        direction: Vector,
        options: ShapeCastOptions,
    ) -> Option<ShapeCastHit> {
        let queries = self.one_way.as_ref()?;
        let shape = self.character.shape.as_ref();
        let up = native_vector(self.character.config.up);
        let feet = shape.as_support_map()?.support_point(pose, -up);
        let end = Pose::from_translation(direction * options.max_time_of_impact) * *pose;
        let bounds = shape
            .compute_aabb(pose)
            .merged(&shape.compute_aabb(&end))
            .loosened(options.target_distance);
        let mut nearest: Option<ShapeCastHit> = None;
        let mut consider_part = |part_pose: &Pose, part: &dyn Shape| {
            let Some(top) = part
                .as_support_map()
                .map(|part| part.support_point(part_pose, up))
            else {
                return;
            };
            if (feet - top).dot(up) < 0.0 {
                return;
            }
            if let Ok(Some(hit)) = cast_shapes(
                part_pose,
                Vector::ZERO,
                part,
                pose,
                direction,
                shape,
                options,
            ) {
                let hit = hit.transform1_by(part_pose);
                if hit.normal1.dot(up) >= GROUND_DOT
                    && nearest.is_none_or(|previous| hit.time_of_impact < previous.time_of_impact)
                {
                    nearest = Some(hit);
                }
            }
        };
        for (_, collider) in queries.intersect_aabb_conservative(bounds) {
            let collider_pose = collider.position();
            if let Some(compound) = collider.shape().as_compound() {
                let local_bounds = bounds.transform_by(&collider_pose.inverse());
                for part in compound
                    .bvh()
                    .leaves(|node| node.aabb().intersects(&local_bounds))
                {
                    let (part_pose, part_shape) = &compound.shapes()[part as usize];
                    consider_part(&(*collider_pose * *part_pose), part_shape.as_ref());
                }
            } else {
                consider_part(collider_pose, collider.shape());
            }
        }
        nearest
    }
}

fn cast_options(margin: f32, distance: f32) -> ShapeCastOptions {
    ShapeCastOptions {
        target_distance: margin,
        stop_at_penetration: false,
        max_time_of_impact: distance,
        compute_impact_geometry_on_penetration: true,
    }
}

/// Fixed-step grace periods for walking off an edge and pressing jump before landing.
///
/// Use [`Self::tick`] once per fixed step with the latest grounded state and the jump press edge.
/// The timers depend only on the supplied inputs and elapsed durations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlatformerJumpTimers {
    coyote_time: Duration,
    jump_buffer: Duration,
    coyote_remaining: Duration,
    buffer_remaining: Duration,
}

impl PlatformerJumpTimers {
    /// Sets the grace period after support and the lifetime of a pending jump input.
    #[must_use]
    pub const fn new(coyote_time: Duration, jump_buffer: Duration) -> Self {
        Self {
            coyote_time,
            jump_buffer,
            coyote_remaining: Duration::ZERO,
            buffer_remaining: Duration::ZERO,
        }
    }

    /// Ages existing windows by `delta`, applies the current inputs, and returns a jump decision.
    /// A window expires when its remaining duration reaches zero. A jump consumes both windows.
    #[must_use]
    pub fn tick(&mut self, delta: Duration, grounded: bool, jump_pressed: bool) -> bool {
        self.coyote_remaining = if grounded {
            self.coyote_time
        } else {
            self.coyote_remaining.saturating_sub(delta)
        };
        self.buffer_remaining = if jump_pressed {
            self.jump_buffer
        } else {
            self.buffer_remaining.saturating_sub(delta)
        };
        if (grounded || !self.coyote_remaining.is_zero())
            && (jump_pressed || !self.buffer_remaining.is_zero())
        {
            self.coyote_remaining = Duration::ZERO;
            self.buffer_remaining = Duration::ZERO;
            true
        } else {
            false
        }
    }
}
