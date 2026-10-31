use super::*;
use std::sync::{Mutex, atomic::AtomicUsize};

use nara_core::{Color, Mat3};
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect, build_image_render_frame_packet,
};

use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuDeviceRequirementsError,
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTextureDescriptor, WgpuRenderProgramBuilder,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct FinalColor;

#[derive(Default)]
struct PolicyProbe {
    created: AtomicUsize,
    dropped: AtomicUsize,
    builds: Mutex<Vec<(usize, u32)>>,
    panic_create: AtomicBool,
    panic_build: AtomicBool,
}

struct ClearPolicy(Arc<PolicyProbe>);

struct ClearCapture {
    tag: u32,
    color: wgpu::Color,
}

struct ClearInstances {
    serial: usize,
    probe: Arc<PolicyProbe>,
}

impl Drop for ClearInstances {
    fn drop(&mut self) {
        self.probe.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

impl WgpuRenderPolicy for ClearPolicy {
    type Capture = ClearCapture;
    type Instances = ClearInstances;

    fn create_instances(
        &self,
        _: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<Self::Instances, WgpuRenderProgramError> {
        assert!(
            !self.0.panic_create.swap(false, Ordering::Relaxed),
            "requested CPU instance creation panic"
        );
        Ok(ClearInstances {
            serial: self.0.created.fetch_add(1, Ordering::Relaxed) + 1,
            probe: Arc::clone(&self.0),
        })
    }

    fn build(
        &self,
        instances: &Self::Instances,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        assert!(
            !self.0.panic_build.swap(false, Ordering::Relaxed),
            "requested CPU program construction panic"
        );
        self.0
            .builds
            .lock()
            .unwrap()
            .push((instances.serial, capture.tag));
        clear_program(context, capture.color, None)
    }
}

// Exercises the Host's CPU instance table without creating a native Host, device or window.
struct CpuInstanceSet {
    backend_instance_id: u64,
    producer: WgpuRenderProducerId,
    closed: Arc<AtomicBool>,
    instances: PolicyInstances,
}

impl CpuInstanceSet {
    fn new(id: u64) -> Self {
        Self {
            backend_instance_id: id,
            producer: WgpuRenderProducerId::new(id),
            closed: Arc::new(AtomicBool::new(false)),
            instances: PolicyInstances::default(),
        }
    }

    fn build(&mut self, input: &WgpuRenderInput) -> Result<BuiltPolicy, WgpuRenderProgramError> {
        self.instances.build(
            input,
            self.backend_instance_id,
            self.producer,
            &self.closed,
            FORMAT,
        )
    }
}

fn image_packet(frame_index: u64) -> RenderFramePacket {
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(2, 2).unwrap(),
    )
    .unwrap();
    let viewport = ViewportRect::from_extent(target.extent()).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Image(target.image()),
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    build_image_render_frame_packet(
        RenderFrameSourceGeneration::try_acquire().unwrap(),
        frame_index,
        &views,
        target,
    )
    .unwrap()
    .unwrap()
}

fn capture(tag: u32) -> Arc<ClearCapture> {
    Arc::new(ClearCapture {
        tag,
        color: wgpu::Color {
            r: f64::from(tag) / 255.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    })
}

fn clear_program(
    context: WgpuPolicyBuildContext<'_>,
    color: wgpu::Color,
    feature: Option<&WgpuFeatureInstance<CpuOnlyFeature>>,
) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
    clear_program_with_limits(context, color, feature, GraphLimits::default())
}

fn clear_program_with_limits(
    context: WgpuPolicyBuildContext<'_>,
    color: wgpu::Color,
    feature: Option<&WgpuFeatureInstance<CpuOnlyFeature>>,
    limits: GraphLimits,
) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
    let packet = context.packet();
    let extent = packet.target().extent();
    let mut builder =
        WgpuRenderProgramBuilder::new(packet.source_generation(), packet.frame_index(), limits)?;
    let output = builder.texture::<FinalColor>(WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("policy-cpu-test-final"),
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
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    })?;
    let writes = [output.clear(color, AccessRole::Attachment)];
    if let Some(feature) = feature {
        builder.operation(1, feature, Arc::new(()), &[], &writes)?;
    } else {
        builder.initialize(1, &writes)?;
    }
    WgpuPolicyProgram::new(
        builder.finish(&[output.output()])?,
        &output,
        WgpuPolicyFrameFacts {
            frame: FrameStats::default(),
            ..Default::default()
        },
    )
}

struct CpuOnlyFeature(WgpuDeviceRequirements);

