use std::collections::{HashMap, HashSet};

use nara_asset::{AssetStates, Assets, image::ImageAsset};
use nara_ecs::World;
use nara_ui::{
    UiSurfaceId,
    kernel::{UiImageId, UiPaintPrimitive, UiResolvedBrush},
};

use super::HudFrame;
#[cfg(feature = "wgpu")]
use crate::RenderFramePacket;

#[cfg(feature = "wgpu")]
pub(crate) fn capture(world: &World, packet: &RenderFramePacket) -> Option<HudFrame> {
    let window = packet.source_window()?;
    let mut slots = nara_ui::active_ui_surface_slots(world)
        .ok()?
        .into_iter()
        .filter(|slot| slot.endpoint().window().get() == window.get());
    let slot = slots.next()?;
    if slots.next().is_some() {
        return None;
    }
    HudFrame::capture_surface(world, slot.surface_id())
}

impl HudFrame {
    /// Captures an explicitly selected surface, including for a direct image target.
    #[must_use]
    pub fn capture_surface(world: &World, surface: UiSurfaceId) -> Option<Self> {
        let frame = nara_ui::ui_candidate_frame(world, surface).ok()??;
        let fonts = nara_ui::presentation::ui_font_face(world)
            .cloned()
            .into_iter()
            .collect();
        let referenced = frame
            .paint()
            .iter()
            .filter_map(|primitive| match primitive {
                UiPaintPrimitive::Rect {
                    brush: Some(UiResolvedBrush::Image { image, .. }),
                    ..
                } => Some(image),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut images = HashMap::new();
        if !referenced.is_empty()
            && let Some(assets) = world.get_resource::<Assets<ImageAsset>>()
            && let Some(states) = world.get_resource::<AssetStates>()
        {
            for (handle, asset) in assets.iter() {
                let id = UiImageId::external::<ImageAsset>(handle.id().raw());
                if referenced.contains(&id)
                    && states.published_slot_revision(handle.id())
                        == Some(&assets.slot_revision(handle))
                {
                    images.insert(id, asset.clone());
                    if images.len() == referenced.len() {
                        break;
                    }
                }
            }
        }
        Some(Self {
            frame,
            images,
            fonts,
        })
    }
}
