#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

#[path = "support/model.rs"]
#[allow(dead_code)]
mod model;

#[path = "support/features.rs"]
#[allow(dead_code)]
mod features;

use model::*;
use nara_render_wgpu::{
    AccessRole, GraphError, LimitKind, WgpuFeatureDefinition, WgpuRenderHost, WgpuRenderProgram,
    WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

fn builder() -> WgpuRenderProgramBuilder {
    WgpuRenderProgramBuilder::new(source(), 1, limits()).expect("bounded public candidate")
}

#[test]
fn distinct_semantic_copies_compile_in_reverse_declaration_order_without_a_device() {
    let source = source();
    let mut builder = WgpuRenderProgramBuilder::new(source, 7, limits()).unwrap();
    let scene = builder.texture::<SceneColor>(texture_descriptor()).unwrap();
    let composite = builder
        .texture::<EffectColor>(texture_descriptor())
        .unwrap();
    let raw = builder.buffer::<RawCounts>(buffer_descriptor()).unwrap();
    let frequencies = builder.buffer::<Frequencies>(buffer_descriptor()).unwrap();
    builder
        .initialize(
            1,
            &[composite.preserve(&scene, AccessRole::CopyDestination)],
        )
        .unwrap();
    builder
        .initialize(
            2,
            &[frequencies.preserve(&raw, AccessRole::CopyDestination)],
        )
        .unwrap();
    builder
        .initialize(90, &[raw.zero(AccessRole::CopyDestination)])
        .unwrap();
    builder
        .initialize(
            100,
            &[scene.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder
        .finish(&[
            frequencies.output(),
            composite.output(),
            raw.output(),
            scene.output(),
        ])
        .unwrap();
    assert_eq!(program.source_generation(), source);
    assert_eq!(program.frame_index(), 7);
    assert_eq!(program.clone().frame_index(), 7);
    assert_eq!(scene.descriptor(), composite.descriptor());
    assert_eq!(raw.descriptor(), frequencies.descriptor());
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<WgpuRenderProgram>();
}

#[test]
fn foreign_candidate_and_second_writer_are_rejected_at_declaration() {
    let mut original = builder();
    let port = original
        .texture::<SceneColor>(texture_descriptor())
        .unwrap();
    let mut foreign = builder();
    assert!(matches!(
        foreign.initialize(1, &[port.clear(wgpu::Color::BLACK, AccessRole::Attachment)]),
        Err(WgpuRenderProgramError::Graph(GraphError::ForeignCandidate))
    ));
    original
        .initialize(1, &[port.clear(wgpu::Color::BLACK, AccessRole::Attachment)])
        .unwrap();
    assert!(matches!(
        original.initialize(2, &[port.clear(wgpu::Color::WHITE, AccessRole::Attachment)]),
        Err(WgpuRenderProgramError::Graph(GraphError::RepeatedProducer(
            _
        )))
    ));
    original
        .finish(&[port.output()])
        .expect("a refused writer leaves the original candidate usable");
}

#[test]
fn missing_producers_and_preservation_cycles_reject_the_whole_candidate() {
    let mut incomplete = builder();
    let unwritten = incomplete.buffer::<RawCounts>(buffer_descriptor()).unwrap();
    assert!(matches!(
        incomplete.finish(&[unwritten.output()]),
        Err(WgpuRenderProgramError::Graph(GraphError::MissingProducer(
            _
        )))
    ));
    let mut cyclic = builder();
    let a = cyclic.texture::<SceneColor>(texture_descriptor()).unwrap();
    let b = cyclic.texture::<EffectColor>(texture_descriptor()).unwrap();
    cyclic
        .initialize(2, &[a.preserve(&b, AccessRole::CopyDestination)])
        .unwrap();
    cyclic
        .initialize(1, &[b.preserve(&a, AccessRole::CopyDestination)])
        .unwrap();
    assert!(matches!(
        cyclic.finish(&[a.output(), b.output()]),
        Err(WgpuRenderProgramError::Graph(GraphError::Cycle))
    ));
}

#[test]
fn finite_resource_and_metadata_allowances_refuse_before_native_work() {
    let mut one = WgpuRenderProgramBuilder::new(
        source(),
        1,
        nara_render_wgpu::GraphLimits {
            max_resources: 1,
            ..limits()
        },
    )
    .unwrap();
    one.buffer::<RawCounts>(buffer_descriptor()).unwrap();
    assert!(matches!(
        one.buffer::<Frequencies>(buffer_descriptor()),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            LimitKind::Resources
        )))
    ));

    let mut small = WgpuRenderProgramBuilder::new(
        source(),
        1,
        nara_render_wgpu::GraphLimits {
            max_modeled_resource_bytes: IMAGE_BYTES - 1,
            ..limits()
        },
    )
    .unwrap();
    assert!(matches!(
        small.texture::<SceneColor>(texture_descriptor()),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            LimitKind::ResourceBytes
        )))
    ));

    assert!(matches!(
        WgpuRenderProgramBuilder::new(
            source(),
            1,
            nara_render_wgpu::GraphLimits {
                max_operations: usize::MAX,
                ..limits()
            }
        ),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            LimitKind::MetadataBytes
        )))
    ));
}

#[test]
fn actual_feature_definitions_capture_before_device_acquisition_and_close_irreversibly() {
    use features::{Axis, Blur, BlurCapture};
    use std::sync::Arc;

    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(Blur(Axis::Horizontal));
    assert!(
        definition
            .device_requirements()
            .required_features()
            .is_empty()
    );
    let retained_definition = definition.clone();
    let instance = host
        .create_feature_instance(&producer, &retained_definition)
        .unwrap();
    drop((definition, retained_definition));
    let mut candidate = builder();
    let input = candidate
        .texture::<SceneColor>(texture_descriptor())
        .unwrap();
    let output = candidate
        .texture::<SceneColor>(texture_descriptor())
        .unwrap();
    let capture = Arc::new(BlurCapture {
        source: input.clone(),
        destination: output.clone(),
    });
    candidate
        .operation(
            1,
            &instance,
            Arc::clone(&capture),
            &[input.read(AccessRole::Sampled)],
            &[output.clear(wgpu::Color::TRANSPARENT, AccessRole::StorageWrite)],
        )
        .unwrap();
    candidate
        .initialize(
            2,
            &[input.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    candidate
        .finish(&[output.output()])
        .expect("an exact captured Feature needs no native device to compile");

    assert!(
        host.retire_producer(&producer).is_none(),
        "no native submission occurred"
    );
    let mut refused = builder();
    assert!(matches!(
        refused.operation(1, &instance, capture, &[], &[]),
        Err(WgpuRenderProgramError::FeatureClosed)
    ));
    assert_eq!(host.retained_output_bytes(), 0);
}
