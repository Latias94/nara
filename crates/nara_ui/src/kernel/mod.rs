//! Renderer-neutral UI trees and immutable layout, paint, and hit-test data.

mod compiler;
mod frame;
mod geometry;
mod identity;
mod limits;
mod mesh;
mod style;
mod tree;

pub use compiler::{UiCompileError, UiCompileWorkStats, UiCompiler};
pub use frame::{
    UiDiagnostic, UiEnvironment, UiEnvironmentError, UiFocusFact, UiFrame, UiFrameRevision,
    UiHitRegion, UiLayoutFact, UiPaintPrimitive, UiPresentationReceipt, UiResolvedBorder,
    UiResolvedBrush, UiRootDiagnostic, UiRootOutcome, UiRootStatus, UiSemanticNode,
    UiSurfaceGenerations, UiSurfaceId,
};
pub use geometry::{
    UiPhysicalRect, UiPoint, UiRect, UiSafeAreaInsets, UiSize, UiSnapPolicy,
    UiSurfaceContentTransform, UiSurfaceContentTransformError,
};
pub use identity::{
    MAX_UI_KEY_BYTES, UiActionId, UiImageId, UiKey, UiKeyAtom, UiKeyError, UiLocalKey, UiRootKey,
};
pub use limits::{UiLimits, UiLimitsError};
pub use mesh::{UiMesh, UiMeshError, UiMeshVertex};
pub use style::{
    UiAlign, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges,
    UiFlexDirection, UiJustify, UiOverflow, UiPositionMode, UiPreparedText, UiRevision, UiStyle,
    UiStyleError, UiText, UiTextAlignment, UiTextOverflow, UiTheme, UiThemePalette, UiVisual,
};
pub use tree::{
    UiAuthoringError, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiInteraction, UiLayer,
    UiNode, UiRoot, UiSemanticRole, UiSemantics,
};
