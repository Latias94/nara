use super::*;

use nara_ecs::{
    World,
    bevy_ecs::system::{RunSystemOnce, SystemState},
};

use crate::gpu::{WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyProgram};

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
    #[cfg(feature = "sprite-render")]
    assert!(!app.has_plugin(WGPU_STOCK_SPRITE_PLUGIN_ID));
    #[cfg(feature = "hud-runtime")]
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
    #[cfg(feature = "hud-runtime")]
    let (capture, ui) = frame.into_parts();
    #[cfg(feature = "hud-runtime")]
    assert!(ui.is_none());
    #[cfg(not(feature = "hud-runtime"))]
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

#[cfg(feature = "sprite-render")]
#[test]
fn sprite_collector_contributes_to_a_custom_policy_without_the_default_policy() {
    let mut app = stock_capture_app();
    app.add_plugins((
        crate::image::ImagePreparePlugin,
        crate::sprite_render::SpriteRenderPlugin,
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

#[cfg(feature = "hud-runtime")]
mod ui {
    use super::*;

    use crate::hud::{
        UiPaintPacket, UiPresentationOutcome, drain_ui_presentation_outcomes,
        presentation::{
            UiPresentationOutcomes, UiPresentationRoute, backend_outcome_receiver,
            commit_ui_backend_outcomes, replace_ui_backend_frame, replace_ui_presentation_routes,
            snapshot_ui_paint_packets,
        },
    };
    use nara_app::{
        RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeControl,
        RuntimeControlRequestResult, RuntimeObligationLedger, RuntimeState,
    };
    use nara_window::WindowId;

    use crate::gpu::frame::WgpuUiSubmissionState;

    fn ui_app() -> App {
        let mut app = stock_capture_app();
        app.add_plugins((
            crate::image::ImagePreparePlugin,
            crate::hud::UiRenderPlugin::default(),
            WgpuStockUiPlugin,
        ))
        .unwrap();
        app
    }

    fn queue_ui_frame(world: &mut World, window_id: WindowId) -> Arc<UiPaintPacket> {
        use nara_ui_kernel::{
            UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiLayer, UiRoot, UiRootKey,
            UiStyle, UiSurfaceId, UiTheme,
        };

        let surface = UiSurfaceId::new(1).unwrap();
        replace_ui_presentation_routes(
            world,
            [UiPresentationRoute::new(surface, window_id).unwrap()],
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
        crate::hud::compile_ui_paint_packets(world, &[frame]).unwrap();
        let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
        let packet = planned.pop().unwrap();
        replace_ui_backend_frame(
            world,
            vec![Arc::clone(&packet)],
            vec![crate::gpu::tests::prepared_ui_attempt(
                Arc::clone(&packet),
                83,
                5,
            )],
        )
        .unwrap();
        packet
    }

    #[test]
    fn only_the_stock_window_ui_route_seals_empty_world_views() {
        use crate::{RenderFrameTarget, RenderImage2d, RenderImagePacket, RenderTarget};
        use nara_app::{CoreStage, RuntimeInstance};
        use nara_window::{WindowPlugin, WindowResolution};
        use std::time::Duration;

        let foreign = WindowId::new(9);
        for (stock_ui, ui_window, width) in [
            (true, Some(WindowId::PRIMARY), 16),
            (true, Some(foreign), 16),
            (true, None, 16),
            (false, None, 16),
            (true, None, 0),
        ] {
            let mut app = super::super::tests::capture_app();
            let window = Window::new("Empty world", WindowResolution::new(width, 16));
            let target = RenderWindowPacket {
                id: window.id,
                resolution: window.resolution,
                present_mode: window.present_mode,
            };
            app.add_plugins((
                WindowPlugin {
                    primary_window: Some(window),
                },
                WgpuStockCapturePlugin,
                WgpuStockPolicyPlugin,
            ))
            .unwrap();
            app.insert_resource(crate::WorldCanvas::new(8, 8).unwrap())
                .unwrap();
            if stock_ui {
                app.add_plugins((
                    crate::image::ImagePreparePlugin,
                    crate::hud::UiRenderPlugin::default(),
                    WgpuStockUiPlugin,
                ))
                .unwrap();
            }
            let original = ui_window.map(|window| queue_ui_frame(app.world_mut().unwrap(), window));
            app.add_systems(CoreStage::Last, move |world: &mut World| {
                let generation = *world.resource::<RuntimeGeneration>();
                let views = world.resource::<ExtractedViews>();
                assert!(views.as_slice().is_empty());
                assert!(views.world_canvas().is_some());
                let original_time = views.captured_time().unwrap();
                let batch = crate::gpu::take_captured_policy_batch(world).unwrap();
                let no_views = batch.no_views_app_frame(generation, target);
                let app_frame = batch.app_frame_index();
                let (input, ui) = batch.into_parts();
                assert_eq!(ui.is_some(), stock_ui);
                if ui_window == Some(foreign) {
                    let Err(WgpuFrameCaptureError::Policy(error)) = input else {
                        panic!("a different window's UI packet cannot target this window");
                    };
                    let message = error.to_string();
                    assert!(
                        message.contains("UI packets must belong to the captured logical target")
                    );
                    assert_eq!(app_frame, None);
                    let completions = ui.unwrap().content_rejected(message);
                    assert_eq!(completions.len(), 1);
                    commit_ui_backend_outcomes(world, completions).unwrap();
                    let outcomes = drain_ui_presentation_outcomes(world).unwrap();
                    assert!(matches!(
                        outcomes.as_slice(),
                        [UiPresentationOutcome::ContentRejected { attempt, .. }]
                            if attempt.admission_id().unwrap().get() == 83
                                && attempt.target() == Some(RenderTarget::Window(foreign))
                                && attempt.packet() == original.as_ref().unwrap().identity()
                    ));
                    assert!(drain_ui_presentation_outcomes(world).unwrap().is_empty());
                    return;
                }
                if stock_ui && width != 0 {
                    let input = input.unwrap().unwrap();
                    let packet = input.packet();
                    assert_eq!(no_views, None);
                    assert!(app_frame.is_some());
                    assert_eq!(
                        packet.source_generation().runtime_generation(),
                        Some(generation)
                    );
                    assert_eq!(packet.target(), RenderFrameTarget::Window(target));
                    assert_eq!(packet.logical_target(), RenderTarget::Window(target.id));
                    assert_eq!(packet.captured_time(), Some(original_time));
                    assert!(packet.views().is_empty());
                    assert!(packet.world_canvas().is_none());
                    assert!(input.world_input_mapping().is_none());
                    let stock = input.captured::<WgpuStockPolicy>().unwrap();
                    assert_eq!(stock.ui_packets().len(), usize::from(original.is_some()));
                    if let Some(original) = &original {
                        assert!(Arc::ptr_eq(&stock.ui_packets()[0], original));
                    }
                    let image = RenderImagePacket::new(
                        RenderImage2d::allocate_handle().unwrap(),
                        packet.target().extent(),
                    )
                    .unwrap();
                    let adapted = input.with_image_target(image).unwrap();
                    assert_eq!(adapted.packet().target(), RenderFrameTarget::Image(image));
                    assert_eq!(adapted.packet().logical_target(), packet.logical_target());
                    assert!(adapted.world_input_mapping().is_none());
                } else {
                    assert!(input.unwrap().is_none());
                    assert_eq!(app_frame.is_some(), width != 0);
                    assert_eq!(no_views, app_frame);
                }
                if let Some(ui) = ui {
                    let submissions = ui.into_prepared_submissions().unwrap();
                    assert_eq!(submissions.len(), usize::from(original.is_some()));
                    if let Some(original) = &original {
                        let submission = submissions[0].submission();
                        assert_eq!(submission.identity().admission_id().get(), 83);
                        assert!(Arc::ptr_eq(submission.packet(), original));
                    }
                }
            })
            .unwrap();
            let mut runtime = RuntimeInstance::start(app).unwrap();
            runtime.drive(Duration::ZERO).unwrap();
            runtime.finish().unwrap();
        }
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
            let packet = queue_ui_frame(world, WindowId::PRIMARY);
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
        let packet = queue_ui_frame(world, WindowId::PRIMARY);
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
        let packet = crate::gpu::tests::ui_outcome_packet();
        let submissions = vec![crate::gpu::tests::admit_ui_attempt(
            &mut world.resource_mut::<UiPresentationOutcomes>(),
            crate::gpu::tests::prepared_ui_attempt(Arc::clone(&packet), 97, 6),
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
        let packet = queue_ui_frame(world, WindowId::PRIMARY);
        let receiver = backend_outcome_receiver(world).unwrap();
        let blocker = crate::gpu::tests::admit_ui_attempt(
            &mut world.resource_mut::<UiPresentationOutcomes>(),
            crate::gpu::tests::prepared_ui_attempt(crate::gpu::tests::ui_outcome_packet(), 84, 6),
        );
        begin_stock_capture(world);
        world.run_system_once(collect_stock_ui).unwrap();
        assert!(world.resource::<StockCaptureBuilder>().draft.is_none());
        world
            .resource_mut::<WgpuFramePacketSlot>()
            .seal(Ok(None), 1, std::time::Instant::now());
        assert!(matches!(
            world
                .resource_mut::<WgpuFramePacketSlot>()
                .take_batch()
                .unwrap()
                .captured,
            Err(WgpuFrameCaptureError::UiPresentation(
                crate::hud::UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
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
