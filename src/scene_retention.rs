//! Shared Scene allocation observations and the retained material record charge.

#[cfg(feature = "serde")]
pub(crate) use nara_scene::retention::{
    ARC_CONTROL_BLOCK_BYTES, runtime_startup_scene_retained_bytes,
};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) use nara_scene::retention::{BTREE_ENTRY_OVERHEAD, STRING_ALLOCATION_OVERHEAD};
#[cfg(feature = "project-source")]
pub(crate) use nara_scene::retention::{
    VALUE_NODE_ALLOCATION_BYTES, compiled_scene_retained_bytes, prefab_retained_bytes,
    scene_retained_bytes, startup_scene_retained_bytes, string_retained_bytes,
};

#[cfg(all(feature = "project-runtime", feature = "runtime-2d", feature = "serde"))]
use std::mem::size_of;

#[cfg(all(feature = "project-runtime", feature = "runtime-2d", feature = "serde"))]
pub(crate) fn material_record_work_bytes(cost: nara_reflect::ComponentValueCost) -> Option<usize> {
    cost.nodes()
        .checked_mul(size_of::<nara_reflect::ComponentValue>().checked_add(BTREE_ENTRY_OVERHEAD)?)?
        .checked_add(cost.logical_bytes())?
        .checked_add(size_of::<nara_render::material::Material2dRecord>())
}
