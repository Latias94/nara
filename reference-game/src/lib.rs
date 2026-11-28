//! Night Relay, a 2D game built on Nara's public runtime APIs.

use nara::prelude::{Component, Schema};

pub const TITLE: &str = "Night Relay";
pub const WINDOW_SIZE: (u32, u32) = (1152, 768);

#[cfg(all(feature = "simulation", feature = "source"))]
fn development_root() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use std::path::{Path, PathBuf};

    let is_project = |root: &Path| root.join("nara.toml").is_file() && root.join("assets").is_dir();
    let root = match std::env::var_os("NARA_PROJECT_ROOT") {
        Some(root) => PathBuf::from(root),
        None => {
            let current = std::env::current_dir()?;
            let reference = current.join("reference-game");
            if !is_project(&current) && is_project(&reference) {
                reference
            } else {
                current
            }
        }
    };
    if !is_project(&root) {
        return Err(format!(
            "{} is not the Night Relay project directory. Run from the project directory or set NARA_PROJECT_ROOT",
            root.display()
        )
        .into());
    }
    Ok(root.canonicalize()?)
}

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

#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod arena;
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod audio;
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod campaign;
#[cfg(feature = "desktop")]
pub mod desktop;
#[cfg(all(feature = "simulation", feature = "source"))]
pub mod headless;
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod route;
#[cfg(any(
    feature = "schema",
    all(feature = "simulation", any(feature = "source", feature = "desktop"))
))]
mod scene_roles;
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
mod simulation;

#[cfg(feature = "schema")]
pub use scene_roles::register_game_schema;
#[cfg(all(feature = "simulation", any(feature = "source", feature = "desktop")))]
pub use simulation::{Failure, GamePlugin, PilotInput, Round, RoundStatus};
