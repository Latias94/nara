#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use nara_render::RenderViewId;
use nara_render_wgpu::{WgpuPolicyDefinition, WgpuRenderHost, WgpuRenderHostError};

pub mod support;

use support::{
    heatmap::{HeatmapPolicy, ScalarField},
    image,
};

#[test]
#[ignore = "requires selected native hardware/backend; acquisition failure fails the prerequisite"]
fn accepted_capture_reconstructs_on_a_fresh_host_after_installation_close() {
    image::require_native_selection();
    let definition = WgpuPolicyDefinition::new(HeatmapPolicy::new(0));
    let mut installation = definition.install().unwrap();
    let source = image::source();
    let target = image::target(16, 16);
    let view_id = RenderViewId::try_acquire().unwrap();
    let capture = ScalarField::default().capture();
    let input = installation
        .capture(
            image::packet(source, view_id, 1, target),
            Arc::clone(&capture),
        )
        .unwrap();
    installation.close();
    assert!(
        installation
            .capture(
                image::packet(source, view_id, 2, target),
                Arc::clone(&capture)
            )
            .is_err(),
    );

    let mut original = WgpuRenderHost::offscreen();
    let producer = original.create_producer().unwrap();
    original.prepare_policy(&producer, &definition).unwrap();
    original
        .begin_frame(1)
        .expect("native acquisition is required");
    image::admit(&mut original, &producer, source, target);
    let output = original
        .submit(&producer, &input)
        .expect("closed admission preserves accepted work");
    let original_backend = output.receipt().backend_instance_id();
    image::assert_heatmap(&mut original, &output, &capture, 0);
    image::settle(&original.retire_producer(&producer).unwrap());
    drop((output, producer));
    image::finish(&mut original);
    assert_eq!(original.retained_resource_bytes(), 0);
    drop(original);

    let replacement = WgpuPolicyDefinition::new(HeatmapPolicy::new(2));
    let replacement_installation = replacement.install().unwrap();
    let replacement_input = replacement_installation
        .capture(
            image::packet(source, view_id, 2, target),
            Arc::clone(&capture),
        )
        .unwrap();
    assert_ne!(input.installation(), replacement_input.installation());
    let mut fresh = WgpuRenderHost::offscreen();
    let fresh_producer = fresh.create_producer().unwrap();
    fresh.prepare_policy(&fresh_producer, &definition).unwrap();
    fresh.prepare_policy(&fresh_producer, &replacement).unwrap();
    drop((installation, definition));
    fresh
        .begin_frame(1)
        .expect("fresh native acquisition is required");
    image::admit(&mut fresh, &fresh_producer, source, target);
    let reconstructed = fresh.submit(&fresh_producer, &input).unwrap();
    assert_ne!(
        reconstructed.receipt().backend_instance_id(),
        original_backend,
    );
    assert_eq!(reconstructed.receipt().source_generation(), source);
    assert_eq!(reconstructed.receipt().render_transaction_frame_index(), 1);
    image::assert_heatmap(&mut fresh, &reconstructed, &capture, 0);

    fresh.begin_frame(2).unwrap();
    let replaced = fresh.submit(&fresh_producer, &replacement_input).unwrap();
    image::assert_heatmap(&mut fresh, &replaced, &capture, 2);
    image::assert_heatmap(&mut fresh, &reconstructed, &capture, 0);
    image::settle(&fresh.retire_producer(&fresh_producer).unwrap());
    drop((reconstructed, replaced, fresh_producer));
    image::finish(&mut fresh);
    assert_eq!(fresh.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires selected native hardware/backend; acquisition failure fails the prerequisite"]
fn producer_a_retires_while_b_keeps_its_own_policy_and_outputs() {
    image::require_native_selection();
    let a_definition = WgpuPolicyDefinition::new(HeatmapPolicy::new(0));
    let b_definition = WgpuPolicyDefinition::new(HeatmapPolicy::new(1));
    let a_installation = a_definition.install().unwrap();
    let b_installation = b_definition.install().unwrap();
    let a_source = image::source();
    let b_source = image::source();
    let a_target = image::target(16, 16);
    let b_target = image::target(16, 16);
    let a_view_id = RenderViewId::try_acquire().unwrap();
    let b_view_id = RenderViewId::try_acquire().unwrap();
    let capture = ScalarField::default().capture();
    let a_input = a_installation
        .capture(
            image::packet(a_source, a_view_id, 1, a_target),
            Arc::clone(&capture),
        )
        .unwrap();
    let b_input = b_installation
        .capture(
            image::packet(b_source, b_view_id, 1, b_target),
            Arc::clone(&capture),
        )
        .unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let a = host.create_producer().unwrap();
    let b = host.create_producer().unwrap();
    host.prepare_policy(&a, &a_definition).unwrap();
    host.prepare_policy(&b, &b_definition).unwrap();
    host.begin_frame(1).expect("native acquisition is required");
    image::admit(&mut host, &a, a_source, a_target);
    image::admit(&mut host, &b, b_source, b_target);
    let a_output = host.submit(&a, &a_input).unwrap();
    let b_first = host.submit(&b, &b_input).unwrap();

    // Start retirement before any readback or blocking drain. B remains independently live.
    let a_prefix = host.retire_producer(&a).unwrap();
    assert!(matches!(
        host.submit(&a, &a_input),
        Err(WgpuRenderHostError::ProducerClosed),
    ));
    assert!(host.reserve_image_target(&a, a_source, a_target).is_err());
    let b_next_input = b_installation
        .capture(
            image::packet(b_source, b_view_id, 2, b_target),
            Arc::clone(&capture),
        )
        .unwrap();
    host.begin_frame(2).unwrap();
    let b_next = host.submit(&b, &b_next_input).unwrap();
    assert_eq!(b_next.receipt().render_transaction_frame_index(), 2);
    assert_ne!(
        a_output.receipt().producer_id(),
        b_next.receipt().producer_id(),
    );
    assert_eq!(host.frame_transaction_stats().surface_acquire_attempts(), 0);
    assert_eq!(host.frame_transaction_stats().presents(), 0);
    image::assert_heatmap(&mut host, &a_output, &capture, 0);
    image::assert_heatmap(&mut host, &b_first, &capture, 1);
    image::assert_heatmap(&mut host, &b_next, &capture, 1);
    image::settle(&a_prefix);
    let b_later_input = b_installation
        .capture(
            image::packet(b_source, b_view_id, 3, b_target),
            Arc::clone(&capture),
        )
        .unwrap();
    host.begin_frame(3).unwrap();
    let b_later = host.submit(&b, &b_later_input).unwrap();
    image::assert_heatmap(&mut host, &b_later, &capture, 1);
    image::settle(&host.retire_producer(&b).unwrap());
    drop((a_output, b_first, b_next, b_later, a, b));
    image::finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
}
