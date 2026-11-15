use super::*;
use std::sync::atomic::AtomicUsize;

use nara_core::{Color, Mat3, Vec2};
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, ViewLoadOp,
    ViewMapping2d, ViewportRect, build_render_frame_packet,
};
use nara_window::{Window, WindowId, WindowResolution};

use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureInterpretation, TextureTransfer,
    WgpuProgramTextureDescriptor, WgpuRenderProgramBuilder, WgpuStockCapture,
    WgpuStockCaptureAdapter, WgpuStockPolicy,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Default)]
struct MappingProbe {
    lineage_calls: AtomicUsize,
    mapping_calls: AtomicUsize,
    validations: AtomicUsize,
    builds: AtomicUsize,
    panic_mapping: AtomicBool,
}

struct MappingPolicy {
    source_from_policy: Mat3,
    probe: Arc<MappingProbe>,
}

impl WgpuRenderPolicy for MappingPolicy {
    type Capture = Mat3;
    type Bindings = ();

    fn history_lineage(&self, _: &Self::Capture) -> Option<crate::WgpuHistoryLineage> {
        self.probe.lineage_calls.fetch_add(1, Ordering::Relaxed);
        None
    }

    fn world_input_mapping(&self, capture: &Self::Capture) -> Option<WorldInputMapping2d> {
        self.probe.mapping_calls.fetch_add(1, Ordering::Relaxed);
        assert!(
            !self.probe.panic_mapping.swap(false, Ordering::Relaxed),
            "requested World input mapping panic"
        );
        WorldInputMapping2d::affine(self.source_from_policy * *capture)
    }

    fn validate_capture(
        &self,
        _: &RenderFramePacket,
        _: &Self::Capture,
    ) -> Result<(), WgpuRenderProgramError> {
        self.probe.validations.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn bind(
        &self,
        _: &Self::Capture,
        _: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        Ok(())
    }

    fn build(
        &self,
        _: &Self::Bindings,
        _: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        self.probe.builds.fetch_add(1, Ordering::Relaxed);
        clear_program(context)
    }
}

struct UnmappedPolicy;

impl WgpuRenderPolicy for UnmappedPolicy {
    type Capture = Mat3;
    type Bindings = ();

    fn bind(
        &self,
        _: &Self::Capture,
        _: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        Ok(())
    }

    fn build(
        &self,
        _: &Self::Bindings,
        _: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        clear_program(context)
    }
}

fn window_packet(runtime: &nara_app::RuntimeInstance, frame_index: u64) -> RenderFramePacket {
    let mut window = Window::new("World input mapping", WindowResolution::new(16, 12));
    window.id = WindowId::new(75);
    let viewport = ViewportRect::new(0, 0, 16, 12).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: nara_render::RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 12.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    build_render_frame_packet(
        runtime.generation(),
        frame_index,
        Some(window.id),
        [&window],
        &views,
    )
    .unwrap()
    .unwrap()
}

fn image_target() -> RenderImagePacket {
    RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(16, 12).unwrap(),
    )
    .unwrap()
}

fn clear_program(
    context: WgpuPolicyBuildContext<'_>,
) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
    struct FinalColor;

    let packet = context.packet();
    let extent = packet.target().extent();
    let mut builder = WgpuRenderProgramBuilder::new(
        packet.source_generation(),
        packet.frame_index(),
        GraphLimits::default(),
    )?;
    let output = builder.texture::<FinalColor>(WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("World input mapping test"),
            size: wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: context.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        },
        interpretation: TextureInterpretation::Color {
            transfer: TextureTransfer::Linear,
            alpha: TextureAlpha::Premultiplied,
        },
    })?;
    builder.initialize(
        1,
        &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
    )?;
    WgpuPolicyProgram::new(
        builder.finish(&[output.output()])?,
        &output,
        WgpuPolicyFrameFacts::default(),
    )
}

fn build(
    instances: &mut PolicyInstances,
    input: &WgpuRenderInput,
    backend_instance_id: u64,
) -> BuiltPolicy {
    instances
        .build(
            input,
            backend_instance_id,
            WgpuRenderProducerId::new(backend_instance_id),
            &Arc::new(AtomicBool::new(false)),
            FORMAT,
            None,
        )
        .unwrap()
}

