use super::*;
use nara_app::{App, RuntimeInstance};
use nara_ecs::{Entity, World, bevy_ecs::system::SystemState};
use nara_render::{
    Color, ExtractedView, ExtractedViews, RenderFramePacket, RenderTarget, ViewLoadOp,
    ViewMapping2d, ViewportRect, WorldPointer, build_render_frame_packet,
};
use nara_window::{Window, WindowResolution};

struct PresentedWorld {
    world: World,
    camera: Entity,
    window: Window,
    packet: RenderFramePacket,
}

impl PresentedWorld {
    fn new(runtime: &RuntimeInstance, window_id: WindowId, frame: u64) -> Self {
        Self::with_scale(runtime, window_id, frame, 1.0)
    }

    fn with_scale(runtime: &RuntimeInstance, window_id: WindowId, frame: u64, scale: f64) -> Self {
        let mut resolution = WindowResolution::new(64, 64);
        resolution.scale_factor = scale;
        Self::with_window(
            runtime,
            Window {
                id: window_id,
                resolution,
                ..Default::default()
            },
            frame,
        )
    }

    fn with_window(runtime: &RuntimeInstance, window: Window, frame: u64) -> Self {
        let mut world = World::new();
        world.spawn(window.clone());
        world.insert_resource(runtime.generation());
        let camera = world.spawn_empty().id();
        let viewport = ViewportRect::new(
            0,
            0,
            window.resolution.physical_width,
            window.resolution.physical_height,
        )
        .unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: nara_render::RenderViewId::try_acquire().unwrap(),
            camera_entity: camera,
            target: RenderTarget::Window(window.id),
            viewport,
            mapping: ViewMapping2d::orthographic(nara_core::Mat3::IDENTITY, 64.0, viewport)
                .unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        let packet = build_render_frame_packet(
            runtime.generation(),
            frame,
            Some(window.id),
            [&window],
            &views,
        )
        .unwrap()
        .unwrap();
        Self {
            world,
            camera,
            window,
            packet,
        }
    }

    fn can_pick(&mut self) -> bool {
        SystemState::<WorldPointer>::new(&mut self.world)
            .get(&self.world)
            .unwrap()
            .window_to_world(self.camera, nara_core::Vec2::splat(32.0))
            .is_some()
    }

    // These CPU state-transition cases deliberately do not claim native presentation.
    fn seed(&mut self, host: &mut WgpuRenderHost, producer: &super::super::WgpuRenderProducer) {
        let (snapshot, guard) =
            nara_render::__private::prepare_world_presentation(&self.packet).unwrap();
        guard.mark_presented();
        let view = host.views.get_mut(&producer.id()).unwrap();
        view.admission.source_generation = Some(self.packet.source_generation());
        view.world_presentation
            .replace(&view.closed, guard.revoker())
            .unwrap();
        view.runtime_presentation = Some(RuntimeWorldPresentation {
            _guard: guard,
            window: nara_render::RenderWindowPacket {
                id: self.window.id,
                resolution: self.window.resolution,
                present_mode: self.window.present_mode,
            },
            canvas: None,
        });
        self.world.insert_resource(snapshot);
    }
}

#[test]
fn producer_drop_revokes_current_and_delayed_world_permissions_without_host_maintenance() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let a = host.create_producer().unwrap();
    let b = host.create_producer().unwrap();
    let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    let mut successor = PresentedWorld::new(&runtime, WindowId::new(1), 2);
    let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
    first.seed(&mut host, &a);
    second.seed(&mut host, &b);
    assert!(first.can_pick());
    successor.seed(&mut host, &a);
    assert!(!first.can_pick());
    assert!(successor.can_pick());
    let delayed = successor
        .world
        .remove_resource::<nara_render::__private::PresentedWorldFrame>()
        .unwrap();
    drop(a);
    // Returning the original result after close cannot reactivate its permission.
    successor.world.insert_resource(delayed);
    assert!(!first.can_pick());
    assert!(!successor.can_pick());
    assert!(second.can_pick());
    // No Host method has run since Drop. Native cleanup is a separate later responsibility.
    runtime.finish().unwrap();
}

