use super::*;
use crate::{AccessRole, TextureAlpha, TextureTransfer, WgpuRenderHost, WgpuRenderHostError};

struct SceneColor;
struct Composite;

struct EmptyFeature;
impl WgpuRenderFeature for EmptyFeature {
    type Capture = ();
    type State = ();
    type Prepared = ();
    type Report = ();

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }
    fn prepare(
        &self,
        _: &mut (),
        _: &(),
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }
    fn encode(
        &self,
        _: &(),
        _: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }
}

fn source() -> RenderFrameSourceGeneration {
    RenderFrameSourceGeneration::try_acquire().unwrap()
}

fn descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("public-scene-color"),
            size: wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

fn builder() -> WgpuRenderProgramBuilder {
    WgpuRenderProgramBuilder::new(source(), 1, GraphLimits::default()).unwrap()
}

#[test]
fn policy_program_metadata_cannot_expand_the_engine_preallocation_envelope() {
    let excessive = GraphLimits {
        max_accesses: 1_000_000,
        max_modeled_metadata_bytes: 1024 * 1024 * 1024,
        ..GraphLimits::default()
    };
    assert!(matches!(
        WgpuRenderProgramBuilder::new(source(), 1, excessive),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            crate::LimitKind::MetadataBytes
        )))
    ));
    assert!(matches!(
        WgpuRenderProgramBuilder::required_metadata_bytes(excessive),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            crate::LimitKind::MetadataBytes
        )))
    ));
    let enlarged_allowance = GraphLimits {
        max_modeled_metadata_bytes: usize::MAX,
        ..GraphLimits::default()
    };
    let accepted = WgpuRenderProgramBuilder::new(source(), 1, enlarged_allowance).unwrap();
    assert_eq!(
        accepted.max_modeled_metadata_bytes,
        MAX_PROGRAM_METADATA_BYTES
    );
}