impl WgpuRenderFeature for CpuOnlyFeature {
    type Capture = ();
    type State = ();
    type Prepared = ();
    type Report = ();

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.0.clone()
    }

    fn preflight(
        &self,
        _: &(),
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        panic!("CPU policy construction must not run native preflight")
    }

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        panic!("CPU policy construction must not create native state")
    }

    fn prepare(
        &self,
        _: &mut (),
        _: &(),
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        panic!("CPU policy construction must not prepare native work")
    }

    fn encode(
        &self,
        _: &(),
        _: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        panic!("CPU policy construction must not encode native work")
    }
}

struct FeaturePolicy {
    feature: WgpuFeatureDefinition<CpuOnlyFeature>,
    declared: WgpuDeviceRequirements,
}

impl WgpuRenderPolicy for FeaturePolicy {
    type Capture = ();
    type Instances = WgpuFeatureInstance<CpuOnlyFeature>;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.declared.clone()
    }

    fn create_instances(
        &self,
        context: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<Self::Instances, WgpuRenderProgramError> {
        context.feature(&self.feature)
    }

    fn build(
        &self,
        instances: &Self::Instances,
        _: &(),
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        clear_program(context, wgpu::Color::BLACK, Some(instances))
    }
}

struct SmuggledFeaturePolicy(WgpuFeatureInstance<CpuOnlyFeature>);

impl WgpuRenderPolicy for SmuggledFeaturePolicy {
    type Capture = ();
    type Instances = ();

    fn create_instances(
        &self,
        _: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn build(
        &self,
        _: &(),
        _: &(),
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        clear_program(context, wgpu::Color::BLACK, Some(&self.0))
    }
}

struct ValidatorPanicPolicy(Arc<AtomicBool>);

impl WgpuRenderPolicy for ValidatorPanicPolicy {
    type Capture = ClearCapture;
    type Instances = ();

    fn create_instances(
        &self,
        _: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn validate_capture(
        &self,
        _: &RenderFramePacket,
        _: &ClearCapture,
    ) -> Result<(), WgpuRenderProgramError> {
        assert!(
            !self.0.swap(false, Ordering::Relaxed),
            "requested policy capture validation panic",
        );
        Ok(())
    }

    fn build(
        &self,
        _: &(),
        capture: &ClearCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        clear_program(context, capture.color, None)
    }
}

struct MetadataBudgetPolicy(Arc<AtomicUsize>);

fn metadata_fixture_limits(maximum: usize) -> GraphLimits {
    GraphLimits {
        max_resources: 1,
        max_operations: 1,
        max_accesses: 1,
        max_outputs: 1,
        max_modeled_metadata_bytes: maximum,
        ..Default::default()
    }
}

impl WgpuRenderPolicy for MetadataBudgetPolicy {
    type Capture = usize;
    type Instances = usize;

