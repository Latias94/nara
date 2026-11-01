use super::*;

use nara_ecs::{
    World,
    bevy_ecs::system::{RunSystemOnce, SystemState},
};

use crate::{WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyProgram};

struct CustomPolicy;

impl WgpuRenderPolicy for CustomPolicy {
    type Capture = WgpuStockCapture;
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
        _: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        Err(WgpuRenderProgramError::feature(
            "the CPU capture fixture does not execute native work",
        ))
    }
}

fn stock_capture_app() -> App {
    let mut app = super::tests::capture_app();
    app.add_plugins(WgpuStockCapturePlugin).unwrap();
    install_wgpu_policy(
        &mut app,
        WgpuPolicyDefinition::new(CustomPolicy).install().unwrap(),
    )
    .unwrap();
    app
}

fn begin_stock_capture(world: &mut World) {
    world.resource_mut::<WgpuFramePacketSlot>().reset();
    world.run_system_once(reset_stock_capture).unwrap();
}

fn take_stock_frame(world: &mut World) -> Option<WgpuStockFrame> {
    let mut state = SystemState::<(
        Res<WgpuPolicyInstallation<CustomPolicy>>,
        WgpuPolicyFrameWriter,
        WgpuStockFrameReader,
    )>::new(world);
    let (installation, writer, mut reader) = state.get_mut(world).unwrap();
    reader.take(&installation, &writer)
}

#[test]
fn stock_collection_selects_no_policy_or_domain() {
    let mut app = super::tests::capture_app();
    app.add_plugins(WgpuStockCapturePlugin).unwrap();

    assert_eq!(
        WgpuStockCapturePlugin::declaration().requires_plugins,
        &[WGPU_CAPTURE_PLUGIN_ID]
    );
    assert_eq!(
        WgpuStockPolicyPlugin::declaration().requires_plugins,
        &[WGPU_STOCK_CAPTURE_PLUGIN_ID]
    );
    assert!(!app.world().contains_resource::<SelectedPolicy>());
    assert!(!app.has_plugin(WGPU_STOCK_POLICY_PLUGIN_ID));
    #[cfg(feature = "stock-sprite")]
    assert!(!app.has_plugin(WGPU_STOCK_SPRITE_PLUGIN_ID));
    #[cfg(feature = "stock-ui")]
    assert!(!app.has_plugin(WGPU_STOCK_UI_PLUGIN_ID));

    let world = app.world_mut().unwrap();
    begin_stock_capture(world);
    assert!(world.resource::<StockCaptureBuilder>().draft.is_none());
}

#[test]
fn stock_reader_moves_once_for_the_exact_selected_custom_installation() {
    let mut app = stock_capture_app();
    let other = WgpuPolicyDefinition::new(CustomPolicy).install().unwrap();
    let world = app.world_mut().unwrap();
    assert!(take_stock_frame(world).is_none());
    begin_stock_capture(world);
    assert!(!world.contains_resource::<WgpuPolicyInstallation<WgpuStockPolicy>>());
    let original = Arc::clone(
        &world
            .resource::<StockCaptureBuilder>()
            .draft
            .as_ref()
            .unwrap()
            .capture,
    );
    let mut state = SystemState::<(
        Res<WgpuPolicyInstallation<CustomPolicy>>,
        WgpuPolicyFrameWriter,
        WgpuStockFrameReader,
    )>::new(world);
    let (installation, mut writer, mut reader) = state.get_mut(world).unwrap();

    assert!(reader.take(&other, &writer).is_none());
    let frame = reader.take(&installation, &writer).unwrap();
    assert!(Arc::ptr_eq(&frame.capture, &original));
    assert!(reader.take(&installation, &writer).is_none());
    #[cfg(feature = "stock-ui")]
    let (capture, ui) = frame.into_parts();
    #[cfg(feature = "stock-ui")]
    assert!(ui.is_none());
    #[cfg(not(feature = "stock-ui"))]
    let capture = frame.into_parts();
    assert!(writer.capture(&installation, capture).unwrap());
}