#[test]
fn a_closed_producer_refuses_and_revokes_a_new_world_permission() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let mut frame = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    let (snapshot, guard) =
        nara_render::__private::prepare_world_presentation(&frame.packet).unwrap();
    guard.mark_presented();
    frame.world.insert_resource(snapshot);
    assert!(frame.can_pick());
    let slot = std::sync::Arc::clone(&producer.world_presentation);
    let closed = std::sync::Arc::clone(&producer.closed);
    drop(producer);
    assert!(matches!(
        slot.replace(&closed, guard.revoker()),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(!frame.can_pick());
    runtime.finish().unwrap();
}

#[test]
fn changed_window_or_retired_authority_revokes_only_its_producer() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    for retire_authority in [false, true] {
        let mut host = WgpuRenderHost::offscreen();
        let a = host.create_producer().unwrap();
        let b = host.create_producer().unwrap();
        let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
        let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
        first.seed(&mut host, &a);
        second.seed(&mut host, &b);
        assert!(first.can_pick());
        assert!(second.can_pick());
        let mut current = nara_render::RenderWindowPacket {
            id: first.window.id,
            resolution: first.window.resolution,
            present_mode: first.window.present_mode,
        };
        if retire_authority {
            host.handles = Some(BackendWindowHandles::default());
        } else {
            current.resolution = WindowResolution::new(128, 64);
        }
        host.validate_runtime_presentation(&a, runtime.generation(), Some(current), None);
        assert!(!first.can_pick());
        assert!(second.can_pick());
        assert!(host.views[&a.id()].runtime_presentation.is_none());
        assert!(host.views[&b.id()].runtime_presentation.is_some());
    }
    runtime.finish().unwrap();
}

#[test]
#[ignore = "requires an explicitly selected native GPU; verifies owner registration, not native window presentation"]
fn native_runtime_world_guards_replace_and_retire_per_producer_and_revoke_on_device_loss() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let namespace = host.prepare_namespace(None).unwrap();
    let a = host.create_producer().unwrap();
    let b = host.create_producer().unwrap();
    let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
    first.world.insert_resource(
        host.commit_runtime_world_presentation(a.id(), &first.packet)
            .unwrap()
            .unwrap(),
    );
    second.world.insert_resource(
        host.commit_runtime_world_presentation(b.id(), &second.packet)
            .unwrap()
            .unwrap(),
    );
    assert!(first.can_pick());
    assert!(second.can_pick());

    let mut successor = PresentedWorld::new(&runtime, WindowId::new(1), 2);
    successor.world.insert_resource(
        host.commit_runtime_world_presentation(a.id(), &successor.packet)
            .unwrap()
            .unwrap(),
    );
    assert!(!first.can_pick());
    assert!(successor.can_pick());
    assert!(second.can_pick());
    let replacement = host.create_producer().unwrap();
    let mut same_window = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    same_window.world.insert_resource(
        host.commit_runtime_world_presentation(replacement.id(), &same_window.packet)
            .unwrap()
            .unwrap(),
    );
    assert!(
        !successor.can_pick(),
        "the same surface no longer displays A's pixels"
    );
    assert!(same_window.can_pick());
    assert!(
        second.can_pick(),
        "another window still displays B's pixels"
    );
    host.retire_producer(&a);
    assert!(!successor.can_pick());
    assert!(
        same_window.can_pick(),
        "closing A cannot revoke its target's successor"
    );
    assert!(second.can_pick());
    namespace.device.destroy();
    namespace.device.poll(wgpu::PollType::Poll).unwrap();
    assert!(!second.can_pick());
    assert!(!same_window.can_pick());
    drop(host);
    runtime.finish().unwrap();
}

#[test]
fn another_runtime_cannot_revoke_the_original_producers_world_permission() {
    let original = RuntimeInstance::start(App::new()).unwrap();
    let foreign = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let mut frame = PresentedWorld::new(&original, WindowId::new(1), 1);
    frame.seed(&mut host, &producer);
    assert!(frame.can_pick());
    // Foreign geometry is not a lifecycle observation of the original producer's Runtime.
    host.validate_runtime_presentation(&producer, foreign.generation(), None, None);
    assert!(frame.can_pick());
    // The actual owner losing its window still revokes the original permission.
    host.validate_runtime_presentation(&producer, original.generation(), None, None);
    assert!(!frame.can_pick());
    drop(host);
    original.finish().unwrap();
    foreign.finish().unwrap();
}

