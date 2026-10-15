use nara_ecs::{Entity, Res, ResMut, Resource};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use nara_render::RenderTarget;
use nara_ui::{
    UiColor, UiNode, UiRoot, UiRuntimeKey, UiSafeAreaInsets, UiSize, UiSurfaceAvailability,
    UiSurfaceContentTransform, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot,
    UiSurfaceSuspensionReason, UiVisual,
};
use std::sync::Arc;

use super::*;
use crate::{PresentationUiPlugins, RuntimeUiPlugins};
#[cfg(all(feature = "desktop-winit", feature = "presentation-ui"))]
use std::time::Duration;

#[path = "test_driver.rs"]
mod driver;
use driver::BridgeRuntime;

#[derive(Debug, Default, Resource)]
struct FakeBackendFrame(Option<nara_ui_render::__private::UiBackendPresentationFrame>);

#[derive(Debug, Default, Resource)]
struct FakeHeldBackendAcceptances(Vec<nara_ui_render::OneShotPresentationCommit>);

#[derive(Debug, Clone, Copy, Resource)]
enum FakeBackendMode {
    Present,
    HoldAcceptance,
    PresentAndReleaseHeld,
    PresentGeneration,
    Deferred,
    FaultWindow(u64),
    DeviceFaultWindowFirst(u64),
    Ignore,
}

fn capture_fake_backend_frame(
    mut reader: nara_ui_render::__private::UiBackendSubmissionReader,
    mode: Res<FakeBackendMode>,
    mut frame: ResMut<FakeBackendFrame>,
) {
    if matches!(*mode, FakeBackendMode::Ignore) {
        return;
    }
    frame.0 = Some(
        reader
            .take()
            .expect("the fake backend outcome mailbox must be reservable"),
    );
}

fn submit_fake_backend_frame(
    mut frame: ResMut<FakeBackendFrame>,
    mode: Res<FakeBackendMode>,
    mut outcomes: ResMut<nara_ui_render::__private::UiPresentationOutcomes>,
) {
    let Some(frame) = frame.0.take() else {
        return;
    };
    let (_, mut submissions) = frame.into_parts();
    if let FakeBackendMode::DeviceFaultWindowFirst(fault_window) = *mode {
        submissions.sort_by_key(|envelope| {
            envelope.submission().identity().target()
                != RenderTarget::Window(nara_window::WindowId::new(fault_window))
        });
    }
    for envelope in submissions {
        let target_window = match envelope.submission().identity().target() {
            RenderTarget::PrimaryWindow => Some(nara_window::WindowId::PRIMARY.get()),
            RenderTarget::Window(window) => Some(window.get()),
            RenderTarget::Image(_) => None,
        };
        if let FakeBackendMode::FaultWindow(window) = *mode
            && target_window == Some(window)
        {
            outcomes.commit_backend(envelope.backend_fault(
                None,
                nara_ui_render::UiPresentationBackendFaultDomain::Target,
                "fake target fault".to_owned(),
            ));
            continue;
        }
        let backend_epoch = if matches!(
            *mode,
            FakeBackendMode::DeviceFaultWindowFirst(_) | FakeBackendMode::PresentGeneration
        ) && target_window == Some(2)
        {
            2
        } else {
            1
        };
        let backend_generation =
            nara_ui_render::UiPresentationBackendGeneration::new(77, backend_epoch)
                .expect("fake backend generation should be non-zero");
        let mut submitted = nara_ui_render::__private::bind_backend_submission_batch(
            vec![envelope],
            backend_generation,
        )
        .expect("fake backend generation should match the prepared attempt");
        let submitted = submitted
            .pop()
            .expect("one prepared fake submission should bind exactly once");
        let completed = match *mode {
            FakeBackendMode::Deferred => submitted.deferred("fake backend deferral".to_owned()),
            FakeBackendMode::DeviceFaultWindowFirst(window) if target_window == Some(window) => {
                submitted.backend_fault(
                    nara_ui_render::UiPresentationBackendFaultDomain::Device,
                    "fake device fault".to_owned(),
                )
            }
            FakeBackendMode::DeviceFaultWindowFirst(_) => {
                submitted.deferred("fake generation-local deferral".to_owned())
            }
            FakeBackendMode::Present
            | FakeBackendMode::HoldAcceptance
            | FakeBackendMode::PresentAndReleaseHeld
            | FakeBackendMode::PresentGeneration
            | FakeBackendMode::FaultWindow(_) => submitted.presented(),
            FakeBackendMode::Ignore => {
                unreachable!("ignored fake frames never reach backend submission")
            }
        };
        outcomes.commit_backend(completed);
    }
}

