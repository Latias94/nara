//! Actual typed UI Feature/Host coverage; this never uses the quad-only readback adapter.

use super::*;
use crate::{WgpuUiCapture, WgpuUiFeature, ui::mesh_tests as fixture};

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_ui_mesh_preserves_interleaved_alpha_clip_mapping_facts_and_retirement() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(WgpuUiFeature::<Color>::new()),
        )
        .unwrap();
    let packet = fixture::packet();
    let mut builder = WgpuRenderProgramBuilder::new(source(), 1, GraphLimits::default()).unwrap();
    let target = builder.texture::<Color>(fixture::descriptor()).unwrap();
    let capture = Arc::new(
        WgpuUiCapture::new(
            &[Arc::clone(&packet)],
            nara_render::RenderTarget::PrimaryWindow,
            None,
            target.clone(),
            fixture::viewport(),
        )
        .unwrap(),
    );
    let original_facts = Arc::clone(&capture.packet_facts);
    builder
        .operation(
            7,
            &instance,
            Arc::clone(&capture),
            &[],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder.finish(&[target.output()]).unwrap();
    host.begin_frame(1).unwrap();
    let outputs = host.submit_program(&producer, &program).unwrap();
    let report = outputs
        .report::<WgpuUiFeature<Color>>(7, &instance)
        .unwrap();
    assert!(Arc::ptr_eq(&report.packet_facts, &original_facts));
    assert_eq!(
        report.packets().collect::<Vec<_>>(),
        capture.packets().collect::<Vec<_>>()
    );
    assert_eq!(
        (
            report.draw_calls(),
            report.meshes(),
            report.mesh_vertices(),
            report.mesh_triangles()
        ),
        (3, 1, 3, 1)
    );
    assert_eq!((report.sprites(), report.image_sprites()), (0, 0));
    assert_eq!(report.packet_facts[0].submitted_quad_count, 2);
    assert_eq!(report.packet_facts[0].submitted_mesh_count, 1);
    let work = host.frame_transaction_stats().native_work();
    assert_eq!(
        work.initialized_buffer_calls, 4,
        "two quad buffers and one shared vertex/index pair"
    );
    assert_eq!(
        work.initialized_buffer_payload_bytes,
        2 * 80 + 3 * 24 + 3 * 4
    );
    assert_eq!(
        work.initialized_buffer_storage_bytes,
        work.initialized_buffer_payload_bytes
    );
    assert_eq!(
        work.issued_buffer_write_calls, 0,
        "mesh uses original checked mapped initialization"
    );
    let output = outputs.texture(&target).unwrap();
    let actual = pixels(&mut host, &output);
    let mut checked = 0;
    let mut mesh_samples = 0;
    let mut clipped_samples = 0;
    for pixel_y in 0..72 {
        for pixel_x in 0..72 {
            let Some((expected, covered, clipped)) =
                pixel_oracle(pixel_x, pixel_y, TextureTransfer::Linear)
            else {
                continue;
            };
            let pixel = actual[(pixel_y * 72 + pixel_x) as usize];
            assert!(
                pixel
                    .into_iter()
                    .zip(expected)
                    .all(|(channel, expected)| channel.abs_diff(expected) <= 2),
                "pixel ({pixel_x}, {pixel_y}) was {pixel:?}, expected {expected:?}"
            );
            checked += 1;
            mesh_samples += usize::from(covered);
            clipped_samples += usize::from(clipped);
        }
    }
    assert!(checked > 5_000);
    assert!(
        mesh_samples > 300,
        "the oracle must exercise actual interpolated mesh pixels"
    );
    assert!(
        clipped_samples > 50,
        "the content clip must remove real triangle coverage"
    );
    let prefix = outputs.submitted_prefix().clone();
    drop((outputs, output, program, capture, packet, original_facts));
    let retired = host.retire_producer(&producer).unwrap();
    drain(&prefix);
    drain(&retired);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !host.poll_retirement().unwrap() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(prefix.is_complete());
    assert_eq!(host.retained_resource_bytes(), 0);
}