#[test]
fn stock_reader_preserves_the_draft_when_the_capture_phase_has_ended() {
    let mut app = stock_capture_app();
    let world = app.world_mut().unwrap();
    begin_stock_capture(world);
    let original = Arc::clone(
        &world
            .resource::<StockCaptureBuilder>()
            .draft
            .as_ref()
            .unwrap()
            .capture,
    );
    world
        .resource_mut::<WgpuFramePacketSlot>()
        .seal(Ok(None), 1, std::time::Instant::now());

    assert!(take_stock_frame(world).is_none());
    assert!(Arc::ptr_eq(
        &world
            .resource::<StockCaptureBuilder>()
            .draft
            .as_ref()
            .unwrap()
            .capture,
        &original,
    ));
}

#[test]
fn a_consumed_capture_error_allows_a_sidecar_free_successor() {
    let mut app = stock_capture_app();
    let world = app.world_mut().unwrap();
    begin_stock_capture(world);
    world
        .resource_mut::<WgpuFramePacketSlot>()
        .reject_capture(WgpuFrameCaptureError::MissingRuntimeGeneration);
    assert!(matches!(
        world
            .resource_mut::<WgpuFramePacketSlot>()
            .take_batch()
            .unwrap()
            .captured,
        Err(WgpuFrameCaptureError::MissingRuntimeGeneration)
    ));

    begin_stock_capture(world);
    assert!(take_stock_frame(world).is_some());
}

#[cfg(feature = "stock-sprite")]
#[test]
fn sprite_collector_contributes_to_a_custom_policy_without_the_default_policy() {
    let mut app = stock_capture_app();
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
        WgpuStockSpritePlugin,
    ))
    .unwrap();
    let world = app.world_mut().unwrap();
    begin_stock_capture(world);
    world.run_system_once(collect_stock_sprites).unwrap();

    let frame = take_stock_frame(world).unwrap();
    assert!(frame.capture().sprite_batches().is_some());
    assert!(!world.contains_resource::<WgpuPolicyInstallation<WgpuStockPolicy>>());
}

#[cfg(feature = "stock-ui")]
mod ui {
    use super::*;

    use nara_app::{
        RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeControl,
        RuntimeControlRequestResult, RuntimeObligationLedger, RuntimeState,
    };
    use nara_ui_render::{
        __private::{
            UiPresentationOutcomes, UiPresentationRoute, backend_outcome_receiver,
            replace_ui_backend_frame, replace_ui_presentation_routes, snapshot_ui_paint_packets,
        },
        UiPaintPacket, UiPresentationOutcome,
    };

    use crate::frame::WgpuUiSubmissionState;

    fn ui_app() -> App {
        let mut app = stock_capture_app();
        app.add_plugins((
            nara_image::ImagePreparePlugin,
            nara_ui_render::UiRenderPlugin::default(),
            WgpuStockUiPlugin,
        ))
        .unwrap();
        app
    }

