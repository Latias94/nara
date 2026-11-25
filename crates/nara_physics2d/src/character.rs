use std::f32::consts::PI;

use nara_core::Vec2;
use rapier2d::{
    control::{CharacterLength, KinematicCharacterController},
    math::{Pose, Vector},
    prelude::{QueryPipeline, Shape},
};

use crate::{native_vector, scene_vector};

/// The native controller's effective world displacement and number of collision callbacks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterMovement {
    /// Apply this displacement once to the character's world pose.
    pub translation: Vec2,
    /// Counts native collision callbacks, which may include repeated hits on one collider.
    pub collision_count: u32,
}

pub(super) fn top_down_controller(margin: f32) -> KinematicCharacterController {
    KinematicCharacterController {
        up: Vector::Y,
        offset: CharacterLength::Absolute(margin),
        slide: true,
        autostep: None,
        snap_to_ground: None,
        max_slope_climb_angle: PI,
        min_slope_slide_angle: -PI,
        ..Default::default()
    }
}

pub(super) fn sweep_character(
    controller: &KinematicCharacterController,
    queries: &QueryPipeline,
    shape: &dyn Shape,
    pose: &Pose,
    displacement: Vec2,
    seconds: f32,
) -> CharacterMovement {
    let mut collision_count = 0;
    let movement = controller.move_shape(
        seconds,
        queries,
        shape,
        pose,
        native_vector(displacement),
        |_| {
            collision_count += 1;
        },
    );
    CharacterMovement {
        translation: scene_vector(movement.translation),
        collision_count,
    }
}
