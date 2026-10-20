use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use nara_app::RuntimeGeneration;
use nara_core::Vec2;
use nara_ecs::{Entity, Query, Res, Resource, system::SystemParam};
use nara_window::{Window, WindowEvent, WindowEvents};

use crate::{
    DisplayMapping2d, DisplayRect, DisplayScale, ExtractedView, RenderFramePacket,
    RenderFrameSourceGeneration, RenderFrameTarget, RenderWindowPacket, WorldCanvas,
};

const PREPARED: u8 = 0;
const PRESENTED: u8 = 1;
const REVOKED: u8 = 2;

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
    mapping: DisplayMapping2d,
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
        let rect = self.mapping.content_rect();
        if !position.is_finite()
            || position.x < rect.min[0]
            || position.y < rect.min[1]
            || position.x >= rect.max[0]
            || position.y >= rect.max[1]
        {
            return None;
        }
        let point = self.mapping.panel_to_target(position.to_array())?;
        let view = self
            .views
            .iter()
            .find(|view| view.camera_entity == camera)?;
        let viewport = view.viewport;
        if point[0] < viewport.physical_x as f32
            || point[1] < viewport.physical_y as f32
            || point[0]
                >= (u64::from(viewport.physical_x) + u64::from(viewport.physical_width)) as f32
            || point[1]
                >= (u64::from(viewport.physical_y) + u64::from(viewport.physical_height)) as f32
        {
            return None;
        }
        view.viewport_to_world(Vec2::from_array(point))
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
) -> Option<(PresentedWorldFrame, WorldPresentationGuard)> {
    let RenderFrameTarget::Window(window) = packet.target() else {
        return None;
    };
    let canvas = packet.world_canvas();
    let mapping = match canvas {
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
            mapping,
            views: packet.views().to_vec(),
            live: Arc::clone(&live),
        },
        WorldPresentationGuard { live },
    ))
}