#[test]
#[ignore = "requires an explicitly selected native GPU; verifies owner registration, not native window presentation"]
fn native_unpickable_successor_revokes_displaced_world_without_issuing_permission() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    for same_producer in [false, true] {
        let mut host = WgpuRenderHost::offscreen();
        host.prepare_namespace(None).unwrap();
        let a = host.create_producer().unwrap();
        let b = host.create_producer().unwrap();
        let c = host.create_producer().unwrap();
        let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
        let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
        let mut third = PresentedWorld::new(&runtime, WindowId::new(3), 1);
        first.world.insert_resource(
            host.commit_runtime_world_presentation(a.id(), &first.packet)
                .unwrap()
                .unwrap(),
        );
        second.world.insert_resource(
            host.commit_runtime_world_presentation(b.id(), &second.packet)
                .unwrap()
                .unwrap(),
        );
        third.world.insert_resource(
            host.commit_runtime_world_presentation(c.id(), &third.packet)
                .unwrap()
                .unwrap(),
        );
        assert!(first.can_pick());
        assert!(second.can_pick());
        assert!(third.can_pick());
        // Packet construction admits physical pixels even when no logical mapping can be made.
        let successor = PresentedWorld::with_scale(&runtime, WindowId::new(1), 2, 0.0);
        let owner = if same_producer { a.id() } else { c.id() };
        assert!(
            host.commit_runtime_world_presentation(owner, &successor.packet)
                .unwrap()
                .is_none()
        );
        assert!(!first.can_pick());
        assert!(second.can_pick());
        assert_eq!(
            third.can_pick(),
            same_producer,
            "an unpickable successor also revokes its own producer's previous window"
        );
        assert!(host.views[&owner].runtime_presentation.is_none());
        drop(host);
    }
    runtime.finish().unwrap();
}

