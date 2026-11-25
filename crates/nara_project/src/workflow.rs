//! Project creation, read-only checks, Schema discovery, and delivery for the CLI and editor.

mod check;
mod delivery;
mod schema;
mod starter;

pub use check::{ProjectCheck, ProjectCheckError, check_project};
pub use delivery::{
    COOKER_BINARY, DeliveryError, DeliveryReceipt, DeliveryRequest, deliver_project,
};
pub use schema::{
    ProjectTrust, SCHEMA_BINARY, SCHEMA_CACHE_PATH, SchemaCacheError, read_schema_cache,
    refresh_schema, refresh_schema_interruptible,
};
pub use starter::{DESKTOP_FEATURE, GAME_BINARY, StarterError, create_project};

#[cfg(test)]
mod tests;
