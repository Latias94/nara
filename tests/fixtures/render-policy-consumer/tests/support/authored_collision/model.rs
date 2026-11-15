use std::{sync::Arc, time::Duration};

use motion_physics::authored::{CollisionDefinition, DebugSnapshot};
use nara::{
    core::{Mat3, Vec2},
    transform::Transform2d,
};
use nara_app::RenderTime;
use nara_ecs::Entity;
use nara_render_wgpu::WgpuRenderInput;

pub const WIDTH: u32 = 128;
pub const HEIGHT: u32 = 80;
pub const TIMESTEP: Duration = Duration::from_millis(16);
pub const CAMERA_CENTER: [f64; 2] = [3.0, 1.5];
pub const CAMERA_HEIGHT: f64 = 10.0;
pub const PLATFORM_COLOR: [u8; 4] = [0, 255, 0, 255];
pub const BODY_COLOR: [u8; 4] = [255, 0, 0, 255];
pub const CLEAR_COLOR: [u8; 4] = [0, 0, 0, 255];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Initial,
    Edited,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Authored {
    pub variant: Variant,
    pub ancestor: Transform2d,
    pub platform: Transform2d,
    pub platform_box: CollisionDefinition,
    pub body: Transform2d,
    pub body_box: CollisionDefinition,
}

pub fn authored(variant: Variant) -> Authored {
    let edited = variant == Variant::Edited;
    let ancestor = Transform2d {
        translation: if edited {
            Vec2::new(2.6, 0.9)
        } else {
            Vec2::new(3.0, -0.4)
        },
        rotation: 0.3,
        scale: if edited {
            Vec2::new(-1.6, 0.9)
        } else {
            Vec2::new(2.0, 0.7)
        },
    };
    let angle = (-f64::from(ancestor.scale.x) * f64::from(ancestor.rotation).sin())
        .atan2(f64::from(ancestor.scale.y) * f64::from(ancestor.rotation).cos())
        as f32;
    let platform = Transform2d {
        translation: Vec2::new(0.2, -0.1),
        rotation: angle,
        scale: Vec2::ONE,
    };
    let platform_box = CollisionDefinition {
        center: Vec2::new(0.3, -0.2),
        half_width: 2.0,
        half_height: if edited { 0.65 } else { 0.4 },
        dynamic: false,
    };
    let center = (ancestor.matrix() * platform.matrix()).transform_point2(platform_box.center);
    Authored {
        variant,
        ancestor,
        platform,
        platform_box,
        body: Transform2d::from_translation(Vec2::new(
            center.x - 0.2,
            if edited { 4.2 } else { 3.0 },
        )),
        body_box: CollisionDefinition {
            center: Vec2::new(0.2, 0.7),
            half_width: 0.35,
            half_height: 0.3,
            dynamic: true,
        },
    }
}

#[derive(Debug, Clone)]
pub struct FixedFact {
    pub body_entity: Entity,
    pub input_stamp: nara::transform::SpatialSnapshotStamp,
    pub result_stamp: nara::transform::SpatialSnapshotStamp,
    pub body_before: Transform2d,
    pub body_after: Transform2d,
    pub body_global: Mat3,
    pub debug: Arc<DebugSnapshot>,
}

#[derive(Clone)]
pub struct Sample {
    pub input: WgpuRenderInput,
    pub authored: Authored,
    pub body: Transform2d,
    pub body_presented: Mat3,
    pub platform_presented: Mat3,
    pub render_time: RenderTime,
    pub paused: bool,
    pub tick: u64,
    pub frame: u64,
    pub captures: usize,
    pub fixed: Option<FixedFact>,
    pub body_entity: Entity,
    pub platform_entity: Entity,
    pub contact: bool,
    pub native_counts: (usize, usize),
    pub debug: Arc<DebugSnapshot>,
    pub revision: nara::scene::SceneAuthoringRevision,
}
