//! Night Relay, a 2D game built on Nara's public runtime APIs.

use nara::{
    prelude::{App, Component, RegisterSchema, Schema},
    reflect::SchemaRegistrationError,
};

pub const TITLE: &str = "Night Relay";
pub const WINDOW_SIZE: (u32, u32) = (1152, 768);
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
const ENTRY_SCENE_ID: &str = "01a219c4-2965-7297-b503-da50fb749023";

#[derive(Clone, Copy, Debug, Component, Schema)]
#[schema(id = "night-relay.RunRules", version = 2, default, alias = "Run Rules")]
pub struct RunRules {
    #[schema(alias = "Time limit (seconds)", range = 1..=180)]
    pub time_limit_seconds: u32,
    #[schema(alias = "Flight speed", range = 40.0..=300.0)]
    pub move_speed: f32,
    #[schema(alias = "Shield charges", range = 1..=5)]
    pub shield_charges: u32,
}

#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
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

#[derive(Component, Default, Schema)]
#[schema(id = "night-relay.Pilot", version = 1, default, alias = "Pilot")]
struct Pilot;

#[derive(Component, Schema)]
#[schema(
    id = "night-relay.PowerCell",
    version = 1,
    authoring_initial,
    alias = "Power Cell"
)]
struct PowerCell {
    #[schema(alias = "Order", range = 1..=3, initial = 1)]
    order: u32,
}

#[derive(Component, Schema)]
#[schema(
    id = "night-relay.PatrolDrone",
    version = 1,
    default,
    alias = "Patrol Drone"
)]
struct PatrolDrone {
    #[schema(alias = "Order", range = 1..=3)]
    order: u32,
}

impl Default for PatrolDrone {
    fn default() -> Self {
        Self { order: 1 }
    }
}

#[derive(Component, Default, Schema)]
#[schema(
    id = "night-relay.ExtractionGate",
    version = 1,
    default,
    alias = "Extraction Gate"
)]
struct ExtractionGate;

pub fn register_game_schema(app: &mut App) -> Result<(), SchemaRegistrationError> {
    app.register_schema::<RunRules>()?
        .register_schema::<Pilot>()?
        .register_schema::<PowerCell>()?
        .register_schema::<PatrolDrone>()?
        .register_schema::<ExtractionGate>()?;
    Ok(())
}

#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod arena;
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod audio;
#[cfg(feature = "desktop")]
pub mod desktop;
#[cfg(all(feature = "simulation", feature = "source"))]
pub mod headless;
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod simulation;

#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
pub use simulation::{Failure, GamePlugin, PilotInput, Round, RoundStatus};
