//! Night Relay, a 2D game built on Nara's public runtime and scene authoring APIs.

use nara::prelude::{Component, PersistentComponent};

pub const TITLE: &str = "Night Relay";
pub const WINDOW_SIZE: (u32, u32) = (1152, 768);
pub const DEFAULT_SCENE_PATH: &str = "assets/night-relay.nara.scene.ron";

#[derive(Clone, Copy, Debug, Component, PersistentComponent)]
#[nara(
    id = "night-relay.RunRules",
    version = 1,
    alias = "Run Rules",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct RunRules {
    #[nara(id = "time-limit-seconds", alias = "Time limit (seconds)")]
    pub time_limit_seconds: u32,
    #[nara(id = "move-speed", alias = "Flight speed")]
    pub move_speed: f32,
    #[nara(id = "shield-charges", alias = "Shield charges")]
    pub shield_charges: u32,
}

#[cfg(feature = "simulation")]
impl RunRules {
    fn checked(self) -> Result<Self, &'static str> {
        if !(1..=180).contains(&self.time_limit_seconds)
            || !(40.0..=300.0).contains(&self.move_speed)
            || !(1..=5).contains(&self.shield_charges)
        {
            return Err("round rules require 1–180 seconds, a speed of 40–300, and 1–5 shields");
        }
        Ok(self)
    }
}

impl Default for RunRules {
    fn default() -> Self {
        Self {
            time_limit_seconds: 38,
            move_speed: 184.0,
            shield_charges: 3,
        }
    }
}

#[cfg(feature = "simulation")]
mod arena;
#[cfg(feature = "simulation")]
mod audio;
#[cfg(feature = "desktop")]
pub mod desktop;
#[cfg(feature = "simulation")]
pub mod headless;
#[cfg(feature = "simulation")]
mod simulation;

#[cfg(feature = "simulation")]
pub use simulation::{Failure, GamePlugin, PilotInput, Round, RoundStatus};