    fn create_instances(
        &self,
        _: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<usize, WgpuRenderProgramError> {
        Ok(17)
    }

    fn build(
        &self,
        _: &usize,
        _: &usize,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        clear_program_with_limits(
            context,
            wgpu::Color::BLACK,
            None,
            metadata_fixture_limits(self.0.load(Ordering::Relaxed)),
        )
    }
}

/// Models this fixture's inline Arc values and strong/weak counters, excluding allocator
/// rounding and policy-owned indirect data. No allocator or production accounting hook is used.
#[repr(C)]
struct ModeledArcValue<T> {
    strong: AtomicUsize,
    weak: AtomicUsize,
    value: T,
}

fn expected_budget_policy_metadata() -> usize {
    // Walk the ownership graph: admission, definition, caller data, erased capture,
    // producer-local instances, and the execution owner each have one Arc allocation.
    let allocations = [
        size_of::<ModeledArcValue<PolicyCaptureAdmission>>(),
        size_of::<ModeledArcValue<PolicyDefinition<MetadataBudgetPolicy>>>(),
        size_of::<ModeledArcValue<usize>>(),
        size_of::<ModeledArcValue<TypedPolicyCapture<MetadataBudgetPolicy>>>(),
        size_of::<ModeledArcValue<usize>>(),
        size_of::<ModeledArcValue<PolicyExecutionCustody>>(),
    ];
    let table = MAX_POLICIES_PER_PRODUCER * size_of::<Option<PolicyInstanceEntry>>();
    // The program already pays for its submission root. Adding policy custody adds one
    // Arc handle to that root, independently of the submission helper's implementation.
    allocations.into_iter().sum::<usize>() + table + size_of::<Arc<PolicyExecutionCustody>>()
}

#[test]
fn two_installations_of_one_definition_cache_independently_and_reuse_each_installation() {
    let probe = Arc::new(PolicyProbe::default());
    let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
    let first = definition.install().unwrap();
    let second = definition.install().unwrap();
    let first_input = first.capture(image_packet(1), capture(10)).unwrap();
    let next_input = first.capture(image_packet(2), capture(11)).unwrap();
    let second_input = second.capture(image_packet(1), capture(20)).unwrap();
    let mut cpu = CpuInstanceSet::new(1);

    let first_built = cpu.build(&first_input).unwrap();
    let replay = cpu.build(&first_input).unwrap();
    let next_built = cpu.build(&next_input).unwrap();
    let second_built = cpu.build(&second_input).unwrap();

    assert_eq!(probe.created.load(Ordering::Relaxed), 2);
    assert_eq!(cpu.instances.len(), 2);
    let first_entry = cpu
        .instances
        .0
        .iter()
        .flatten()
        .find(|entry| entry.installation == first.id())
        .unwrap();
    let second_entry = cpu
        .instances
        .0
        .iter()
        .flatten()
        .find(|entry| entry.installation == second.id())
        .unwrap();
    assert_eq!(first_entry.definition, second_entry.definition);
    assert_ne!(first_entry.installation, second_entry.installation,);
    assert!(Arc::ptr_eq(
        &first_built.custody._instances,
        &replay.custody._instances,
    ));
    assert!(Arc::ptr_eq(
        &first_built.custody._instances,
        &next_built.custody._instances,
    ));
    assert!(!Arc::ptr_eq(
        &first_built.custody._instances,
        &second_built.custody._instances,
    ));
    assert_eq!(
        *probe.builds.lock().unwrap(),
        [(1, 10), (1, 10), (1, 11), (2, 20)],
    );
}

#[test]
fn accepted_input_keeps_its_original_definition_and_capture_across_close_drop_and_reconstruction() {
    let probe = Arc::new(PolicyProbe::default());
    let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
    let original_definition = Arc::downgrade(&definition.0);
    let mut installation = definition.install().unwrap();
    let data = capture(41);
    let original_capture = Arc::downgrade(&data);
    let input = installation
        .capture(image_packet(7), Arc::clone(&data))
        .unwrap();
    drop(data);
    drop(definition);

    installation.close();
    assert!(matches!(
        installation.capture(image_packet(8), capture(42)),
        Err(WgpuRenderProgramError::PolicyClosed)
    ));
    let mut original_host_instances = CpuInstanceSet::new(1);
    let built = original_host_instances.build(&input).unwrap();
    drop(installation);
    assert!(original_definition.upgrade().is_some());
    assert!(original_capture.upgrade().is_some());
    let replay = original_host_instances.build(&input).unwrap();
    assert!(Arc::ptr_eq(
        &built.custody._instances,
        &replay.custody._instances,
    ));

    original_host_instances
        .closed
        .store(true, Ordering::Release);
    drop(original_host_instances);
    let mut replacement_host_instances = CpuInstanceSet::new(2);
    let reconstructed = replacement_host_instances.build(&input).unwrap();
    assert!(!Arc::ptr_eq(
        &built.custody._instances,
        &reconstructed.custody._instances,
    ));
    assert_eq!(probe.created.load(Ordering::Relaxed), 2);
    assert_eq!(*probe.builds.lock().unwrap(), [(1, 41), (1, 41), (2, 41)]);
    assert_eq!(
        reconstructed.output.program.source_generation(),
        input.packet().source_generation(),
    );
    assert_eq!(reconstructed.output.program.frame_index(), 7);

    drop(input);
    assert!(original_definition.upgrade().is_some());
    assert!(original_capture.upgrade().is_some());
    drop((built, replay, reconstructed));
    replacement_host_instances.instances.maintain();
    assert!(replacement_host_instances.instances.is_empty());
    assert!(original_definition.upgrade().is_none());
    assert!(original_capture.upgrade().is_none());
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 2);
}

#[test]
fn maintenance_follows_installation_input_and_built_custody_instead_of_lifetime_ids() {
    let probe = Arc::new(PolicyProbe::default());
    let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
    let mut cpu = CpuInstanceSet::new(1);

    // More than two complete capacity windows must not consume a lifetime registration cap.
    for frame in 0..129 {
        let mut installation = definition.install().unwrap();
        let admission = Arc::downgrade(installation.admission());
        let input = installation
            .capture(image_packet(frame), capture(1))
            .unwrap();
        let built = cpu.build(&input).unwrap();
        let entry = cpu
            .instances
            .0
            .iter()
            .flatten()
            .find(|entry| entry.installation == installation.id())
            .unwrap();
        let instances = Arc::downgrade(&entry.value);
        drop((built, input));
        cpu.instances.maintain();
        assert_eq!(
            cpu.instances.len(),
            1,
            "installation alone retains its cache",
        );

        let input = installation
            .capture(image_packet(frame), capture(2))
            .unwrap();
        installation.close();
        cpu.instances.maintain();
        assert_eq!(
            cpu.instances.len(),
            1,
            "close does not revoke accepted input",
        );
        drop(installation);
        cpu.instances.maintain();
        assert_eq!(
            cpu.instances.len(),
            1,
            "accepted input alone retains its cache",
        );
        let built = cpu.build(&input).unwrap();
        assert_eq!(probe.created.load(Ordering::Relaxed), frame as usize + 1);
        drop(input);
        cpu.instances.maintain();
        assert_eq!(
            cpu.instances.len(),
            1,
            "BuiltPolicy alone retains its cache",
        );

        let custody = Arc::clone(&built.custody);
        drop(built);
        cpu.instances.maintain();
        assert_eq!(
            cpu.instances.len(),
            1,
            "submitted custody outlives its wrapper",
        );
        assert!(instances.upgrade().is_some());
        drop(custody);
        cpu.instances.maintain();
        assert!(cpu.instances.is_empty());
        assert!(admission.upgrade().is_none());
        assert!(instances.upgrade().is_none());
        assert_eq!(probe.dropped.load(Ordering::Relaxed), frame as usize + 1);
    }
}

#[test]
fn exactly_64_live_captures_fill_the_cache_and_releasing_one_allows_the_refused_input() {
    let probe = Arc::new(PolicyProbe::default());
    let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
    let mut cpu = CpuInstanceSet::new(1);
    let mut retained_inputs = Vec::new();
    for frame in 0..64 {
        let installation = definition.install().unwrap();
        let input = installation
            .capture(image_packet(frame), capture(1))
            .unwrap();
        drop(installation);
        drop(cpu.build(&input).unwrap());
        retained_inputs.push(input);
    }
    assert_eq!(cpu.instances.len(), 64);
    assert_eq!(probe.created.load(Ordering::Relaxed), 64);
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 0);

