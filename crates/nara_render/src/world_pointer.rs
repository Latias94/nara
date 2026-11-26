use std::sync::Arc;

use nara_core::{Mat3, Vec2};
use nara_ecs::{Commands, Entity, Query, Res, Resource, SystemParam, World};
use nara_window::{Window, WindowEvent, WindowEvents};

use crate::{
    ExtractedView, RenderFramePacket, RenderFrameTarget, RenderViewId, RenderWindowPacket,
    WorldCanvas, WorldCanvasFrame,
};

/// A frozen affine inverse from final-target physical pixels to original-target physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldInputMapping2d {
    final_to_source_physical: Mat3,
}

impl WorldInputMapping2d {
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            final_to_source_physical: Mat3::IDENTITY,
        }
    }

    /// Accepts finite affine matrices with a finite inverse and a bottom row of `[0, 0, 1]`.
    #[must_use]
    pub fn affine(final_to_source_physical: Mat3) -> Option<Self> {
        if !final_to_source_physical.is_finite()
            || final_to_source_physical.x_axis.z != 0.0
            || final_to_source_physical.y_axis.z != 0.0
            || final_to_source_physical.z_axis.z != 1.0
        {
            return None;
        }
        let determinant = final_to_source_physical.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return None;
        }
        final_to_source_physical.try_inverse().map(|_| Self {
            final_to_source_physical,
        })
    }

    #[must_use]
    pub const fn final_to_source_physical(self) -> Mat3 {
        self.final_to_source_physical
    }
}

/// Read-only world picking through the last successfully presented view.
///
/// Input coordinates are native-window logical points. Canvas bars and maximum edges are
/// excluded. Resizing or retiring the target clears the mapping until the next present.
#[derive(SystemParam)]
pub struct WorldPointer<'w, 's> {
    presented: Option<Res<'w, PresentedWorldFrame>>,
    windows: Query<'w, 's, &'static Window>,
    canvas: Option<Res<'w, WorldCanvas>>,
}

impl WorldPointer<'_, '_> {
    /// Picks the first presented subview of `camera` that contains the point.
    #[must_use]
    pub fn window_to_world(&self, camera: Entity, logical_position: Vec2) -> Option<Vec2> {
        self.presented()?
            .window_to_world(logical_position, |view| view.camera_entity == camera)
    }

    /// Picks through one exact view of the last presented frame if it contains the point.
    #[must_use]
    pub fn window_to_world_in_view(
        &self,
        view_id: RenderViewId,
        logical_position: Vec2,
    ) -> Option<Vec2> {
        self.presented()?
            .window_to_world(logical_position, |view| view.id == view_id)
    }

    fn presented(&self) -> Option<&PresentedWorldFrame> {
        let frame = self.presented.as_deref()?;
        frame
            .matches_geometry(&self.windows, self.canvas.as_deref().copied())
            .then_some(frame)
    }
}

#[derive(Debug, Resource)]
pub(crate) struct PresentedWorldFrame {
    window: RenderWindowPacket,
    canvas: Option<WorldCanvasFrame>,
    input_mapping: WorldInputMapping2d,
    views: Arc<[ExtractedView]>,
}

impl PresentedWorldFrame {
    fn matches_geometry(&self, windows: &Query<&Window>, canvas: Option<WorldCanvas>) -> bool {
        self.canvas.map(WorldCanvasFrame::canvas) == canvas
            && windows.iter().any(|window| {
                window.id == self.window.id
                    && window.resolution == self.window.resolution
                    && !window.resolution.is_zero_sized()
            })
    }

    fn window_to_world(
        &self,
        position: Vec2,
        matches_view: impl Fn(&ExtractedView) -> bool,
    ) -> Option<Vec2> {
        let resolution = self.window.resolution;
        let scale = resolution.scale_factor as f32;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let extent = Vec2::new(
            resolution.physical_width as f32,
            resolution.physical_height as f32,
        );
        let final_physical = position * scale;
        if !contains(final_physical, Vec2::ZERO, extent) {
            return None;
        }
        let source_physical = self
            .input_mapping
            .final_to_source_physical()
            .transform_point2(final_physical);
        if !contains(source_physical, Vec2::ZERO, extent) {
            return None;
        }
        let raster_point = if let Some(canvas) = self.canvas {
            let mapping = canvas.mapping();
            let logical = source_physical / mapping.pixels_per_point();
            let rect = mapping.content_rect();
            if !contains(
                logical,
                Vec2::from_array(rect.min),
                Vec2::from_array(rect.max),
            ) {
                return None;
            }
            Vec2::from_array(mapping.panel_to_target(logical.to_array())?)
        } else {
            source_physical
        };
        let view = self.views.iter().find(|view| {
            let viewport = view.viewport;
            let min = Vec2::new(viewport.physical_x as f32, viewport.physical_y as f32);
            let max = Vec2::new(
                (u64::from(viewport.physical_x) + u64::from(viewport.physical_width)) as f32,
                (u64::from(viewport.physical_y) + u64::from(viewport.physical_height)) as f32,
            );
            matches_view(view) && contains(raster_point, min, max)
        })?;
        view.viewport_to_world(raster_point)
            .filter(|point| point.is_finite())
    }
}

fn contains(point: Vec2, min: Vec2, max: Vec2) -> bool {
    point.cmpge(min).all() && point.cmplt(max).all()
}

/// Publishes the camera mapping after the host successfully presents this window packet.
///
/// Pass `None` when the rendered output has no declared inverse mapping. This and a successful
/// overlay with no world views clear picking. Offscreen image packets leave window picking alone.
pub fn publish_presented_world_frame(
    world: &mut World,
    packet: &RenderFramePacket,
    mapping: Option<WorldInputMapping2d>,
) {
    let RenderFrameTarget::Window(window) = packet.target() else {
        return;
    };
    if let Some(input_mapping) = mapping.filter(|_| !packet.views().is_empty()) {
        world.insert_resource(PresentedWorldFrame {
            window,
            canvas: packet.world_canvas(),
            input_mapping,
            views: packet.shared_views(),
        });
    } else {
        clear_presented_world_frame(world);
    }
}

/// Clears picking when the host suspends, closes, or otherwise retires the displayed target.
pub fn clear_presented_world_frame(world: &mut World) {
    world.remove_resource::<PresentedWorldFrame>();
}

pub(crate) fn invalidate_window_presentation(
    events: Option<Res<WindowEvents>>,
    presented: Option<Res<PresentedWorldFrame>>,
    windows: Query<&Window>,
    canvas: Option<Res<WorldCanvas>>,
    mut commands: Commands,
) {
    if let Some(frame) = presented
        && (!frame.matches_geometry(&windows, canvas.as_deref().copied())
            || events.is_some_and(|events| {
                events.as_slice().iter().any(|event| {
                    event.window_id() == frame.window.id
                        && matches!(
                            event,
                            WindowEvent::Resized { .. }
                                | WindowEvent::ScaleFactorChanged { .. }
                                | WindowEvent::Closed { .. }
                        )
                })
            }))
    {
        commands.remove_resource::<PresentedWorldFrame>();
    }
}

#[cfg(test)]
mod tests;
