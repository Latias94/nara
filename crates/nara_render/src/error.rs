use nara_window::WindowId;

use crate::{Extent2d, RenderViewId};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderFramePacketError {
    #[error("one target transaction supports at most {maximum} views, but captured {actual}")]
    TooManyViews { actual: usize, maximum: usize },
    #[error("captured views repeat View lifetime identity {id:?}")]
    DuplicateViewId { id: RenderViewId },
    #[error("one target transaction cannot contain views for different targets")]
    MixedRenderTargets,
    #[error("captured views must retain their extracted stable order")]
    UnorderedViews,
    #[error("the first view must initialize its target with a clear")]
    UninitializedTarget,
    #[error("the desktop renderer supports exactly one window, but captured {actual}")]
    UnsupportedWindowCount { actual: usize },
    #[error("an image target requires an explicit physical target descriptor")]
    MissingImageTargetDescriptor,
    #[error("the image target descriptor does not match the extracted target")]
    ImageTargetMismatch,
    #[error("an image render target must have a non-zero physical extent")]
    ImageTargetHasZeroExtent,
    #[error("an Editor preview render packet must target an image")]
    EditorPreviewRequiresImageTarget,
    #[error("image adaptation requires an original window render target")]
    ImageAdaptationRequiresWindowTarget,
    #[error("the image extent {image:?} does not match captured physical window extent {window:?}")]
    ImageAdaptationExtentMismatch { window: Extent2d, image: Extent2d },
    #[error("render target window {window_id:?} is not the admitted window")]
    TargetWindowMismatch { window_id: WindowId },
    #[error("a render viewport lies outside its physical target")]
    ViewportOutsideTarget,
    #[error("a fixed world canvas requires a window target")]
    WorldCanvasRequiresWindowTarget,
    #[error("the fixed world canvas cannot map into the captured window geometry")]
    InvalidWorldCanvasMapping,
}
