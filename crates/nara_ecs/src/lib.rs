//! ECS substrate for nara.
//!
//! nara intentionally uses `bevy_ecs` for archetype storage, queries,
//! schedules, commands, change detection, and derives. This crate is the
//! narrow public boundary where nara can add conventions without pretending to
//! own a separate ECS implementation.

pub use nara_ecs_derive::{Component, Resource, ScheduleLabel, SystemSet};

pub use bevy_ecs::{
    bundle, change_detection, component, entity, error, event, hierarchy, lifecycle, message, name,
    observer, query, relationship, resource, schedule, system, world,
};

pub use bevy_ecs::prelude::*;

pub mod prelude {
    pub use crate::{Component, Resource, ScheduleLabel, SystemSet};
    pub use bevy_ecs::prelude::*;
}