fn hold_fake_backend_acceptances(world: &mut World) {
    if !matches!(
        *world.resource::<FakeBackendMode>(),
        FakeBackendMode::HoldAcceptance
    ) {
        return;
    }
    let outcomes = nara_ui_render::drain_ui_presentation_outcomes(world)
        .expect("the fake backend should publish complete outcomes before Host cleanup");
    let mut held = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        match outcome {
            nara_ui_render::UiPresentationOutcome::Presented(commit) => held.push(commit),
            outcome => panic!("hold mode expected only presented outcomes, got {outcome:?}"),
        }
    }
    world
        .resource_mut::<FakeHeldBackendAcceptances>()
        .0
        .extend(held);
}

fn release_fake_backend_acceptances(world: &mut World) {
    if !matches!(
        *world.resource::<FakeBackendMode>(),
        FakeBackendMode::PresentAndReleaseHeld
    ) {
        return;
    }
    let held = std::mem::take(&mut world.resource_mut::<FakeHeldBackendAcceptances>().0);
    for commit in held {
        commit_presented_attempt(world, commit);
    }
}

fn install_fake_backend(app: &mut App, mode: FakeBackendMode) {
    app.insert_resource(FakeBackendFrame::default())
        .expect("fake backend frame resource should install");
    app.insert_resource(FakeHeldBackendAcceptances::default())
        .expect("fake held-acceptance resource should install");
    app.insert_resource(mode)
        .expect("fake backend mode should install");
    app.add_systems(
        CoreStage::Render,
        capture_fake_backend_frame.before(submit_fake_backend_frame),
    )
    .expect("fake backend capture should install");
    app.add_systems(CoreStage::Render, submit_fake_backend_frame)
        .expect("fake backend submit should install");
    app.add_systems(
        CoreStage::Cleanup,
        hold_fake_backend_acceptances.before(commit_runtime_ui_presentations),
    )
    .expect("fake backend acceptance hold should install");
    app.add_systems(
        CoreStage::Cleanup,
        release_fake_backend_acceptances.after(commit_runtime_ui_presentations),
    )
    .expect("fake backend acceptance release should install");
}

fn install_test_surface_and_root(app: &mut App, surface: u64, window: u64) -> Entity {
    let surface_id = UiSurfaceId::new(surface).expect("test surface should be non-zero");
    let world = app.world_mut().expect("test app should expose its world");
    let window_id = nara_window::WindowId::new(window);
    let window_exists = {
        let mut query = world.query::<&nara_window::Window>();
        query.iter(world).any(|candidate| candidate.id == window_id)
    };
    if !window_exists {
        let window = nara_window::Window::new(
            format!("test-{window}"),
            nara_window::WindowResolution::new(320, 180),
        )
        .with_id(window_id);
        if window_id == nara_window::WindowId::PRIMARY {
            world.spawn((window, nara_window::PrimaryWindow));
        } else {
            world.spawn(window);
        }
    }
    nara_ui::install_ui_surface(
        world,
        UiSurfaceSlot::new(
            surface_id,
            UiSurfaceEndpoint::new(
                InputWindowId::new(window),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::new(
                UiSize::new(320.0, 180.0).expect("test authored size should be valid"),
                UiSize::new(320.0, 180.0).expect("test physical size should be valid"),
                UiSafeAreaInsets::default(),
                1.0,
            )
            .expect("test transform should be valid"),
        )
        .expect("test surface slot should be valid"),
    )
    .expect("test surface should install");
    let root = world
        .spawn((
            UiRoot::default(),
            UiNode::default(),
            UiRuntimeKey::new("test.presentation-bridge", format!("root-{surface}"))
                .expect("test root key should be valid"),
        ))
        .id();
    HierarchyWriter::new(world)
        .apply_batch(&[HierarchyOperation::Place {
            entity: root,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }])
        .expect("test root should enter structural order");
    root
}

#[test]
fn bridge_mirrors_and_retires_explicit_host_surface_routes() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    assert!(
        !app.world()
            .contains_resource::<NativeAccessibilityPresentationPort>(),
        "a compiled desktop adapter must not install native accessibility without the explicit contribution",
    );
    install_fake_backend(&mut app, FakeBackendMode::Present);
    let surface_id = UiSurfaceId::new(7).expect("test surface should be non-zero");
    let root = install_test_surface_and_root(&mut app, surface_id.get(), 1);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the bridge should compile one explicitly routed UI frame");

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.last_admitted_packets, 1);
    assert_eq!(stats.last_rejected_packets, 0);
    assert_eq!(stats.last_error, None);
    assert!(app.world().get::<UiRoot>(root).is_some());
    let frame = nara_ui::latest_ui_frame(app.world(), surface_id)
        .expect("UI runtime should remain installed")
        .expect("the active surface should have one compiled frame");

    app.retire_surface(surface_id)
        .expect("test surface retirement should succeed");
    app.update()
        .expect("route retirement should be synchronized without a stale packet");

    let compile = app
        .compile_packets(vec![frame])
        .expect("the UI packet compiler should remain available");
    assert_eq!(compile.admitted_packets, 0);
    assert_eq!(compile.rejected_packets, 1);
}

