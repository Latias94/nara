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
    WgpuFeatureDefinition, WgpuFeatureEncodeContext, WgpuFeaturePreflightContext,
    WgpuFeaturePrepareContext, WgpuProgramTextureDescriptor, WgpuRenderProgramBuilder,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct FinalColor;

#[derive(Default)]
struct PolicyProbe {
    created: AtomicUsize,
    dropped: AtomicUsize,
    builds: Mutex<Vec<(usize, u32)>>,
    panic_bind: AtomicBool,
    panic_build: AtomicBool,
}

struct ClearPolicy(Arc<PolicyProbe>);

struct ClearCapture {
    tag: u32,
    color: wgpu::Color,
}

struct ClearBindings {
    serial: usize,
    probe: Arc<PolicyProbe>,
}

impl Drop for ClearBindings {
    fn drop(&mut self) {
        self.probe.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

impl WgpuRenderPolicy for ClearPolicy {
    type Capture = ClearCapture;
    type Bindings = ClearBindings;

    fn bind(
        &self,
        _: &ClearCapture,
        _: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        assert!(
            !self.0.panic_bind.swap(false, Ordering::Relaxed),
            "requested CPU binding panic"
        );
        Ok(ClearBindings {
            serial: self.0.created.fetch_add(1, Ordering::Relaxed) + 1,
            probe: Arc::clone(&self.0),
        })
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
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
            .push((bindings.serial, capture.tag));
        clear_program(context, capture.color, None)
    }
}

// Exercises candidate staging and publication without a native Host, device or window.
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
            None,
        )
    }

    fn commit(&mut self, built: &BuiltPolicy) -> RetiredPolicyBindings {
        self.instances.commit(built)
    }

    fn published(&self, installation: WgpuPolicyInstallationId) -> Option<&Arc<PolicyBindings>> {
        self.instances
            .policies
            .iter()
            .flatten()
            .find(|entry| entry.installation == installation)
            .and_then(|entry| entry.value.as_ref())
    }
}

fn bindings<P: WgpuRenderPolicy>(built: &BuiltPolicy) -> &P::Bindings {
    built
        .custody
        ._bindings
        ._value
        .downcast_ref::<P::Bindings>()
        .unwrap()
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
        id: nara_render::RenderViewId::try_acquire().unwrap(),
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
    feature: WgpuFeatureDeclaration<CpuOnlyFeature>,
    declared: WgpuDeviceRequirements,
}

impl WgpuRenderPolicy for FeaturePolicy {
    type Capture = ();
    type Bindings = Arc<WgpuFeatureInstance<CpuOnlyFeature>>;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.declared.clone()
    }

    fn bind(
        &self,
        _: &(),
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        context.feature(&self.feature)
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        _: &(),
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        clear_program(context, wgpu::Color::BLACK, Some(bindings))
    }
}

#[derive(Default)]
struct DynamicCapture {
    effects: Vec<WgpuFeatureDeclaration<CpuOnlyFeature>>,
    fail_bind: bool,
    panic_bind: bool,
    fail_build: bool,
    panic_build: bool,
}

struct DynamicPolicy;

impl WgpuRenderPolicy for DynamicPolicy {
    type Capture = DynamicCapture;
    type Bindings = Vec<Arc<WgpuFeatureInstance<CpuOnlyFeature>>>;

    fn bind(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        let bindings = capture
            .effects
            .iter()
            .map(|declaration| context.feature(declaration))
            .collect::<Result<Vec<_>, _>>()?;
        assert!(!capture.panic_bind, "requested dynamic binding panic");
        if capture.fail_bind {
            return Err(WgpuRenderProgramError::feature("requested binding refusal"));
        }
        Ok(bindings)
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        assert!(!capture.panic_build, "requested dynamic construction panic");
        if capture.fail_build {
            return Err(WgpuRenderProgramError::feature(
                "requested construction refusal",
            ));
        }
        clear_program(
            context,
            wgpu::Color::BLACK,
            bindings.first().map(Arc::as_ref),
        )
    }
}

