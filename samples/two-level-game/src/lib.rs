//! A complete small exploration product: authored levels, finite travel and explicit checkpoints.

pub mod animation;
#[cfg(all(windows, feature = "audio-cpal"))]
pub mod audio;
pub mod command;
#[cfg(feature = "development")]
pub mod development;
mod gameplay;
mod host;
pub mod levels;
#[cfg(feature = "development")]
pub mod replay;
pub mod resume;
pub mod save;
pub mod sound_bank;

#[cfg(feature = "desktop")]
pub use gameplay::cooked_desktop_recipe;
pub use gameplay::input;
#[cfg(feature = "desktop")]
pub use gameplay::movement_keyboard_bindings;
pub use gameplay::resume::{GameResumeError, capture_resume, request_resume};
#[cfg(feature = "desktop")]
pub use gameplay::resume::{capture_desktop_resume, restore_desktop_resume};
pub use gameplay::{
    CollisionBox, GamePhase, GameSnapshot, RunOptions, Scenario, SoundPalette, configured_recipe,
    cooked_recipe, player_launch, player_launch_result, product_recipe,
};
pub use host::{open_explicit_project_root, open_project_root};
#[cfg(feature = "development")]
#[doc(hidden)]
pub use nara::project_tooling::{ProductObservationTarget, record_product_observation};
