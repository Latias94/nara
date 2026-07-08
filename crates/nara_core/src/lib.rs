//! Shared engine primitives that are independent from ECS and rendering.

use std::time::Duration;

pub use glam::{Mat3, Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self::rgba(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::rgba(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Time {
    pub delta_seconds: f32,
    pub elapsed_seconds: f64,
    pub frame: u64,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta_seconds: 0.0,
            elapsed_seconds: 0.0,
            frame: 0,
        }
    }
}

impl Time {
    pub fn advance(&mut self, delta: Duration) {
        self.delta_seconds = delta.as_secs_f32();
        self.elapsed_seconds += delta.as_secs_f64();
        self.frame += 1;
    }
}
