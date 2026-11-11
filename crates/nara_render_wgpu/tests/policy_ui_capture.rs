#![cfg(feature = "stock-ui")]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_app::{App, CoreStage};
use nara_ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs};
use nara_render::{RenderFramePacket, RenderTarget, ViewportRect};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuCapturePlugin,
    WgpuDeviceRequirements, WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureInstance,
    WgpuFrameCaptureError, WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyCaptureSet,
    WgpuPolicyDefinition, WgpuPolicyFrameFacts, WgpuPolicyFrameWriter, WgpuPolicyInstallation,
    WgpuPolicyProgram, WgpuPolicyUiCaptureFailure, WgpuProgramTextureDescriptor, WgpuRenderPolicy,
    WgpuRenderProgramBuilder, WgpuRenderProgramError, WgpuUiCapture, WgpuUiFeature,
    WgpuUiProgramBinding, WgpuUiSubmission, install_wgpu_policy,
};
use nara_ui_render::{
    UiPaintPacket, UiPresentationAttemptId, UiPresentationCapabilityGeneration,
    UiPresentationEpoch, UiPresentationIntegrationError, UiPresentationOutcome,
    UiPresentationRuntimeGeneration,
    advanced::{UiBackendPresentationFrame, UiBackendSubmissionReader, commit_ui_backend_outcomes},
    drain_ui_presentation_outcomes,
};
use nara_window::WindowId;

// This integration test is compiled as an external crate. The collector and completion path
// use only public imports; engine-private helpers below only seed and inspect the CPU fixture.
struct CustomUiPolicy {
    feature: WgpuFeatureDeclaration<WgpuUiFeature<UiColorTarget>>,
}
struct UiColorTarget;

struct CustomUiCapture {
    packets: Vec<Arc<UiPaintPacket>>,
    reject: bool,
    validations: AtomicUsize,
}

impl WgpuRenderPolicy for CustomUiPolicy {
    type Capture = CustomUiCapture;
    type Bindings = Arc<WgpuFeatureInstance<WgpuUiFeature<UiColorTarget>>>;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.feature.device_requirements().clone()
    }

    fn bind(
        &self,
        _: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        context.feature(&self.feature)
    }

    fn validate_capture(
        &self,
        _packet: &RenderFramePacket,
        capture: &Self::Capture,
    ) -> Result<(), WgpuRenderProgramError> {
        capture.validations.fetch_add(1, Ordering::Relaxed);
        if capture.reject {
            return Err(WgpuRenderProgramError::feature(
                "external UI capture rejected",
            ));
        }
        Ok(())
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let packet = context.packet();
        let extent = packet.target().extent();
        let mut builder = WgpuRenderProgramBuilder::new(
            packet.source_generation(),
            packet.frame_index(),
            GraphLimits::default(),
        )?;
        let target = builder.texture::<UiColorTarget>(WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("external_policy_ui"),
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
            transfer: if context.format().has_srgb_suffix() {
                TextureTransfer::Srgb
            } else {
                TextureTransfer::Linear
            },
            alpha: TextureAlpha::Premultiplied,
        })?;
        let ui = Arc::new(WgpuUiCapture::new(
            &capture.packets,
            RenderTarget::Window(WindowId::PRIMARY),
            Some(WindowId::PRIMARY),
            target.clone(),
            ViewportRect::from_extent(extent).unwrap(),
        )?);
        let binding = WgpuUiProgramBinding::new(1, bindings, Arc::clone(&ui));
        builder.operation(
            1,
            bindings,
            ui,
            &[],
            &[target.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
        )?;
        WgpuPolicyProgram::new(
            builder.finish(&[target.output()])?,
            &target,
            WgpuPolicyFrameFacts::default(),
        )?
        .with_ui(binding)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Pending,
    NoViews,
    Rejected,
    Foreign,
    Closed,
    Busy,
    ClosedAfterCapture,
}

#[derive(Resource)]
struct ForeignInstallation(WgpuPolicyInstallation<CustomUiPolicy>);

#[derive(Resource)]
struct Probe {
    case: Case,
    consume_after: usize,
    frames: usize,
    takes: usize,
    blocked: usize,
    refusals: usize,
    original: Option<Arc<CustomUiCapture>>,
    returned: Option<WgpuUiSubmission>,
    outcomes: Vec<UiPresentationOutcome>,
}

