//! CPU expectations fixed before native execution. Pixels use canonical RGBA byte order.

use nara_stock_grade_effect::GradeParameters;

/// Fixed before reading GPU results: 3 encoded channel values for GPU sRGB conversion,
/// UNORM rounding and float arithmetic. Opaque solid/HUD pixels are checked exactly.
pub const EFFECT_TOLERANCE: u8 = 3;

#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub pixels: Vec<[u8; 4]>,
}

impl Image {
    /// Builds a bounded oracle from predetermined linear-premultiplied pixels. Native Editor
    /// tests can use the captured public View/hit geometry instead of copying transfer math.
    pub fn from_linear_pixels(
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        pixels: impl IntoIterator<Item = [f32; 4]>,
    ) -> Self {
        assert!(width > 0 && width <= 2048 && height > 0 && height <= 2048);
        let count = (width * height) as usize;
        let pixels: Vec<_> = pixels
            .into_iter()
            .take(count + 1)
            .map(|pixel| encode(pixel, format))
            .collect();
        assert_eq!(
            pixels.len(),
            count,
            "one predetermined pixel per physical texel"
        );
        Self {
            width,
            height,
            format,
            pixels,
        }
    }

    pub fn solid(width: u32, height: u32, format: wgpu::TextureFormat, linear: [f32; 4]) -> Self {
        Self {
            width,
            height,
            format,
            pixels: vec![encode(linear, format); (width * height) as usize],
        }
    }