    fn queue_ui_frame(world: &mut World) -> Arc<UiPaintPacket> {
        use nara_ui_kernel::{
            UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiLayer, UiRoot, UiRootKey,
            UiStyle, UiSurfaceId, UiTheme,
        };

        let surface = UiSurfaceId::new(1).unwrap();
        replace_ui_presentation_routes(
            world,
            [UiPresentationRoute::new(surface, nara_window::WindowId::PRIMARY).unwrap()],
        )
        .unwrap();
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(UiRoot::new(
                UiRootKey::runtime("stock-capture", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            ))
            .build()
            .unwrap();
        let frame = UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(
                &snapshot,
                UiEnvironment::new(surface, 16.0, 16.0, 1.0, 1, 1, 1, 1).unwrap(),
                UiTheme::default(),
            )
            .unwrap();
        nara_ui_render::compile_ui_paint_packets(world, &[frame]).unwrap();
        let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
        let packet = planned.pop().unwrap();
        replace_ui_backend_frame(
            world,
            vec![Arc::clone(&packet)],
            vec![crate::tests::prepared_ui_attempt(
                Arc::clone(&packet),
                83,
                5,
            )],
        )
        .unwrap();
        packet
    }

    #[test]
    fn ui_collector_leaves_the_queue_untouched_when_capture_is_unavailable() {
        #[derive(Clone, Copy, Debug)]
        enum Unavailable {
            NoRoute,
            ClosedRoute,
            BusyConnection,
            ReplacedConnection,
            OccupiedSlot,
        }

        for case in [
            Unavailable::NoRoute,
            Unavailable::ClosedRoute,
            Unavailable::BusyConnection,
            Unavailable::ReplacedConnection,
            Unavailable::OccupiedSlot,
        ] {
            let mut app = ui_app();
            let world = app.world_mut().unwrap();
            let packet = queue_ui_frame(world);
            let receiver = backend_outcome_receiver(world).unwrap();
            begin_stock_capture(world);
            match case {
                Unavailable::NoRoute => {
                    world.remove_resource::<WgpuPolicyInstallation<CustomPolicy>>();
                    world.remove_resource::<SelectedPolicy>();
                }
                Unavailable::ClosedRoute => {
                    world
                        .resource_mut::<WgpuPolicyInstallation<CustomPolicy>>()
                        .close();
                }
                Unavailable::BusyConnection => {
                    world
                        .resource_mut::<WgpuRuntimeConnection>()
                        .publish(runtime::PendingFrame {
                            captured: Ok(None),
                            frame: RenderFrame::default(),
                            window: None,
                            canvas: None,
                            ui: None,
                        });
                }
                Unavailable::ReplacedConnection => {
                    world.insert_resource(WgpuRuntimeConnection::default());
                }
                Unavailable::OccupiedSlot => {
                    world.resource_mut::<WgpuFramePacketSlot>().seal(
                        Ok(None),
                        1,
                        std::time::Instant::now(),
                    );
                }
            }
            world.run_system_once(collect_stock_ui).unwrap();

            assert_eq!(receiver.pending_backend_count(), 0, "{case:?}");
            let mut state = SystemState::<UiBackendSubmissionReader>::new(world);
            let mut reader = state.get_mut(world).unwrap();
            let (packets, submissions) = reader.take().unwrap().into_parts();
            assert_eq!(packets.len(), 1, "{case:?}");
            assert!(Arc::ptr_eq(&packets[0], &packet), "{case:?}");
            assert_eq!(submissions.len(), 1, "{case:?}");
            assert_eq!(
                submissions[0].submission().identity().admission_id().get(),
                83
            );
        }
    }

    #[test]
    fn ui_collector_hands_the_custom_policy_original_packets_and_sidecar() {
        let mut app = ui_app();
        let world = app.world_mut().unwrap();
        let packet = queue_ui_frame(world);
        let receiver = backend_outcome_receiver(world).unwrap();
        begin_stock_capture(world);
        world.run_system_once(collect_stock_ui).unwrap();

        let (capture, ui) = take_stock_frame(world).unwrap().into_parts();
        assert!(Arc::ptr_eq(&capture.ui_packets()[0], &packet));
        assert_eq!(receiver.pending_backend_count(), 1);
        let submissions = ui.unwrap().into_prepared_submissions().unwrap();
        assert_eq!(submissions.len(), 1);
        assert_eq!(
            submissions[0].submission().identity().admission_id().get(),
            83
        );
        assert!(Arc::ptr_eq(submissions[0].submission().packet(), &packet));
        drop(submissions);
        assert_eq!(receiver.pending_backend_count(), 0);
    }

    fn seed_original_draft(world: &mut World) -> usize {
        world.init_resource::<UiPresentationOutcomes>();
        let packet = crate::tests::ui_outcome_packet();
        let submissions = vec![crate::tests::admit_ui_attempt(
            &mut world.resource_mut::<UiPresentationOutcomes>(),
            crate::tests::prepared_ui_attempt(Arc::clone(&packet), 97, 6),
        )];
        let allocation = submissions.as_ptr() as usize;
        world.resource_mut::<StockCaptureBuilder>().draft = Some(WgpuStockFrame {
            capture: Arc::new(WgpuStockCapture::new().with_ui(vec![packet], None, None)),
            ui: Some(WgpuUiSubmission::from_state(
                WgpuUiSubmissionState::Prepared(submissions),
            )),
        });
        allocation
    }

    #[test]
    fn ui_collection_retries_after_the_first_admission_refusal_is_resolved() {
        let mut app = ui_app();
        let world = app.world_mut().unwrap();
        let packet = queue_ui_frame(world);
        let receiver = backend_outcome_receiver(world).unwrap();
        let blocker = crate::tests::admit_ui_attempt(
            &mut world.resource_mut::<UiPresentationOutcomes>(),
            crate::tests::prepared_ui_attempt(crate::tests::ui_outcome_packet(), 84, 6),
        );
        begin_stock_capture(world);
        world.run_system_once(collect_stock_ui).unwrap();
        assert!(world.resource::<StockCaptureBuilder>().draft.is_none());
        world
            .resource_mut::<WgpuFramePacketSlot>()
            .seal(Ok(None), 1, std::time::Instant::now());
        assert!(matches!(
            world.resource_mut::<WgpuFramePacketSlot>().take_batch().unwrap().captured,
            Err(WgpuFrameCaptureError::UiPresentation(
                nara_ui_render::UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: 1,
                }
            ))
        ));

        drop(blocker);
        assert_eq!(receiver.drain().unwrap().len(), 1);
        begin_stock_capture(world);
        world.run_system_once(collect_stock_ui).unwrap();
        let (capture, ui) = take_stock_frame(world).unwrap().into_parts();
        assert!(Arc::ptr_eq(&capture.ui_packets()[0], &packet));
        let submissions = ui.unwrap().into_prepared_submissions().unwrap();
        assert_eq!(submissions.len(), 1);
        assert_eq!(
            submissions[0].submission().identity().admission_id().get(),
            83
        );
        drop(submissions);
        assert_eq!(receiver.pending_backend_count(), 0);
    }

