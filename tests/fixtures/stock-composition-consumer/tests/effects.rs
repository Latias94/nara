#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

//! Finite format/alpha coverage for the two separately authored public effect packages.

use std::sync::Arc;

use nara::render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDefinition,
    WgpuProgramTextureDescriptor, WgpuRenderHost, WgpuRenderProgramBuilder,
};
use nara_stock_blur_effect::{Blur, BlurCapture};
use nara_stock_composition_consumer::{oracle, readback};
use nara_stock_grade_effect::{Grade, GradeCapture, GradeParameters};

struct Color;

#[test]
#[ignore = "requires an explicitly selected native GPU; compares all actual RGBA8/BGRA8 formats"]
fn independently_authored_effects_preserve_actual_format_transfer_and_premultiplied_alpha() {
    readback::require_native_selection();
    let parameters = GradeParameters {
        exposure_stops: 0.75,
        saturation: 0.4,
        contrast: 0.8,
    };
    let source_color = [0.2, 0.1, 0.05, 0.5];
    // The matrix, oracle and fixed encoded-byte tolerance are selected before GPU execution.
    let expected: Vec<_> = [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ]
    .into_iter()
    .map(|format| {
        let source = oracle::Image::solid(8, 6, format, source_color);
        let blur = source.blur(2);
        let grade = blur.grade(parameters);
        (source, blur, grade)
    })
    .collect();
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    for (source_expected, blur_expected, grade_expected) in expected {
        let producer = host.create_producer().unwrap();
        let blur = host
            .create_feature_instance(
                &producer,
                &WgpuFeatureDefinition::new(Blur::<Color, Color>::new()),
            )
            .unwrap();
        let grade = host
            .create_feature_instance(
                &producer,
                &WgpuFeatureDefinition::new(Grade::<Color, Color>::new()),
            )
            .unwrap();
        let format = source_expected.format;
        let descriptor = WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("external-effects-format-matrix"),
                size: wgpu::Extent3d {
                    width: 8,
                    height: 6,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            transfer: if format.is_srgb() {
                TextureTransfer::Srgb
            } else {
                TextureTransfer::Linear
            },
            alpha: TextureAlpha::Premultiplied,
        };
        let mut graph = WgpuRenderProgramBuilder::new(
            RenderFrameSourceGeneration::try_acquire().unwrap(),
            1,
            GraphLimits::default(),
        )
        .unwrap();
        let source = graph.texture::<Color>(descriptor.clone()).unwrap();
        let blurred = graph.texture::<Color>(descriptor.clone()).unwrap();
        let graded = graph.texture::<Color>(descriptor).unwrap();
        graph
            .initialize(
                1,
                &[source.clear(
                    wgpu::Color {
                        r: f64::from(source_color[0]),
                        g: f64::from(source_color[1]),
                        b: f64::from(source_color[2]),
                        a: f64::from(source_color[3]),
                    },
                    AccessRole::Attachment,
                )],
            )
            .unwrap();
        graph
            .operation(
                2,
                &blur,
                Arc::new(BlurCapture {
                    source: source.clone(),
                    destination: blurred.clone(),
                    radius: 2,
                }),
                &[source.read(AccessRole::Sampled)],
                &[blurred.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        graph
            .operation(
                3,
                &grade,
                Arc::new(GradeCapture {
                    source: blurred.clone(),
                    destination: graded.clone(),
                    parameters,
                }),
                &[blurred.read(AccessRole::Sampled)],
                &[graded.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        let program = graph
            .finish(&[source.output(), blurred.output(), graded.output()])
            .unwrap();
        let outputs = host.submit_program(&producer, &program).unwrap();
        source_expected.assert_matches(
            &readback::read(&mut host, &outputs.texture(&source).unwrap()),
            1,
            "native format source clear",
        );
        blur_expected.assert_matches(
            &readback::read(&mut host, &outputs.texture(&blurred).unwrap()),
            oracle::EFFECT_TOLERANCE,
            "native format half-float blur",
        );
        grade_expected.assert_matches(
            &readback::read(&mut host, &outputs.texture(&graded).unwrap()),
            oracle::EFFECT_TOLERANCE,
            "native format straight-color grade and premultiplication",
        );
        host.retire_producer(&producer);
    }
    readback::finish(&mut host);
}
