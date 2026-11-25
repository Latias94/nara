//! Renderer-neutral, bounded UI authoring and immutable frame compilation.
//!
//! The kernel owns no ECS, window, host, editor, or renderer types. Runtime and
//! tooling adapters lower their concrete data into the validated snapshot below and consume the
//! resulting immutable frame.

mod authoring;
mod compiler;
mod frame;
mod geometry;
mod identity;
mod interaction;
mod journal;
mod layout;
mod limits;
mod mesh;
mod style;

pub use authoring::{
    UiAuthoringError, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiCaptureContinuity,
    UiInteraction, UiLayer, UiNode, UiPointerButtons, UiRoot, UiSemanticRole, UiSemantics,
};
pub use compiler::{UiCompileError, UiCompileWorkStats, UiCompiler};
pub use frame::{
    UiDiagnostic, UiEnvironment, UiEnvironmentError, UiFocusFact, UiFrame, UiFrameRevision,
    UiHitRegion, UiLayoutFact, UiPaintPrimitive, UiPresentationReceipt, UiResolvedBorder,
    UiResolvedBrush, UiRootDiagnostic, UiRootOutcome, UiRootStatus, UiSemanticNode,
    UiSurfaceGenerations,
};
pub use geometry::{
    UiPhysicalRect, UiPoint, UiRect, UiSafeAreaInsets, UiSize, UiSnapPolicy,
    UiSurfaceContentTransform, UiSurfaceContentTransformError,
};
pub use identity::{
    MAX_UI_KEY_BYTES, UiActionId, UiImageId, UiKey, UiKeyAtom, UiKeyError, UiLocalKey, UiOpaqueId,
    UiPersistentIdentity, UiRootKey,
};
pub use interaction::{
    UiAccessibilityInput, UiAccessibilityRequest, UiActivationSource, UiButtonPhase,
    UiCandidateReconciliation, UiDispatchBatch, UiDispatchError, UiDispatchOutcome, UiInputEvent,
    UiInteractionSession, UiModifiers, UiNavigation, UiNonActivation, UiPointerButton, UiPointerId,
    UiPointerInput, UiPointerInputKind, UiRoutePhase, UiRoutedEvent, UiRoutedEventKind,
    UiScrollDelta, UiSeatId, UiSemanticFact, UiSemanticSnapshot, UiSemanticState,
};
pub use journal::{
    UiFrameJournal, UiFrameJournalAuthority, UiJournalError, UiRouteStamp, UiRouteTarget,
    UiSurfaceId, UiSurfaceRetirementToken,
};
pub use layout::{
    UiAvailableSpace, UiCustomLayout, UiLayout, UiLayoutChildren, UiLayoutConstraints,
    UiLayoutError, UiLayoutMeasurement, UiLayoutOutput,
};
pub use limits::{UiLimits, UiLimitsError};
pub use mesh::{UiMesh, UiMeshError, UiMeshVertex};
pub use style::{
    UiAlign, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges,
    UiFlexDirection, UiJustify, UiOverflow, UiPositionMode, UiPreparedText, UiRevision, UiStyle,
    UiStyleError, UiText, UiTextAlignment, UiTextOverflow, UiTheme, UiThemePalette, UiVisual,
};

#[cfg(test)]
mod tests;