    let installation = definition.install().unwrap();
    let overflow = installation.capture(image_packet(64), capture(2)).unwrap();
    drop(installation);
    assert!(matches!(
        cpu.build(&overflow),
        Err(WgpuRenderProgramError::PolicyCapacity)
    ));
    assert_eq!(cpu.instances.len(), 64);
    assert_eq!(probe.created.load(Ordering::Relaxed), 64);
    assert!(
        !cpu.instances
            .0
            .iter()
            .flatten()
            .any(|entry| entry.installation == overflow.installation())
    );

    // Full capacity still admits another build of an already cached installation.
    drop(cpu.build(&retained_inputs[0]).unwrap());
    assert_eq!(probe.created.load(Ordering::Relaxed), 64);
    drop(retained_inputs.pop().unwrap());
    // build itself must reclaim the released entry before retrying admission.
    let recovered = cpu.build(&overflow).unwrap();
    assert_eq!(cpu.instances.len(), 64);
    assert_eq!(probe.created.load(Ordering::Relaxed), 65);
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 1);

    drop((retained_inputs, overflow, recovered));
    cpu.instances.maintain();
    assert!(cpu.instances.is_empty());
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 65);
}

#[test]
fn producer_close_refuses_both_cached_and_first_execution_of_accepted_input() {
    for cached in [false, true] {
        let probe = Arc::new(PolicyProbe::default());
        let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
        let installation = definition.install().unwrap();
        let input = installation.capture(image_packet(1), capture(1)).unwrap();
        let mut cpu = CpuInstanceSet::new(1);
        if cached {
            drop(cpu.build(&input).unwrap());
        }
        let builds_before_close = probe.builds.lock().unwrap().len();
        cpu.closed.store(true, Ordering::Release);
        assert!(matches!(
            cpu.build(&input),
            Err(WgpuRenderProgramError::FeatureClosed)
        ));
        assert_eq!(probe.created.load(Ordering::Relaxed), usize::from(cached));
        assert_eq!(probe.builds.lock().unwrap().len(), builds_before_close);
        assert_eq!(cpu.instances.len(), usize::from(cached));
    }
}

