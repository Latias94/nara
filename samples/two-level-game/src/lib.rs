//! A complete small exploration product: authored levels, finite travel and explicit checkpoints.

mod gameplay;
mod host;
pub mod levels;
pub mod save;

pub use gameplay::{
    GamePhase, GameSnapshot, RunOptions, Scenario, configured_recipe, product_recipe,
};
pub use host::{open_explicit_project_root, open_project_root};
#[doc(hidden)]
pub use nara::project_tooling::{ProductObservationTarget, record_product_observation};
