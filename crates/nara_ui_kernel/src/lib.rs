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

/// First-party cross-crate implementation seams that are not product API.
#[doc(hidden)]
pub mod __private {
    pub use crate::authoring::with_rejected_root;
    pub use crate::interaction::UiCandidateReconciliation;
    pub use crate::journal::UiFrameJournalAuthority;

    use crate::{UiFrameJournal, UiJournalError, UiLimits};

    /// Creates one journal together with the exclusive first-party submission capability for its
    /// lineage. Ordinary consumers should use [`UiFrameJournal::with_limits`] instead.
    pub fn new_hosted_frame_journal(
        limits: UiLimits,
    ) -> Result<(UiFrameJournal, UiFrameJournalAuthority), UiJournalError> {
        UiFrameJournal::new_hosted(limits)
    }
}

pub use authoring::{
    UiAuthoringError, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiInteraction, UiLayer,
    UiNode, UiRoot, UiSemanticRole, UiSemantics,
};
pub use compiler::{UiCompileError, UiCompiler};
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
    UiDispatchBatch, UiDispatchError, UiDispatchOutcome, UiInputEvent, UiInteractionSession,
    UiNavigation, UiNonActivation, UiPointerButton, UiPointerId, UiRoutePhase, UiRoutedEvent,
    UiRoutedEventKind, UiSeatId, UiSemanticFact, UiSemanticSnapshot, UiSemanticState,
};
pub use journal::{
    UiFrameJournal, UiJournalError, UiRouteStamp, UiRouteTarget, UiSurfaceId,
    UiSurfaceRetirementToken,
};
pub use limits::{UiLimits, UiLimitsError};
pub use style::{
    UiAlign, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges,
    UiFlexDirection, UiJustify, UiOverflow, UiPositionMode, UiRevision, UiStyle, UiStyleError,
    UiText, UiTextAlignment, UiTextOverflow, UiTheme, UiThemePalette, UiVisual,
};

#[cfg(test)]
mod tests;
