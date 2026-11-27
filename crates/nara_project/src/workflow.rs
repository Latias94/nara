//! Project creation, data checks, Schema discovery, and standalone delivery.

mod check;
#[cfg(feature = "delivery")]
pub mod delivery;
mod schema;
mod starter;

pub use crate::play::ProjectTrust;
pub use check::{CargoPackage, ProjectCheckError, check_project, read_project_manifest};
pub use schema::{
    SCHEMA_BINARY, SCHEMA_CACHE_PATH, SchemaCacheError, read_schema_cache, refresh_schema,
    refresh_schema_interruptible,
};
pub use starter::{DESKTOP_FEATURE, GAME_BINARY, StarterError, create_project};

#[cfg(test)]
mod tests;
