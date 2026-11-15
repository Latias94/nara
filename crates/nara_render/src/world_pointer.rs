use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use nara_app::RuntimeGeneration;
use nara_core::{Mat3, Vec2};
use nara_ecs::{Entity, Query, Res, Resource, system::SystemParam};
use nara_window::{Window, WindowEvent, WindowEvents};

use crate::{
    DisplayMapping2d, DisplayRect, DisplayScale, ExtractedView, RenderFramePacket,
    RenderFrameSourceGeneration, RenderFrameTarget, RenderWindowPacket, WorldCanvas,
};

const PREPARED: u8 = 0;
const PRESENTED: u8 = 1;
const REVOKED: u8 = 2;

/// A frozen affine inverse from final-target physical pixels to original-target physical pixels.
///
/// Policies declare this value with their original capture. Identity explicitly preserves pixel
/// coordinates; successful rendering alone does not establish an input mapping.
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

    /// Accepts only finite, invertible affine matrices with an exact bottom row of `[0, 0, 1]`.
    /// The inverse must also be finite; no approximate singularity threshold is applied.
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

/// Read-only world picking through the last successfully presented camera and display mapping.
///
/// Positions are native-window logical points, as supplied by the input timeline. Bars and maximum
/// edges are excluded. A resize, canvas change, backend retirement or runtime replacement revokes
/// conversion until the matching new frame is presented. This parameter does not consume input or
/// override UI routing; gameplay still respects the input route's disposition.
#[derive(SystemParam)]
pub struct WorldPointer<'w, 's> {
    presented: Option<Res<'w, PresentedWorldFrame>>,
    windows: Query<'w, 's, &'static Window>,
    canvas: Option<Res<'w, WorldCanvas>>,
    generation: Option<Res<'w, RuntimeGeneration>>,
}

impl WorldPointer<'_, '_> {
    #[must_use]
    pub fn window_to_world(&self, camera: Entity, logical_position: Vec2) -> Option<Vec2> {
        let presented = self.presented.as_deref()?;
        if presented.live.load(Ordering::Acquire) != PRESENTED {
            return None;
        }
        if presented.source_generation.runtime_generation() != self.generation.as_deref().copied()
            || presented.canvas != self.canvas.as_deref().copied()
            || !self.windows.iter().any(|window| {
                window.id == presented.window.id
                    && window.resolution == presented.window.resolution
                    && !window.resolution.is_zero_sized()
            })
        {
            presented.live.store(REVOKED, Ordering::Release);
            return None;
        }
        presented.window_to_world(camera, logical_position)
    }
}

/// Platform events carry lifecycle changes even when the final component geometry returns to its
/// old value within one batch. Revoke before ordinary Update systems can observe that old image.
pub(crate) fn invalidate_window_presentation(
    events: Option<Res<WindowEvents>>,
    presented: Option<Res<PresentedWorldFrame>>,
) {
    if let (Some(events), Some(presented)) = (events, presented)
        && events.as_slice().iter().any(|event| {
            event.window_id() == presented.window.id
                && matches!(
                    event,
                    WindowEvent::Resized { .. }
                        | WindowEvent::ScaleFactorChanged { .. }
                        | WindowEvent::Closed { .. }
                )
        })
    {
        presented.live.store(REVOKED, Ordering::Release);
    }
}

/// Backend-integration state. Construction alone confers no presented-frame authority.
#[doc(hidden)]
#[derive(Debug, Resource)]
pub struct PresentedWorldFrame {
    window: RenderWindowPacket,
    source_generation: RenderFrameSourceGeneration,
    frame_index: u64,
    canvas: Option<WorldCanvas>,
    display_mapping: DisplayMapping2d,
    input_mapping: WorldInputMapping2d,
    views: Vec<ExtractedView>,
    live: Arc<AtomicU8>,
}

impl PresentedWorldFrame {
    #[must_use]
    pub const fn window(&self) -> RenderWindowPacket {
        self.window
    }