#[test]
fn feature_creation_refuses_an_undeclared_feature_before_any_native_work() {
    let required =
        WgpuDeviceRequirements::new(wgpu::Features::TIMESTAMP_QUERY, wgpu::Limits::default());
    let definition = WgpuPolicyDefinition::new(FeaturePolicy {
        feature: WgpuFeatureDefinition::new(CpuOnlyFeature(required)),
        declared: WgpuDeviceRequirements::default(),
    });
    let installation = definition.install().unwrap();
    let input = installation.capture(image_packet(1), Arc::new(())).unwrap();
    let mut cpu = CpuInstanceSet::new(1);
    assert!(matches!(
        cpu.build(&input),
        Err(WgpuRenderProgramError::PolicyRequirements(
            WgpuDeviceRequirementsError::RequiredFeaturesNotEnabled { missing }
        )) if missing == wgpu::Features::TIMESTAMP_QUERY
    ));
    assert!(cpu.instances.is_empty());
}

#[test]
fn feature_creation_refuses_a_stronger_limit_than_the_original_policy_request() {
    let baseline = wgpu::Limits::default();
    let required_buffer_size = baseline.max_buffer_size + 4;
    let required = WgpuDeviceRequirements::new(
        wgpu::Features::empty(),
        wgpu::Limits {
            max_buffer_size: required_buffer_size,
            ..Default::default()
        },
    );
    let definition = WgpuPolicyDefinition::new(FeaturePolicy {
        feature: WgpuFeatureDefinition::new(CpuOnlyFeature(required)),
        declared: WgpuDeviceRequirements::default(),
    });
    let installation = definition.install().unwrap();
    let input = installation.capture(image_packet(1), Arc::new(())).unwrap();
    let mut cpu = CpuInstanceSet::new(1);
    assert!(matches!(
        cpu.build(&input),
        Err(WgpuRenderProgramError::PolicyRequirements(
            WgpuDeviceRequirementsError::RequiredLimitNotEnabled { name, required, enabled }
        )) if name == "max_buffer_size"
            && required == required_buffer_size
            && enabled == baseline.max_buffer_size
    ));
    assert!(cpu.instances.is_empty());
}

#[test]
fn matching_declared_features_and_limits_build_an_exact_feature_operation_using_only_cpu() {
    let required = WgpuDeviceRequirements::new(
        wgpu::Features::TIMESTAMP_QUERY,
        wgpu::Limits {
            max_buffer_size: wgpu::Limits::default().max_buffer_size + 4,
            ..Default::default()
        },
    );
    let definition = WgpuPolicyDefinition::new(FeaturePolicy {
        feature: WgpuFeatureDefinition::new(CpuOnlyFeature(required.clone())),
        declared: required,
    });
    let mut installation = definition.install().unwrap();
    let input = installation.capture(image_packet(1), Arc::new(())).unwrap();
    let mut cpu = CpuInstanceSet::new(1);
    let built = cpu.build(&input).unwrap();
    installation.close();
    drop(installation);
    let replay = cpu.build(&input).unwrap();
    assert!(Arc::ptr_eq(
        &built.custody._instances,
        &replay.custody._instances,
    ));
    let instance = cpu
        .instances
        .0
        .iter()
        .flatten()
        .find(|entry| entry.installation == input.installation())
        .unwrap()
        .value
        .downcast_ref::<WgpuFeatureInstance<CpuOnlyFeature>>()
        .unwrap();
    let operation = built.output.program.0.operations[0]
        .capture
        .as_ref()
        .unwrap();
    assert!(Arc::ptr_eq(operation.identity(), &instance.identity));
    assert_eq!(
        instance.identity.backend_instance_id,
        cpu.backend_instance_id
    );
    assert_eq!(instance.identity.producer, cpu.producer);
    assert!(!instance.identity.is_closed());

    cpu.closed.store(true, Ordering::Release);
    let mut replacement = CpuInstanceSet::new(2);
    let reconstructed = replacement.build(&input).unwrap();
    let new_operation = reconstructed.output.program.0.operations[0]
        .capture
        .as_ref()
        .unwrap();
    assert_ne!(new_operation.identity().id, instance.identity.id);
    assert_eq!(new_operation.identity().backend_instance_id, 2);
    assert_eq!(new_operation.identity().producer, replacement.producer);
    assert!(!new_operation.identity().is_closed());
}