#[cfg(all(feature = "desktop-winit", feature = "presentation-ui"))]
#[test]
fn explicit_winit_accessibility_contribution_installs_the_private_presentation_port() {
    let mut app = App::new();
    app.add_plugins((
        RuntimeUiPlugins,
        PresentationUiPlugins,
        crate::DesktopWinitPlugins,
        WinitUiAccessibilityContributionPlugin,
    ))
    .expect("the explicit native accessibility contribution should install");

    assert!(
        app.world()
            .contains_resource::<NativeAccessibilityPresentationPort>()
    );
    assert!(
        nara_winit::__private::capture_winit_accessibility_activation(app.world()).is_enabled()
    );
}

#[cfg(all(feature = "desktop-winit", feature = "presentation-ui"))]
#[test]
fn explicit_winit_accessibility_publication_uses_the_exact_presented_receipt() {
    let mut app = App::new();
    app.add_plugins((
        RuntimeUiPlugins,
        PresentationUiPlugins,
        crate::DesktopWinitPlugins,
        WinitUiAccessibilityContributionPlugin,
    ))
    .expect("the explicit native accessibility contribution should install");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    install_test_surface_and_root(&mut app, 71, nara_window::WindowId::PRIMARY.get());

    let mut runtime =
        nara_app::RuntimeInstance::start(app).expect("the focused test runtime should start");

    runtime
        .drive(Duration::ZERO)
        .expect("the explicit contribution should publish one presented UI frame");
    let receipt = runtime
        .world()
        .resource::<UiPresentationBridgeStats>()
        .last_presented_receipt
        .expect("the fake backend should commit one exact presentation receipt");
    let updates = runtime
        .with_driver_scope(nara_winit::__private::take_winit_accessibility_updates)
        .expect("the managed runtime should grant its platform driver scope")
        .expect("the explicit contribution should install the platform update port");
    let mut presentations = updates.into_iter().filter_map(|update| match update {
        nara_winit::__private::WinitAccessibilityPlatformUpdate::Present {
            window_id,
            snapshot,
        } => Some((window_id, snapshot)),
        _ => None,
    });
    let (window_id, snapshot) = presentations
        .next()
        .expect("the platform adapter should receive one presented semantic snapshot");
    assert!(
        presentations.next().is_none(),
        "one committed UI packet must publish exactly one platform presentation"
    );
    assert_eq!(window_id, nara_window::WindowId::PRIMARY);
    assert_eq!(snapshot.surface_id(), receipt.surface_id());
    assert_eq!(snapshot.frame_revision(), receipt.frame_revision());
    assert_eq!(
        snapshot.route().slot_generation(),
        receipt.slot_generation()
    );
    assert_eq!(snapshot.route().route_revision(), receipt.route_revision());
    assert_eq!(
        snapshot.route().geometry_generation(),
        receipt.geometry_generation()
    );
    assert_eq!(snapshot.presentation_epoch(), receipt.presentation_epoch());

    runtime
        .finish()
        .expect("the focused test runtime should close cleanly");
}