/// An authored-space CPU oracle independent of packet lowering, native clip-space conversion
/// and batching. The opaque blue background is followed by straight-alpha vertex interpolation
/// and a full-surface green wash. Nearest-edge box clips and viewport offset are applied once.
fn pixel_oracle(
    pixel_x: u32,
    pixel_y: u32,
    transfer: TextureTransfer,
) -> Option<([u8; 4], bool, bool)> {
    let px = f64::from(pixel_x) + 0.5 - 3.0;
    let py = f64::from(pixel_y) + 0.5 - 5.0;
    let in_root = (1.0..61.0).contains(&px) && (4.0..57.0).contains(&py);
    let positions = fixture::POSITIONS.map(|point| {
        [
            1.25 + (f64::from(fixture::NODE_ORIGIN[0]) + f64::from(point[0])) * 1.875,
            4.25 + (f64::from(fixture::NODE_ORIGIN[1]) + f64::from(point[1])) * 1.875,
        ]
    });
    let [first, second, third] = positions;
    let area = cross(sub(second, first), sub(third, first));
    let weights = [
        cross(sub(second, [px, py]), sub(third, [px, py])) / area,
        cross(sub(third, [px, py]), sub(first, [px, py])) / area,
        cross(sub(first, [px, py]), sub(second, [px, py])) / area,
    ];
    if weights.iter().any(|weight| weight.abs() < 0.000_01) {
        return None;
    }
    let in_triangle = weights.iter().all(|weight| *weight >= 0.0);
    let in_clip = (2.0..44.0).contains(&px) && (5.0..43.0).contains(&py);
    let mut color = if in_root {
        [0.0, 0.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    if in_triangle && in_clip {
        let interpolated = std::array::from_fn(|channel| {
            weights
                .iter()
                .zip(fixture::COLORS)
                .map(|(weight, color)| weight * f64::from(color[channel]))
                .sum()
        });
        color = blend(interpolated, color, transfer);
    }
    if in_root {
        color = blend([0.0, 1.0, 0.0, 0.25], color, transfer);
    }
    Some((
        color.map(|channel| (channel * 255.0).round() as u8),
        in_triangle && in_clip,
        in_triangle && in_root && !in_clip,
    ))
}

fn blend(source: [f64; 4], mut destination: [f64; 4], transfer: TextureTransfer) -> [f64; 4] {
    if transfer == TextureTransfer::Srgb {
        for channel in &mut destination[..3] {
            *channel = if *channel <= 0.04045 {
                *channel / 12.92
            } else {
                ((*channel + 0.055) / 1.055).powf(2.4)
            };
        }
    }
    let mut result = std::array::from_fn(|channel| {
        source[channel] * source[3] + destination[channel] * (1.0 - source[3])
    });
    result[3] = source[3] + destination[3] * (1.0 - source[3]);
    if transfer == TextureTransfer::Srgb {
        for channel in &mut result[..3] {
            *channel = if *channel <= 0.0031308 {
                *channel * 12.92
            } else {
                1.055 * channel.powf(1.0 / 2.4) - 0.055
            };
        }
    }
    result.map(|channel| (channel * 255.0).round() / 255.0)
}

fn sub(first: [f64; 2], second: [f64; 2]) -> [f64; 2] {
    [first[0] - second[0], first[1] - second[1]]
}
fn cross(first: [f64; 2], second: [f64; 2]) -> f64 {
    first[0] * second[1] - first[1] * second[0]
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_direct_ui_geometry_metadata_refuses_before_work_and_recovers_at_exact_budget() {
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(0);
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(WgpuUiFeature::<Color>::new()),
        )
        .unwrap();
    let mut builder = WgpuRenderProgramBuilder::new(source(), 1, GraphLimits::default()).unwrap();
    let target = builder.texture::<Color>(fixture::descriptor()).unwrap();
    let capture = Arc::new(
        WgpuUiCapture::new(
            &[fixture::packet()],
            nara_render::RenderTarget::PrimaryWindow,
            None,
            target.clone(),
            fixture::viewport(),
        )
        .unwrap(),
    );
    let metadata = fixture::native_metadata(&capture);
    builder
        .operation(
            7,
            &instance,
            capture,
            &[],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let mut program = builder.finish(&[target.output()]).unwrap();
    let maximum = program.modeled_metadata_bytes() + metadata;
    Arc::get_mut(&mut program.0)
        .unwrap()
        .max_modeled_metadata_bytes = maximum - 1;
    host.begin_frame(1).unwrap();
    let allocation = crate::allocation::AllocationFailureGuard::at("nara_ui_mesh_shader");
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::Graph(
            crate::GraphError::LimitExceeded(crate::LimitKind::MetadataBytes)
        ))
    ));
    assert!(
        !allocation.was_consumed(),
        "known geometry metadata must refuse before native work"
    );
    assert_eq!(
        host.frame_transaction_stats().native_work(),
        crate::WgpuNativeWorkStats::default()
    );
    assert_eq!(host.retained_resource_bytes(), 0);
    assert!(!host.backend.texture_stats().has_fallback_texture);
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
    drop(allocation);
    drain(
        host.views[&producer.id()]
            .submitted_prefix
            .as_ref()
            .unwrap(),
    );
    Arc::get_mut(&mut program.0)
        .unwrap()
        .max_modeled_metadata_bytes = maximum;
    host = host.with_resource_byte_limit(1024 * 1024);
    let outputs = host.submit_program(&producer, &program).unwrap();
    let report = outputs
        .report::<WgpuUiFeature<Color>>(7, &instance)
        .unwrap();
    assert_eq!((report.draw_calls(), report.meshes()), (3, 1));
    drain(outputs.submitted_prefix());
}

