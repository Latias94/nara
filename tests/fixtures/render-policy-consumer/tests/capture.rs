use std::sync::Arc;

use nara_render::RenderViewId;
use nara_render_wgpu::{
    WgpuDeviceRequirements, WgpuPolicyDefinition, WgpuRenderHost, WgpuRenderProgramError,
};

pub mod support;

use support::{
    heatmap::{HeatmapPolicy, ScalarField},
    image,
};

#[test]
fn installation_close_refuses_new_captures_and_preserves_original_input() {
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
    let original_installation = installation.id();
    let clone = input.clone();
    installation.close();
    assert!(!installation.is_open());
    assert!(matches!(
        installation.capture(image::packet(source, view_id, 2, target), capture),
        Err(WgpuRenderProgramError::PolicyClosed),
    ));
    let replacement = definition.install().unwrap();
    assert_ne!(replacement.id(), original_installation);
    assert!(replacement.is_open());
    drop((installation, replacement, definition));

    assert_eq!(clone.installation(), original_installation);
    assert_eq!(clone.packet().source_generation(), source);
    assert_eq!(clone.packet().frame_index(), 1);
    assert_eq!(clone.packet().target(), input.packet().target());
    assert_eq!(clone.packet().views(), input.packet().views());
}

#[test]
fn capture_copies_normal_domain_data_and_validates_before_native_work() {
    let definition = WgpuPolicyDefinition::new(HeatmapPolicy::new(0));
    let installation = definition.install().unwrap();
    let mut field = ScalarField::default();
    let capture = field.capture();
    let original_samples = field.samples;
    field.samples.fill(3);
    assert_eq!(capture.samples, original_samples);
    assert_ne!(capture.samples, field.samples);

    let source = image::source();
    let target = image::target(16, 16);
    let view_id = RenderViewId::try_acquire().unwrap();
    installation
        .capture(image::packet(source, view_id, 1, target), capture)
        .unwrap();
    field.samples[5] = 4;
    assert!(matches!(
        installation.capture(image::packet(source, view_id, 2, target), field.capture()),
        Err(WgpuRenderProgramError::Feature { .. }),
    ));
}

#[test]
fn original_closed_request_is_admitted_without_native_acquisition() {
    let definition = WgpuPolicyDefinition::new(HeatmapPolicy::new(2));
    assert_eq!(
        definition.device_requirements(),
        &WgpuDeviceRequirements::default(),
    );
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    host.prepare_policy(&producer, &definition).unwrap();
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.frame_transaction_stats().surface_acquire_attempts(), 0);
    assert!(host.retire_producer(&producer).is_none());
    assert!(host.prepare_policy(&producer, &definition).is_err());
    assert!(host.poll_retirement().unwrap());
}
