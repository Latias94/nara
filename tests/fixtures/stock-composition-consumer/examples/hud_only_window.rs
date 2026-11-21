#![deny(unsafe_code)]

#[cfg(windows)]
#[allow(dead_code, unsafe_code)]
#[path = "picking/native.rs"]
mod native;

#[cfg(windows)]
mod windows {
    use std::{
        any::Any,
        cell::RefCell,
        io,
        panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    use nara::{
        Presentation2dPlugins, PresentationUiPlugins, Runtime2dPlugins, RuntimeUiPlugins,
        WgpuBackendPlugins,
        app::{App, AppExit, CoreStage, RuntimeInstance},
        core::{Color, Vec2},
        ecs::{
            Entity, Query, ResMut, Resource, bevy_ecs::system::SystemState,
            schedule::IntoScheduleConfigs,
        },
        hierarchy::{HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
        input::{
            ButtonTransitionPhase, GamepadButton, InputPointerId, InputSeatId, InputSet,
            InputTimeline, InputWindowId,
        },
        project_host::{
            DesktopPresentationControl, DesktopPresentationObservation, DesktopRun,
            DesktopRunOptions, DesktopRunOutcome, DesktopRunProgress,
        },
        render::{
            Camera2d, ExtractedViews, RenderFrame, RenderFrameTarget, RenderImage2d,
            RenderImagePacket, RenderTarget, ViewportRect, WorldPointer,
        },
        sprite::Sprite,
        transform::Transform2d,
        ui::{
            UiActionConsumerId, UiActionCursor, UiActionId, UiActionJournal, UiActivationSource,
            UiButton, UiColor, UiNode, UiRoot, UiRuntimeKey, UiSafeAreaInsets, UiSize, UiStyle,
            UiSurfaceContentTransform, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiVisual,
            install_ui_surface,
        },
        ui_render::advanced::{
            PresentedUiObservation, UiPacketFill, UiPacketPrimitive, UiPacketRect,
            latest_presented_ui_observation,
        },
        window::{Window, WindowId, WindowPlugin, WindowResolution},
    };
    use nara_render_wgpu::{
        WgpuCompositeColor, WgpuPolicyCaptureSet, WgpuPolicyFrameReader, WgpuRenderHost,
        WgpuRenderInput, WgpuStockPolicy, WgpuStockSpritePlugin, WgpuStockUiPlugin,
    };
    use nara_stock_composition_consumer::readback;

    use super::native::NativeProbe;

    pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    type WindowAndPointer = SystemState<(
        Query<'static, 'static, &'static Window>,
        WorldPointer<'static, 'static>,
    )>;

    const WIDE: [u32; 2] = [480, 240];
    const CLIPPED: [u32; 2] = [320, 240];
    const SURFACE: UiSurfaceId = UiSurfaceId::new(1).unwrap();
    const INPUT_WINDOW: InputWindowId = InputWindowId::new(WindowId::PRIMARY.get());
    const MENU_ACTION: &str = "continue";
    const POLL: Duration = Duration::from_millis(10);

    #[derive(Resource)]
    struct RuntimeLifetime(Arc<AtomicBool>);

    impl Drop for RuntimeLifetime {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[derive(Default, Resource)]
    struct CapturedInput(Option<WgpuRenderInput>);

    #[derive(Resource)]
    struct MenuInput {
        requested: Arc<AtomicBool>,
        cursor: UiActionCursor,
        press: Option<u64>,
    }

    #[derive(Debug)]
    struct Presented {
        input: WgpuRenderInput,
        ui: PresentedUiObservation,
        resolution: WindowResolution,
    }

    #[derive(Default)]
    struct Observed {
        latest: Option<Presented>,
        failure: Option<Box<dyn Any + Send>>,
    }

    pub(super) fn run() -> Result<()> {
        assert_eq!(std::env::var("WGPU_BACKEND").as_deref(), Ok("dx12"));
        assert_eq!(
            std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
            Ok("0")
        );
        let title = format!("Nara HUD without a View {}", std::process::id());
        let dropped = Arc::new(AtomicBool::new(false));
        let menu = Arc::new(AtomicBool::new(false));
        let (runtime, camera, mut pointer) = fixture(&title, &dropped, &menu)?;
        let observed = Rc::new(RefCell::new(Observed::default()));
        let callback = Rc::clone(&observed);
        let options = DesktopRunOptions::new()
            .with_initial_visibility(false)
            .with_input_isolation(true)
            .with_presentation_observer(move |observation| {
                if callback.borrow().failure.is_some() {
                    return DesktopPresentationControl::Exit(AppExit::Requested);
                }
                match catch_unwind(AssertUnwindSafe(|| {
                    observe(observation, &mut pointer, camera)
                })) {
                    Ok(frame) => {
                        if let Some(frame) = frame {
                            callback.borrow_mut().latest = Some(frame);
                        }
                        DesktopPresentationControl::Continue
                    }
                    Err(payload) => {
                        callback.borrow_mut().failure = Some(payload);
                        DesktopPresentationControl::Exit(AppExit::Requested)
                    }
                }
            });
        let mut run = DesktopRun::from_runtime(runtime, options);
        let mut native = None;
        let trajectory = catch_unwind(AssertUnwindSafe(|| -> Result<Presented> {
            let initial = wait_for(&mut run, &observed, WIDE, Duration::from_secs(30))?;
            native = Some(NativeProbe::find_exact(&title).map_err(io::Error::other)?);
            let native = native.as_ref().unwrap();
            verify_geometry(native, &initial)?;
            assert!(native.resize_client(CLIPPED).map_err(io::Error::other)?);
            let clipped = wait_for(&mut run, &observed, CLIPPED, Duration::from_secs(8))?;
            verify_geometry(native, &clipped)?;

            menu.store(true, Ordering::Release);
            let activated = wait_for(&mut run, &observed, CLIPPED, Duration::from_secs(8))?;
            run.observe(|world| {
                let input = world.resource::<MenuInput>();
                let actions = world.resource::<UiActionJournal>().pending(&input.cursor).unwrap();
                let [action] = actions else {
                    panic!("expected one menu activation from the synthetic South press, got {actions:?}");
                };
                assert_eq!(action.input_sequence(), Some(input.press.expect("accepted South press")));
                assert_eq!(action.action().as_str(), MENU_ACTION);
                assert_eq!(action.source(), UiActivationSource::Gamepad);
                let receipt = clipped.ui.receipt();
                assert_eq!(action.surface_id(), receipt.surface_id());
                assert_eq!(action.presentation_epoch(), receipt.presentation_epoch());
                assert_eq!(action.frame_revision(), receipt.frame_revision().get());
            })?;

            assert!(native.resize_client(WIDE).map_err(io::Error::other)?);
            let recovered = wait_for(&mut run, &observed, WIDE, Duration::from_secs(8))?;
            verify_geometry(native, &recovered)?;
            assert_eq!(initial.resolution, recovered.resolution);
            assert_eq!(
                initial.input.packet().views(),
                recovered.input.packet().views()
            );
            for (before, after) in [
                (&initial, &clipped),
                (&clipped, &activated),
                (&activated, &recovered),
            ] {
                assert_eq!(
                    before.ui.backend_generation(),
                    after.ui.backend_generation()
                );
                assert_eq!(before.input.installation(), after.input.installation());
                assert_eq!(
                    before.input.packet().source_generation(),
                    after.input.packet().source_generation()
                );
                assert!(before.input.packet().frame_index() < after.input.packet().frame_index());
                assert!(
                    before.ui.receipt().presentation_epoch()
                        < after.ui.receipt().presentation_epoch()
                );
            }
            Ok(clipped)
        }));
        let cleanup = retire(&mut run, native.as_ref(), &dropped);
        drop(run);
        if let Err(error) = &cleanup {
            eprintln!("{error}");
        }
        if let Some(payload) = observed.borrow_mut().failure.take() {
            resume_unwind(payload);
        }
        let clipped = trajectory.unwrap_or_else(|panic| resume_unwind(panic))?;
        cleanup?;
        verify_pixels(&clipped)?;
        println!(
            "HUD_ONLY_WINDOW_OK verified_presentations=4 same_view_recovered=true original_ui_receipt=true menu_activated=true exact_replay_pixels=true native_retired=true"
        );
        Ok(())
    }

    fn fixture(
        title: &str,
        dropped: &Arc<AtomicBool>,
        menu: &Arc<AtomicBool>,
    ) -> Result<(RuntimeInstance, Entity, WindowAndPointer)> {
        let mut app = App::new();
        app.add_plugins((
            Runtime2dPlugins,
            RuntimeUiPlugins,
            Presentation2dPlugins,
            PresentationUiPlugins,
            WindowPlugin {
                primary_window: Some(Window::new(title, WindowResolution::new(WIDE[0], WIDE[1]))),
            },
            WgpuBackendPlugins,
            WgpuStockSpritePlugin,
            WgpuStockUiPlugin,
        ))?;
        app.insert_resource(RuntimeLifetime(Arc::clone(dropped)))?;
        app.init_resource::<CapturedInput>()?;
        app.add_systems(
            CoreStage::First,
            activate_menu.before(InputSet::ProjectTimeline),
        )?;
        app.add_systems(
            CoreStage::Render,
            retain_input
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        let world = app.world_mut()?;
        let endpoint =
            UiSurfaceEndpoint::new(INPUT_WINDOW, InputSeatId::PRIMARY, InputPointerId::PRIMARY);
        {
            let mut timeline = world.resource_mut::<InputTimeline>();
            timeline.connect_endpoint(endpoint.window(), endpoint.seat())?;
            timeline.connect_pointer(endpoint.window(), endpoint.seat(), endpoint.pointer())?;
        }
        let size = |extent: [u32; 2]| UiSize::new(extent[0] as f32, extent[1] as f32).unwrap();
        install_ui_surface(
            world,
            UiSurfaceSlot::new(
                SURFACE,
                endpoint,
                UiSurfaceContentTransform::new(
                    size(CLIPPED),
                    size(WIDE),
                    UiSafeAreaInsets::default(),
                    1.0,
                )?,
            )?,
        )?;
        let cursor = world
            .resource_mut::<UiActionJournal>()
            .register_consumer(UiActionConsumerId::new("hud-only-window")?)?;
        world.insert_resource(MenuInput {
            requested: Arc::clone(menu),
            cursor,
            press: None,
        });

        let camera = world
            .spawn((
                Camera2d {
                    viewport: ViewportRect::new(CLIPPED[0], 0, WIDE[0] - CLIPPED[0], WIDE[1]),
                    viewport_height: WIDE[1] as f32,
                    clear_color: Some(Color::rgba(0.0, 0.0, 1.0, 1.0)),
                    ..Default::default()
                },
                Transform2d::IDENTITY,
            ))
            .id();
        let sprite = world
            .spawn((
                Sprite::from_color(Vec2::splat(32.0), Color::WHITE),
                Transform2d::IDENTITY,
            ))
            .id();
        let key = |local| UiRuntimeKey::new("consumer.hud-only-window", local);
        let root = world
            .spawn((UiRoot::default(), UiNode::default(), key("hud")?))
            .id();
        let swatch = world
            .spawn((
                UiNode::new(UiStyle::absolute(12.0, 12.0, 16.0, 16.0)),
                UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
                key("swatch")?,
            ))
            .id();
        let button = world
            .spawn((
                UiNode::new(UiStyle::absolute(48.0, 12.0, 96.0, 36.0)),
                UiVisual::solid(UiColor::rgba(0.0, 0.0, 1.0, 1.0)),
                UiButton::new("Continue", UiActionId::new(MENU_ACTION)?, 16.0)?,
                key(MENU_ACTION)?,
            ))
            .id();
        let mut hierarchy = HierarchyWriter::new(world);
        for entity in [camera, sprite, root] {
            hierarchy.place(
                entity,
                HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            )?;
        }
        for entity in [swatch, button] {
            hierarchy.place(
                entity,
                HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: root }),
            )?;
        }
        let pointer = SystemState::new(world);
        Ok((RuntimeInstance::start(app)?, camera, pointer))
    }

    fn retain_input(
        reader: WgpuPolicyFrameReader,
        mut retained: ResMut<CapturedInput>,
    ) -> nara::ecs::bevy_ecs::error::Result {
        retained.0 = reader
            .read()
            .map_err(|error| io::Error::other(error.to_string()))?
            .cloned();
        Ok(())
    }

    fn activate_menu(
        mut menu: ResMut<MenuInput>,
        mut timeline: ResMut<InputTimeline>,
    ) -> nara::ecs::bevy_ecs::error::Result {
        if !menu.requested.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        let gamepad = timeline.connect_synthetic_gamepad(INPUT_WINDOW, InputSeatId::PRIMARY, 0)?;
        for button in [GamepadButton::DPadDown, GamepadButton::South] {
            let press = timeline.admit_gamepad_button(
                &gamepad,
                button.code(),
                ButtonTransitionPhase::Pressed,
            )?;
            if button == GamepadButton::South {
                menu.press = press.first_sequence();
            }
            timeline.admit_gamepad_button(
                &gamepad,
                button.code(),
                ButtonTransitionPhase::Released,
            )?;
        }
        Ok(())
    }

    fn observe(
        observation: DesktopPresentationObservation<'_>,
        pointer: &mut WindowAndPointer,
        camera: Entity,
    ) -> Option<Presented> {
        let receipt = observation.presented_frame()?;
        let world = observation.world().expect("the original live Runtime");
        let input = world.resource::<CapturedInput>().0.as_ref()?;
        let RenderFrameTarget::Window(target) = input.packet().target() else {
            panic!("the original input must target its native Window");
        };
        let resolution = target.resolution;
        let size = [resolution.physical_width, resolution.physical_height];
        if ![WIDE, CLIPPED].contains(&size) {
            return None;
        }
        let ui = latest_presented_ui_observation(world)?;
        let facts = ui.wgpu().expect("original WGPU UI submission facts");
        let frame = receipt.render_transaction_frame_index();
        if facts.render_transaction_frame_index() != frame {
            return None;
        }
        let adapter = observation
            .adapter_summary()
            .expect("original native adapter");
        assert_eq!(adapter.backend(), "dx12");
        assert!(!adapter.device_type().eq_ignore_ascii_case("cpu"));
        let generation = observation.runtime_generation();
        assert_eq!(receipt.runtime_generation(), generation);
        assert_eq!(target.id, WindowId::PRIMARY);
        assert_eq!(receipt.window_id(), target.id);
        assert_eq!(
            input.packet().source_generation().runtime_generation(),
            Some(generation)
        );
        assert_eq!(input.packet().frame_index(), frame);
        assert_eq!(world.resource::<RenderFrame>().index, frame);
        assert_eq!(facts.app_frame_index(), receipt.app_frame_index());
        assert_eq!(
            ui.backend_generation().backend_instance_id(),
            receipt.backend_instance_id()
        );
        assert_eq!(
            ui.backend_generation().device_epoch(),
            receipt.device_epoch()
        );

        let capture = input
            .captured::<WgpuStockPolicy>()
            .expect("ordinary selected stock policy");
        let [captured] = capture.ui_packets() else {
            panic!("one original stock HUD packet is required");
        };
        let packet = ui.packet();
        assert!(Arc::ptr_eq(captured, &packet));
        assert_eq!(ui.submission().packet(), packet.identity());
        assert_eq!(ui.submission().runtime_generation().get(), generation.get());
        assert!(ui.submission().matches_receipt(ui.receipt()));
        assert_eq!(ui.receipt().surface_id(), SURFACE);
        assert_eq!(ui.submission().target(), RenderTarget::Window(target.id));
        assert_eq!(packet.target(), ui.submission().target());
        assert!(!packet.primitives().is_empty());
        assert!(facts.submitted_quad_count() > 0);

        let views = input.packet().views();
        assert_eq!(views, world.resource::<ExtractedViews>().as_slice());
        let (windows, pointer) = pointer
            .get(world)
            .expect("original Window and WorldPointer");
        let window = windows.single().expect("one original primary Window");
        assert_eq!(window.id, target.id);
        assert_eq!(window.resolution, resolution);
        let viewport = world
            .get::<Camera2d>(camera)
            .expect("the same authored camera remains")
            .viewport
            .unwrap();
        let center = Vec2::new(
            viewport.physical_x as f32 + viewport.physical_width as f32 * 0.5,
            viewport.physical_height as f32 * 0.5,
        ) / resolution.scale_factor as f32;
        let picked = pointer.window_to_world(camera, center);
        if size == CLIPPED {
            assert_eq!(viewport.physical_x, size[0]);
            assert!(views.is_empty());
            assert_eq!(
                input.packet().logical_target(),
                RenderTarget::Window(target.id)
            );
            assert!(input.packet().world_canvas().is_none());
            assert!(input.world_input_mapping().is_none());
            assert!(picked.is_none());
            assert_eq!(facts.frame_submitted_sprite_count(), 0);
        } else {
            let [view] = views else {
                panic!("one original world View is required");
            };
            assert_eq!(view.camera_entity, camera);
            assert_eq!(view.viewport, viewport);
            assert!(input.world_input_mapping().is_some());
            assert!(picked.is_some_and(|point| point.abs_diff_eq(Vec2::ZERO, 1.0e-4)));
            assert_eq!(facts.frame_submitted_sprite_count(), 1);
        }
        Some(Presented {
            input: input.clone(),
            ui,
            resolution,
        })
    }

    fn wait_for(
        run: &mut DesktopRun,
        observed: &RefCell<Observed>,
        size: [u32; 2],
        timeout: Duration,
    ) -> Result<Presented> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let progress = run.poll(POLL);
            if progress != DesktopRunProgress::Running {
                return Err(io::Error::other(format!(
                    "original Runtime left Running: {progress:?}"
                ))
                .into());
            }
            if let Some(frame) = observed.borrow_mut().latest.take()
                && [
                    frame.resolution.physical_width,
                    frame.resolution.physical_height,
                ] == size
            {
                println!(
                    "HUD_ONLY_PRESENT size={size:?} frame={} views={}",
                    frame.input.packet().frame_index(),
                    frame.input.packet().views().len()
                );
                return Ok(frame);
            }
        }
        Err(io::Error::other(format!(
            "no joined native Window and UI receipt for {size:?} before {timeout:?}"
        ))
        .into())
    }

