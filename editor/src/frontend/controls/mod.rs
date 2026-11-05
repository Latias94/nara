//! Private Nara controls shared by the Editor frontends.

pub(in crate::frontend) mod authoring;
pub(in crate::frontend) mod field_drafts;
mod hierarchy;
mod owned_hierarchy;
pub(crate) mod owned_inspector;
mod owned_ui_hierarchy;
pub(in crate::frontend) mod source_tool;
pub(in crate::frontend) mod value_draft;
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
