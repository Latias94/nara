use nara_ecs::{Entity, Res, ResMut, Resource};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use nara_render::RenderTarget;
use nara_ui::{
    UiColor, UiNode, UiRoot, UiRuntimeKey, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform,
    UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiVisual,
};

use super::*;
use crate::{PresentationUiPlugins, RuntimeUiPlugins};

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
fn physical_targets_preserve_one_design_space_and_normalize_platform_logical_input() {
    let slot = UiSurfaceSlot::new(
        UiSurfaceId::new(7).expect("test surface should be non-zero"),
        UiSurfaceEndpoint::new(
            InputWindowId::new(1),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
        ),
        UiSurfaceContentTransform::new(
            UiSize::new(960.0, 540.0).expect("test authored size should be valid"),
            UiSize::new(1280.0, 720.0).expect("test physical size should be valid"),
            UiSafeAreaInsets::default(),
            1.0,
        )
        .expect("test transform should be valid"),
    )
    .expect("test surface slot should be valid");

    for (resolution, expected_render_scale, expected_input_scale) in [
        (
            nara_window::WindowResolution::new(1_280, 720),
            4.0 / 3.0,
            4.0 / 3.0,
        ),
        (
            nara_window::WindowResolution::new(1_920, 1_080).with_scale_factor(2.0),
            2.0,
            1.0,
        ),
        (
            nara_window::WindowResolution::new(2_560, 1_440).with_scale_factor(2.0),
            8.0 / 3.0,
            4.0 / 3.0,
        ),
    ] {
        let transform =
            surface_transform(slot, resolution).expect("target mapping should be valid");
        assert!((transform.content_scale_factor() - expected_render_scale).abs() < 1.0e-6);
        let input_scale = transform.content_scale_factor() / transform.platform_scale_factor();
        assert!((input_scale - expected_input_scale).abs() < 1.0e-6);
    }

    assert!(surface_transform(slot, nara_window::WindowResolution::new(0, 720)).is_none());
    assert!(
        surface_transform(
            slot,
            nara_window::WindowResolution::new(1_280, 720).with_scale_factor(0.0),
        )
        .is_none()
    );
}

#[test]
fn bridge_mirrors_and_retires_explicit_host_surface_routes() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    let surface_id = UiSurfaceId::new(7).expect("test surface should be non-zero");
    let root = {
        let world = app.world_mut().expect("test app should expose its world");
        nara_ui::install_ui_surface(
            world,
            UiSurfaceSlot::new(
                surface_id,
                UiSurfaceEndpoint::new(
                    InputWindowId::new(1),
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
                UiRuntimeKey::new("test.presentation-bridge", "root")
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
    };

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

    nara_ui::retire_ui_surface(
        app.world_mut().expect("test app should expose its world"),
        surface_id,
    )
    .expect("test surface retirement should succeed");
    app.update()
        .expect("route retirement should be synchronized without a stale packet");

    let compile = nara_ui_render::compile_ui_paint_packets(
        app.world_mut().expect("test app should expose its world"),
        &[frame],
    )
    .expect("the UI packet compiler should remain available");
    assert_eq!(compile.admitted_packets, 0);
    assert_eq!(compile.rejected_packets, 1);
}

#[test]
fn host_acceptance_commits_one_exact_receipt_without_a_post_submit_failure() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::Present);
    install_test_surface_and_root(&mut app, 8, 1);

    app.update()
        .expect("the fake backend should accept the UI packet");

    let stats = app.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.presented_packets, 1);
    assert_eq!(stats.aborted_packets, 0);
    assert!(stats.last_presented_receipt.is_some());
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

    app.update()
        .expect("the ignored backend frame should remain prepared");
    {
        let stats = app.world().resource::<UiPresentationBridgeStats>();
        assert_eq!(stats.presented_packets, 0);
        assert!(stats.last_presented_receipt.is_none());
    }
    assert!(
        !app.world()
            .resource::<UiPresentationAttemptLedger>()
            .is_empty()
    );

    *app.world_mut()
        .expect("test app should expose its world")
        .resource_mut::<FakeBackendMode>() = FakeBackendMode::Present;
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

    *app.world_mut()
        .expect("test app should expose its world")
        .resource_mut::<FakeBackendMode>() = FakeBackendMode::PresentAndReleaseHeld;
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
fn host_device_fault_retires_only_matching_backend_generation() {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .expect("runtime and presentation UI products should install");
    install_fake_backend(&mut app, FakeBackendMode::DeviceFaultWindowFirst(3));
    install_test_surface_and_root(&mut app, 20, 1);
    install_test_surface_and_root(&mut app, 21, 2);
    install_test_surface_and_root(&mut app, 22, 3);

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

    *app.world_mut()
        .expect("test app should expose its world")
        .resource_mut::<FakeBackendMode>() = FakeBackendMode::PresentGeneration;
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

    app.update()
        .expect("the fake backend should defer the first packet");
    assert_eq!(
        app.world()
            .resource::<UiPresentationBridgeStats>()
            .deferred_packets,
        1
    );

    app.world_mut()
        .expect("test app should expose its world")
        .entity_mut(root)
        .insert(UiVisual::solid(UiColor::rgba(1.0, 0.0, 0.0, 1.0)));
    *app.world_mut()
        .expect("test app should expose its world")
        .resource_mut::<FakeBackendMode>() = FakeBackendMode::Present;
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
