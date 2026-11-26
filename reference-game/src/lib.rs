//! Night Relay, a code-first 2D game built on Nara's public runtime API.

use nara::prelude::{Component, PersistentComponent};

pub const TITLE: &str = "Night Relay";
pub const WINDOW_SIZE: (u32, u32) = (1152, 768);

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
