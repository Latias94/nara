use nara_core::Color;
use nara_ecs::Resource;

use crate::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d};

/// Optional fixed raster extent for the world, independent of the native window and runtime UI.
///
/// The finished world uses nearest sampling and integer physical scaling when it fits. Smaller
/// windows downscale the complete image fractionally. Simulation and camera coordinates remain
/// fractional. Insert this resource directly or supply it to [`crate::plugin`].
#[derive(Debug, Clone, Copy, PartialEq, Resource)]
pub struct WorldCanvas {
    extent: Extent2d,
    bar_color: Color,
}

impl WorldCanvas {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        match Extent2d::new(width, height) {
            Some(extent) => Some(Self {
                extent,
                bar_color: Color::BLACK,
            }),
            None => None,
        }
    }

    /// Selects the window clear behind the fitted world. Every channel must be finite.
    #[must_use]
    pub fn with_bar_color(mut self, color: Color) -> Option<Self> {
        if ![color.r, color.g, color.b, color.a]
            .into_iter()
            .all(f32::is_finite)
        {
            return None;
        }
        self.bar_color = color;
        Some(self)
    }

    #[must_use]
    pub const fn extent(self) -> Extent2d {
        self.extent
    }

    #[must_use]
    pub const fn bar_color(self) -> Color {
        self.bar_color
    }
}

/// Validated world-to-window composition facts captured with one immutable frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldCanvasFrame {
    canvas: WorldCanvas,
    mapping: DisplayMapping2d,
}

impl WorldCanvasFrame {
    pub(crate) fn new(
        canvas: WorldCanvas,
        resolution: nara_window::WindowResolution,
    ) -> Option<Self> {
        let pixels_per_point = resolution.scale_factor as f32;
        let mapping = DisplayMapping2d::fit(
            canvas.extent,
            DisplayRect {
                min: [0.0, 0.0],
                max: [
                    resolution.physical_width as f32 / pixels_per_point,
                    resolution.physical_height as f32 / pixels_per_point,
                ],
            },
            pixels_per_point,
            DisplayScale::IntegerFit,
        )?;
        Some(Self { canvas, mapping })
    }

    #[must_use]
    pub const fn extent(self) -> Extent2d {
        self.canvas.extent
    }

    /// Mapping in native-window logical points, with the captured physical pixels per point.
    #[must_use]
    pub const fn mapping(self) -> DisplayMapping2d {
        self.mapping
    }

    #[must_use]
    pub const fn bar_color(self) -> Color {
        self.canvas.bar_color
    }

    pub(crate) const fn canvas(self) -> WorldCanvas {
        self.canvas
    }
}
