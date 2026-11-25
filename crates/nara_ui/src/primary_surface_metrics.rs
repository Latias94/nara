use nara_ecs::Resource;

use crate::{UiSafeAreaInsets, UiSize, UiSurfaceContentTransform, UiSurfaceId};

/// Authored geometry for one primary UI surface, independent of its host target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct UiPrimarySurfaceMetrics {
    surface_id: UiSurfaceId,
    logical_width: u32,
    logical_height: u32,
    safe_area: [u32; 4],
}

impl UiPrimarySurfaceMetrics {
    /// Records the authored size and logical safe-area insets in left, top, right, bottom order.
    #[must_use]
    pub const fn new(
        surface_id: UiSurfaceId,
        logical_width: u32,
        logical_height: u32,
        safe_area: [u32; 4],
    ) -> Self {
        Self {
            surface_id,
            logical_width,
            logical_height,
            safe_area,
        }
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub fn logical_size(self) -> Option<UiSize> {
        UiSize::new(self.logical_width as f32, self.logical_height as f32)
    }

    /// Fits the authored surface at a platform scale of one, applying its authored safe area.
    #[must_use]
    pub fn direct_transform(self) -> Option<UiSurfaceContentTransform> {
        let logical_size = self.logical_size()?;
        let safe_area = self.authored_safe_area(1.0, UiSafeAreaInsets::default())?;
        UiSurfaceContentTransform::new(logical_size, logical_size, safe_area, 1.0).ok()
    }

    /// Fits the authored surface using the larger authored or host inset on each edge.
    #[must_use]
    pub fn target_transform(
        self,
        physical_size: UiSize,
        host_safe_area: UiSafeAreaInsets,
        platform_scale_factor: f32,
    ) -> Option<UiSurfaceContentTransform> {
        let logical_size = self.logical_size()?;
        let authored_scale = (physical_size.width() / logical_size.width())
            .min(physical_size.height() / logical_size.height());
        if !authored_scale.is_finite() || authored_scale <= 0.0 {
            return None;
        }
        let safe_area = self.authored_safe_area(authored_scale, host_safe_area)?;
        UiSurfaceContentTransform::new(
            logical_size,
            physical_size,
            safe_area,
            platform_scale_factor,
        )
        .ok()
    }

    fn authored_safe_area(
        self,
        authored_scale: f32,
        host_safe_area: UiSafeAreaInsets,
    ) -> Option<UiSafeAreaInsets> {
        let [left, top, right, bottom] = self.safe_area;
        UiSafeAreaInsets::new(
            host_safe_area.left().max(left as f32 * authored_scale),
            host_safe_area.top().max(top as f32 * authored_scale),
            host_safe_area.right().max(right as f32 * authored_scale),
            host_safe_area.bottom().max(bottom as f32 * authored_scale),
        )
    }
}