#[test]
fn original_policy_and_capture_mapping_is_frozen_across_adaptation_and_reconstruction() {
    let runtime = nara_app::RuntimeInstance::start(nara_app::App::new()).unwrap();
    let packet = window_packet(&runtime, 7);
    let probe = Arc::new(MappingProbe::default());
    let source_from_policy = Mat3::from_translation(Vec2::new(16.0, 0.0));
    let capture = Arc::new(Mat3::from_scale(Vec2::new(-1.0, 1.0)));
    let expected = WorldInputMapping2d::affine(source_from_policy * *capture).unwrap();
    let definition = WgpuPolicyDefinition::new(MappingPolicy {
        source_from_policy,
        probe: Arc::clone(&probe),
    });
    let installation = definition.install().unwrap();
    let input = installation
        .capture(packet.clone(), Arc::clone(&capture))
        .unwrap();
    assert_eq!(input.world_input_mapping(), Some(expected));
    assert!(std::ptr::eq(
        input.captured::<MappingPolicy>().unwrap(),
        capture.as_ref()
    ));
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 1);

    let successor_probe = Arc::new(MappingProbe::default());
    let successor_definition = WgpuPolicyDefinition::new(MappingPolicy {
        source_from_policy: Mat3::from_scale(Vec2::splat(0.5)),
        probe: Arc::clone(&successor_probe),
    });
    let successor_installation = successor_definition.install().unwrap();
    let successor = successor_installation
        .capture(packet, Arc::new(Mat3::IDENTITY))
        .unwrap();
    assert_eq!(
        successor.world_input_mapping(),
        WorldInputMapping2d::affine(Mat3::from_scale(Vec2::splat(0.5)))
    );
    assert_ne!(input.world_input_mapping(), successor.world_input_mapping());
    assert_ne!(input.installation(), successor.installation());
    drop(installation);
    drop(definition);
    probe.panic_mapping.store(true, Ordering::Relaxed);

    let cloned = input.clone();
    let target = image_target();
    let adapted = cloned.with_image_target(target).unwrap();
    assert_eq!(adapted.packet().target().extent(), target.extent());
    assert!(Arc::ptr_eq(&input.capture, &cloned.capture));
    assert!(Arc::ptr_eq(&input.capture, &adapted.capture));
    for backend_instance_id in [1, 2] {
        let mut instances = PolicyInstances::default();
        for retained in [&input, &cloned, &adapted] {
            assert_eq!(retained.world_input_mapping(), Some(expected));
            assert_eq!(retained.installation(), input.installation());
            let built = build(&mut instances, retained, backend_instance_id);
            assert_eq!(built.output.program.frame_index(), 7);
            assert_eq!(
                built.output.program.source_generation(),
                input.packet().source_generation()
            );
            drop(instances.commit(&built));
            assert_eq!(retained.world_input_mapping(), Some(expected));
        }
    }
    assert_eq!(probe.validations.load(Ordering::Relaxed), 8);
    assert_eq!(probe.builds.load(Ordering::Relaxed), 6);
    assert_eq!(probe.lineage_calls.load(Ordering::Relaxed), 1);
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 1);
    assert!(probe.panic_mapping.load(Ordering::Relaxed));
    assert_eq!(successor_probe.mapping_calls.load(Ordering::Relaxed), 1);
    runtime.finish().unwrap();
}

#[test]
fn custom_policy_defaults_remain_unmapped_even_with_a_shared_or_foreign_capture_type() {
    let runtime = nara_app::RuntimeInstance::start(nara_app::App::new()).unwrap();
    let packet = window_packet(&runtime, 11);
    let capture = Arc::new(Mat3::IDENTITY);
    let probe = Arc::new(MappingProbe::default());
    let mapped = WgpuPolicyDefinition::new(MappingPolicy {
        source_from_policy: Mat3::from_translation(Vec2::new(1.0, 2.0)),
        probe: Arc::clone(&probe),
    })
    .install()
    .unwrap()
    .capture(packet.clone(), Arc::clone(&capture))
    .unwrap();
    let installation = WgpuPolicyDefinition::new(UnmappedPolicy).install().unwrap();
    let adapter = WgpuStockCaptureAdapter::new(installation, move |_| Ok(Arc::clone(&capture)));
    let unmapped = adapter
        .capture(packet.clone(), WgpuStockCapture::new())
        .unwrap();
    assert!(mapped.captured::<UnmappedPolicy>().is_none());
    assert!(unmapped.captured::<MappingPolicy>().is_none());
    assert!(unmapped.captured::<WgpuStockPolicy>().is_none());
    assert!(unmapped.captured::<UnmappedPolicy>().is_some());
    assert!(mapped.world_input_mapping().is_some());
    assert_eq!(unmapped.world_input_mapping(), None);
    assert_eq!(unmapped.clone().world_input_mapping(), None);
    let adapted = unmapped.with_image_target(image_target()).unwrap();
    assert_eq!(adapted.world_input_mapping(), None);
    let mut instances = PolicyInstances::default();
    for input in [&unmapped, &adapted] {
        let built = build(&mut instances, input, 3);
        drop(instances.commit(&built));
        assert_eq!(input.world_input_mapping(), None);
    }

    let stock = WgpuStockCaptureAdapter::stock()
        .unwrap()
        .capture(packet, WgpuStockCapture::new())
        .unwrap();
    assert_eq!(
        stock.world_input_mapping(),
        Some(WorldInputMapping2d::identity())
    );
    assert!(stock.captured::<UnmappedPolicy>().is_none());
    assert!(stock.captured::<WgpuStockPolicy>().is_some());
    let adapted_stock = stock.with_image_target(image_target()).unwrap();
    assert_eq!(
        adapted_stock.world_input_mapping(),
        stock.world_input_mapping()
    );
    let built = build(&mut instances, &adapted_stock, 3);
    drop(instances.commit(&built));
    assert_eq!(
        adapted_stock.world_input_mapping(),
        Some(WorldInputMapping2d::identity())
    );
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 1);
    runtime.finish().unwrap();
}

