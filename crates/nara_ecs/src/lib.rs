//! Native Bevy ECS types with Nara's selected authoring surface.
//!
//! Use the crate root or [`prelude`] for ordinary components, resources, queries, commands and
//! system composition. The six selected derives resolve a direct `nara` or `nara_ecs` dependency,
//! including renamed dependencies, without requiring a direct Bevy dependency.
//!
//! [`bevy_ecs`] exposes the complete substrate for advanced integration. Its API follows the
//! bound Bevy version; upstream derives outside the selected six do not receive Nara's dependency
//! resolution guarantee. All re-exported runtime types are the original Bevy types.

// The explicit type list is separate so facade derives can shadow the upstream macros while
// retaining the traits with the same names.
mod authoring {
    pub use bevy_ecs::{
        bundle::Bundle,
        change_detection::{DetectChanges, DetectChangesMut, Mut, Ref},
        component::Component,
        entity::Entity,
        query::{Added, AnyOf, Changed, Has, Or, QueryState, With, Without},
        resource::Resource,
        schedule::{
            IntoScheduleConfigs, IntoSystemSet, Schedule, ScheduleLabel, SystemCondition, SystemSet,
        },
        system::{
            Command, Commands, EntityCommand, EntityCommands, In, IntoSystem, Local, NonSend,
            NonSendMut, ParamSet, Populated, Query, Res, ResMut, Single, SystemParam,
        },
        world::{EntityMut, EntityRef, EntityWorldMut, FromWorld, World},
    };
}

pub use prelude::*;

/// The complete Bevy ECS substrate, coupled to the version selected by Nara.
///
/// Use this path for component metadata, custom parameter implementations, observers, messages,
/// raw scheduling configuration and other advanced integration. Upgrades can require upstream
/// API migrations even when the selected Nara authoring imports remain unchanged.
///
/// The complete substrate has one explicit public name:
///
/// ```compile_fail,E0432
/// use nara_ecs::upstream;
/// ```
pub use bevy_ecs;

/// Bundles and their facade-aware derive.
pub mod bundle {
    pub use crate::Bundle;
}

/// Common system authoring types, traits and the facade-aware parameter derive.
pub mod system {
    pub use crate::{
        Command, Commands, EntityCommand, EntityCommands, In, IntoSystem, Local, NonSend,
        NonSendMut, ParamSet, Populated, Query, Res, ResMut, Single, SystemParam,
    };
}

/// Ordinary schedule composition and facade-aware label derives.
pub mod schedule {
    pub use crate::{
        IntoScheduleConfigs, IntoSystemSet, Schedule, ScheduleLabel, SystemCondition, SystemSet,
    };
}

#[doc(hidden)]
pub mod __private;

mod transaction;

/// Nara's maintained ordinary ECS authoring surface.
///
/// This is an explicit selection, independent of additions to Bevy's prelude. Advanced substrate
/// APIs remain available through [`crate::bevy_ecs`].
pub mod prelude {
    pub use crate::authoring::*;
    pub use nara_ecs_derive::{Bundle, Component, Resource, ScheduleLabel, SystemParam, SystemSet};
}