#[test]
fn final_compiled_program_refuses_a_feature_created_outside_the_closed_policy_request() {
    let required =
        WgpuDeviceRequirements::new(wgpu::Features::TIMESTAMP_QUERY, wgpu::Limits::default());
    let feature = WgpuFeatureDefinition::new(CpuOnlyFeature(required.clone()));
    let mut cpu = CpuInstanceSet::new(1);
    let instance = WgpuPolicyInstanceContext {
        backend_instance_id: cpu.backend_instance_id,
        producer: cpu.producer,
        closed: &cpu.closed,
        requirements: &required,
    }
    .feature(&feature)
    .unwrap();
    let definition = WgpuPolicyDefinition::new(SmuggledFeaturePolicy(instance));
    let installation = definition.install().unwrap();
    let input = installation.capture(image_packet(1), Arc::new(())).unwrap();

    // The public graph builder accepts the live exact instance. The policy boundary must
    // independently reject its undeclared request after the complete graph is compiled.
    definition
        .policy()
        .build(
            &(),
            &(),
            WgpuPolicyBuildContext {
                packet: input.packet(),
                format: FORMAT,
            },
        )
        .unwrap();
    assert!(matches!(
        cpu.build(&input),
        Err(WgpuRenderProgramError::PolicyRequirements(
            WgpuDeviceRequirementsError::RequiredFeaturesNotEnabled { missing }
        )) if missing == wgpu::Features::TIMESTAMP_QUERY
    ));
}

#[test]
fn create_or_build_panic_retires_only_the_affected_installation_and_allows_reconstruction() {
    for panic_in_create in [true, false] {
        let probe = Arc::new(PolicyProbe::default());
        let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
        let healthy = definition.install().unwrap();
        let failing = definition.install().unwrap();
        let healthy_input = healthy.capture(image_packet(1), capture(1)).unwrap();
        let failing_input = failing.capture(image_packet(1), capture(2)).unwrap();
        let mut cpu = CpuInstanceSet::new(1);
        let healthy_built = cpu.build(&healthy_input).unwrap();
        let previous = if panic_in_create {
            probe.panic_create.store(true, Ordering::Relaxed);
            None
        } else {
            let previous = cpu.build(&failing_input).unwrap();
            probe.panic_build.store(true, Ordering::Relaxed);
            Some(previous)
        };
        let retired = previous
            .as_ref()
            .map(|built| Arc::downgrade(&built.custody._instances));

        assert!(matches!(
            cpu.build(&failing_input),
            Err(WgpuRenderProgramError::PolicyPanicked)
        ));
        assert_eq!(cpu.instances.len(), 1);
        let healthy_entry = cpu
            .instances
            .0
            .iter()
            .flatten()
            .find(|entry| entry.installation == healthy.id())
            .unwrap();
        assert!(Arc::ptr_eq(
            &healthy_entry.value,
            &healthy_built.custody._instances,
        ));
        if let Some(retired) = &retired {
            assert!(
                retired.upgrade().is_some(),
                "earlier built work retains retired instances",
            );
        }
        drop(previous);
        if let Some(retired) = retired {
            assert!(retired.upgrade().is_none());
        }

        let created_before_retry = probe.created.load(Ordering::Relaxed);
        let replay = cpu.build(&healthy_input).unwrap();
        assert!(Arc::ptr_eq(
            &replay.custody._instances,
            &healthy_built.custody._instances,
        ));
        assert_eq!(probe.created.load(Ordering::Relaxed), created_before_retry);
        let recovered = cpu.build(&failing_input).unwrap();
        assert_eq!(
            probe.created.load(Ordering::Relaxed),
            created_before_retry + 1
        );
        assert!(!Arc::ptr_eq(
            &recovered.custody._instances,
            &healthy_built.custody._instances,
        ));
        assert_eq!(cpu.instances.len(), 2);
    }
}

#[test]
fn first_capture_contains_validator_panic_and_allows_the_original_installation_to_retry() {
    let panic = Arc::new(AtomicBool::new(true));
    let definition = WgpuPolicyDefinition::new(ValidatorPanicPolicy(Arc::clone(&panic)));
    let installation = definition.install().unwrap();
    let data = capture(41);
    let mut cpu = CpuInstanceSet::new(1);

    assert!(matches!(
        installation.capture(image_packet(1), Arc::clone(&data)),
        Err(WgpuRenderProgramError::PolicyPanicked)
    ));
    assert!(installation.is_open());
    assert!(cpu.instances.is_empty());
    assert_eq!(
        Arc::strong_count(&data),
        1,
        "the refused draft must not retain caller data"
    );

    let input = installation
        .capture(image_packet(2), Arc::clone(&data))
        .unwrap();
    assert_eq!(input.installation(), installation.id());
    assert!(std::ptr::eq(
        input.captured::<ValidatorPanicPolicy>().unwrap(),
        data.as_ref(),
    ));
    let built = cpu.build(&input).unwrap();
    assert_eq!(built.output.program.frame_index(), 2);
    assert_eq!(cpu.instances.len(), 1);
}

