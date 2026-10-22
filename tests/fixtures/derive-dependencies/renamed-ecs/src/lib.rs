//! Invalid derives retain upstream diagnostics through the renamed facade.
//!
//! Bundle reports an invalid container attribute instead of looking for Bevy:
//!
//! ```compile_fail
//! #[derive(substrate::bundle::Bundle)]
//! #[bundle(unknown)]
//! struct InvalidBundle;
//! ```
//!
//! SystemParam explains its supported World and state lifetime names:
//!
//! ```compile_fail
//! #[derive(substrate::system::SystemParam)]
//! struct InvalidParam<'wrong> {
//!     local: substrate::system::Local<'wrong, usize>,
//! }
//! ```
//!
//! Unsupported field attributes are also rejected at the derive boundary:
//!
//! ```compile_fail
//! #[derive(substrate::system::SystemParam)]
//! struct InvalidField<'s> {
//!     #[system_param(unknown)]
//!     local: substrate::system::Local<'s, usize>,
//! }
//! ```

use substrate::{Component, Resource, ScheduleLabel, SystemSet, World};

#[cfg(test)]
use substrate as ecs;

#[cfg(test)]
#[path = "../../facade_contract.rs"]
mod facade_contract;

#[derive(Component)]
#[component(storage = "SparseSet", immutable)]
pub struct RenamedEcsComponent;

#[derive(Debug, Default, Resource)]
pub struct RenamedEcsResource<T: Send + Sync + 'static>(pub T);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct RenamedSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct RenamedSet;

pub fn insert_component(world: &mut World) {
    world.spawn(RenamedEcsComponent);
    world.init_resource::<RenamedEcsResource<u32>>();
}
