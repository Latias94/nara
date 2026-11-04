//! Renderer-facing values and optional runtime integration.
//!
//! The `runtime` feature enables App/ECS extraction, frame packets and plugins.

mod canvas;
mod error;
#[cfg(feature = "runtime")]
mod frame;
mod prepare;
#[cfg(test)]
mod prepare_tests;
mod presentation;
#[cfg(feature = "runtime")]
mod runtime;
mod target;
#[cfg(feature = "runtime")]
mod view;
mod view_id;
#[cfg(feature = "runtime")]
mod world_pointer;

pub use canvas::{WorldCanvas, WorldCanvasFrame};
pub use error::RenderFramePacketError;
#[cfg(feature = "runtime")]
pub use frame::{
    MAX_VIEWS_PER_TARGET, RenderFramePacket, RenderFrameSourceGeneration,
    build_image_render_frame_packet, build_render_frame_packet,
    build_render_frame_packet_with_world_canvas,
};
pub use nara_core::Color;
pub use prepare::{
    RenderResourceContentHash, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};
pub use presentation::{DisplayMapping2d, DisplayRect, DisplaySampling, DisplayScale};
#[cfg(feature = "runtime")]
pub use runtime::*;
pub use target::{
    Extent2d, RenderFrameTarget, RenderImage2d, RenderImagePacket, RenderTarget,
    RenderWindowPacket, ViewportRect,
};
#[cfg(feature = "runtime")]
pub use view::{
    Camera2d, ExtractedView, ExtractedViews, ViewLoadOp, ViewMapping2d, viewport_clip_to_physical,
    viewport_physical_to_clip,
};
pub use view_id::RenderViewId;
#[cfg(feature = "runtime")]
pub use world_pointer::WorldPointer;