#[cfg(all(feature = "desktop-winit", feature = "presentation-ui"))]
#[test]
fn compiled_desktop_ui_without_contribution_drives_without_platform_state() {
    let mut app = App::new();
    app.add_plugins((
        RuntimeUiPlugins,
        PresentationUiPlugins,
        crate::DesktopWinitPlugins,
    ))
    .expect("desktop UI product layers should install without native accessibility");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    install_test_surface_and_root(&mut app, 70, 1);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("a compiled but unrequested accessibility adapter must not affect the UI frame");

    assert!(
        !app.world()
            .contains_resource::<NativeAccessibilityPresentationPort>()
    );
    assert_eq!(
        nara_winit::__private::capture_winit_accessibility_activation(app.world()),
        nara_winit::__private::WinitAccessibilityActivation::Disabled
    );
    assert_eq!(
        app.world()
            .resource::<UiPresentationBridgeStats>()
            .presented_packets,
        1
    );
}

#[test]
fn host_minimize_suspends_routes_and_restore_requires_fresh_presentation() {
    let mut app = App::new();
    app.add_plugin(nara_window::WindowPlugin {
        primary_window: None,
    })
    .expect("the resize driver needs the window event owner");
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    let surface_id = UiSurfaceId::new(30).unwrap();
    install_test_surface_and_root(&mut app, surface_id.get(), 1);

    let mut app = BridgeRuntime::start(app);
    app.update().expect("the initial surface should present");
    let initial_receipt = app
        .world()
        .resource::<UiPresentationBridgeStats>()
        .last_presented_receipt
        .expect("the initial surface should own one receipt");
    app.set_backend_mode(FakeBackendMode::HoldAcceptance);
    app.resize_primary_window(nara_window::WindowResolution::new(640, 360));
    app.update()
        .expect("the resized surface should retain one uncommitted backend acceptance");
    assert_eq!(
        app.world().resource::<FakeHeldBackendAcceptances>().0.len(),
        1
    );
    assert!(
        !app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
    app.resize_primary_window(nara_window::WindowResolution::new(0, 720));

    app.update()
        .expect("zero extent should suspend without submitting the old packet");
    let snapshots = nara_ui::ui_surface_slot_snapshots(app.world()).unwrap();
    assert_eq!(
        snapshots[0].availability(),
        UiSurfaceAvailability::Suspended(UiSurfaceSuspensionReason::ZeroExtent)
    );
    assert!(
        nara_ui::active_ui_surface_slots(app.world())
            .unwrap()
            .is_empty()
    );
    assert!(
        nara_ui::latest_ui_frame(app.world(), surface_id)
            .unwrap()
            .is_none()
    );
    assert!(
        nara_ui::ui_semantic_snapshot(app.world(), initial_receipt, nara_ui::UiSeatId::PRIMARY)
            .is_err()
    );
    let (_, suspended_packets) = nara_ui_render::__private::snapshot_ui_paint_packets(app.world())
        .unwrap()
        .into_parts();
    assert!(suspended_packets.is_empty());
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
    assert_eq!(
        app.world().resource::<FakeHeldBackendAcceptances>().0.len(),
        1
    );

    app.set_backend_mode(FakeBackendMode::PresentAndReleaseHeld);
    app.resize_primary_window(nara_window::WindowResolution::new(1_024, 768));
    app.update()
        .expect("a valid 4:3 target should restore and present a fresh generation");

    let restored = nara_ui::ui_surface_slot_snapshots(app.world()).unwrap()[0];
    assert_eq!(restored.availability(), UiSurfaceAvailability::Active);
    assert_eq!(restored.slot().slot_generation(), 4);
    assert!((restored.slot().transform().physical_content_origin().y() - 96.0).abs() < 1.0e-6);
    let restored_receipt = app
        .world()
        .resource::<UiPresentationBridgeStats>()
        .last_presented_receipt
        .expect("restore should publish one fresh receipt");
    assert_ne!(
        restored_receipt.slot_generation(),
        initial_receipt.slot_generation()
    );
    assert!(
        nara_ui::ui_semantic_snapshot(app.world(), initial_receipt, nara_ui::UiSeatId::PRIMARY)
            .is_err()
    );
    assert!(
        nara_ui::ui_semantic_snapshot(app.world(), restored_receipt, nara_ui::UiSeatId::PRIMARY)
            .is_ok()
    );
    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.aborted_packets, 1);
    assert_eq!(stats.stale_backend_acceptances, 1);
    assert!(
        app.world()
            .resource::<FakeHeldBackendAcceptances>()
            .0
            .is_empty()
    );
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
}