fn capture_ui(
    mut reader: UiBackendSubmissionReader,
    mut installation: ResMut<WgpuPolicyInstallation<CustomUiPolicy>>,
    foreign: Res<ForeignInstallation>,
    mut writer: WgpuPolicyFrameWriter,
    mut probe: ResMut<Probe>,
) {
    if probe.original.is_some() {
        assert!(!writer.can_capture_with_ui(&installation));
        probe.blocked += 1;
        return;
    }
    assert!(writer.can_capture_with_ui(&installation));
    assert!(!writer.can_capture_with_ui(&foreign.0));
    let frame: UiBackendPresentationFrame = reader.take().unwrap();
    probe.takes += 1;
    let (packets, ui) = WgpuUiSubmission::capture(frame);
    assert_eq!(packets.len(), 1);
    let capture = Arc::new(CustomUiCapture {
        packets,
        reject: probe.case == Case::Rejected,
        validations: AtomicUsize::new(0),
    });
    probe.original = Some(Arc::clone(&capture));

    if probe.case == Case::Closed {
        installation.close();
        assert!(!writer.can_capture_with_ui(&installation));
    } else if probe.case == Case::Busy {
        assert!(writer.capture(&installation, Arc::clone(&capture)).unwrap());
        assert!(!writer.can_capture_with_ui(&installation));
    }
    let attempted = if probe.case == Case::Foreign {
        &foreign.0
    } else {
        &*installation
    };
    match writer.capture_with_ui(attempted, Arc::clone(&capture), ui) {
        Ok(()) => assert!(!matches!(
            probe.case,
            Case::Foreign | Case::Closed | Case::Busy
        )),
        Err(failure) => {
            let failure: WgpuPolicyUiCaptureFailure = failure;
            assert!(matches!(
                probe.case,
                Case::Foreign | Case::Closed | Case::Busy
            ));
            if probe.case == Case::Closed {
                assert!(matches!(
                    failure.error(),
                    WgpuRenderProgramError::PolicyClosed
                ));
            } else {
                assert!(matches!(
                    failure.error(),
                    WgpuRenderProgramError::Feature { .. }
                ));
            }
            let (_, original_ui) = failure.into_parts();
            probe.refusals += 1;
            if probe.case == Case::Foreign {
                assert!(writer.can_capture_with_ui(&installation));
                writer
                    .capture_with_ui(&installation, Arc::clone(&capture), original_ui)
                    .unwrap();
            } else {
                probe.returned = Some(original_ui);
            }
        }
    }
    assert_eq!(
        capture.validations.load(Ordering::Relaxed),
        0,
        "validation belongs to seal"
    );
    if probe.case == Case::ClosedAfterCapture {
        installation.close();
    }
    assert!(!writer.can_capture_with_ui(&installation));
}

fn consume_ui(world: &mut World) {
    let (case, original, returned) = {
        let mut probe = world.resource_mut::<Probe>();
        probe.frames += 1;
        if probe.frames < probe.consume_after {
            return;
        }
        (
            probe.case,
            Arc::clone(probe.original.as_ref().unwrap()),
            probe.returned.take(),
        )
    };
    let batch = nara_render_wgpu::__private::take_captured_policy_batch(world).unwrap();
    assert!(nara_render_wgpu::__private::take_captured_policy_batch(world).is_none());
    let (captured, ui) = batch.into_parts();
    match case {
        Case::NoViews => assert!(matches!(captured, Ok(None))),
        Case::Rejected => {
            let error = captured.unwrap_err();
            assert!(error.to_string().contains("external UI capture rejected"));
            assert!(matches!(error, WgpuFrameCaptureError::Policy(_)));
        }
        Case::Closed => assert!(matches!(
            captured,
            Err(WgpuFrameCaptureError::MissingPolicy),
        )),
        _ => {
            let input = captured.unwrap().unwrap();
            assert_eq!(
                input.packet().frame_index(),
                1,
                "backpressure keeps the first frame"
            );
            assert!(std::ptr::eq(
                input.captured::<CustomUiPolicy>().unwrap(),
                original.as_ref()
            ));
            assert!(Arc::ptr_eq(
                &input.captured::<CustomUiPolicy>().unwrap().packets[0],
                &original.packets[0],
            ));
            assert_eq!(
                input.installation(),
                world
                    .resource::<WgpuPolicyInstallation<CustomUiPolicy>>()
                    .id()
            );
        }
    }
    assert_eq!(
        original.validations.load(Ordering::Relaxed),
        usize::from(!matches!(case, Case::NoViews | Case::Closed)),
    );
    let ui = if matches!(case, Case::Closed | Case::Busy) {
        assert!(
            ui.is_none(),
            "a refusal must not partially write the sidecar"
        );
        returned.unwrap()
    } else {
        assert!(returned.is_none());
        ui.unwrap()
    };
    assert!(matches!(
        drain_ui_presentation_outcomes(world),
        Err(UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 1 }),
    ));
    let completed =
        ui.content_rejected("external CPU consumer completed the original attempt".to_owned());
    assert_eq!(completed.len(), 1);
    commit_ui_backend_outcomes(world, completed).unwrap();
    let outcomes = drain_ui_presentation_outcomes(world).unwrap();
    assert!(matches!(
        outcomes.as_slice(),
        [UiPresentationOutcome::ContentRejected { attempt, .. }]
            if attempt.admission_id().unwrap().get() == 81
                && attempt.packet() == original.packets[0].identity()
                && attempt.target() == Some(original.packets[0].target()),
    ));
    assert!(drain_ui_presentation_outcomes(world).unwrap().is_empty());
    world.resource_mut::<Probe>().outcomes = outcomes;
}