fn declaration() -> WgpuFeatureDeclaration<CpuOnlyFeature> {
    WgpuFeatureDefinition::new(CpuOnlyFeature(WgpuDeviceRequirements::default())).declare()
}

fn dynamic_input(
    installation: &WgpuPolicyInstallation<DynamicPolicy>,
    frame: u64,
    effects: &[WgpuFeatureDeclaration<CpuOnlyFeature>],
) -> WgpuRenderInput {
    installation
        .capture(
            image_packet(frame),
            Arc::new(DynamicCapture {
                effects: effects.to_vec(),
                ..Default::default()
            }),
        )
        .unwrap()
}

struct SmuggledFeaturePolicy(WgpuFeatureInstance<CpuOnlyFeature>);

impl WgpuRenderPolicy for SmuggledFeaturePolicy {
    type Capture = ();
    type Bindings = ();

    fn bind(
        &self,
        _: &(),
        _: &mut WgpuPolicyBindingContext<'_>,
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
    type Bindings = ();

    fn bind(
        &self,
        _: &ClearCapture,
        _: &mut WgpuPolicyBindingContext<'_>,
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
    type Bindings = usize;

    fn bind(
        &self,
        _: &usize,
        _: &mut WgpuPolicyBindingContext<'_>,
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
    // typed Bindings, the immutable binding snapshot and execution custody each have one
    // Arc allocation. Multiple Arc handles retain each allocation without counting it twice.
    let allocations = [
        size_of::<ModeledArcValue<PolicyCaptureAdmission>>(),
        size_of::<ModeledArcValue<PolicyDefinition<MetadataBudgetPolicy>>>(),
        size_of::<ModeledArcValue<usize>>(),
        size_of::<ModeledArcValue<TypedPolicyCapture<MetadataBudgetPolicy>>>(),
        size_of::<ModeledArcValue<usize>>(),
        size_of::<ModeledArcValue<PolicyBindings>>(),
        size_of::<ModeledArcValue<PolicyExecutionCustody>>(),
    ];
    let tables = size_of::<PolicyInstances>();
    let stage = MAX_FEATURE_BINDINGS_PER_PRODUCER * size_of::<Arc<FeatureBinding>>()
        + size_of::<PolicyBindingStage>()
        + size_of::<RetiredPolicyBindings>();
    // The program already pays for its submission root. Adding policy custody adds one
    // Arc handle to that root, independently of the submission helper's implementation.
    allocations.into_iter().sum::<usize>()
        + tables
        + stage
        + size_of::<Arc<PolicyExecutionCustody>>()
}

#[test]
fn every_capture_gets_fresh_bindings_and_only_commit_changes_published_membership() {
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

    assert_eq!(probe.created.load(Ordering::Relaxed), 4);
    assert_eq!(cpu.instances.len(), 2);
    assert!(cpu.published(first.id()).is_none());
    assert!(cpu.published(second.id()).is_none());
    assert!(!Arc::ptr_eq(
        &first_built.custody._bindings,
        &replay.custody._bindings,
    ));
    drop(cpu.commit(&first_built));
    assert!(Arc::ptr_eq(
        cpu.published(first.id()).unwrap(),
        &first_built.custody._bindings,
    ));
    let retired = cpu.commit(&next_built);
    assert!(Arc::ptr_eq(
        cpu.published(first.id()).unwrap(),
        &next_built.custody._bindings,
    ));
    assert!(Arc::ptr_eq(
        retired._bindings.as_ref().unwrap(),
        &first_built.custody._bindings,
    ));
    drop(cpu.commit(&second_built));
    assert!(Arc::ptr_eq(
        cpu.published(second.id()).unwrap(),
        &second_built.custody._bindings,
    ));
    assert_eq!(
        *probe.builds.lock().unwrap(),
        [(1, 10), (2, 10), (3, 11), (4, 20)],
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
    assert!(!Arc::ptr_eq(
        &built.custody._bindings,
        &replay.custody._bindings,
    ));

    original_host_instances
        .closed
        .store(true, Ordering::Release);
    drop(original_host_instances);
    let mut replacement_host_instances = CpuInstanceSet::new(2);
    let reconstructed = replacement_host_instances.build(&input).unwrap();
    assert!(!Arc::ptr_eq(
        &built.custody._bindings,
        &reconstructed.custody._bindings,
    ));
    assert_eq!(probe.created.load(Ordering::Relaxed), 3);
    assert_eq!(*probe.builds.lock().unwrap(), [(1, 41), (2, 41), (3, 41)]);
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
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 3);
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
        drop(cpu.commit(&built));
        let first_bindings = Arc::downgrade(cpu.published(installation.id()).unwrap());
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
        let retired = cpu.commit(&built);
        assert!(first_bindings.upgrade().is_some());
        assert_eq!(
            probe.created.load(Ordering::Relaxed),
            2 * (frame as usize + 1)
        );
        drop(retired);
        assert!(first_bindings.upgrade().is_none());
        let bindings = Arc::downgrade(&built.custody._bindings);
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
        assert!(bindings.upgrade().is_some());
        drop(custody);
        cpu.instances.maintain();
        assert!(cpu.instances.is_empty());
        assert!(admission.upgrade().is_none());
        assert!(bindings.upgrade().is_none());
        assert_eq!(
            probe.dropped.load(Ordering::Relaxed),
            2 * (frame as usize + 1)
        );
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
        let built = cpu.build(&input).unwrap();
        drop(cpu.commit(&built));
        drop(built);
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
            .policies
            .iter()
            .flatten()
            .any(|entry| entry.installation == overflow.installation())
    );

    // Full capacity still admits another build of an already cached installation.
    drop(cpu.build(&retained_inputs[0]).unwrap());
    assert_eq!(probe.created.load(Ordering::Relaxed), 65);
    drop(retained_inputs.pop().unwrap());
    // build itself must reclaim the released entry before retrying admission.
    let recovered = cpu.build(&overflow).unwrap();
    assert_eq!(cpu.instances.len(), 64);
    assert_eq!(probe.created.load(Ordering::Relaxed), 66);
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 2);

    drop((retained_inputs, overflow, recovered));
    cpu.instances.maintain();
    assert!(cpu.instances.is_empty());
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 66);
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
        feature: WgpuFeatureDefinition::new(CpuOnlyFeature(required)).declare(),
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
        feature: WgpuFeatureDefinition::new(CpuOnlyFeature(required)).declare(),
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
        feature: WgpuFeatureDefinition::new(CpuOnlyFeature(required.clone())).declare(),
        declared: required,
    });
    let mut installation = definition.install().unwrap();
    let input = installation.capture(image_packet(1), Arc::new(())).unwrap();
    let mut cpu = CpuInstanceSet::new(1);
    let built = cpu.build(&input).unwrap();
    installation.close();
    drop(installation);
    let replay = cpu.build(&input).unwrap();
    assert!(!Arc::ptr_eq(
        &built.custody._bindings,
        &replay.custody._bindings,
    ));
    let instance = bindings::<FeaturePolicy>(&built);
    assert!(Arc::ptr_eq(instance, bindings::<FeaturePolicy>(&replay)));
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
    let instance = WgpuFeatureInstance::new(
        cpu.backend_instance_id,
        cpu.producer,
        Arc::clone(&cpu.closed),
        &feature,
    )
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
                history: None,
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
fn bind_or_build_panic_preserves_both_published_snapshots_and_allows_retry() {
    for panic_in_bind in [true, false] {
        let probe = Arc::new(PolicyProbe::default());
        let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
        let healthy = definition.install().unwrap();
        let failing = definition.install().unwrap();
        let healthy_input = healthy.capture(image_packet(1), capture(1)).unwrap();
        let failing_input = failing.capture(image_packet(1), capture(2)).unwrap();
        let mut cpu = CpuInstanceSet::new(1);
        let healthy_built = cpu.build(&healthy_input).unwrap();
        drop(cpu.commit(&healthy_built));
        let previous = cpu.build(&failing_input).unwrap();
        drop(cpu.commit(&previous));
        let previous_bindings = Arc::downgrade(&previous.custody._bindings);
        drop(previous);
        if panic_in_bind {
            probe.panic_bind.store(true, Ordering::Relaxed);
        } else {
            probe.panic_build.store(true, Ordering::Relaxed);
        }

        assert!(matches!(
            cpu.build(&failing_input),
            Err(WgpuRenderProgramError::PolicyPanicked)
        ));
        assert_eq!(cpu.instances.len(), 2);
        assert!(Arc::ptr_eq(
            cpu.published(healthy.id()).unwrap(),
            &healthy_built.custody._bindings,
        ));
        assert!(Arc::ptr_eq(
            cpu.published(failing.id()).unwrap(),
            &previous_bindings.upgrade().unwrap(),
        ));
        assert_eq!(
            probe.dropped.load(Ordering::Relaxed),
            usize::from(!panic_in_bind),
        );

        let created_before_retry = probe.created.load(Ordering::Relaxed);
        let replay = cpu.build(&healthy_input).unwrap();
        assert!(!Arc::ptr_eq(
            &replay.custody._bindings,
            &healthy_built.custody._bindings,
        ));
        assert_eq!(
            probe.created.load(Ordering::Relaxed),
            created_before_retry + 1,
        );
        let recovered = cpu.build(&failing_input).unwrap();
        assert_eq!(
            probe.created.load(Ordering::Relaxed),
            created_before_retry + 2
        );
        let retired = cpu.commit(&recovered);
        assert!(previous_bindings.upgrade().is_some());
        assert!(Arc::ptr_eq(
            cpu.published(failing.id()).unwrap(),
            &recovered.custody._bindings,
        ));
        drop(retired);
        assert!(previous_bindings.upgrade().is_none());
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
        id: nara_render::RenderViewId::try_acquire().unwrap(),
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
    assert!(!Arc::ptr_eq(
        &original_built.custody._bindings,
        &retried.custody._bindings,
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
                history: None,
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
    assert!(cpu.published(input.installation()).is_none());
    assert!(
        cpu.instances.is_empty(),
        "refusal must not reserve a policy installation",
    );
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
    let table_bytes = size_of::<PolicyInstances>();
    assert!(table_bytes >= 64 * size_of::<Option<PolicyInstanceEntry>>());
    assert!(table_bytes >= 1024 * size_of::<Option<FeatureResolution>>());
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
        drop(cpu.commit(&built));
        assert_eq!(built.metadata_bytes, expected);
        inputs.push(input);
    }
    assert_eq!(cpu.instances.len(), 64);
    assert_eq!(std::mem::size_of_val(&cpu.instances), table_bytes);
    // Retain an installation from the middle. Published storage keeps its full admitted
    // capacity after pruning, while outstanding candidates keep their original slots.
    let survivor = inputs.swap_remove(31);
    let dense = cpu.build(&survivor).unwrap();
    drop(inputs);
    cpu.instances.maintain();
    assert_eq!(cpu.instances.len(), 1);
    assert_eq!(std::mem::size_of_val(&cpu.instances), table_bytes);
    assert_eq!(
        cpu.instances
            .policies
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
    assert!(!Arc::ptr_eq(
        &dense.custody._bindings,
        &sparse.custody._bindings
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
    assert!(!Arc::ptr_eq(
        &dense.custody._bindings,
        &retry.custody._bindings
    ));

    drop((survivor, dense, sparse, retry));
    cpu.instances.maintain();
    assert!(cpu.instances.is_empty());
    assert_eq!(std::mem::size_of_val(&cpu.instances), table_bytes);
}

#[test]
fn declarations_clone_one_process_identity_and_independent_effects_declare_separately() {
    fn assert_cpu<T: Send + Sync>() {}
    assert_cpu::<WgpuFeatureDeclaration<CpuOnlyFeature>>();
    assert_cpu::<DynamicCapture>();
    assert_cpu::<<DynamicPolicy as WgpuRenderPolicy>::Bindings>();

    let definition = WgpuFeatureDefinition::new(CpuOnlyFeature(WgpuDeviceRequirements::default()));
    let first = definition.declare();
    let retained = first.clone();
    let second = definition.declare();
    assert_eq!(first.id(), retained.id());
    assert_ne!(first.id(), second.id());
    assert!(Weak::ptr_eq(&first.lifetime(), &retained.lifetime()));
    assert!(!Weak::ptr_eq(&first.lifetime(), &second.lifetime()));
    assert_eq!(
        first.device_requirements().required_features(),
        definition.device_requirements().required_features(),
    );
    let lifetime = first.lifetime();
    drop((first, definition));
    assert!(lifetime.upgrade().is_some());
    drop(retained);
    assert!(lifetime.upgrade().is_none());
    assert!(second.lifetime().upgrade().is_some());
}

#[test]
fn unpublished_candidates_and_different_installations_share_each_original_actual_owner() {
    let definition = WgpuPolicyDefinition::new(DynamicPolicy);
    let first = definition.install().unwrap();
    let second = definition.install().unwrap();
    let original = declaration();
    let distinct = original.definition().declare();
    let input = dynamic_input(&first, 1, &[original.clone(), distinct.clone()]);
    let reordered = dynamic_input(&second, 2, &[distinct.clone(), original.clone()]);
    let mut cpu = CpuInstanceSet::new(1);
    let built = cpu.build(&input).unwrap();
    let sibling = cpu.build(&input).unwrap();
    let changed_route = cpu.build(&reordered).unwrap();

    assert!(!Arc::ptr_eq(
        &built.custody._bindings,
        &sibling.custody._bindings,
    ));
    for index in 0..2 {
        assert!(Arc::ptr_eq(
            &bindings::<DynamicPolicy>(&built)[index],
            &bindings::<DynamicPolicy>(&sibling)[index],
        ));
        assert!(Arc::ptr_eq(
            &bindings::<DynamicPolicy>(&built)[index],
            &bindings::<DynamicPolicy>(&changed_route)[1 - index],
        ));
    }
    assert!(!Arc::ptr_eq(
        &bindings::<DynamicPolicy>(&built)[0],
        &bindings::<DynamicPolicy>(&built)[1],
    ));
    assert!(
        cpu.instances
            .features
            .0
            .iter()
            .flatten()
            .all(|entry| entry.published.is_none())
    );
    drop(cpu.commit(&built));
    assert!(
        cpu.instances
            .features
            .0
            .iter()
            .flatten()
            .all(|entry| entry.published.is_some())
    );

    // Composition and installation identity can change while the original declaration stays.
    let replacement = WgpuPolicyDefinition::new(DynamicPolicy).install().unwrap();
    let replacement_input = dynamic_input(&replacement, 3, &[original.clone()]);
    let rebound = cpu.build(&replacement_input).unwrap();
    assert!(Arc::ptr_eq(
        &bindings::<DynamicPolicy>(&built)[0],
        &bindings::<DynamicPolicy>(&rebound)[0],
    ));
    let mut another_producer = CpuInstanceSet::new(2);
    let other = another_producer.build(&input).unwrap();
    assert_ne!(
        bindings::<DynamicPolicy>(&built)[0].identity.id,
        bindings::<DynamicPolicy>(&other)[0].identity.id,
    );
}

#[test]
fn first_preparation_after_replacement_and_installation_withdrawal_uses_original_capture() {
    let definition = WgpuPolicyDefinition::new(DynamicPolicy);
    let mut installation = definition.install().unwrap();
    let installation_id = installation.id();
    let effects = (0..4).map(|_| declaration()).collect::<Vec<_>>();
    let original_input = dynamic_input(&installation, 1, &effects);
    let mut replaced = effects.clone();
    replaced[1] = declaration();
    let latest_input = dynamic_input(&installation, 2, &replaced);
    let mut cpu = CpuInstanceSet::new(1);
    let latest = cpu.build(&latest_input).unwrap();
    drop(cpu.commit(&latest));
    installation.close();
    assert!(matches!(
        installation.capture(image_packet(3), Arc::new(DynamicCapture::default())),
        Err(WgpuRenderProgramError::PolicyClosed),
    ));
    drop((installation, definition));

    // This input has never been prepared before the replacement and withdrawal.
    let original = cpu.build(&original_input).unwrap();
    for index in [0, 2, 3] {
        assert!(Arc::ptr_eq(
            &bindings::<DynamicPolicy>(&original)[index],
            &bindings::<DynamicPolicy>(&latest)[index],
        ));
    }
    assert!(!Arc::ptr_eq(
        &bindings::<DynamicPolicy>(&original)[1],
        &bindings::<DynamicPolicy>(&latest)[1],
    ));
    assert!(Arc::ptr_eq(
        cpu.published(installation_id).unwrap(),
        &latest.custody._bindings,
    ));
    let owner = Arc::downgrade(&bindings::<DynamicPolicy>(&original)[1]);
    let identity = Arc::clone(&bindings::<DynamicPolicy>(&original)[1].identity);
    let custody = Arc::clone(&original.custody);
    drop((
        cpu,
        latest,
        latest_input,
        original,
        original_input,
        effects,
        replaced,
    ));
    assert!(
        owner.upgrade().is_some(),
        "native custody retains the actual Drop-close owner",
    );
    assert!(!identity.is_closed());
    drop(custody);
    assert!(owner.upgrade().is_none());
    assert!(identity.is_closed());
}

#[test]
fn binding_and_construction_refusals_or_panics_leave_published_owners_unchanged() {
    for failure in 0..4 {
        let definition = WgpuPolicyDefinition::new(DynamicPolicy);
        let installation = definition.install().unwrap();
        let original = declaration();
        let replacement = declaration();
        let input = dynamic_input(&installation, 1, &[original]);
        let mut cpu = CpuInstanceSet::new(1);
        let published = cpu.build(&input).unwrap();
        drop(cpu.commit(&published));
        let refused = installation
            .capture(
                image_packet(2),
                Arc::new(DynamicCapture {
                    effects: vec![replacement.clone()],
                    fail_bind: failure == 0,
                    panic_bind: failure == 1,
                    fail_build: failure == 2,
                    panic_build: failure == 3,
                }),
            )
            .unwrap();
        let result = cpu.build(&refused);
        if failure == 1 || failure == 3 {
            assert!(matches!(
                result,
                Err(WgpuRenderProgramError::PolicyPanicked)
            ));
        } else {
            assert!(matches!(
                result,
                Err(WgpuRenderProgramError::Feature { .. })
            ));
        }
        assert!(Arc::ptr_eq(
            cpu.published(installation.id()).unwrap(),
            &published.custody._bindings,
        ));
        assert!(
            !bindings::<DynamicPolicy>(&published)[0]
                .identity
                .is_closed()
        );
        cpu.instances.maintain();
        assert_eq!(cpu.instances.features.0.iter().flatten().count(), 1);

        let recovery_input = dynamic_input(&installation, 3, &[replacement]);
        let recovery = cpu.build(&recovery_input).unwrap();
        assert_eq!(cpu.instances.features.0.iter().flatten().count(), 2);
        let pending_identity = Arc::clone(&bindings::<DynamicPolicy>(&recovery)[0].identity);
        drop(recovery);
        assert!(pending_identity.is_closed());
        cpu.instances.maintain();
        assert_eq!(cpu.instances.features.0.iter().flatten().count(), 1);
        assert!(Arc::ptr_eq(
            cpu.published(installation.id()).unwrap(),
            &published.custody._bindings,
        ));
    }
}

#[test]
fn dropping_declaration_clones_cannot_close_an_actual_owner_retained_by_a_candidate() {
    let definition = WgpuPolicyDefinition::new(DynamicPolicy);
    let installation = definition.install().unwrap();
    let effect = declaration();
    let cloned = effect.clone();
    let input = dynamic_input(&installation, 1, &[cloned]);
    let mut cpu = CpuInstanceSet::new(1);
    let built = cpu.build(&input).unwrap();
    let identity = Arc::clone(&bindings::<DynamicPolicy>(&built)[0].identity);
    drop((effect, input, installation, definition));
    cpu.instances.maintain();
    assert!(!identity.is_closed());
    assert_eq!(built.custody._bindings.features.values.len(), 1);
    drop(built);
    cpu.instances.maintain();
    assert!(identity.is_closed());
    assert!(cpu.instances.is_empty());
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 0);
}

#[test]
fn latest_bindings_replace_one_snapshot_and_retired_user_values_drop_after_commit_returns() {
    let probe = Arc::new(PolicyProbe::default());
    let definition = WgpuPolicyDefinition::new(ClearPolicy(Arc::clone(&probe)));
    let installation = definition.install().unwrap();
    let mut cpu = CpuInstanceSet::new(1);
    for frame in 0..8 {
        let input = installation
            .capture(image_packet(frame), capture(1))
            .unwrap();
        let built = cpu.build(&input).unwrap();
        let retired = cpu.commit(&built);
        let expected_before_retirement = if frame == 0 { 0 } else { frame - 1 };
        assert_eq!(
            probe.dropped.load(Ordering::Relaxed),
            expected_before_retirement as usize,
        );
        drop(retired);
        assert_eq!(probe.dropped.load(Ordering::Relaxed), frame as usize);
        assert!(Arc::ptr_eq(
            cpu.published(installation.id()).unwrap(),
            &built.custody._bindings,
        ));
        drop((built, input));
    }
    assert_eq!(probe.created.load(Ordering::Relaxed), 8);
    assert_eq!(cpu.instances.len(), 1);
    drop((installation, definition));
    cpu.instances.maintain();
    assert_eq!(probe.dropped.load(Ordering::Relaxed), 8);
    assert!(cpu.instances.is_empty());
}

#[test]
fn binding_byte_refusal_reclaims_unpublished_owners_without_reserving_a_policy() {
    let definition = WgpuPolicyDefinition::new(DynamicPolicy);
    let installation = definition.install().unwrap();
    let effects = (0..MAX_FEATURE_BINDINGS_PER_PRODUCER)
        .map(|_| declaration())
        .collect::<Vec<_>>();
    let input = dynamic_input(&installation, 1, &effects);
    let mut cpu = CpuInstanceSet::new(1);
    assert!(matches!(
        cpu.build(&input),
        Err(WgpuRenderProgramError::PolicyCapacity)
    ));
    assert!(cpu.instances.is_empty());
    assert_eq!(Arc::strong_count(&input.capture), 1);
    assert!(
        cpu.instances
            .features
            .0
            .iter()
            .flatten()
            .all(|entry| { entry.published.is_none() && entry.binding.strong_count() == 0 })
    );
    cpu.instances.maintain();
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 0);

    let smaller = dynamic_input(&installation, 1, &effects[..128]);
    let admitted = cpu.build(&smaller).unwrap();
    assert_eq!(bindings::<DynamicPolicy>(&admitted).len(), 128);
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 128);
    drop(cpu.commit(&admitted));
    assert!(Arc::ptr_eq(
        cpu.published(installation.id()).unwrap(),
        &admitted.custody._bindings
    ));
}

#[test]
fn feature_binding_capacity_counts_active_owners_and_reclaims_a_retired_declaration() {
    assert_eq!(MAX_FEATURE_BINDINGS_PER_PRODUCER, 1024);
    let definition = WgpuPolicyDefinition::new(DynamicPolicy);
    let mut effects = (0..1024).map(|_| declaration()).collect::<Vec<_>>();
    let mut cpu = CpuInstanceSet::new(1);
    // Producer-wide owner capacity is independent of a single candidate's binding-byte cap.
    // Keep eight actual published policies, each within its own admitted staging allowance.
    let installations = (0..8)
        .map(|_| definition.install().unwrap())
        .collect::<Vec<_>>();
    let mut full_inputs = Vec::new();
    for (installation, batch) in installations.iter().zip(effects.chunks(128)) {
        let input = dynamic_input(installation, 1, batch);
        let built = cpu.build(&input).unwrap();
        drop(cpu.commit(&built));
        full_inputs.push(input);
    }
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 1024);
    let overflow = dynamic_input(&installations[0], 2, &[declaration()]);
    assert!(matches!(
        cpu.build(&overflow),
        Err(WgpuRenderProgramError::PolicyCapacity),
    ));
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 1024);

    let shrink_input = dynamic_input(&installations[0], 3, &effects[1..128]);
    drop(full_inputs.remove(0));
    drop(effects.remove(0));
    let shrink = cpu.build(&shrink_input).unwrap();
    let retired = cpu.commit(&shrink);
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 1024);
    drop(retired);
    cpu.instances.maintain();
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 1023);
    let recovered = cpu.build(&overflow).unwrap();
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 1024);
    assert!(
        !bindings::<DynamicPolicy>(&recovered)[0]
            .identity
            .is_closed()
    );

    drop((
        recovered,
        shrink,
        shrink_input,
        overflow,
        effects,
        full_inputs,
        installations,
        definition,
    ));
    cpu.instances.maintain();
    assert!(cpu.instances.is_empty());
    assert_eq!(cpu.instances.features.0.iter().flatten().count(), 0);
}

