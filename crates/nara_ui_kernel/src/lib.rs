//! Renderer-neutral, bounded UI authoring and immutable frame compilation.
//!
//! The crate deliberately owns no ECS, window, host, editor, or renderer types. Runtime and
//! tooling adapters lower their concrete data into the validated snapshot below and consume the
//! resulting immutable frame.

mod authoring;
mod compiler;
mod frame;
mod geometry;
mod identity;
mod interaction;
mod journal;
mod limits;
mod style;

pub use authoring::{
    UiAuthoringError, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiInteraction, UiLayer,
    UiNode, UiRoot, UiSemanticRole, UiSemantics,
};
pub use compiler::{UiCompileError, UiCompiler};
pub use frame::{
    UiDiagnostic, UiEnvironment, UiEnvironmentError, UiFocusFact, UiFrame, UiFrameRevision,
    UiHitRegion, UiLayoutFact, UiPaintPrimitive, UiPresentationReceipt, UiResolvedBorder,
    UiResolvedBrush, UiRootDiagnostic, UiRootOutcome, UiRootStatus, UiSemanticNode,
};
pub use geometry::{UiPhysicalRect, UiPoint, UiRect, UiSize, UiSnapPolicy};
pub use identity::{
    UiActionId, UiImageId, UiKey, UiKeyError, UiLocalKey, UiOpaqueId, UiPersistentIdentity,
    UiProjectionPath, UiProjectionStep, UiRootKey,
};
pub use interaction::{
    UiActivationSource, UiButtonPhase, UiDispatchBatch, UiDispatchError, UiInputEvent,
    UiInteractionSession, UiNavigation, UiNonActivation, UiPointerButton, UiPointerId,
    UiRoutePhase, UiRoutedEvent, UiRoutedEventKind, UiSeatId, UiSemanticFact, UiSemanticSnapshot,
    UiSemanticState,
};
pub use journal::{UiFrameJournal, UiJournalError, UiRouteStamp, UiRouteTarget, UiSurfaceId};
pub use limits::{UiLimits, UiLimitsError};
pub use style::{
    UiAlign, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges,
    UiFlexDirection, UiJustify, UiOverflow, UiPositionMode, UiRevision, UiStyle, UiStyleError,
    UiText, UiTextAlignment, UiTextOverflow, UiTheme, UiThemePalette, UiVisual,
};

#[cfg(test)]
mod tests;
