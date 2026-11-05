use nara::render::{DisplayMapping2d, DisplayRect};
use winit::window::Window;

use super::{EditorPanelImage, EditorPanelImageError, EditorPanelWindowGeometry};

impl EditorPanelWindowGeometry {
    pub(super) fn read(window: &Window) -> Self {
        let size = window.inner_size();
        Self {
            physical_size: [size.width, size.height],
            scale_factor: window.scale_factor(),
        }
    }

    /// Logical window dimensions corresponding to these exact native facts.
    #[must_use]
    pub fn logical_size(self) -> Option<[f32; 2]> {
        if self.physical_size.contains(&0)
            || !self.scale_factor.is_finite()
            || self.scale_factor <= 0.0
            || !(self.scale_factor as f32).is_finite()
            || self.scale_factor as f32 <= 0.0
        {
            return None;
        }
        let size = self
            .physical_size
            .map(|extent| (f64::from(extent) / self.scale_factor) as f32);
        size.iter()
            .all(|extent| extent.is_finite() && *extent > 0.0)
            .then_some(size)
    }

    pub(super) fn logical_position(self, physical: [f64; 2]) -> Option<[f32; 2]> {
        self.logical_size()?;
        let position = physical.map(|coordinate| (coordinate / self.scale_factor) as f32);
        position
            .iter()
            .all(|coordinate| coordinate.is_finite())
            .then_some(position)
    }
}

pub(super) fn validate_image<S>(
    image: &EditorPanelImage<S>,
    geometry: EditorPanelWindowGeometry,
) -> Result<(), EditorPanelImageError> {
    let size = geometry
        .logical_size()
        .ok_or(EditorPanelImageError::InvalidGeometry)?;
    let mapping = image.mapping;
    if mapping.target() != image.display.output().extent() {
        return Err(EditorPanelImageError::ExtentMismatch);
    }
    if mapping.uv_rect() != DisplayRect::UNIT {
        return Err(EditorPanelImageError::CroppedImage);
    }
    let clip = image.clip;
    let content = mapping.content_rect();
    if !mapping.panel_rect().is_valid()
        || !content.is_valid()
        || !clip.is_valid()
        || mapping.pixels_per_point() != geometry.scale_factor as f32
        || (0..2).any(|axis| {
            clip.min[axis] < 0.0
                || clip.max[axis] > size[axis]
                || clip.max[axis].min(content.max[axis]) <= clip.min[axis].max(content.min[axis])
        })
    {
        return Err(EditorPanelImageError::InvalidGeometry);
    }
    Ok(())
}

/// Extends the existing complete-image inverse affinely; capture may leave its content rectangle.
pub(super) fn target_position(
    mapping: DisplayMapping2d,
    logical: [f32; 2],
    extrapolate: bool,
) -> Option<[f32; 2]> {
    if let Some(point) = mapping.panel_to_target(logical) {
        return point.iter().all(|v| v.is_finite()).then_some(point);
    }
    if !extrapolate {
        return None;
    }
    let content = mapping.content_rect();
    let target = mapping.target();
    let extent = [target.width as f64, target.height as f64];
    let point = std::array::from_fn(|axis| {
        ((f64::from(logical[axis]) - f64::from(content.min[axis]))
            / (f64::from(content.max[axis]) - f64::from(content.min[axis]))
            * extent[axis]) as f32
    });
    point.iter().all(|v| v.is_finite()).then_some(point)
}

pub(super) fn hit<S>(image: &EditorPanelImage<S>, logical: [f32; 2]) -> bool {
    hit_mapping(image.mapping, image.clip, logical)
}

pub(super) fn hit_mapping(mapping: DisplayMapping2d, clip: DisplayRect, logical: [f32; 2]) -> bool {
    clip.contains(logical) && target_position(mapping, logical, false).is_some()
}