    #[must_use]
    pub const fn source_generation(&self) -> RenderFrameSourceGeneration {
        self.source_generation
    }

    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    fn window_to_world(&self, camera: Entity, position: Vec2) -> Option<Vec2> {
        let resolution = self.window.resolution;
        let contains_target = |point: Vec2| {
            point.is_finite()
                && point.x >= 0.0
                && point.y >= 0.0
                && point.x < resolution.physical_width as f32
                && point.y < resolution.physical_height as f32
        };
        let final_physical = position * resolution.scale_factor as f32;
        if !contains_target(final_physical) {
            return None;
        }
        let source_physical = self
            .input_mapping
            .final_to_source_physical()
            .transform_point2(final_physical);
        if !contains_target(source_physical) {
            return None;
        }
        let point = if self.canvas.is_some() {
            let source_logical = source_physical / self.display_mapping.pixels_per_point();
            let rect = self.display_mapping.content_rect();
            if source_logical.x < rect.min[0]
                || source_logical.y < rect.min[1]
                || source_logical.x >= rect.max[0]
                || source_logical.y >= rect.max[1]
            {
                return None;
            }
            Vec2::from_array(
                self.display_mapping
                    .panel_to_target(source_logical.to_array())?,
            )
        } else {
            source_physical
        };
        let view = self
            .views
            .iter()
            .find(|view| view.camera_entity == camera)?;
        let viewport = view.viewport;
        if point.x < viewport.physical_x as f32
            || point.y < viewport.physical_y as f32
            || point.x
                >= (u64::from(viewport.physical_x) + u64::from(viewport.physical_width)) as f32
            || point.y
                >= (u64::from(viewport.physical_y) + u64::from(viewport.physical_height)) as f32
        {
            return None;
        }
        view.viewport_to_world(point)
            .filter(|world| world.is_finite())
    }
}

/// The execution owner retains this guard for exactly the displayed backend/window lifetime.
///
/// Activate only after the matching surface present (or exact Editor composite) succeeds. Drop or
/// invalidate it on target retirement, suspension, namespace loss or replacement. A retained
/// `PresentedWorldFrame` cannot reactivate itself after this owner releases the guard.
#[doc(hidden)]
#[derive(Debug)]
pub struct WorldPresentationGuard {
    live: Arc<AtomicU8>,
}

impl WorldPresentationGuard {
    pub fn mark_presented(&self) {
        let _ =
            self.live
                .compare_exchange(PREPARED, PRESENTED, Ordering::AcqRel, Ordering::Acquire);
    }

    pub fn invalidate(&self) {
        self.live.store(REVOKED, Ordering::Release);
    }

    #[must_use]
    pub fn revoker(&self) -> WorldPresentationRevoker {
        WorldPresentationRevoker {
            live: Arc::clone(&self.live),
        }
    }
}

/// Backend callback access that can revoke a presentation but cannot activate or retain its owner.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct WorldPresentationRevoker {
    live: Arc<AtomicU8>,
}

impl WorldPresentationRevoker {
    pub fn invalidate(&self) {
        self.live.store(REVOKED, Ordering::Release);
    }
}

impl Drop for WorldPresentationGuard {
    fn drop(&mut self) {
        self.invalidate();
    }
}

/// Captures bounded camera facts before execution; publish and activate only after actual present.
#[doc(hidden)]
#[must_use]
pub fn prepare_world_presentation(
    packet: &RenderFramePacket,
    mapping: WorldInputMapping2d,
) -> Option<(PresentedWorldFrame, WorldPresentationGuard)> {
    let RenderFrameTarget::Window(window) = packet.target() else {
        return None;
    };
    let canvas = packet.world_canvas();
    let display_mapping = match canvas {
        Some(canvas) => canvas.mapping(),
        None => {
            let pixels_per_point = window.resolution.scale_factor as f32;
            DisplayMapping2d::fit(
                packet.target().extent(),
                DisplayRect {
                    min: [0.0, 0.0],
                    max: [
                        window.resolution.physical_width as f32 / pixels_per_point,
                        window.resolution.physical_height as f32 / pixels_per_point,
                    ],
                },
                pixels_per_point,
                DisplayScale::Fit,
            )?
        }
    };
    let live = Arc::new(AtomicU8::new(PREPARED));
    Some((
        PresentedWorldFrame {
            window,
            source_generation: packet.source_generation(),
            frame_index: packet.frame_index(),
            canvas: canvas.map(|canvas| canvas.canvas()),
            display_mapping,
            input_mapping: mapping,
            views: packet.views().to_vec(),
            live: Arc::clone(&live),
        },
        WorldPresentationGuard { live },
    ))
}

#[cfg(test)]
mod tests;