    pub fn scene(width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    if x >= width / 4 && x < width * 3 / 4 && y >= height / 3 && y < height * 2 / 3
                    {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }
                })
            })
            .collect();
        Self {
            width,
            height,
            format,
            pixels,
        }
    }

    pub fn translucent_scene(
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        tint: [f32; 4],
    ) -> Self {
        let premultiplied = [
            tint[0] * tint[3],
            tint[1] * tint[3],
            tint[2] * tint[3],
            tint[3],
        ];
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    if x >= width / 4 && x < width * 3 / 4 && y >= height / 3 && y < height * 2 / 3
                    {
                        encode(premultiplied, format)
                    } else {
                        [0; 4]
                    }
                })
            })
            .collect();
        Self {
            width,
            height,
            format,
            pixels,
        }
    }

    pub fn convert(&self, format: wgpu::TextureFormat) -> Self {
        let pixels = (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| encode(self.linear_pixel(x, y), format)))
            .collect();
        Self {
            width: self.width,
            height: self.height,
            format,
            pixels,
        }
    }

    pub fn linear_pixel(&self, x: u32, y: u32) -> [f32; 4] {
        let bytes = self.pixels[(y * self.width + x) as usize];
        std::array::from_fn(|channel| {
            let value = f32::from(bytes[channel]) / 255.0;
            if channel < 3 && self.format.has_srgb_suffix() {
                srgb_decode(value)
            } else {
                value
            }
        })
    }

    pub fn blur(&self, radius: u32) -> Self {
        let taps = (2 * radius + 1) as f32;
        let mut horizontal = vec![[0.0; 4]; self.pixels.len()];
        for y in 0..self.height {
            for x in 0..self.width {
                let mut sum = [0.0; 4];
                for offset in -(radius as i32)..=radius as i32 {
                    let sx = (x as i32 + offset).clamp(0, self.width as i32 - 1) as u32;
                    let sample = self.linear_pixel(sx, y);
                    for channel in 0..4 {
                        sum[channel] += sample[channel];
                    }
                }
                horizontal[(y * self.width + x) as usize] =
                    sum.map(|value| half::f16::from_f32(value / taps).to_f32());
            }
        }
        let mut result = self.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let mut sum = [0.0; 4];
                for offset in -(radius as i32)..=radius as i32 {
                    let sy = (y as i32 + offset).clamp(0, self.height as i32 - 1) as u32;
                    let sample = horizontal[(sy * self.width + x) as usize];
                    for channel in 0..4 {
                        sum[channel] += sample[channel];
                    }
                }
                result.pixels[(y * self.width + x) as usize] =
                    encode(sum.map(|value| value / taps), self.format);
            }
        }
        result
    }

    pub fn grade(&self, parameters: GradeParameters) -> Self {
        let mut result = self.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let rgba = self.linear_pixel(x, y);
                let graded = if rgba[3] == 0.0 {
                    [0.0; 4]
                } else {
                    let exposed: [f32; 3] = std::array::from_fn(|channel| {
                        rgba[channel] / rgba[3] * parameters.exposure_stops.exp2()
                    });
                    let luminance = exposed[0] * 0.2126 + exposed[1] * 0.7152 + exposed[2] * 0.0722;
                    let rgb = exposed.map(|channel| {
                        let saturated = luminance + (channel - luminance) * parameters.saturation;
                        ((saturated - 0.5) * parameters.contrast + 0.5).clamp(0.0, 1.0) * rgba[3]
                    });
                    [rgb[0], rgb[1], rgb[2], rgba[3]]
                };
                result.pixels[(y * self.width + x) as usize] = encode(graded, self.format);
            }
        }
        result
    }

    pub fn resolve(&self, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        // This fixture deliberately uses an exact 4x fit without bars. Canvas mapping itself
        // remains stock-owned; this independent oracle models only this fixed accepted shape.
        assert_eq!(width % self.width, 0);
        assert_eq!(height % self.height, 0);
        assert_eq!(width / self.width, height / self.height);
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    encode(
                        self.linear_pixel(x * self.width / width, y * self.height / height),
                        format,
                    )
                })
            })
            .collect();
        Self {
            width,
            height,
            format,
            pixels,
        }
    }

    pub fn hud(&self) -> Self {
        let mut result = self.clone();
        for y in 1..self.height - 1 {
            result.pixels[(y * self.width + 1) as usize] = [255; 4];
        }
        result
    }

    pub fn assert_matches(&self, actual: &Self, tolerance: u8, label: &str) {
        assert_eq!(
            (actual.width, actual.height, actual.format),
            (self.width, self.height, self.format),
            "{label}"
        );
        assert_eq!(actual.pixels.len(), self.pixels.len());
        for (index, (actual, expected)) in actual.pixels.iter().zip(&self.pixels).enumerate() {
            for channel in 0..4 {
                assert!(
                    actual[channel].abs_diff(expected[channel]) <= tolerance,
                    "{label} ({}, {}) channel {channel}: {actual:?} != {expected:?}, tolerance={tolerance}",
                    index as u32 % self.width,
                    index as u32 / self.width
                );
            }
        }
    }

    /// Transparent regions have an exact oracle even when nonzero effect pixels admit rounding.
    pub fn assert_transparent_regions(&self, actual: &Self, label: &str) {
        assert_eq!(self.pixels.len(), actual.pixels.len());
        let mut count = 0;
        for (index, (expected, actual)) in self.pixels.iter().zip(&actual.pixels).enumerate() {
            if expected[3] == 0 {
                assert_eq!(*expected, [0; 4], "CPU transparent black: {label}");
                assert_eq!(
                    *actual,
                    [0; 4],
                    "{label} alpha-zero pixel ({}, {}) must be exact transparent black",
                    index as u32 % self.width,
                    index as u32 / self.width
                );
                count += 1;
            }
        }
        assert!(
            count > 0,
            "{label} must actually exercise an alpha-zero region"
        );
    }
}

fn encode(linear: [f32; 4], format: wgpu::TextureFormat) -> [u8; 4] {
    std::array::from_fn(|channel| {
        let value = if channel < 3 && format.has_srgb_suffix() {
            srgb_encode(linear[channel])
        } else {
            linear[channel]
        };
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    })
}

fn srgb_decode(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_encode(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}