#[cfg(all(feature = "editor-imgui", target_os = "windows"))]
mod native {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
        sync::Arc,
        time::Instant,
    };

    use nara_render::{Extent2d, RenderFrame, RenderFrameState, RenderImage2d, RenderImagePacket};
    use nara_window::backend::WindowHandleProvider;
    use winit::{
        application::ApplicationHandler,
        dpi::PhysicalSize,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop},
        platform::windows::EventLoopBuilderExtWindows,
        window::{Window as NativeWindow, WindowId as NativeWindowId},
    };

    use super::*;

    #[test]
    #[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
    fn native_window_clear_revokes_only_displaced_runtime_after_success() {
        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        let event_loop = builder.build().unwrap();
        let mut app = ReplacementApp { result: None };
        event_loop.run_app(&mut app).unwrap();
        if let Err(failure) = app
            .result
            .expect("the native replacement trajectory must run")
        {
            resume_unwind(failure);
        }
    }

    struct ReplacementApp {
        result: Option<std::thread::Result<()>>,
    }

    impl ApplicationHandler for ReplacementApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.result.is_some() {
                return;
            }
            self.result = Some(catch_unwind(AssertUnwindSafe(|| {
                verify_replacement(event_loop)
            })));
            event_loop.exit();
        }

        fn window_event(
            &mut self,
            _event_loop: &ActiveEventLoop,
            _window_id: NativeWindowId,
            _event: WindowEvent,
        ) {
        }
    }

    fn verify_replacement(event_loop: &ActiveEventLoop) {
        let windows = ["Nara replaced Runtime", "Nara retained Runtime"].map(|title| {
            Arc::new(
                event_loop
                    .create_window(
                        NativeWindow::default_attributes()
                            .with_title(title)
                            .with_visible(false)
                            .with_inner_size(PhysicalSize::new(320, 240)),
                    )
                    .unwrap(),
            )
        });
        let runtime_a = RuntimeInstance::start(App::new()).unwrap();
        let runtime_b = RuntimeInstance::start(App::new()).unwrap();
        let mut host = WgpuRenderHost::new(
            WindowId::new(1),
            WindowHandleProvider::new(Arc::clone(&windows[0])),
        )
        .unwrap();
        host.register_window(
            WindowId::new(2),
            WindowHandleProvider::new(Arc::clone(&windows[1])),
        )
        .unwrap();
        let a = host.create_producer().unwrap();
        let b = host.create_producer().unwrap();
        let mut first =
            PresentedWorld::with_window(&runtime_a, window_state(&windows[0], WindowId::new(1)), 1);
        let mut second =
            PresentedWorld::with_window(&runtime_b, window_state(&windows[1], WindowId::new(2)), 1);
        present_runtime(&mut host, &a, &mut first);
        present_runtime(&mut host, &b, &mut second);

        // Keep a real immutable output alive across both the failed and successful paint.
        let image_producer = host.create_producer().unwrap();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            Extent2d::new(
                first.window.resolution.physical_width,
                first.window.resolution.physical_height,
            )
            .unwrap(),
        )
        .unwrap();
        let image_packet = first.packet.with_image_target(target).unwrap();
        let reservation = host
            .reserve_image_target(&image_producer, image_packet.source_generation(), target)
            .unwrap();
        host.activate_image_target(&image_producer, reservation)
            .unwrap();
        let output = host
            .submit(
                &image_producer,
                &crate::host::tests::test_stock_input(image_packet),
            )
            .unwrap();
        let receipt = output.receipt();
        assert_eq!(
            crate::host::tests::read_output_pixel(&mut host, &output, 16, 16),
            [0, 0, 0, 255]
        );
        let RenderFrameTarget::Window(window) = first.packet.target() else {
            unreachable!()
        };
        let previous = (
            host.last_presentation_generation,
            host.frame_transaction_stats().presents(),
        );
        let error = host
            .present_window(
                window,
                2,
                &[],
                |frame| {
                    clear(frame);
                    Err(WgpuRenderHostError::window_paint(
                        "injected composition refusal",
                    ))
                },
                || panic!("a refused paint must not reach pre-present"),
            )
            .unwrap_err();
        assert!(matches!(error, WgpuRenderHostError::WindowPaint { .. }));
        assert!(
            first.can_pick(),
            "a failed paint still displays Runtime A's pixels"
        );
        assert!(second.can_pick());
        assert!(host.views[&a.id()].runtime_presentation.is_some());
        assert_eq!(
            (
                host.last_presentation_generation,
                host.frame_transaction_stats().presents(),
            ),
            previous
        );

        let outcome = host
            .present_window(
                window,
                3,
                &[],
                |frame| {
                    clear(frame);
                    Ok(())
                },
                || windows[0].pre_present_notify(),
            )
            .unwrap();
        assert!(
            outcome.window().is_some(),
            "the clear must physically present"
        );
        assert!(outcome.composites().is_empty());
        assert!(
            !first.can_pick(),
            "the successful clear displaced Runtime A's pixels"
        );
        assert!(
            second.can_pick(),
            "another window retains Runtime B's authority"
        );
        assert!(host.views[&a.id()].runtime_presentation.is_none());
        assert!(host.views[&b.id()].runtime_presentation.is_some());
        assert!(output.is_current());
        assert_eq!(output.receipt(), receipt);
        assert_eq!(
            crate::host::tests::read_output_pixel(&mut host, &output, 16, 16),
            [0, 0, 0, 255]
        );
        host.display_image(output).unwrap();
        host.wait_for_idle().unwrap();
        drop(host);
        runtime_a.finish().unwrap();
        runtime_b.finish().unwrap();
    }

    fn window_state(window: &NativeWindow, id: WindowId) -> Window {
        let size = window.inner_size();
        Window {
            id,
            resolution: WindowResolution::new(size.width, size.height)
                .with_scale_factor(window.scale_factor()),
            ..Default::default()
        }
    }

    fn present_runtime(
        host: &mut WgpuRenderHost,
        producer: &crate::host::WgpuRenderProducer,
        world: &mut PresentedWorld,
    ) {
        let handles = host.handles.clone().unwrap();
        let mut frame = RenderFrame {
            index: world.packet.frame_index(),
            state: RenderFrameState::Rendering,
        };
        let mut presented = None;
        host.render_runtime_packet(
            producer,
            &handles,
            crate::frame::WgpuCapturedFrame {
                input: crate::host::tests::test_stock_input(world.packet.clone()),
                app_frame_index: frame.index,
                work_started_at: Instant::now(),
            },
            #[cfg(feature = "stock-ui")]
            None,
            &mut frame,
            &mut FrameStats::default(),
            &mut RenderBackendStatus::default(),
            &mut presented,
        )
        .unwrap();
        assert_eq!(frame.state, RenderFrameState::Submitted);
        world
            .world
            .insert_resource(presented.expect("the Runtime frame must physically present"));
        assert!(world.can_pick());
    }

    fn clear(frame: &mut crate::host::WgpuWindowFrame<'_>) {
        frame.encode(|_, _, encoder, target| {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("runtime-world-replacement-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        });
    }
}