#[test]
fn missing_window_suspends_and_a_recreated_endpoint_restores_the_same_surface_lifetime() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    let surface_id = UiSurfaceId::new(31).unwrap();
    install_test_surface_and_root(&mut app, surface_id.get(), 1);
    let mut app = BridgeRuntime::start(app);
    app.update().expect("the initial surface should present");

    let removed_window = app.remove_primary_window();
    app.update()
        .expect("a missing endpoint should suspend instead of retaining stale authority");
    assert_eq!(
        nara_ui::ui_surface_slot_snapshots(app.world()).unwrap()[0].availability(),
        UiSurfaceAvailability::Suspended(UiSurfaceSuspensionReason::EndpointUnavailable)
    );

    app.restore_primary_window(removed_window);
    app.update()
        .expect("recreating the endpoint should restore with fresh generations");
    let restored = nara_ui::ui_surface_slot_snapshots(app.world()).unwrap()[0];
    assert_eq!(restored.availability(), UiSurfaceAvailability::Active);
    assert_eq!(restored.slot().slot_generation(), 3);
    assert!(
        app.world()
            .resource::<UiPresentationBridgeStats>()
            .last_presented_receipt
            .is_some()
    );
}

#[test]
fn host_acceptance_commits_one_exact_receipt_without_a_post_submit_failure() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    install_test_surface_and_root(&mut app, 8, 1);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the fake backend should accept the UI packet");

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.presented_packets, 1);
    assert_eq!(stats.aborted_packets, 0);
    assert!(stats.last_presented_receipt.is_some());
    let observation = latest_presented_ui_observation(app.world())
        .expect("one accepted packet should publish one presented observation");
    let receipt = stats
        .last_presented_receipt
        .expect("one accepted packet should commit one receipt");
    assert_eq!(observation.receipt(), receipt);
    assert!(observation.submission().matches_receipt(receipt));
    assert_eq!(
        observation.backend_generation(),
        nara_ui_render::UiPresentationBackendGeneration::new(77, 1).unwrap()
    );
    let packet = observation.packet();
    let frame = observation.frame();
    assert_eq!(packet.identity(), observation.submission().packet());
    assert_eq!(frame.revision(), receipt.frame_revision());
    assert!(Arc::ptr_eq(&packet.frame(), &frame));
    #[cfg(feature = "render-wgpu")]
    assert!(
        observation.wgpu().is_none(),
        "a non-WGPU fake backend must not inherit unrelated WGPU transaction facts"
    );
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
}

#[test]
fn host_aborts_an_unconsumed_backend_frame_before_later_acceptance() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Ignore);
    install_test_surface_and_root(&mut app, 12, 1);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the ignored backend frame should remain prepared");
    {
        let stats = app.world().resource::<UiPresentationBridgeStats>();
        assert_eq!(stats.presented_packets, 0);
        assert!(stats.last_presented_receipt.is_none());
    }
    assert!(latest_presented_ui_observation(app.world()).is_none());
    assert!(
        !app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );

    app.set_backend_mode(FakeBackendMode::Present);
    app.update()
        .expect("a later backend frame should still present successfully");

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.aborted_packets, 1);
    assert_eq!(stats.presented_packets, 1);
    assert!(stats.last_presented_receipt.is_some());
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
}

