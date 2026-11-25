//! Game UI layout, text, widgets, and ECS authoring.
//!
//! The default `runtime` feature installs the ECS adapter. Layout, widgets, and text are
//! also available independently of that adapter.

#[cfg(feature = "runtime")]
mod authoring;
#[cfg(feature = "runtime")]
mod authoring_composition;
#[cfg(feature = "runtime")]
mod authoring_model;
#[cfg(feature = "runtime")]
mod canvas;
#[cfg(feature = "runtime")]
mod codec;
#[cfg(feature = "runtime")]
mod interaction;
pub mod kernel;
#[cfg(feature = "runtime")]
mod persistent_key;
#[cfg(feature = "runtime")]
mod plugin;
#[cfg(feature = "runtime")]
mod primary_surface_metrics;
#[cfg(feature = "runtime")]
mod runtime;
#[cfg(feature = "runtime")]
mod style;
pub mod text;
pub mod widgets;

pub use text::{FontFace, FontFaceError};

#[cfg(feature = "runtime")]
pub use authoring::{
    UiButton, UiButtonVisuals, UiNode, UiRoot, UiRuntimeKey, UiSemantics, UiText, UiTheme, UiVisual,
};
#[cfg(feature = "runtime")]
pub use codec::register_ui_components;
#[cfg(feature = "runtime")]
pub use interaction::{
    MAX_UI_ACTION_CONSUMERS, MAX_UI_ACTION_JOURNAL_ENTRIES, UiActionAdmissionError,
    UiActionConsumerError, UiActionConsumerId, UiActionCursor, UiActionEvent, UiActionJournal,
    UiActionSequence, UiInputProjectionIssue, UiRouteDisposition, UiRouteRejection,
    UiRouteRejectionLog, UiRouteRejectionRecord, dispatch_ui_accessibility_input,
};
pub use kernel::{
    UiAccessibilityInput, UiAccessibilityRequest, UiActionId, UiActivationSource,
    UiPresentationReceipt, UiRouteStamp, UiSafeAreaInsets, UiSeatId, UiSize,
    UiSurfaceContentTransform, UiSurfaceContentTransformError, UiSurfaceId,
};
#[cfg(feature = "runtime")]
pub use plugin::{
    UI_PLUGIN_DECLARATION, UI_PLUGIN_ID, UI_SCHEMA_OWNER_ID, UI_SCHEMA_PROVIDER,
    UI_SCHEMA_PROVIDER_ID, UiPlugin, plugin,
};
#[cfg(feature = "runtime")]
pub use primary_surface_metrics::UiPrimarySurfaceMetrics;
#[cfg(feature = "runtime")]
pub use runtime::{
    UiAdapterDiagnostic, UiAdapterDiagnostics, UiAdapterError, UiSurfaceAvailability,
    UiSurfaceEndpoint, UiSurfaceRetirement, UiSurfaceSlot, UiSurfaceSlotError,
    UiSurfaceSlotSnapshot, UiSurfaceSuspension, UiSurfaceSuspensionReason, active_ui_surface_slots,
    install_ui_surface, restore_ui_surface, retire_ui_surface, suspend_ui_surface,
    ui_candidate_frame, ui_candidate_frames, ui_semantic_snapshot, ui_surface_slot_snapshots,
    update_ui_surface_slot,
};
#[cfg(feature = "runtime")]
pub use style::*;