#[test]
fn public_builder_rejects_foreign_writes_before_consuming_the_operation_key() {
    let mut first = builder();
    let foreign = first.texture::<SceneColor>(descriptor()).unwrap();
    let mut second = builder();
    let local = second.texture::<SceneColor>(descriptor()).unwrap();
    assert!(matches!(
        second.initialize(
            7,
            &[foreign.clear(wgpu::Color::BLACK, AccessRole::Attachment)]
        ),
        Err(WgpuRenderProgramError::Graph(GraphError::ForeignCandidate))
    ));
    second
        .initialize(
            7,
            &[local.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    second.finish(&[local.output()]).unwrap();
}

#[test]
fn explicit_copy_changes_output_semantics_without_relabeling_its_predecessor() {
    let mut builder = builder();
    let scene = builder.texture::<SceneColor>(descriptor()).unwrap();
    let composite = builder.texture::<Composite>(descriptor()).unwrap();
    // Reversed declaration order still follows the preserve dependency.
    builder
        .initialize(1, &[composite.preserve(&scene, AccessRole::Attachment)])
        .unwrap();
    builder
        .initialize(
            9,
            &[scene.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder
        .finish(&[scene.output(), composite.output()])
        .unwrap();
    assert_eq!(
        program
            .0
            .graph
            .operations()
            .map(|op| op.key)
            .collect::<Vec<_>>(),
        [9, 1]
    );
    let meanings = program
        .0
        .graph
        .resources()
        .iter()
        .map(|resource| match &resource.desc {
            ResourceDescriptor::Texture { meaning, .. } => meaning.semantic,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        meanings,
        [
            std::any::TypeId::of::<SceneColor>(),
            std::any::TypeId::of::<Composite>()
        ]
    );
}

#[test]
fn retained_cpu_program_cannot_reopen_its_dropped_instance_or_initialize_a_device() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(EmptyFeature);
    let instance = host
        .create_feature_instance(&producer, &definition)
        .unwrap();
    let mut builder = builder();
    let output = builder.texture::<SceneColor>(descriptor()).unwrap();
    builder
        .operation(
            1,
            &instance,
            Arc::new(()),
            &[],
            &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder.finish(&[output.output()]).unwrap();
    drop(instance);
    assert!(matches!(
        host.submit_program(&producer, &program),
        Err(WgpuRenderHostError::Program(
            WgpuRenderProgramError::FeatureClosed
        ))
    ));
    assert_eq!(host.device_epoch(), 0);
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
fn a_live_instance_cannot_be_submitted_through_another_producer() {
    let mut host = WgpuRenderHost::offscreen();
    let a = host.create_producer().unwrap();
    let b = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(&a, &WgpuFeatureDefinition::new(EmptyFeature))
        .unwrap();
    let mut builder = builder();
    let output = builder.texture::<SceneColor>(descriptor()).unwrap();
    builder
        .operation(
            1,
            &instance,
            Arc::new(()),
            &[],
            &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder.finish(&[output.output()]).unwrap();
    assert!(matches!(
        host.submit_program(&b, &program),
        Err(WgpuRenderHostError::Program(
            WgpuRenderProgramError::FeatureClosed
        ))
    ));
    assert_eq!(host.device_epoch(), 0);
}

#[test]
fn a_program_cannot_initialize_the_device_without_a_host_transaction() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let mut builder = builder();
    let output = builder.texture::<SceneColor>(descriptor()).unwrap();
    builder
        .initialize(
            1,
            &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder.finish(&[output.output()]).unwrap();
    assert!(matches!(
        host.submit_program(&producer, &program),
        Err(WgpuRenderHostError::FrameTransactionUnavailable)
    ));
    assert_eq!(host.device_epoch(), 0);
}

#[test]
fn metadata_admission_includes_native_import_output_tables_without_operations() {
    let mut limits = GraphLimits {
        max_resources: 64,
        max_operations: 0,
        max_accesses: 0,
        max_outputs: 64,
        max_modeled_metadata_bytes: usize::MAX,
        ..Default::default()
    };
    let graph_only = limits.modeled_metadata_bytes().unwrap();
    let complete = WgpuRenderProgramBuilder::required_metadata_bytes(limits).unwrap();
    assert!(complete > graph_only + 64 * size_of::<ProgramResourceStorage>());
    limits.max_modeled_metadata_bytes = complete - 1;
    assert!(matches!(
        WgpuRenderProgramBuilder::new(source(), 1, limits),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            crate::LimitKind::MetadataBytes
        )))
    ));
    limits.max_modeled_metadata_bytes = complete;
    let builder = WgpuRenderProgramBuilder::new(source(), 1, limits).unwrap();
    assert_eq!(builder.modeled_metadata_bytes, complete);
}

#[test]
fn typed_wrapper_budget_is_admitted_before_capture_allocation_and_writer_commit() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(&producer, &WgpuFeatureDefinition::new(EmptyFeature))
        .unwrap();
    let mut limits = GraphLimits::default();
    let base = WgpuRenderProgramBuilder::required_metadata_bytes(limits).unwrap();
    let typed = feature::operation_metadata_bytes::<EmptyFeature>().unwrap();
    limits.max_modeled_metadata_bytes = base + typed - 1;
    let mut short = WgpuRenderProgramBuilder::new(source(), 1, limits).unwrap();
    let output = short.texture::<SceneColor>(descriptor()).unwrap();
    assert!(matches!(
        short.operation(
            7,
            &instance,
            Arc::new(()),
            &[],
            &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)]
        ),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            crate::LimitKind::MetadataBytes
        )))
    ));
    assert_eq!(short.modeled_metadata_bytes, base);
    assert!(short.operations.is_empty());
    short
        .initialize(
            7,
            &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    short.finish(&[output.output()]).unwrap();

    limits.max_modeled_metadata_bytes = base + typed;
    let mut exact = WgpuRenderProgramBuilder::new(source(), 1, limits).unwrap();
    let output = exact.texture::<SceneColor>(descriptor()).unwrap();
    exact
        .operation(
            7,
            &instance,
            Arc::new(()),
            &[],
            &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let admitted = exact.modeled_metadata_bytes;
    assert!(
        exact
            .operation(7, &instance, Arc::new(()), &[], &[])
            .is_err()
    );
    assert_eq!(exact.modeled_metadata_bytes, admitted);
    let exact = exact.finish(&[output.output()]).unwrap();
    assert_eq!(exact.modeled_metadata_bytes(), base + typed);
}

#[test]
fn enabled_device_hard_limits_are_content_refusals_while_budget_pressure_can_defer() {
    let hard = WgpuRenderHostError::Program(WgpuRenderProgramError::DeviceResourceLimit {
        name: "max_buffer_size",
        required: 2048,
        enabled: 1024,
    });
    assert!(hard.is_content_rejected());
    assert!(!hard.is_deferred());
    let budget = WgpuRenderHostError::Program(WgpuRenderProgramError::ResourceCapacity);
    assert!(budget.is_deferred());
    assert!(!budget.is_content_rejected());
    let structural = WgpuRenderHostError::DeviceRequirements(
        crate::WgpuDeviceRequirementsError::ReconstructionRequired,
    );
    assert!(structural.is_content_rejected());
    assert!(!structural.is_deferred());
    assert!(
        !structural.requires_owner_replacement(),
        "a failed late contribution does not invalidate existing producers"
    );
}

#[test]
fn automatic_submission_custody_has_a_finite_preallocation_bound() {
    let mut limits = GraphLimits {
        max_resources: crate::submission::MAX_SUBMISSION_RESOURCES - 2,
        max_operations: 0,
        max_accesses: 0,
        max_modeled_metadata_bytes: usize::MAX,
        ..Default::default()
    };
    assert!(WgpuRenderProgramBuilder::required_metadata_bytes(limits).is_ok());
    limits.max_resources += 1;
    assert!(matches!(
        WgpuRenderProgramBuilder::new(source(), 1, limits),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            crate::LimitKind::SubmissionResources
        )))
    ));
}

#[test]
fn native_feature_errors_keep_the_original_backend_fault_domain() {
    let deferred = WgpuRenderHostError::from(WgpuRenderProgramError::from(
        crate::WgpuRenderError::UploadDeferred {
            message: "exact images are still retained".into(),
        },
    ));
    assert!(deferred.is_deferred());
    assert!(!deferred.is_content_rejected());
    let lost = WgpuRenderHostError::from(WgpuRenderProgramError::from(
        crate::WgpuRenderError::DeviceLost,
    ));
    assert!(lost.requires_owner_replacement());
    assert!(!lost.is_content_rejected());
}
