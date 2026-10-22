//! ECS substrate for nara.
//!
//! nara intentionally uses `bevy_ecs` for archetype storage, queries,
//! schedules, commands, change detection, and derives. This crate is the
//! narrow public boundary where nara can add conventions without pretending to
//! own a separate ECS implementation.

pub use nara_ecs_derive::{Bundle, Component, Resource, ScheduleLabel, SystemSet};

pub use bevy_ecs::{
    change_detection, component, entity, error, event, hierarchy, lifecycle, message, name,
    observer, query, relationship, resource, schedule, world,
};

pub use bevy_ecs::prelude::*;

/// Bundles and their facade-aware derive.
pub mod bundle {
    pub use bevy_ecs::bundle::*;
    pub use nara_ecs_derive::Bundle;
}

/// Systems and their facade-aware parameter derive.
pub mod system {
    pub use bevy_ecs::system::*;
    pub use nara_ecs_derive::SystemParam;
}

pub use system::SystemParam;

#[doc(hidden)]
pub use bevy_ecs::{__macro_exports, ptr};

#[doc(hidden)]
pub mod __private;

mod transaction;

pub use transaction::{
    LifecycleFreeDespawn, LifecycleFreeDespawnError, LifecycleFreeInsertion,
    LifecycleFreeInsertionError, LifecycleFreeInsertionPlan, prepare_lifecycle_free_despawn,
};

pub mod prelude {
    pub use crate::{Bundle, Component, Resource, ScheduleLabel, SystemParam, SystemSet};
    pub use bevy_ecs::prelude::*;
}
