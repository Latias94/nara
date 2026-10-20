use crate::Extent2d;

/// A rectangle in logical panel coordinates, or normalized image coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl DisplayRect {
    pub const UNIT: Self = Self {
        min: [0.0, 0.0],
        max: [1.0, 1.0],
    };

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.min.into_iter().chain(self.max).all(f32::is_finite)
            && self.max[0] > self.min[0]
            && self.max[1] > self.min[1]
    }

    #[must_use]
    pub fn contains(self, point: [f32; 2]) -> bool {
        point.into_iter().enumerate().all(|(axis, value)| {
            value.is_finite() && value >= self.min[axis] && value <= self.max[axis]
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayScale {
    #[default]
    Fit,
    /// Use whole physical pixels per target pixel when the panel can fit the target.
    /// Smaller panels use a fractional downscale so the complete image remains visible.
    IntegerFit,
}

/// Sampling of the completed target while it is scaled into a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplaySampling {
    Linear,
    Nearest,
}

impl DisplayScale {
    #[must_use]
    pub const fn sampling(self) -> DisplaySampling {
        match self {
            Self::Fit => DisplaySampling::Linear,
            Self::IntegerFit => DisplaySampling::Nearest,
        }
    }
}

/// One shared forward/inverse mapping between a rendered target and its displayed image.
///
/// Coordinates are logical panel points; integer scaling is measured in physical pixels.
/// This policy never changes simulation or camera coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayMapping2d {
    target: Extent2d,
    panel: DisplayRect,
    content: DisplayRect,
    uv: DisplayRect,
    pixels_per_point: f32,
}

impl DisplayMapping2d {
    #[must_use]
    pub fn fit(
        target: Extent2d,
        panel: DisplayRect,
        pixels_per_point: f32,
        scale: DisplayScale,
    ) -> Option<Self> {
        if target.is_empty()
            || !panel.is_valid()
            || !pixels_per_point.is_finite()
            || pixels_per_point <= 0.0
        {
            return None;
        }
        let available = [
            (panel.max[0] - panel.min[0]) * pixels_per_point,
            (panel.max[1] - panel.min[1]) * pixels_per_point,
        ];
        let fit = (available[0] / target.width as f32).min(available[1] / target.height as f32);
        let fit = if scale == DisplayScale::IntegerFit && fit >= 1.0 {
            fit.floor()
        } else {
            fit
        };
        let physical_size = [target.width as f32 * fit, target.height as f32 * fit];
        let mut physical_min = [
            panel.min[0] * pixels_per_point + (available[0] - physical_size[0]) * 0.5,
            panel.min[1] * pixels_per_point + (available[1] - physical_size[1]) * 0.5,
        ];
        if scale == DisplayScale::IntegerFit && fit >= 1.0 {
            physical_min = physical_min.map(f32::round);
        }
        let content = DisplayRect {
            min: physical_min.map(|point| point / pixels_per_point),
            max: [
                (physical_min[0] + physical_size[0]) / pixels_per_point,
                (physical_min[1] + physical_size[1]) / pixels_per_point,
            ],
        };
        content.is_valid().then_some(Self {
            target,
            panel,
            content,
            uv: DisplayRect::UNIT,
            pixels_per_point,
        })
    }

    #[must_use]
    pub fn with_uv(mut self, uv: DisplayRect) -> Option<Self> {
        if !uv.is_valid()
            || !DisplayRect::UNIT.contains(uv.min)
            || !DisplayRect::UNIT.contains(uv.max)
        {
            return None;
        }
        self.uv = uv;
        Some(self)
    }

    #[must_use]
    pub const fn target(self) -> Extent2d {
        self.target
    }
    #[must_use]
    pub const fn content_rect(self) -> DisplayRect {
        self.content
    }
    #[must_use]
    pub const fn panel_rect(self) -> DisplayRect {
        self.panel
    }
    #[must_use]
    pub const fn uv_rect(self) -> DisplayRect {
        self.uv
    }
    #[must_use]
    pub const fn pixels_per_point(self) -> f32 {
        self.pixels_per_point
    }

    #[must_use]
    pub fn panel_to_uv(self, point: [f32; 2]) -> Option<[f32; 2]> {
        self.content.contains(point).then(|| {
            std::array::from_fn(|axis| {
                self.uv.min[axis]
                    + (point[axis] - self.content.min[axis])
                        / (self.content.max[axis] - self.content.min[axis])
                        * (self.uv.max[axis] - self.uv.min[axis])
            })
        })
    }

    #[must_use]
    pub fn uv_to_panel(self, uv: [f32; 2]) -> Option<[f32; 2]> {
        self.uv.contains(uv).then(|| {
            std::array::from_fn(|axis| {
                self.content.min[axis]
                    + (uv[axis] - self.uv.min[axis]) / (self.uv.max[axis] - self.uv.min[axis])
                        * (self.content.max[axis] - self.content.min[axis])
            })
        })
    }

    #[must_use]
    pub fn panel_to_target(self, point: [f32; 2]) -> Option<[f32; 2]> {
        self.panel_to_uv(point).map(|uv| {
            [
                uv[0] * self.target.width as f32,
                uv[1] * self.target.height as f32,
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_canvas_letterboxes_in_physical_pixels_and_inverse_rejects_bars() {
        assert_eq!(
            DisplayScale::IntegerFit.sampling(),
            DisplaySampling::Nearest
        );
        assert_eq!(DisplayScale::Fit.sampling(), DisplaySampling::Linear);
        let mapping = DisplayMapping2d::fit(
            Extent2d::new(320, 180).unwrap(),
            DisplayRect {
                min: [10.0, 20.0],
                max: [510.0, 320.0],
            },
            2.0,
            DisplayScale::IntegerFit,
        )
        .unwrap();
        assert_eq!(
            mapping.content_rect(),
            DisplayRect {
                min: [20.0, 35.0],
                max: [500.0, 305.0]
            }
        );
        assert_eq!(mapping.panel_to_target([260.0, 170.0]), Some([160.0, 90.0]));
        assert_eq!(mapping.panel_to_uv([15.0, 170.0]), None);
        assert_eq!(mapping.panel_to_uv([f32::NAN, 170.0]), None);
    }

    #[test]
    fn undersized_integer_canvas_downscales_and_crop_round_trips() {
        let mapping = DisplayMapping2d::fit(
            Extent2d::new(320, 180).unwrap(),
            DisplayRect {
                min: [0.0, 0.0],
                max: [80.0, 60.0],
            },
            2.0,
            DisplayScale::IntegerFit,
        )
        .unwrap()
        .with_uv(DisplayRect {
            min: [0.25, 0.25],
            max: [0.75, 0.75],
        })
        .unwrap();
        assert_eq!(
            mapping.content_rect(),
            DisplayRect {
                min: [0.0, 7.5],
                max: [80.0, 52.5]
            }
        );
        let panel = mapping.uv_to_panel([0.5, 0.5]).unwrap();
        assert_eq!(mapping.panel_to_uv(panel), Some([0.5, 0.5]));
        assert!(
            DisplayMapping2d::fit(
                Extent2d {
                    width: 0,
                    height: 1
                },
                DisplayRect::UNIT,
                1.0,
                DisplayScale::Fit
            )
            .is_none()
        );
    }
}