#[test]
fn host_discards_a_late_backend_acceptance_after_committing_its_successor() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::HoldAcceptance);
    let surface_id = UiSurfaceId::new(13).expect("test surface should be non-zero");
    install_test_surface_and_root(&mut app, surface_id.get(), 1);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the fake backend should hold the first accepted packet");
    let stale_epoch = app
        .world()
        .resource::<FakeHeldBackendAcceptances>()
        .0
        .first()
        .expect("one backend acceptance should be held")
        .presentation_epoch();
    assert_eq!(
        app.world().resource::<FakeHeldBackendAcceptances>().0.len(),
        1
    );
    assert!(
        !app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );

    app.set_backend_mode(FakeBackendMode::PresentAndReleaseHeld);
    app.update().expect(
        "the Host should abort the old attempt, commit its successor, then discard the late acceptance",
    );

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    let receipt = stats
        .last_presented_receipt
        .expect("the successor should own the presented receipt");
    assert_eq!(stats.presented_packets, 1);
    assert_eq!(stats.aborted_packets, 1);
    assert_eq!(stats.stale_backend_acceptances, 1);
    assert_ne!(receipt.presentation_epoch(), stale_epoch.get());
    let observation = latest_presented_ui_observation(app.world())
        .expect("the successor should publish the only presented observation");
    assert_eq!(observation.receipt(), receipt);
    assert_ne!(
        observation.submission().presentation_epoch().get(),
        stale_epoch.get()
    );
    let (_, presented) = nara_ui_render::__private::snapshot_ui_paint_packets(app.world())
        .expect("the planner should retain the successor packet")
        .into_parts();
    assert_eq!(presented.len(), 1);
    assert_eq!(presented[0].identity().surface_id(), surface_id);
    assert_eq!(
        presented[0].identity().frame_revision(),
        receipt.frame_revision()
    );
    assert!(
        app.world()
            .resource::<FakeHeldBackendAcceptances>()
            .0
            .is_empty()
    );
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
}

#[test]
fn a_newer_unpresented_candidate_cannot_replace_the_presented_observation() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    let root = install_test_surface_and_root(&mut app, 14, 1);

    let mut app = BridgeRuntime::start(app);
    app.update().expect("the initial candidate should present");
    let presented = latest_presented_ui_observation(app.world())
        .expect("the initial packet should publish an observation");
    let presented_frame = presented.frame();

    app.set_visual(root, UiVisual::solid(UiColor::rgba(1.0, 0.0, 0.0, 1.0)));
    app.set_backend_mode(FakeBackendMode::HoldAcceptance);
    app.update()
        .expect("the newer candidate should remain unpresented");

    let latest_candidate = nara_ui::latest_ui_frame(
        app.world(),
        UiSurfaceId::new(14).expect("test surface should be non-zero"),
    )
    .expect("the UI runtime should remain available")
    .expect("the changed root should produce a newer candidate");
    let retained = latest_presented_ui_observation(app.world())
        .expect("the previous presented observation should remain available");
    assert_ne!(
        latest_candidate.revision(),
        presented.receipt().frame_revision()
    );
    assert_eq!(retained.receipt(), presented.receipt());
    assert!(Arc::ptr_eq(&retained.frame(), &presented_frame));
}

#[test]
fn a_rejected_successor_keeps_the_previous_presented_observation() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    let root = install_test_surface_and_root(&mut app, 15, 1);
    install_fake_backend(&mut app, FakeBackendMode::Present);

    let mut app = BridgeRuntime::start(app);
    app.update().expect("the initial candidate should present");
    let presented = latest_presented_ui_observation(app.world())
        .expect("the initial packet should publish an observation");
    let presented_frame = presented.frame();

    app.set_visual(root, UiVisual::solid(UiColor::rgba(0.0, 1.0, 0.0, 1.0)));
    app.set_backend_mode(FakeBackendMode::FaultWindow(1));
    app.update()
        .expect("the rejected successor should terminate without replacing presentation");

    let retained = latest_presented_ui_observation(app.world())
        .expect("the previous presented observation should remain available");
    assert_eq!(retained.receipt(), presented.receipt());
    assert!(Arc::ptr_eq(&retained.frame(), &presented_frame));
    assert_eq!(
        app.world()
            .resource::<UiPresentationBridgeStats>()
            .backend_faults,
        1
    );
}

