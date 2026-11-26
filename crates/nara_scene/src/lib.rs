//! Code-first scene components and 2D spatial propagation over the Bevy hierarchy.

pub mod spatial;

pub use spatial::{SPATIAL_PLUGIN_ID, SpatialPlugin, SpatialSet, Visibility};

use bevy_ecs::component::Component;

/// A human-readable label for an entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Component)]
pub struct Name(pub String);

impl Name {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
