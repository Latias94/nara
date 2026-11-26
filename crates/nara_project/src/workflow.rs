//! Project creation, read-only checks, and Schema discovery for the CLI and editor.

mod check;
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
