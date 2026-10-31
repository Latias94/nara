use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_core::{Color, Mat3};
use nara_ecs::{Entity, Res, ResMut, schedule::IntoScheduleConfigs};
use nara_render::{
    ExtractedView, ExtractedViews, RenderImage2d, RenderImagePacket, RenderTarget, ViewLoadOp,
    ViewMapping2d, ViewportRect,
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuPolicyBuildContext, WgpuPolicyCaptureSet, WgpuPolicyDefinition,
    WgpuPolicyFrameWriter, WgpuPolicyInstallation, WgpuPolicyInstanceContext, WgpuPolicyProgram,
    WgpuRenderPolicy, WgpuRenderProgramError, install_wgpu_policy,
};
use nara_window::{
    WindowId, WindowPlugin, WindowResolution, backend::WindowPresentationRequirement,
};

#[derive(Default)]
struct SelectedSnapshot {
    validations: AtomicUsize,
}

#[derive(Resource)]
struct CaptureSource {
    snapshot: Arc<SelectedSnapshot>,
    window: Window,
}

struct SelectedPolicy {
    reject: bool,
}

impl WgpuRenderPolicy for SelectedPolicy {
    type Capture = SelectedSnapshot;
    type Instances = ();

    fn create_instances(
        &self,
        _: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        panic!("Game capture transport must not instantiate a renderer")
    }

    fn validate_capture(
        &self,
        _: &RenderFramePacket,
        snapshot: &SelectedSnapshot,
    ) -> Result<(), WgpuRenderProgramError> {
        snapshot.validations.fetch_add(1, Ordering::Relaxed);
        if self.reject {
            return Err(WgpuRenderProgramError::feature("selected capture refused"));
        }
        Ok(())
    }

    fn build(
        &self,
        _: &(),
        _: &SelectedSnapshot,
        _: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        panic!("Game capture transport must not execute a renderer")
    }
}

fn extract_views(source: Res<CaptureSource>, mut views: ResMut<ExtractedViews>) {
    let viewport = ViewportRect::new(0, 0, 2, 2).unwrap();
    views.clear();
    views.push(ExtractedView {
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Window(source.window.id),
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
}

fn capture_selected(
    source: Res<CaptureSource>,
    installation: Res<WgpuPolicyInstallation<SelectedPolicy>>,
    mut writer: WgpuPolicyFrameWriter,
) {
    writer
        .capture(&installation, Arc::clone(&source.snapshot))
        .unwrap();
}

fn runtime(
    reject: bool,
    mismatched_window: bool,
) -> (
    nara_app::RuntimeInstance,
    Arc<SelectedSnapshot>,
    nara_render_wgpu::WgpuPolicyInstallationId,
) {
    let window =
        Window::new("CPU Game capture", WindowResolution::new(2, 2)).with_id(WindowId::new(29));
    let mut config = EditorGameViewCaptureConfig {
        logical_window: RenderWindowPacket {
            id: window.id,
            resolution: window.resolution,
            present_mode: window.present_mode,
        },
        resizable: true,
        sprites: false,
        ui: false,
    };
    if mismatched_window {
        config.logical_window.id = WindowId::new(30);
    }
    let snapshot = Arc::new(SelectedSnapshot::default());
    let installation = WgpuPolicyDefinition::new(SelectedPolicy { reject })
        .install()
        .unwrap();
    let installation_id = installation.id();
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        WindowPlugin {
            primary_window: Some(window.clone()),
        },
        WgpuCapturePlugin,
        EditorGameViewBridgePlugin { config },
    ))
    .unwrap();
    install_wgpu_policy(&mut app, installation).unwrap();
    app.insert_resource(CaptureSource {
        snapshot: Arc::clone(&snapshot),
        window,
    })
    .unwrap();
    app.add_systems(CoreStage::Prepare, extract_views).unwrap();
    app.add_systems(
        CoreStage::Render,
        capture_selected.in_set(WgpuPolicyCaptureSet::Capture),
    )
    .unwrap();
    (
        nara_app::RuntimeInstance::start(app).unwrap(),
        snapshot,
        installation_id,
    )
}

#[test]
fn game_transports_the_original_selected_input_once_and_preserves_secondary_logical_routing() {
    let (mut runtime, snapshot, installation) = runtime(false, false);
    runtime.drive(Duration::ZERO).unwrap();
    let candidate = take_candidate(&mut runtime).unwrap().unwrap();
    assert_eq!(candidate.input.installation(), installation);
    assert!(std::ptr::eq(
        candidate.input.captured::<SelectedPolicy>().unwrap(),
        snapshot.as_ref(),
    ));
    assert_eq!(snapshot.validations.load(Ordering::Relaxed), 1);
    assert!(matches!(
        take_candidate(&mut runtime),
        Err(EditorGameViewBridgeFailure::CandidateBusy)
    ));
    assert!(
        !runtime
            .world()
            .contains_resource::<WindowPresentationRequirement>()
    );

    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(2, 2).unwrap(),
    )
    .unwrap();
    let adapted = candidate.input.with_image_target(target).unwrap();
    assert_eq!(adapted.installation(), installation);
    assert!(std::ptr::eq(
        adapted.captured::<SelectedPolicy>().unwrap(),
        snapshot.as_ref(),
    ));
    assert_eq!(adapted.packet().views(), candidate.input.packet().views());
    assert_eq!(
        adapted.packet().views()[0].target,
        RenderTarget::Window(WindowId::new(29)),
    );
    assert_eq!(
        adapted.packet().source_generation(),
        candidate.input.packet().source_generation(),
    );
    assert_eq!(
        adapted.packet().frame_index(),
        candidate.identity.frame_index
    );
    assert_eq!(adapted.packet().target(), RenderFrameTarget::Image(target));
    drop(candidate);
    runtime.finish().unwrap();
    assert!(std::ptr::eq(
        adapted.captured::<SelectedPolicy>().unwrap(),
        snapshot.as_ref(),
    ));
}

#[test]
fn a_failed_selected_capture_is_reported_without_falling_back_to_stock() {
    let (mut runtime, snapshot, _) = runtime(true, false);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        take_candidate(&mut runtime),
        Err(EditorGameViewBridgeFailure::Capture(message))
            if message.contains("selected capture refused")
    ));
    assert_eq!(snapshot.validations.load(Ordering::Relaxed), 1);
    assert!(runtime.world().resource::<EditorGameViewBridge>().is_idle());
    runtime.finish().unwrap();
}

#[test]
fn a_sealed_input_for_another_logical_window_is_rejected() {
    let (mut runtime, snapshot, _) = runtime(false, true);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        take_candidate(&mut runtime),
        Err(EditorGameViewBridgeFailure::CandidateSourceMismatch)
    ));
    assert_eq!(snapshot.validations.load(Ordering::Relaxed), 1);
    assert!(runtime.world().resource::<EditorGameViewBridge>().is_idle());
    runtime.finish().unwrap();
}
