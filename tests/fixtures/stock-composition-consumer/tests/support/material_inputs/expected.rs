use nara::material::{FilterMode, SamplerDescriptor};
use nara_stock_composition_consumer::oracle::Image;

use crate::scene;

use super::{
    GRADE,
    capture::{ACCENT, DEFAULT_PALETTE, SCALE, TINT},
};

pub struct Expected {
    pub palette: Image,
    pub branch: Image,
    pub world: Image,
}

impl Expected {
    pub fn new(
        colors: [[f32; 4]; 4],
        format: wgpu::TextureFormat,
        sampler: SamplerDescriptor,
    ) -> Self {
        let palette = Image::from_linear_pixels(2, 2, format, colors);
        let branch = palette.grade(GRADE);
        let world = world(&palette, sampler);
        Self {
            palette,
            branch,
            world,
        }
    }

    pub fn ordinary(sampler: SamplerDescriptor) -> Image {
        world(
            &Image {
                width: 1,
                height: 1,
                format: wgpu::TextureFormat::Rgba8Unorm,
                pixels: vec![DEFAULT_PALETTE],
            },
            sampler,
        )
    }
}

fn world(palette: &Image, sampler: SamplerDescriptor) -> Image {
    Image::from_linear_pixels(
        scene::WIDTH,
        scene::HEIGHT,
        wgpu::TextureFormat::Rgba8Unorm,
        (0..scene::HEIGHT).flat_map(|y| {
            (0..scene::WIDTH).map(move |x| {
                if !(8..24).contains(&x) || !(8..16).contains(&y) {
                    return [0.0; 4];
                }
                let uv = [(x as f32 - 7.5) / 16.0, (y as f32 - 7.5) / 8.0];
                let sample = sample(palette, uv, sampler.mag_filter);
                std::array::from_fn(|channel| {
                    if channel == 3 {
                        sample[3] * TINT[3]
                    } else {
                        let accent = f32::from(ACCENT[channel]) / 255.0;
                        sample[channel] * accent * accent * SCALE[channel] * TINT[channel] * TINT[3]
                    }
                })
            })
        }),
    )
}

fn sample(image: &Image, uv: [f32; 2], filter: FilterMode) -> [f32; 4] {
    let texel = |x: i32, y: i32| {
        image.linear_pixel(
            x.clamp(0, image.width as i32 - 1) as u32,
            y.clamp(0, image.height as i32 - 1) as u32,
        )
    };
    if filter == FilterMode::Nearest {
        return texel(
            (uv[0] * image.width as f32).floor() as i32,
            (uv[1] * image.height as f32).floor() as i32,
        );
    }
    let position = [
        uv[0] * image.width as f32 - 0.5,
        uv[1] * image.height as f32 - 0.5,
    ];
    let base = position.map(|coordinate| coordinate.floor() as i32);
    let fraction = [position[0] - base[0] as f32, position[1] - base[1] as f32];
    let [a, b, c, d] = [
        texel(base[0], base[1]),
        texel(base[0] + 1, base[1]),
        texel(base[0], base[1] + 1),
        texel(base[0] + 1, base[1] + 1),
    ];
    std::array::from_fn(|channel| {
        let top = a[channel] * (1.0 - fraction[0]) + b[channel] * fraction[0];
        let bottom = c[channel] * (1.0 - fraction[0]) + d[channel] * fraction[0];
        top * (1.0 - fraction[1]) + bottom * fraction[1]
    })
}

#[test]
fn reference_world_pixels_distinguish_rgb_loss_sampler_and_srgb_transfer_errors() {
    use super::{PALETTE_A, PALETTE_B, TOLERANCE};

    let max_rgb_difference = |expected: &Image, incorrect: &Image| {
        expected
            .pixels
            .iter()
            .zip(&incorrect.pixels)
            .flat_map(|(expected, incorrect)| {
                (0..3).map(move |channel| expected[channel].abs_diff(incorrect[channel]))
            })
            .max()
            .unwrap()
    };
    for (name, colors) in [("A", PALETTE_A), ("B", PALETTE_B)] {
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let palette = Image::from_linear_pixels(2, 2, format, colors);
            let nearest = world(&palette, SamplerDescriptor::NEAREST_CLAMP);
            let linear = world(&palette, SamplerDescriptor::LINEAR_CLAMP);
            let sampler_signal = max_rgb_difference(&nearest, &linear);
            assert!(
                sampler_signal > TOLERANCE,
                "palette {name} {format:?}: wrong sampler differs by only {sampler_signal}"
            );
            for (sampler, expected) in [
                (SamplerDescriptor::NEAREST_CLAMP, &nearest),
                (SamplerDescriptor::LINEAR_CLAMP, &linear),
            ] {
                for channel in 0..3 {
                    let signal = expected
                        .pixels
                        .iter()
                        .map(|pixel| pixel[channel])
                        .max()
                        .unwrap();
                    assert!(
                        signal > TOLERANCE,
                        "palette {name} {format:?} {sampler:?}: dropping RGB channel {channel} differs by only {signal}"
                    );
                }
                if format.has_srgb_suffix() {
                    let mut encoded_as_linear = palette.clone();
                    encoded_as_linear.format = wgpu::TextureFormat::Rgba8Unorm;
                    let incorrect = world(&encoded_as_linear, sampler);
                    let transfer_signal = max_rgb_difference(expected, &incorrect);
                    assert!(
                        transfer_signal > TOLERANCE,
                        "palette {name} {sampler:?}: sRGB bytes read as linear differ by only {transfer_signal}"
                    );
                }
            }
        }
    }
}