fn run_case(case: Case) {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_window::WindowPlugin::default(),
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
    ))
    .unwrap();
    app.add_plugins(WgpuCapturePlugin).unwrap();
    let definition = WgpuPolicyDefinition::new(CustomUiPolicy {
        feature: WgpuFeatureDefinition::new(WgpuUiFeature::new()).declare(),
    });
    app.insert_resource(ForeignInstallation(definition.install().unwrap()))
        .unwrap();
    install_wgpu_policy(&mut app, definition.install().unwrap()).unwrap();
    if case != Case::NoViews {
        app.world_mut().unwrap().spawn((
            nara_render::Camera2d::default(),
            nara_transform::Transform2d::default(),
        ));
    }
    seed_backend_frame(app.world_mut().unwrap());
    let consume_after = if case == Case::Pending { 2 } else { 1 };
    app.insert_resource(Probe {
        case,
        consume_after,
        frames: 0,
        takes: 0,
        blocked: 0,
        refusals: 0,
        original: None,
        returned: None,
        outcomes: Vec::new(),
    })
    .unwrap();
    app.add_systems(
        CoreStage::Render,
        capture_ui.in_set(WgpuPolicyCaptureSet::Capture),
    )
    .unwrap();
    app.add_systems(CoreStage::Last, consume_ui).unwrap();
    let mut runtime = nara_app::RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            nara_app::RuntimeObligationLedger::new(),
            Default::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();
    for _ in 0..consume_after {
        runtime.drive(std::time::Duration::ZERO).unwrap();
    }
    let probe = runtime.world().resource::<Probe>();
    assert_eq!(probe.takes, 1);
    assert_eq!(probe.blocked, consume_after - 1);
    assert_eq!(
        probe.refusals,
        usize::from(matches!(case, Case::Foreign | Case::Closed | Case::Busy))
    );
    assert_eq!(probe.outcomes.len(), 1);
    assert!(probe.returned.is_none());
    runtime.finish().unwrap();
}

fn seed_backend_frame(world: &mut World) {
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
        UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
    };
    use nara_ui_render::__private::{
        UiPresentationRoute, prepare_backend_submission, replace_ui_backend_frame,
        replace_ui_presentation_routes, snapshot_ui_paint_packets,
    };

    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("external-policy-ui", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(
                UiNode::new(UiStyle::fill())
                    .with_key(UiLocalKey::runtime("external-policy-ui", "fill").unwrap())
                    .with_visual(UiVisual::solid(UiColor::rgba(1.0, 0.0, 0.0, 1.0))),
            ),
        )
        .build()
        .unwrap();
    let surface = UiSurfaceId::new(1).unwrap();
    let frame = UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(
            &snapshot,
            UiEnvironment::new(surface, 16.0, 16.0, 1.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    replace_ui_presentation_routes(
        world,
        [UiPresentationRoute::new(surface, WindowId::PRIMARY).unwrap()],
    )
    .unwrap();
    nara_ui_render::compile_ui_paint_packets(world, &[frame]).unwrap();
    let (mut packets, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
    let packet = packets.pop().unwrap();
    let prepared = prepare_backend_submission(
        UiPresentationAttemptId::new(81).unwrap(),
        Arc::clone(&packet),
        UiPresentationEpoch::new(3).unwrap(),
        UiPresentationRuntimeGeneration::new(7).unwrap(),
        UiPresentationCapabilityGeneration::new(11).unwrap(),
    );
    assert!(
        replace_ui_backend_frame(world, vec![packet], vec![prepared])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn external_policy_ui_bridge_preserves_pending_capture_without_a_second_take() {
    run_case(Case::Pending);
}

#[test]
fn external_policy_ui_bridge_returns_permissions_when_there_are_no_views() {
    run_case(Case::NoViews);
}

#[test]
fn external_policy_ui_bridge_returns_permissions_when_sealing_rejects_the_capture() {
    run_case(Case::Rejected);
}

#[test]
fn external_policy_ui_bridge_returns_the_foreign_refusal_for_retry_on_the_original_route() {
    run_case(Case::Foreign);
}

#[test]
fn external_policy_ui_bridge_returns_the_closed_refusal_for_original_completion() {
    run_case(Case::Closed);
}

#[test]
fn external_policy_ui_bridge_keeps_the_occupied_capture_and_returns_the_sidecar() {
    run_case(Case::Busy);
}

#[test]
fn external_policy_ui_bridge_keeps_an_accepted_pair_after_installation_close() {
    run_case(Case::ClosedAfterCapture);
}