#[test]
fn image_adaptation_contains_validator_panic_without_rebinding_or_consuming_the_window_input() {
    use nara_window::{Window, WindowId, WindowResolution};

    let runtime = nara_app::RuntimeInstance::start(nara_app::App::new()).unwrap();
    let mut window = Window::new("policy validation capture", WindowResolution::new(2, 2));
    window.id = WindowId::new(73);
    let viewport = ViewportRect::new(0, 0, 2, 2).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    let packet = nara_render::build_render_frame_packet(
        runtime.generation(),
        7,
        Some(window.id),
        [&window],
        &views,
    )
    .unwrap()
    .unwrap();
    let original_target = packet.target();
    let panic = Arc::new(AtomicBool::new(false));
    let definition = WgpuPolicyDefinition::new(ValidatorPanicPolicy(Arc::clone(&panic)));
    let mut installation = definition.install().unwrap();
    let identity = installation.id();
    let data = capture(42);
    let input = installation.capture(packet, Arc::clone(&data)).unwrap();
    let original_packet = Arc::clone(&input.packet);
    let original_capture = Arc::clone(&input.capture);
    installation.close();
    drop((installation, definition));
    let image_target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(2, 2).unwrap(),
    )
    .unwrap();

    panic.store(true, Ordering::Relaxed);
    assert!(matches!(
        input.with_image_target(image_target),
        Err(WgpuRenderProgramError::PolicyPanicked)
    ));
    assert_eq!(input.installation(), identity);
    assert_eq!(input.packet().target(), original_target);
    assert_eq!(input.packet().frame_index(), 7);
    assert_eq!(
        input.packet().source_generation(),
        runtime.generation().into()
    );
    assert!(Arc::ptr_eq(&input.packet, &original_packet));
    assert!(Arc::ptr_eq(&input.capture, &original_capture));
    assert!(std::ptr::eq(
        input.captured::<ValidatorPanicPolicy>().unwrap(),
        data.as_ref(),
    ));

    let mut cpu = CpuInstanceSet::new(1);
    let original_built = cpu.build(&input).unwrap();
    let adapted = input.with_image_target(image_target).unwrap();
    assert_eq!(adapted.installation(), identity);
    assert_eq!(
        adapted.packet().target(),
        nara_render::RenderFrameTarget::Image(image_target)
    );
    assert_eq!(adapted.packet().views(), input.packet().views());
    assert_eq!(
        adapted.packet().source_generation(),
        input.packet().source_generation()
    );
    assert_eq!(adapted.packet().frame_index(), input.packet().frame_index());
    assert!(Arc::ptr_eq(&adapted.capture, &original_capture));
    assert!(std::ptr::eq(
        adapted.captured::<ValidatorPanicPolicy>().unwrap(),
        data.as_ref(),
    ));
    let retried = cpu.build(&adapted).unwrap();
    assert!(Arc::ptr_eq(
        &original_built.custody._instances,
        &retried.custody._instances,
    ));
    assert_eq!(cpu.instances.len(), 1);
    assert_eq!(input.packet().target(), original_target);
    runtime.finish().unwrap();
}

