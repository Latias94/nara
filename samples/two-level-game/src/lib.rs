//! A complete small exploration product: authored levels, finite travel and explicit checkpoints.

pub mod development;
mod gameplay;
mod host;
pub mod levels;
pub mod resume;
pub mod save;

#[cfg(feature = "desktop")]
pub use gameplay::movement_keyboard_bindings;
pub use gameplay::resume::{GameResumeError, capture_resume, request_resume};
#[cfg(feature = "desktop")]
pub use gameplay::resume::{capture_desktop_resume, restore_desktop_resume};
pub use gameplay::{
    GamePhase, GameSnapshot, RunOptions, Scenario, configured_recipe, cooked_recipe, player_launch,
    product_recipe,
};
pub use host::{open_explicit_project_root, open_project_root};
#[doc(hidden)]
pub use nara::project_tooling::{ProductObservationTarget, record_product_observation};