/// Prefab authoring, structural edits, and canvas projection.
#[cfg(feature = "runtime")]
pub mod prefab_authoring {
    pub use crate::authoring_composition::{
        UiAuthoringHierarchyKind, UiAuthoringStructuralCapability,
        UiAuthoringStructuralReadOnlyReason, UiCatalogNodeKind, UiCreateParameters, UiEntityRecipe,
        UiImageChoice, UiImageChoiceGeneration, UiImageChoiceSelection, UiImageChoiceSet,
        UiPlacementCandidate, UiPreparedStructure, UiStructureIntent, UiStructureRejection,
    };
    pub use crate::authoring_model::{
        UiAuthoringCanvasFact, UiAuthoringCanvasProjection, UiAuthoringCanvasReady,
        UiAuthoringCanvasUnavailableReason, UiAuthoringCommitPolicy, UiAuthoringComponentKind,
        UiAuthoringDiagnostic, UiAuthoringDiagnosticReason, UiAuthoringDimensionKind,
        UiAuthoringDirectEdit, UiAuthoringEditError, UiAuthoringEditability,
        UiAuthoringFieldDescriptor, UiAuthoringFieldGroup, UiAuthoringFieldKey,
        UiAuthoringFieldKind, UiAuthoringFieldMember, UiAuthoringFieldValue,
        UiAuthoringGizmoCapabilities, UiAuthoringHierarchyNode, UiAuthoringProjection,
        UiAuthoringProjectionError, UiAuthoringReadOnlyReason, UiAuthoringScalarMetadata,
        UiAuthoringSourceMapEntry, UiBrushAuthoringKind, UiDirectManipulationCapability,
        UiDirectManipulationReadOnlyReason, project_prefab_authoring,
    };
    pub use crate::canvas::{
        UiAuthoringCanvasCandidate, UiAuthoringCanvasCompileError, UiAuthoringCanvasCompileRequest,
        UiAuthoringCanvasCompiler,
    };
    pub use crate::codec::{
        UI_VISUAL_ID, UI_VISUAL_IMAGE_FIELD_ID, persisted_ui_visual_image_value,
        prefab_contains_ui_root,
    };
}

/// Exact presentation transactions and surface retirement for host integrations.
#[cfg(feature = "runtime")]
pub mod presentation {
    pub use crate::runtime::{
        UiDeferredPresentationAuthority, UiDeferredPresentationRetry,
        UiPreparedPresentationAuthority, UiPresentationCommit, UiPresentationInvalidationReason,
        abort_prepared_ui_presentation, can_abort_prepared_ui_presentation,
        commit_prepared_ui_presentation, defer_prepared_ui_presentation, prepare_ui_presentation,
        retired_ui_surface_token, retry_deferred_ui_presentation,
        supersede_deferred_ui_presentation, supersede_prepared_ui_presentation,
    };
}

/// Ordinary runtime authoring surface. Compiler, journal, and session types stay out of this prelude.
#[cfg(feature = "runtime")]
pub mod prelude {
    pub use crate::{
        UiActionConsumerError, UiActionConsumerId, UiActionCursor, UiActionEvent, UiActionId,
        UiActionJournal, UiActionSequence, UiAlign, UiBorder, UiBrush, UiButton, UiButtonVisuals,
        UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges, UiFlexDirection, UiImageId,
        UiJustify, UiLayer, UiNode, UiOverflow, UiPlugin, UiPositionMode, UiRoot, UiRuntimeKey,
        UiSemanticRole, UiSemantics, UiStyle, UiText, UiTextAlignment, UiTextOverflow, UiTheme,
        UiThemePalette, UiVisual,
    };
}

/// Exact presentation and presented-input boundary for first-party product integrations.
pub mod advanced {
    pub use crate::kernel::{
        UiFrame, UiKey, UiNonActivation, UiPaintPrimitive, UiSemanticSnapshot,
    };
    #[cfg(feature = "runtime")]
    pub use crate::{
        UiAccessibilityInput, UiAccessibilityRequest, UiActivationSource, UiAdapterDiagnostic,
        UiAdapterDiagnostics, UiAdapterError, UiInputProjectionIssue, UiPresentationReceipt,
        UiRouteDisposition, UiRouteRejection, UiRouteRejectionLog, UiRouteRejectionRecord,
        UiRouteStamp, UiSafeAreaInsets, UiSeatId, UiSize, UiSurfaceAvailability,
        UiSurfaceContentTransform, UiSurfaceContentTransformError, UiSurfaceEndpoint, UiSurfaceId,
        UiSurfaceRetirement, UiSurfaceSlot, UiSurfaceSlotError, UiSurfaceSlotSnapshot,
        UiSurfaceSuspension, UiSurfaceSuspensionReason, active_ui_surface_slots,
        dispatch_ui_accessibility_input, install_ui_surface, restore_ui_surface, retire_ui_surface,
        suspend_ui_surface, ui_candidate_frame, ui_candidate_frames, ui_semantic_snapshot,
        ui_surface_slot_snapshots, update_ui_surface_slot,
    };
}

#[cfg(all(test, feature = "runtime"))]
mod adapter_tests;

#[cfg(all(test, feature = "runtime"))]
mod component_schema_tests;