    fn verify_geometry(native: &NativeProbe, frame: &Presented) -> Result<()> {
        let geometry = native.geometry().map_err(io::Error::other)?;
        let resolution = frame.resolution;
        assert_eq!(
            geometry.size,
            [resolution.physical_width, resolution.physical_height]
        );
        assert_eq!(geometry.scale, resolution.scale_factor);
        Ok(())
    }

    fn retire(
        run: &mut DesktopRun,
        native: Option<&NativeProbe>,
        dropped: &AtomicBool,
    ) -> Result<()> {
        run.request_close();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !matches!(
            run.progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        ) {
            if Instant::now() >= deadline {
                return Err(io::Error::other("original DesktopRun retirement timed out").into());
            }
            if run.poll(POLL) == DesktopRunProgress::CleanupIncomplete {
                run.execute();
            }
        }
        let report = run.execute();
        let pump_returned = run.take_event_loop().is_some();
        if let Some(native) = native {
            native.verify_destroyed().map_err(io::Error::other)?;
        }
        if report.outcome() != DesktopRunOutcome::Completed(AppExit::Requested)
            || report.diagnostics().has_errors()
            || !dropped.load(Ordering::Acquire)
            || !pump_returned
        {
            return Err(io::Error::other(format!(
                "original DesktopRun retirement was incomplete: {report:?}"
            ))
            .into());
        }
        Ok(())
    }