fn build_mesh_program(
    instance: &crate::WgpuFeatureInstance<WgpuUiFeature<Color>>,
    source: RenderFrameSourceGeneration,
    frame: u64,
    format: wgpu::TextureFormat,
) -> (
    WgpuRenderProgram,
    WgpuProgramTexture<Color>,
    Arc<WgpuUiCapture<Color>>,
) {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, GraphLimits::default()).unwrap();
    let mut descriptor = fixture::descriptor();
    descriptor.native.format = format;
    descriptor.transfer = if format.is_srgb() {
        TextureTransfer::Srgb
    } else {
        TextureTransfer::Linear
    };
    let target = builder.texture::<Color>(descriptor).unwrap();
    let capture = Arc::new(
        WgpuUiCapture::new(
            &[fixture::packet()],
            nara_render::RenderTarget::PrimaryWindow,
            None,
            target.clone(),
            fixture::viewport(),
        )
        .unwrap(),
    );
    builder
        .operation(
            7,
            instance,
            Arc::clone(&capture),
            &[],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    (builder.finish(&[target.output()]).unwrap(), target, capture)
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_ui_mesh_index_allocation_failure_settles_original_custody_and_retries_same_capture() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(WgpuUiFeature::<Color>::new()),
        )
        .unwrap();
    let source = source();
    host.begin_frame(1).unwrap();
    {
        let (program, _, _) =
            build_mesh_program(&instance, source, 1, wgpu::TextureFormat::Rgba8Unorm);
        let outputs = host.submit_program(&producer, &program).unwrap();
        assert_eq!(outputs.report(7, &instance).unwrap().meshes(), 1);
        drain(outputs.submitted_prefix());
    }
    host.begin_frame(2).unwrap();
    let baseline = host.retained_resource_bytes();
    let (program, target, capture) =
        build_mesh_program(&instance, source, 2, wgpu::TextureFormat::Rgba8Unorm);
    let failure = crate::allocation::AllocationFailureGuard::at("nara_ui_mesh_indices");
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(
        failure.was_consumed(),
        "the original vertex initialization must succeed first"
    );
    assert!(
        matches!(&error, WgpuRenderHostError::Backend(WgpuRenderError::UploadDeferred { message })
        if message.contains("nara_ui_mesh_indices"))
    );
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    assert_eq!(
        host.backend.stock_quad_resources().instance_charges(),
        (0, 0)
    );
    let refused_work = host.frame_transaction_stats().native_work();
    assert_eq!(
        refused_work,
        crate::WgpuNativeWorkStats {
            initialized_buffer_calls: 3,
            initialized_buffer_payload_bytes: 2 * 80 + 3 * 24,
            initialized_buffer_storage_bytes: 2 * 80 + 3 * 24,
            ..Default::default()
        }
    );
    drop(failure);
    let prefix = host.views[&producer.id()]
        .submitted_prefix
        .as_ref()
        .unwrap()
        .clone();
    assert!(prefix.submission_index().is_some());
    drain(&prefix);
    assert_eq!(prefix.cleanup_fault_count(), 0);
    assert_eq!(Arc::strong_count(&program.0), 1);
    assert_eq!(host.retained_resource_bytes(), baseline);
    assert_eq!(host.frame_transaction_stats().native_work(), refused_work);

    let failure = crate::allocation::AllocationFailureGuard::at("nara_ui_mesh_shader");
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(
        failure.was_consumed(),
        "failed instance state must be discarded"
    );
    assert!(
        matches!(&error, WgpuRenderHostError::Backend(WgpuRenderError::UploadDeferred { message })
        if message.contains("nara_ui_mesh_shader"))
    );
    drop(failure);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    drain(
        host.views[&producer.id()]
            .submitted_prefix
            .as_ref()
            .unwrap(),
    );
    assert_eq!(host.retained_resource_bytes(), baseline);
    assert_eq!(
        host.backend.stock_quad_resources().instance_charges(),
        (0, 0)
    );
    let outputs = host.submit_program(&producer, &program).unwrap();
    let report = outputs.report(7, &instance).unwrap();
    assert!(Arc::ptr_eq(&report.packet_facts, &capture.packet_facts));
    assert_eq!((report.draw_calls(), report.meshes()), (3, 1));
    assert_eq!(outputs.texture(&target).unwrap().frame_index(), 2);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(2)
    );
    let work = host.frame_transaction_stats().native_work();
    assert_eq!(work.initialized_buffer_calls, 3 + 2 + 4);
    assert_eq!(
        work.initialized_buffer_payload_bytes,
        3 * (2 * 80) + 2 * (3 * 24) + 3 * 4
    );
    assert_eq!(
        work.initialized_buffer_storage_bytes,
        work.initialized_buffer_payload_bytes
    );
    assert_eq!(
        host.backend.stock_quad_resources().instance_charges(),
        (0, 2 * 80)
    );
    let retired = host.retire_producer(&producer).unwrap();
    drain(outputs.submitted_prefix());
    drain(&retired);
    drop((outputs, program, capture));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !host.poll_retirement().unwrap() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_ui_mesh_format_change_preserves_original_output_and_srgb_blending_until_retirement() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(WgpuUiFeature::<Color>::new()),
        )
        .unwrap();
    let source = source();
    let (old_program, old_target, old_capture) =
        build_mesh_program(&instance, source, 1, wgpu::TextureFormat::Rgba8Unorm);
    host.begin_frame(1).unwrap();
    let old_outputs = host.submit_program(&producer, &old_program).unwrap();
    let old_prefix = old_outputs.submitted_prefix().clone();
    let old_output = old_outputs.texture(&old_target).unwrap();
    let (program, target, capture) =
        build_mesh_program(&instance, source, 2, wgpu::TextureFormat::Rgba8UnormSrgb);
    host.begin_frame(2).unwrap();
    let outputs = host.submit_program(&producer, &program).unwrap();
    let report = outputs.report(7, &instance).unwrap();
    assert!(Arc::ptr_eq(&report.packet_facts, &capture.packet_facts));
    assert!(Arc::ptr_eq(
        &old_outputs.report(7, &instance).unwrap().packet_facts,
        &old_capture.packet_facts,
    ));
    assert_eq!((report.draw_calls(), report.meshes()), (3, 1));
    let output = outputs.texture(&target).unwrap();
    let original_pixels = pixels(&mut host, &old_output);
    let actual = pixels(&mut host, &output);
    assert_eq!(pixels(&mut host, &old_output), original_pixels);
    let mut checked = 0;
    let mut mesh_samples = 0;
    for pixel_y in 0..72 {
        for pixel_x in 0..72 {
            let Some((expected, covered, _)) =
                pixel_oracle(pixel_x, pixel_y, TextureTransfer::Srgb)
            else {
                continue;
            };
            let pixel = actual[(pixel_y * 72 + pixel_x) as usize];
            assert!(
                pixel
                    .into_iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
                "sRGB pixel ({pixel_x}, {pixel_y}) was {pixel:?}, expected {expected:?}"
            );
            let (expected, _, _) = pixel_oracle(pixel_x, pixel_y, TextureTransfer::Linear).unwrap();
            let pixel = original_pixels[(pixel_y * 72 + pixel_x) as usize];
            assert!(
                pixel
                    .into_iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
                "retained linear pixel ({pixel_x}, {pixel_y}) was {pixel:?}, expected {expected:?}"
            );
            checked += 1;
            mesh_samples += usize::from(covered);
        }
    }
    assert!(checked > 5_000 && mesh_samples > 300);
    assert_ne!(
        actual, original_pixels,
        "RGB transfer must change the stored blend values"
    );
    let retired = host.retire_producer(&producer).unwrap();
    drain(&old_prefix);
    drain(outputs.submitted_prefix());
    drain(&retired);
    drop((
        old_outputs,
        old_output,
        old_program,
        old_capture,
        outputs,
        output,
        program,
        capture,
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !host.poll_retirement().unwrap() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(old_prefix.is_complete() && retired.is_complete());
    assert_eq!(host.retained_resource_bytes(), 0);
}