#[test]
fn host_device_fault_retires_only_matching_backend_generation() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::DeviceFaultWindowFirst(3));
    install_test_surface_and_root(&mut app, 20, 1);
    install_test_surface_and_root(&mut app, 21, 2);
    install_test_surface_and_root(&mut app, 22, 3);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the fake backend should defer two generations before faulting one of them");

    let generation_a = nara_ui_render::UiPresentationBackendGeneration::new(77, 1)
        .expect("fake generation A should be valid");
    let generation_b = nara_ui_render::UiPresentationBackendGeneration::new(77, 2)
        .expect("fake generation B should be valid");
    let ledger = app.world().resource::<UiPresentationAttemptLedger>();
    assert!(ledger.deferred_ids_for_backend(generation_a).is_empty());
    assert_eq!(ledger.deferred_ids_for_backend(generation_b).len(), 1);

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.backend_faults, 1);
    assert_eq!(stats.deferred_packets, 2);
    assert_eq!(stats.aborted_packets, 2);

    app.set_backend_mode(FakeBackendMode::PresentGeneration);
    app.update()
        .expect("the unrelated backend generation should retry and present");

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.presented_packets, 3);
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
    let (_, presented) = nara_ui_render::__private::snapshot_ui_paint_packets(app.world())
        .expect("the planner should expose all successfully presented packets")
        .into_parts();
    assert!(
        presented
            .iter()
            .any(|packet| packet.identity().surface_id().get() == 21)
    );
}

#[test]
fn host_target_fault_isolated_from_an_unrelated_surface_acceptance() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::FaultWindow(1));
    install_test_surface_and_root(&mut app, 9, 1);
    install_test_surface_and_root(&mut app, 10, 2);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the fake backend should publish both outcomes");

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.backend_faults, 1);
    assert_eq!(stats.presented_packets, 1);
    assert_eq!(stats.last_error.as_deref(), Some("fake target fault"));
    assert_eq!(stats.last_presented_receipt.unwrap().surface_id().get(), 10);
    let active_surfaces = nara_ui::active_ui_surface_slots(app.world())
        .expect("a target fault must leave both Host-owned Surface Slots installed")
        .into_iter()
        .map(|slot| slot.surface_id().get())
        .collect::<Vec<_>>();
    assert_eq!(active_surfaces, vec![9, 10]);
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
}

#[test]
fn deferred_attempt_is_superseded_when_its_behavior_changes_before_retry() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Deferred);
    let root = install_test_surface_and_root(&mut app, 11, 1);

    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the fake backend should defer the first packet");
    assert_eq!(
        app.world()
            .resource::<UiPresentationBridgeStats>()
            .deferred_packets,
        1
    );

    app.set_visual(root, UiVisual::solid(UiColor::rgba(1.0, 0.0, 0.0, 1.0)));
    app.set_backend_mode(FakeBackendMode::Present);
    app.update()
        .expect("the changed frame should supersede the deferred packet");

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.superseded_packets, 1);
    assert_eq!(stats.presented_packets, 1);
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
}

#[test]
fn host_external_backend_rendezvous_reexports_the_exact_deferred_retry() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_test_surface_and_root(&mut app, 23, 1);
    let mut app = BridgeRuntime::start(app);
    app.update()
        .expect("the Runtime should prepare one external backend frame");

    let first = app
        .take_backend_frame()
        .expect("the external Host should reserve the prepared UI frame");
    let (_, submissions) = first.into_parts();
    let backend_generation = nara_ui_render::UiPresentationBackendGeneration::new(91, 4).unwrap();
    let mut submitted =
        nara_ui_render::__private::bind_backend_submission_batch(submissions, backend_generation)
            .unwrap();
    let first_identity = submitted[0].submission().identity();
    let completed = submitted
        .pop()
        .unwrap()
        .deferred("external Editor window deferral".to_owned());
    app.commit_backend_outcome(completed)
        .expect("the Runtime should reclaim its exact deferred permit");
    app.commit_presentations();

    app.update()
        .expect("the Runtime should re-export the exact eligible retry");
    let retry = app
        .take_backend_frame()
        .expect("the external Host should receive the deferred retry");
    let (_, submissions) = retry.into_parts();
    assert_eq!(submissions[0].submission().identity(), first_identity);
    assert_eq!(
        submissions[0].submission().required_backend_generation(),
        Some(backend_generation)
    );
    let mut submitted =
        nara_ui_render::__private::bind_backend_submission_batch(submissions, backend_generation)
            .unwrap();
    app.commit_backend_outcome(submitted.pop().unwrap().presented())
        .expect("the exact retry should reach one terminal presented outcome");
    app.commit_presentations();

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.deferred_packets, 1);
    assert_eq!(stats.presented_packets, 1);
    assert!(
        app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );
}
