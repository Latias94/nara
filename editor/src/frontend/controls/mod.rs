//! Private Nara controls shared by the Editor frontends.

mod authoring;
mod hierarchy;
mod owned_hierarchy;
mod owned_ui_hierarchy;
mod work;

pub(in crate::frontend) use authoring::{
    compose_action, editing_enabled, persistence_allows, placement_intent_label,
};
pub(in crate::frontend) use hierarchy::HierarchyControls;
#[cfg(feature = "imgui-reference")]
pub(in crate::frontend) use hierarchy::HierarchySubmission;
pub(in crate::frontend) use work::OwnedPanelWork;
#[cfg(all(test, feature = "imgui-reference"))]
pub(in crate::frontend) use work::measure_calls;