#[test]
fn repeated_declaration_resolution_counts_one_owner_and_one_shared_bindings_allocation() {
    let definition = WgpuPolicyDefinition::new(DynamicPolicy);
    let installation = definition.install().unwrap();
    let effect = declaration();
    let single_input = dynamic_input(&installation, 1, &[effect.clone()]);
    let duplicate_input = dynamic_input(&installation, 2, &[effect.clone(), effect.clone()]);
    let pair_input = dynamic_input(&installation, 3, &[effect, declaration()]);
    let mut cpu = CpuInstanceSet::new(1);
    let single = cpu.build(&single_input).unwrap();
    let duplicate = cpu.build(&duplicate_input).unwrap();
    let pair = cpu.build(&pair_input).unwrap();
    assert_eq!(single.metadata_bytes, duplicate.metadata_bytes);
    assert_eq!(duplicate.custody._bindings.features.values.len(), 1);
    assert_eq!(pair.custody._bindings.features.values.len(), 2);
    assert!(Arc::ptr_eq(
        &bindings::<DynamicPolicy>(&duplicate)[0],
        &bindings::<DynamicPolicy>(&duplicate)[1],
    ));
    assert!(pair.metadata_bytes > single.metadata_bytes);
    assert_eq!(Arc::strong_count(&single.custody._bindings), 2);
    drop(cpu.commit(&single));
    assert_eq!(Arc::strong_count(&single.custody._bindings), 3);
    let replay = cpu.build(&single_input).unwrap();
    assert_eq!(single.metadata_bytes, replay.metadata_bytes);
}
