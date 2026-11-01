#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

//! Real translucent Scene capture, original-input replay and the independent effect format matrix.

use std::sync::Arc;

use nara::render::{RenderImage2d, RenderImagePacket};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureTransfer, WgpuFeatureDefinition, WgpuRenderHost,
    WgpuRenderProgramBuilder, WgpuWorldColor,
};
use nara_stock_blur_effect::{Blur, BlurCapture};
use nara_stock_composition_consumer::{CompositionPolicy, oracle, readback};
use nara_stock_grade_effect::{Grade, GradeCapture, GradeParameters};

#[path = "support/scene.rs"]
#[allow(dead_code)]
mod scene;

struct EffectColor;

#[test]
#[ignore = "requires an explicitly selected native GPU; replays a real translucent Sprite Scene"]
fn translucent_sprite_scene_blur_and_grade_keep_premultiplied_alpha_and_exact_transparent_regions()
{
    readback::require_native_selection();
    let original = scene::capture_translucent();
    let captured = original.captured::<CompositionPolicy>().unwrap();
    assert!(!captured.configuration.enabled);
    assert!(captured.stock.ui_packets().is_empty());
    assert!(original.packet().world_canvas().is_none());

    // Fix source, matrix, grade controls and tolerance before acquiring a native Host. The
    // original target is RGBA8Unorm; normal stock blending stores straight tint * alpha over
    // transparent black. Conversion then accounts for that original UNORM quantization.
    let source_expected = oracle::Image::translucent_scene(
        scene::WIDTH,
        scene::HEIGHT,
        wgpu::TextureFormat::Rgba8Unorm,
        scene::TRANSLUCENT_TINT,
    );
    let parameters = GradeParameters {
        exposure_stops: 0.75,
        saturation: 0.4,
        contrast: 0.8,
    };
    let expected: Vec<_> = [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ]
    .into_iter()
    .map(|format| {
        let converted = source_expected.convert(format);
        let blurred = converted.blur(2);
        let graded = blurred.grade(parameters);
        assert!(graded.pixels.iter().any(|pixel| pixel[3] == 0));
        assert!(
            graded
                .pixels
                .iter()
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
        (converted, blurred, graded)
    })
    .collect();

    // The normal source Runtime has already retired. Replay its accepted original input,
    // installation, View topology and stock capture; no batches or replacement packets are built.
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let source_producer = host.create_producer().unwrap();
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        original.packet().target().extent(),
    )
    .unwrap();
    let input = original.with_image_target(target).unwrap();
    assert_eq!(input.installation(), original.installation());
    assert_eq!(
        input.packet().source_generation(),
        original.packet().source_generation()
    );
    assert_eq!(
        input.packet().frame_index(),
        original.packet().frame_index()
    );
    assert_eq!(input.packet().views(), original.packet().views());
    let reservation = host
        .reserve_image_target(&source_producer, input.packet().source_generation(), target)
        .unwrap();
    host.activate_image_target(&source_producer, reservation)
        .unwrap();
    let candidate = host.prepare(&source_producer, &input).unwrap();
    let frame = host
        .submit_candidate(&source_producer, candidate)
        .unwrap_or_else(|failure| panic!("translucent Scene replay failed: {:?}", failure.error));
    let source = frame.final_texture::<WgpuWorldColor>().unwrap();
    assert_eq!(
        source.source_generation(),
        original.packet().source_generation()
    );
    assert_eq!(source.frame_index(), original.packet().frame_index());
    let source_actual = readback::read(&mut host, &source);
    // One encoded value is a predetermined source UNORM rounding bound. Opaque source and HUD
    // checks in the existing consumers remain exact; effect tolerance remains three.
    source_expected.assert_matches(&source_actual, 1, "ordinary translucent Sprite blending");
    source_expected.assert_transparent_regions(&source_actual, "ordinary transparent Camera clear");
    let center = (scene::HEIGHT / 2 * scene::WIDTH + scene::WIDTH / 2) as usize;
    assert!(
        source_actual.pixels[center][3] > 0 && source_actual.pixels[center][3] < 255,
        "the ordinary Sprite must actually be translucent within the declared one-value source rounding bound"
    );
    assert!(
        source_actual.pixels[center][..3]
            .iter()
            .all(|channel| *channel > 0 && *channel < 128)
    );

    for (converted_expected, blurred_expected, graded_expected) in expected {
        let producer = host.create_producer().unwrap();
        let convert = host
            .create_feature_instance(
                &producer,
                &WgpuFeatureDefinition::new(Grade::<WgpuWorldColor, EffectColor>::new()),
            )
            .unwrap();
        let blur = host
            .create_feature_instance(
                &producer,
                &WgpuFeatureDefinition::new(Blur::<EffectColor, EffectColor>::new()),
            )
            .unwrap();
        let grade = host
            .create_feature_instance(
                &producer,
                &WgpuFeatureDefinition::new(Grade::<EffectColor, EffectColor>::new()),
            )
            .unwrap();
        let format = converted_expected.format;
        let mut descriptor = source.descriptor();
        descriptor.native.label = Some("translucent-Scene-effect-format");
        descriptor.native.format = format;
        descriptor.transfer = if format.is_srgb() {
            TextureTransfer::Srgb
        } else {
            TextureTransfer::Linear
        };
        let mut graph = WgpuRenderProgramBuilder::new(
            source.source_generation(),
            source.frame_index(),
            GraphLimits::default(),
        )
        .unwrap();
        let original_port = graph.import_texture(&source).unwrap();
        let converted = graph.texture::<EffectColor>(descriptor.clone()).unwrap();
        let blurred = graph.texture::<EffectColor>(descriptor.clone()).unwrap();
        let graded = graph.texture::<EffectColor>(descriptor).unwrap();
        // The image Host's stock target is RGBA8Unorm. The public identity Grade converts that
        // exact stock output to each actual port format; stock lowering is never duplicated.
        graph
            .operation(
                1,
                &convert,
                Arc::new(GradeCapture {
                    source: original_port.clone(),
                    destination: converted.clone(),
                    parameters: GradeParameters::default(),
                }),
                &[original_port.read(AccessRole::Sampled)],
                &[converted.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        graph
            .operation(
                2,
                &blur,
                Arc::new(BlurCapture {
                    source: converted.clone(),
                    destination: blurred.clone(),
                    radius: 2,
                }),
                &[converted.read(AccessRole::Sampled)],
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
            .finish(&[
                original_port.output(),
                converted.output(),
                blurred.output(),
                graded.output(),
            ])
            .unwrap();
        let outputs = host.submit_program(&producer, &program).unwrap();
        let untouched = outputs.texture(&original_port).unwrap();
        assert_eq!(untouched.producer_id(), source.producer_id());
        assert_eq!(untouched.source_generation(), source.source_generation());
        let converted_actual = readback::read(&mut host, &outputs.texture(&converted).unwrap());
        let blurred_actual = readback::read(&mut host, &outputs.texture(&blurred).unwrap());
        let graded_actual = readback::read(&mut host, &outputs.texture(&graded).unwrap());
        for (expected, actual, label) in [
            (
                &converted_expected,
                &converted_actual,
                "translucent Scene format conversion",
            ),
            (
                &blurred_expected,
                &blurred_actual,
                "translucent Scene half-float blur",
            ),
            (
                &graded_expected,
                &graded_actual,
                "translucent Scene grade including alpha-zero",
            ),
        ] {
            expected.assert_matches(actual, oracle::EFFECT_TOLERANCE, label);
            expected.assert_transparent_regions(actual, label);
        }
        for (before, after) in blurred_actual.pixels.iter().zip(&graded_actual.pixels) {
            assert_eq!(
                before[3], after[3],
                "grade must preserve each actual blurred alpha byte"
            );
        }
        assert!(
            graded_actual
                .pixels
                .iter()
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
        host.retire_producer(&producer);
    }
    // Every independently formatted effect has executed. The retained original Scene remains
    // the same storage and exact bytes, including its translucent center and transparent border.
    source_actual.assert_matches(
        &readback::read(&mut host, &source),
        0,
        "untouched retained translucent Scene",
    );
    host.retire_producer(&source_producer);
    drop(frame);
    readback::finish(&mut host);
}
