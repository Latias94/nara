//! World-space editor overlays for a Nara render target.

mod geometry;
mod pipeline;

#[cfg(all(test, not(target_family = "wasm")))]
mod native_tests;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
};

use nara_core::{Color, Vec2};
use nara_render::{
    RenderViewId,
    gpu::{PassAttachments, RenderPass, RenderPassContext, WgpuRenderError},
};

use geometry::{Instance, ProjectedView};

/// A stroke in linear sRGB with straight alpha and a width in final target pixels.
/// Finite color channels are clamped to the unit interval. Empty strokes are skipped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayStroke {
    pub color: Color,
    pub width_px: f32,
}

impl OverlayStroke {
    #[must_use]
    pub const fn new(color: Color, width_px: f32) -> Self {
        Self { color, width_px }
    }
}

/// An outline in world coordinates. Draw order follows the frame's primitive order.
/// Use segments, quadrilaterals and circles for selection, collider, and Gizmo geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayPrimitive {
    /// A round-capped segment. Coincident endpoints make a round marker.
    Line {
        from: Vec2,
        to: Vec2,
        stroke: OverlayStroke,
    },
    /// Four perimeter corners in order. The first and last corners are connected.
    Rect {
        corners: [Vec2; 4],
        stroke: OverlayStroke,
    },
    Circle {
        center: Vec2,
        radius: f32,
        stroke: OverlayStroke,
    },
}

impl OverlayPrimitive {
    /// Builds an axis-aligned rectangle in world space.
    #[must_use]
    pub fn rectangle(min: Vec2, max: Vec2, stroke: OverlayStroke) -> Self {
        Self::Rect {
            corners: [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)],
            stroke,
        }
    }
}

/// A world-aligned grid anchored at `origin`. The visible spacing doubles as needed at low zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayGrid {
    pub origin: Vec2,
    pub spacing: f32,
    pub line: OverlayStroke,
    pub x_axis: OverlayStroke,
    pub y_axis: OverlayStroke,
}

impl Default for OverlayGrid {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            spacing: 32.0,
            line: OverlayStroke::new(Color::rgba(0.030, 0.038, 0.053, 0.85), 1.0),
            x_axis: OverlayStroke::new(Color::rgba(0.807, 0.309, 0.309, 0.5), 1.0),
            y_axis: OverlayStroke::new(Color::rgba(0.361, 0.631, 0.332, 0.5), 1.0),
        }
    }
}

/// Overlay content for one view. The grid is drawn first, followed by `primitives` in order.
/// An empty frame clears the overlay on the next render.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverlayFrame {
    /// `None` uses the last extracted view. An explicit ID absent from the packet draws nothing.
    pub view: Option<RenderViewId>,
    pub grid: Option<OverlayGrid>,
    pub primitives: Vec<OverlayPrimitive>,
}

/// A cloneable producer handle. Each render observes one whole published snapshot.
/// The caller schedules a render after publishing changes.
#[derive(Clone, Default)]
pub struct OverlayState(Arc<Mutex<Arc<OverlayFrame>>>);

impl OverlayState {
    pub fn update(&self, frame: OverlayFrame) {
        let frame = Arc::new(frame);
        let previous = std::mem::replace(
            &mut *self.0.lock().unwrap_or_else(PoisonError::into_inner),
            frame,
        );
        drop(previous);
    }

    fn snapshot(&self) -> Arc<OverlayFrame> {
        Arc::clone(&self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// Blends world-space overlay geometry over the renderer's final blendable color attachment.
/// Install it with [`nara_render::gpu::Renderer::set_editor_overlay`].
pub struct EditorOverlayPass {
    state: OverlayState,
    instances: Vec<Instance>,
    device: Option<wgpu::Device>,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl EditorOverlayPass {
    /// Creates a pass and its frame publisher.
    #[must_use]
    pub fn new() -> (Self, OverlayState) {
        let state = OverlayState::default();
        (Self::with_state(state.clone()), state)
    }

    /// Attaches a new pass to an existing publisher, for example when replacing a renderer.
    #[must_use]
    pub fn with_state(state: OverlayState) -> Self {
        Self {
            state,
            instances: Vec::new(),
            device: None,
            pipelines: HashMap::new(),
        }
    }
}

impl RenderPass for EditorOverlayPass {
    fn attachments(&self) -> PassAttachments {
        PassAttachments::OVERLAY
    }

    fn execute(&mut self, context: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        let frame = self.state.snapshot();
        if frame.grid.is_none() && frame.primitives.is_empty() {
            return Ok(());
        }
        let packet = context.frame.packet();
        let view = match frame.view {
            Some(id) => packet.views().iter().find(|view| view.id == id),
            None => packet.views().last(),
        };
        let Some(view) = view else {
            return Ok(());
        };
        let Some(projection) =
            ProjectedView::new(view, packet.world_canvas(), context.target.extent)
        else {
            return Ok(());
        };
        projection.collect(&frame, &mut self.instances);
        if self.instances.is_empty() {
            return Ok(());
        }

        let instance_count = u32::try_from(self.instances.len())
            .map_err(|_| WgpuRenderError::Capacity("editor overlay instances"))?;
        let buffer = context.upload_buffer(
            "nara_editor_overlay_instances",
            wgpu::BufferUsages::VERTEX,
            bytemuck::cast_slice(&self.instances),
        )?;
        if self.device.as_ref() != Some(context.device) {
            self.clear_caches();
            self.device = Some(context.device.clone());
        }
        let pipeline = self
            .pipelines
            .entry(context.target.format)
            .or_insert_with(|| pipeline::create(context.device, context.target.format));
        let [x, y, width, height] = projection.scissor;
        let mut pass = context.begin_color_pass("nara_editor_overlay", wgpu::LoadOp::Load);
        pass.set_scissor_rect(x, y, width, height);
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..4, 0..instance_count);
        Ok(())
    }

    fn clear_caches(&mut self) {
        self.pipelines.clear();
        self.device = None;
    }
}