    #[test]
    fn a_refused_custom_capture_returns_the_same_handed_off_sidecar_allocation() {
        let mut app = ui_app();
        let other = WgpuPolicyDefinition::new(CustomPolicy).install().unwrap();
        let world = app.world_mut().unwrap();
        begin_stock_capture(world);
        let allocation = seed_original_draft(world);
        let mut state = SystemState::<(
            Res<WgpuPolicyInstallation<CustomPolicy>>,
            WgpuPolicyFrameWriter,
            WgpuStockFrameReader,
        )>::new(world);
        let (installation, mut writer, mut reader) = state.get_mut(world).unwrap();

        assert!(reader.take(&other, &writer).is_none());
        let (capture, ui) = reader.take(&installation, &writer).unwrap().into_parts();
        let failure = writer
            .capture_with_ui(&other, capture, ui.unwrap())
            .unwrap_err();
        let (_, returned) = failure.into_parts();
        let submissions = returned.into_prepared_submissions().unwrap();
        assert_eq!(submissions.as_ptr() as usize, allocation);
        assert_eq!(
            submissions[0].submission().identity().admission_id().get(),
            97
        );
        assert!(writer.can_capture_with_ui(&installation));
    }

    #[test]
    fn reset_preserves_an_incomplete_draft_and_logical_close_returns_its_original_ui_once() {
        use std::time::Duration;

        let mut app = ui_app();
        let world = app.world_mut().unwrap();
        begin_stock_capture(world);
        seed_original_draft(world);
        let receiver = backend_outcome_receiver(world).unwrap();
        let original = Arc::clone(
            &world
                .resource::<StockCaptureBuilder>()
                .draft
                .as_ref()
                .unwrap()
                .capture,
        );
        world.run_system_once(reset_stock_capture).unwrap();
        assert!(matches!(
            world
                .resource_mut::<WgpuFramePacketSlot>()
                .take_batch()
                .unwrap()
                .captured,
            Err(WgpuFrameCaptureError::Policy(_))
        ));
        assert!(Arc::ptr_eq(
            &world
                .resource::<StockCaptureBuilder>()
                .draft
                .as_ref()
                .unwrap()
                .capture,
            &original,
        ));
        assert_eq!(receiver.pending_backend_count(), 1);

        let mut runtime = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap()
            .complete_startup()
            .unwrap()
            .promote();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        for _ in 0..20 {
            if runtime.state() == RuntimeState::Stopped {
                break;
            }
            runtime.drive(Duration::ZERO).unwrap();
        }

        assert_eq!(runtime.state(), RuntimeState::Stopped);
        let builder = runtime.world().resource::<StockCaptureBuilder>();
        assert!(builder.closed);
        assert!(builder.draft.is_none());
        assert_eq!(receiver.pending_backend_count(), 0);
        assert!(matches!(
            receiver.drain().unwrap().as_slice(),
            [UiPresentationOutcome::Aborted { attempt, .. }]
                if attempt.admission_id().unwrap().get() == 97
        ));
        assert!(receiver.drain().unwrap().is_empty());
        runtime.finish().unwrap();
    }
}