#[test]
fn fixed_policy_metadata_fits_the_exact_program_residual_and_refuses_one_byte_less() {
    let maximum = Arc::new(AtomicUsize::new(usize::MAX));
    let definition = WgpuPolicyDefinition::new(MetadataBudgetPolicy(Arc::clone(&maximum)));
    let installation = definition.install().unwrap();
    let data = Arc::new(41_usize);
    let input = installation
        .capture(image_packet(7), Arc::clone(&data))
        .unwrap();
    // Program storage has its own admission contract. Independently account for the
    // policy layer rather than learning its required bytes from a successful build.
    let program_bytes =
        WgpuRenderProgramBuilder::required_metadata_bytes(metadata_fixture_limits(usize::MAX))
            .unwrap();
    let expected = expected_budget_policy_metadata();
    let exact = program_bytes + expected;
    let mut cpu = CpuInstanceSet::new(1);

    maximum.store(exact - 1, Ordering::Relaxed);
    let standalone = definition
        .policy()
        .build(
            &17,
            data.as_ref(),
            WgpuPolicyBuildContext {
                packet: input.packet(),
                format: FORMAT,
            },
        )
        .unwrap();
    assert_eq!(standalone.program.modeled_metadata_bytes(), program_bytes);
    assert!(matches!(
        cpu.build(&input),
        Err(WgpuRenderProgramError::Graph(
            crate::GraphError::LimitExceeded(crate::LimitKind::MetadataBytes,)
        ))
    ));
    assert_eq!(
        Arc::strong_count(&input.capture),
        1,
        "refusal must not retain execution custody"
    );
    let entry = cpu
        .instances
        .0
        .iter()
        .flatten()
        .find(|entry| entry.installation == input.installation())
        .unwrap();
    assert_eq!(Arc::strong_count(&entry.value), 1);
    assert!(std::ptr::eq(
        input.captured::<MetadataBudgetPolicy>().unwrap(),
        data.as_ref(),
    ));

    maximum.store(exact, Ordering::Relaxed);
    let built = cpu.build(&input).unwrap();
    assert_eq!(built.metadata_bytes, expected);
    assert_eq!(built.output.program.modeled_metadata_bytes(), program_bytes);
    assert_eq!(built.output.program.0.max_modeled_metadata_bytes, exact);
    assert_eq!(
        built.output.program.modeled_metadata_bytes() + built.metadata_bytes,
        exact
    );
    assert_eq!(cpu.instances.len(), 1);
}

#[test]
fn pruning_64_policy_instances_to_one_keeps_the_entire_fixed_table_in_the_metadata_cost() {
    assert_eq!(MAX_POLICIES_PER_PRODUCER, 64);
    let table_bytes = 64 * size_of::<Option<PolicyInstanceEntry>>();
    let expected = expected_budget_policy_metadata();
    let program_bytes =
        WgpuRenderProgramBuilder::required_metadata_bytes(metadata_fixture_limits(usize::MAX))
            .unwrap();
    let exact = program_bytes + expected;
    let maximum = Arc::new(AtomicUsize::new(exact));
    let definition = WgpuPolicyDefinition::new(MetadataBudgetPolicy(Arc::clone(&maximum)));
    let mut cpu = CpuInstanceSet::new(1);
    let mut inputs = Vec::new();
    for frame in 0..64 {
        let installation = definition.install().unwrap();
        let input = installation
            .capture(image_packet(frame), Arc::new(frame as usize))
            .unwrap();
        drop(installation);
        let built = cpu.build(&input).unwrap();
        assert_eq!(built.metadata_bytes, expected);
        inputs.push(input);
    }
    assert_eq!(cpu.instances.len(), 64);
    assert_eq!(std::mem::size_of_val(&cpu.instances), table_bytes);
    // Retain an installation from the middle of the admission sequence. Compaction may
    // move its slot, but the original installation and CPU instance remain its identity.
    let survivor = inputs.swap_remove(31);
    let dense = cpu.build(&survivor).unwrap();
    drop(inputs);
    cpu.instances.maintain();
    assert_eq!(cpu.instances.len(), 1);
    assert_eq!(std::mem::size_of_val(&cpu.instances), table_bytes);
    assert_eq!(
        cpu.instances
            .0
            .iter()
            .flatten()
            .next()
            .unwrap()
            .installation,
        survivor.installation()
    );
    let sparse = cpu.build(&survivor).unwrap();
    assert_eq!(sparse.metadata_bytes, expected);
    assert_eq!(sparse.metadata_bytes, dense.metadata_bytes);
    assert!(Arc::ptr_eq(
        &dense.custody._instances,
        &sparse.custody._instances
    ));

    maximum.store(exact - 1, Ordering::Relaxed);
    assert!(matches!(
        cpu.build(&survivor),
        Err(WgpuRenderProgramError::Graph(
            crate::GraphError::LimitExceeded(crate::LimitKind::MetadataBytes,)
        ))
    ));
    maximum.store(exact, Ordering::Relaxed);
    let retry = cpu.build(&survivor).unwrap();
    assert_eq!(retry.metadata_bytes, expected);
    assert!(Arc::ptr_eq(
        &dense.custody._instances,
        &retry.custody._instances
    ));

    drop((survivor, dense, sparse, retry));
    cpu.instances.maintain();
    assert!(cpu.instances.is_empty());
    assert_eq!(std::mem::size_of_val(&cpu.instances), table_bytes);
}