#[test]
fn explicitly_unmapped_capture_freezes_absence_without_recalling_source_callbacks() {
    let runtime = nara_app::RuntimeInstance::start(nara_app::App::new()).unwrap();
    let probe = Arc::new(MappingProbe::default());
    let installation = WgpuPolicyDefinition::new(MappingPolicy {
        source_from_policy: Mat3::IDENTITY,
        probe: Arc::clone(&probe),
    })
    .install()
    .unwrap();
    let input = installation
        .capture(
            window_packet(&runtime, 12),
            Arc::new(Mat3::from_scale(Vec2::new(0.0, 1.0))),
        )
        .unwrap();
    assert_eq!(input.world_input_mapping(), None);
    probe.panic_mapping.store(true, Ordering::Relaxed);
    let cloned = input.clone();
    let adapted = cloned.with_image_target(image_target()).unwrap();
    let mut instances = PolicyInstances::default();
    for retained in [&input, &cloned, &adapted] {
        assert_eq!(retained.world_input_mapping(), None);
        let built = build(&mut instances, retained, 6);
        drop(instances.commit(&built));
        assert_eq!(retained.world_input_mapping(), None);
    }
    assert_eq!(probe.lineage_calls.load(Ordering::Relaxed), 1);
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 1);
    assert_eq!(probe.validations.load(Ordering::Relaxed), 5);
    assert_eq!(probe.builds.load(Ordering::Relaxed), 3);
    assert!(probe.panic_mapping.load(Ordering::Relaxed));
    runtime.finish().unwrap();
}

#[test]
fn mapping_panic_preserves_admission_and_previously_accepted_inputs() {
    let runtime = nara_app::RuntimeInstance::start(nara_app::App::new()).unwrap();
    let packet = window_packet(&runtime, 13);
    let probe = Arc::new(MappingProbe::default());
    let source_from_policy = Mat3::from_translation(Vec2::new(0.0, 12.0));
    let capture = Arc::new(Mat3::from_scale(Vec2::new(1.0, -1.0)));
    let expected = WorldInputMapping2d::affine(source_from_policy * *capture).unwrap();
    let mut installation = WgpuPolicyDefinition::new(MappingPolicy {
        source_from_policy,
        probe: Arc::clone(&probe),
    })
    .install()
    .unwrap();
    let input = installation
        .capture(packet.clone(), Arc::clone(&capture))
        .unwrap();
    let mut instances = PolicyInstances::default();
    let before = build(&mut instances, &input, 4);
    drop(instances.commit(&before));
    let admission_owners = Arc::strong_count(installation.admission());
    let capture_owners = Arc::strong_count(&capture);
    probe.panic_mapping.store(true, Ordering::Relaxed);
    assert!(matches!(
        installation.capture(packet.clone(), Arc::clone(&capture)),
        Err(WgpuRenderProgramError::PolicyPanicked)
    ));
    assert!(installation.is_open());
    assert_eq!(installation.id(), input.installation());
    assert_eq!(
        Arc::strong_count(installation.admission()),
        admission_owners
    );
    assert_eq!(Arc::strong_count(&capture), capture_owners);
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 2);
    assert_eq!(input.world_input_mapping(), Some(expected));
    let after = build(&mut instances, &input, 4);
    drop(instances.commit(&after));
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 2);

    let recovered = installation
        .capture(packet.clone(), Arc::clone(&capture))
        .unwrap();
    assert_eq!(recovered.installation(), input.installation());
    assert_eq!(recovered.world_input_mapping(), Some(expected));
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 3);
    installation.close();
    assert!(matches!(
        installation.capture(packet, capture),
        Err(WgpuRenderProgramError::PolicyClosed)
    ));
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 3);
    let retained = recovered.clone().with_image_target(image_target()).unwrap();
    assert_eq!(retained.world_input_mapping(), Some(expected));
    let mut rebuilt_instances = PolicyInstances::default();
    let rebuilt = build(&mut rebuilt_instances, &retained, 5);
    drop(rebuilt_instances.commit(&rebuilt));
    assert_eq!(probe.mapping_calls.load(Ordering::Relaxed), 3);
    assert_eq!(input.world_input_mapping(), Some(expected));
    runtime.finish().unwrap();
}