    fn verify_pixels(original: &Presented) -> Result<()> {
        let packet = original.ui.packet();
        let swatch = UiPacketRect::new(12, 12, 28, 28).unwrap();
        assert!(packet.primitives().iter().any(|primitive| matches!(primitive,
            UiPacketPrimitive::Rect { rect, fill: Some(UiPacketFill::Solid { color }), border: None, .. }
                if *rect == swatch && *color == UiColor::rgba(1.0, 1.0, 1.0, 1.0)
        )));

        let source = original.input.packet();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            source.target().extent(),
        )?;
        let input = original.input.with_image_target(target)?;
        assert_eq!(input.installation(), original.input.installation());
        assert_eq!(input.packet().logical_target(), source.logical_target());
        assert_eq!(
            input.packet().source_generation(),
            source.source_generation()
        );
        assert_eq!(input.packet().frame_index(), source.frame_index());
        assert!(input.packet().views().is_empty());
        assert!(input.packet().world_canvas().is_none());
        assert!(input.world_input_mapping().is_none());

        let mut host = WgpuRenderHost::offscreen();
        host.begin_frame(1)?;
        let producer = host.create_producer()?;
        let reservation =
            host.reserve_image_target(&producer, source.source_generation(), target)?;
        host.activate_image_target(&producer, reservation)?;
        let candidate = host.prepare(&producer, &input)?;
        let frame = host
            .submit_candidate(&producer, candidate)
            .map_err(|failure| failure.error)?;
        let receipt = frame.image().receipt();
        assert_ne!(
            receipt.backend_instance_id(),
            original.ui.backend_generation().backend_instance_id()
        );
        assert_eq!(
            receipt.render_transaction_frame_index(),
            source.frame_index()
        );
        assert_eq!(receipt.source_generation(), source.source_generation());
        assert_eq!(receipt.target(), target);
        let output = frame.final_texture::<WgpuCompositeColor>()?;
        let pixels = readback::read(&mut host, &output);
        drop(output);
        drop(frame);
        let prefix = host
            .retire_producer(&producer)
            .expect("replay submission prefix");
        assert!(prefix.poll()?);
        readback::finish(&mut host);

        assert_eq!([pixels.width, pixels.height], CLIPPED);
        let pixel = |x: u32, y: u32| pixels.pixels[(y * pixels.width + x) as usize];
        for y in 12..28 {
            for x in 12..28 {
                assert_eq!(pixel(x, y), [255; 4], "HUD colour at ({x}, {y})");
            }
        }
        for [x, y] in [[0, 0], [11, 12], [28, 12], [12, 28], [160, 120], [319, 239]] {
            assert_eq!(
                pixel(x, y),
                [0, 0, 0, 255],
                "opaque black base at ({x}, {y})"
            );
        }
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> windows::Result<()> {
    windows::run()
}

#[cfg(not(windows))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err(std::io::Error::other("this native HUD qualification requires Windows and DX12").into())
}
