use super::delegated_window_id;
use nara_window::WindowId;

#[test]
fn secondary_identity_mapping_excludes_any_primary_without_reusing_surfaces() {
    for primary in [0, 1, 2, 17, u64::MAX] {
        let mapped = (1..=32)
            .map(|surface| delegated_window_id(WindowId::new(primary), surface).unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(mapped.len(), 32);
        assert!(!mapped.contains(&WindowId::new(primary)));
    }
    assert!(delegated_window_id(WindowId::PRIMARY, u64::MAX).is_none());
}

#[cfg(target_os = "windows")]
mod bare_primary {
    use std::sync::Arc;

    use nara_render::RenderWindowPacket;
    use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
    use winit::{
        application::ApplicationHandler,
        dpi::LogicalSize,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, EventLoop},
        platform::windows::EventLoopBuilderExtWindows,
        window::{Window, WindowId as NativeWindowId},
    };

    use super::super::WgpuEditorExecution;

    #[test]
    #[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
    fn configured_primary_without_imgui_retires_before_first_present() {
        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        let event_loop = builder.build().unwrap();
        let mut app = BarePrimaryApp { result: None };
        event_loop.run_app(&mut app).unwrap();
        app.result.expect("the primary lifecycle must run").unwrap();
    }

    struct BarePrimaryApp {
        result: Option<Result<(), String>>,
    }

    impl ApplicationHandler for BarePrimaryApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.result.is_some() {
                return;
            }
            self.result = Some((|| {
                let window = Arc::new(
                    event_loop
                        .create_window(
                            Window::default_attributes()
                                .with_title("Nara bare primary retirement")
                                .with_inner_size(LogicalSize::new(320.0, 240.0)),
                        )
                        .map_err(|error| error.to_string())?,
                );
                let mut execution = WgpuEditorExecution::new(
                    WindowId::PRIMARY,
                    WindowHandleProvider::new(Arc::clone(&window)),
                )
                .map_err(|error| error.to_string())?;
                let size = window.inner_size();
                let packet = RenderWindowPacket {
                    id: WindowId::PRIMARY,
                    resolution: WindowResolution::new(size.width, size.height)
                        .with_scale_factor(window.scale_factor()),
                    present_mode: PresentMode::AutoVsync,
                };
                execution
                    .with_device(packet, |scope| {
                        eprintln!(
                            "bare-primary configured: adapter={:?}, format={:?}",
                            scope.adapter().get_info(),
                            scope.surface_format(),
                        );
                    })
                    .map_err(|error| error.to_string())?;
                execution
                    .wait_for_idle()
                    .map_err(|error| error.to_string())?;
                let stats = execution.frame_transaction_stats();
                eprintln!("bare-primary flushed and polled; dropping configured execution");
                drop(execution);
                eprintln!("bare-primary execution retired; dropping native window");
                drop(window);
                eprintln!("bare-primary native window retired");
                if stats.surface_acquire_attempts() != 0 || stats.presents() != 0 {
                    return Err("the bare lifecycle unexpectedly acquired or presented".to_owned());
                }
                Ok(())
            })());
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
}

#[cfg(target_os = "windows")]
mod native {
    use std::{
        any::Any,
        panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
        sync::Arc,
        time::{Duration, Instant},
    };

    use dear_imgui_rs::{Condition, Context, FontSource, Ui};
    use dear_imgui_wgpu::{ExternalTextureId, WgpuInitInfo, WgpuRenderer};
    use dear_imgui_winit::{HiDpiMode, WinitPlatform};
    use nara_asset::{AssetId, Handle};
    use nara_ecs::Entity;
    use nara_render::{
        __private::{
            EditorRenderGeneration, build_editor_render_frame_packet, editor_viewport_image_target,
        },
        Color, Extent2d, ExtractedView, ExtractedViews, RenderFramePacket, RenderImage2d,
        RenderImagePacket, RenderTarget, RenderWindowPacket, ViewportRect,
    };
    use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
    use winit::{
        application::ApplicationHandler,
        dpi::{LogicalPosition, LogicalSize},
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        platform::windows::EventLoopBuilderExtWindows,
        window::{Window, WindowId as NativeWindowId},
    };

    use super::super::{
        WgpuEditorDeviceNamespace, WgpuEditorExecution, WgpuEditorExecutionError,
        WgpuSubmittedImageReceipt, WinitViewportRoute,
    };
    use crate::editor_execution::WgpuEditorViewId;

    #[test]
    #[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
    fn secondary_bridge_rejects_foreign_replay_stale_and_unsampled_images() {
        let mut event_loop_builder = EventLoop::builder();
        event_loop_builder.with_any_thread(true);
        let event_loop = event_loop_builder.build().unwrap();
        let mut app = NativeTestApp {
            deadline: Instant::now() + Duration::from_secs(15),
            fixture: None,
            failure: None,
            finished: false,
        };
        event_loop.run_app(&mut app).unwrap();
        if let Some(failure) = app.failure {
            resume_unwind(failure);
        }
        assert!(app.finished, "the native bridge trajectory must finish");
    }

    struct NativeTestApp {
        deadline: Instant,
        fixture: Option<NativeFixture>,
        failure: Option<Box<dyn Any + Send>>,
        finished: bool,
    }

    impl NativeTestApp {
        fn finish(&mut self, event_loop: &ActiveEventLoop) {
            if let Some(fixture) = self.fixture.take() {
                let shutdown = catch_unwind(AssertUnwindSafe(move || {
                    let mut fixture = fixture;
                    fixture.shutdown()
                }));
                match shutdown {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        self.failure.get_or_insert(Box::new(error));
                    }
                    Err(failure) => {
                        self.failure.get_or_insert(failure);
                    }
                }
            }
            event_loop.exit();
        }
    }

    impl ApplicationHandler for NativeTestApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.fixture.is_some() || self.finished || self.failure.is_some() {
                return;
            }
            match catch_unwind(AssertUnwindSafe(|| NativeFixture::new(event_loop))) {
                Ok(fixture) => self.fixture = Some(fixture),
                Err(failure) => {
                    self.failure = Some(failure);
                    self.finish(event_loop);
                }
            }
        }

        fn window_event(
            &mut self,
            _event_loop: &ActiveEventLoop,
            _window_id: NativeWindowId,
            _event: WindowEvent,
        ) {
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if self.failure.is_some() || self.finished {
                self.finish(event_loop);
                return;
            }
            let progress = catch_unwind(AssertUnwindSafe(|| {
                assert!(
                    Instant::now() < self.deadline,
                    "timed out waiting for actual secondary GPU presentation"
                );
                self.fixture
                    .as_mut()
                    .is_some_and(|fixture| fixture.advance(event_loop))
            }));
            match progress {
                Ok(true) => {
                    self.finished = true;
                    self.finish(event_loop);
                }
                Ok(false) => {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                }
                Err(failure) => {
                    self.failure = Some(failure);
                    self.finish(event_loop);
                }
            }
        }
    }

    struct NativeFixture {
        window: Arc<Window>,
        platform: WinitPlatform,
        route: WinitViewportRoute,
        execution: WgpuEditorExecution,
        namespace: WgpuEditorDeviceNamespace,
        foreign_device: wgpu::Device,
        foreign_queue: wgpu::Queue,
        texture: ExternalTextureId,
        alias: wgpu::TextureView,
        canonical: wgpu::TextureView,
        image: WgpuSubmittedImageReceipt,
        generation: EditorRenderGeneration,
        target: RenderImagePacket,
        frame_index: u64,
        phase: Phase,
        context: Context,
    }

    #[derive(Clone, Copy)]
    enum Phase {
        Exact,
        Alias,
        Overwritten,
    }

    impl NativeFixture {
        fn new(event_loop: &ActiveEventLoop) -> Self {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Nara secondary bridge contract")
                            .with_position(LogicalPosition::new(80.0, 80.0))
                            .with_inner_size(LogicalSize::new(320.0, 240.0)),
                    )
                    .unwrap(),
            );
            let mut context = Context::create();
            context
                .set_ini_filename(None::<std::path::PathBuf>)
                .unwrap();
            context
                .font_atlas()
                .add_font(&[FontSource::default_font_with_size(16.0)]);
            let mut platform = WinitPlatform::new(&mut context).unwrap();
            platform
                .attach_window(Arc::clone(&window), HiDpiMode::Default, &mut context)
                .unwrap();
            context.enable_multi_viewport();
            platform.enable_viewports(&mut context).unwrap();
            let mut execution = WgpuEditorExecution::new(
                WindowId::PRIMARY,
                WindowHandleProvider::new(Arc::clone(&window)),
            )
            .unwrap();
            let packet = window_packet(&window);
            let namespace = execution
                .backend
                .prepare_editor_window(&execution.handles, packet)
                .unwrap();
            let renderer = WgpuRenderer::new(
                WgpuInitInfo::new(
                    namespace.device.clone(),
                    namespace.queue.clone(),
                    namespace.surface_format,
                )
                .with_instance(namespace.instance.clone())
                .with_adapter(namespace.adapter.clone()),
                &mut context,
            )
            .unwrap();
            let route = WinitViewportRoute::attach(&mut context, &platform, renderer).unwrap();
            let (foreign_device, foreign_queue) = pollster::block_on(
                namespace
                    .adapter
                    .request_device(&wgpu::DeviceDescriptor::default()),
            )
            .unwrap();
            let generation = EditorRenderGeneration::try_acquire().unwrap();
            let target = editor_viewport_image_target(
                Handle::<RenderImage2d>::new(AssetId::from_raw(791)),
                Extent2d::new(64, 64).unwrap(),
            )
            .unwrap();
            let image_packet = image_packet(generation, target, 1);
            execution.begin_editor_frame(packet, 1).unwrap();
            let (reservation, _) = execution
                .reserve_viewport_target_with_texture(
                    WgpuEditorViewId::new(1),
                    packet,
                    image_packet.source_generation(),
                    target,
                    |_, _| (),
                )
                .unwrap();
            execution.activate_viewport_target(reservation).unwrap();
            let image = execution
                .submit_image(WgpuEditorViewId::new(1), packet, image_packet)
                .unwrap();
            let image_target = execution.views[&image.view_id()].target.as_ref().unwrap();
            let canonical = image_target.view.clone();
            let alias = image_target
                ._texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            assert_ne!(alias, canonical);
            let texture = route.register_external_texture(&canonical).unwrap();
            Self {
                window,
                context,
                platform,
                route,
                execution,
                namespace,
                foreign_device,
                foreign_queue,
                texture,
                alias,
                canonical,
                image,
                generation,
                target,
                frame_index: 1,
                phase: Phase::Exact,
            }
        }

        fn advance(&mut self, event_loop: &ActiveEventLoop) -> bool {
            self.frame_index += 1;
            let packet = window_packet(&self.window);
            self.execution
                .begin_editor_frame(packet, self.frame_index)
                .unwrap();
            self.platform
                .prepare_frame(&mut self.context, &self.window)
                .unwrap();
            let frame = self.context.begin_frame();
            draw_secondary_image(frame.ui(), self.texture);
            self.platform
                .prepare_render(frame.ui(), &self.window)
                .unwrap();
            let (prepared, outcomes) = self
                .execution
                .prepare_imgui_viewports(packet, &self.route, event_loop, frame, &[self.image])
                .unwrap();
            let report = prepared.secondary_report();
            if report.presented_frames().is_empty() {
                return false;
            }
            assert_eq!(report.presented_frames().len(), 1);
            assert_eq!(outcomes.len(), 1);
            assert!(outcomes[0].window.is_some());
            assert_eq!(
                self.execution
                    .frame_transaction_stats()
                    .surface_acquire_attempts(),
                0
            );
            assert_eq!(self.execution.frame_transaction_stats().presents(), 1);
            let presented = &report.presented_frames()[0];
            assert_ne!(presented.window_id(), self.window.id());
            if !presented.sampled_texture_views().contains(&self.canonical)
                && !presented.sampled_texture_views().contains(&self.alias)
            {
                assert!(outcomes[0].composites.is_empty());
                return false;
            }
            match self.phase {
                Phase::Exact => {
                    assert!(presented.sampled_texture_views().contains(&self.canonical));
                    assert_eq!(outcomes[0].composites.len(), 1);
                    assert_eq!(outcomes[0].composites[0].image(), self.image);
                    let accepted = (
                        self.execution.last_imgui_presentation_id,
                        self.execution.last_presentation_generation,
                        self.execution.frame_transaction_stats(),
                    );
                    let matching_device = self.namespace.device.clone();
                    self.namespace.device = self.foreign_device.clone();
                    assert_toolkit_rejection(
                        self.execution.join_imgui_secondary_presentations(
                            &self.namespace,
                            self.frame_index,
                            report,
                            &[self.image],
                        ),
                        "another GPU device or queue",
                    );
                    self.namespace.device = matching_device;
                    let matching_queue = self.namespace.queue.clone();
                    self.namespace.queue = self.foreign_queue.clone();
                    assert_toolkit_rejection(
                        self.execution.join_imgui_secondary_presentations(
                            &self.namespace,
                            self.frame_index,
                            report,
                            &[self.image],
                        ),
                        "another GPU device or queue",
                    );
                    self.namespace.queue = matching_queue;
                    assert_toolkit_rejection(
                        self.execution.join_imgui_secondary_presentations(
                            &self.namespace,
                            self.frame_index,
                            report,
                            &[self.image],
                        ),
                        "already consumed",
                    );
                    assert_eq!(
                        (
                            self.execution.last_imgui_presentation_id,
                            self.execution.last_presentation_generation,
                            self.execution.frame_transaction_stats(),
                        ),
                        accepted,
                        "rejected reports must not advance presentation authority or counters"
                    );
                    drop(prepared);
                    self.route
                        .update_external_texture(self.texture, &self.alias)
                        .unwrap();
                    self.phase = Phase::Alias;
                }
                Phase::Alias => {
                    assert!(presented.sampled_texture_views().contains(&self.alias));
                    assert!(!presented.sampled_texture_views().contains(&self.canonical));
                    assert!(outcomes[0].composites.is_empty());
                    drop(prepared);
                    let old = self.image;
                    self.image = self
                        .execution
                        .submit_image(
                            old.view_id(),
                            packet,
                            image_packet(self.generation, self.target, 2),
                        )
                        .unwrap();
                    assert_eq!(old.target_generation(), self.image.target_generation());
                    assert_ne!(
                        old.render_transaction_frame_index(),
                        self.image.render_transaction_frame_index()
                    );
                    let accepted = self.execution.last_imgui_presentation_id;
                    self.platform
                        .prepare_frame(&mut self.context, &self.window)
                        .unwrap();
                    let stale_frame = self.context.begin_frame();
                    draw_secondary_image(stale_frame.ui(), self.texture);
                    assert!(matches!(
                        self.execution.prepare_imgui_viewports(
                            packet,
                            &self.route,
                            event_loop,
                            stale_frame,
                            &[old],
                        ),
                        Err(WgpuEditorExecutionError::StaleImageSubmission)
                    ));
                    assert_eq!(self.execution.last_imgui_presentation_id, accepted);
                    let (updated, faults) = self
                        .platform
                        .with_event_loop(event_loop, |_| self.context.update_platform_windows())
                        .into_parts();
                    assert_eq!(updated, Some(()));
                    assert!(faults.is_empty(), "aborted frame update failed: {faults:?}");
                    assert_eq!(self.execution.last_imgui_presentation_id, accepted);
                    self.route
                        .update_external_texture(self.texture, &self.canonical)
                        .unwrap();
                    self.phase = Phase::Overwritten;
                }
                Phase::Overwritten => {
                    assert_eq!(outcomes[0].composites.len(), 1);
                    assert_eq!(outcomes[0].composites[0].image(), self.image);
                    return true;
                }
            }
            false
        }

        fn shutdown(&mut self) -> Result<(), String> {
            self.context.end_frame();
            let idle = self.execution.wait_for_idle();
            let renderer = self.route.shutdown(&mut self.context);
            let viewports = if self.platform.viewports_enabled() {
                self.platform.disable_viewports(&mut self.context)
            } else {
                Ok(())
            };
            let platform = self.platform.shutdown(&mut self.context);
            idle.map_err(|error| error.to_string())
                .and(renderer.map_err(|error| error.to_string()))
                .and(viewports.map_err(|error| error.to_string()))
                .and(platform.map_err(|error| error.to_string()))
        }
    }

    fn assert_toolkit_rejection(
        result: Result<Vec<super::super::WgpuEditorFrameOutcome>, WgpuEditorExecutionError>,
        expected: &str,
    ) {
        assert!(matches!(
            result,
            Err(WgpuEditorExecutionError::ToolkitPaint { message }) if message.contains(expected)
        ));
    }

    fn draw_secondary_image(ui: &Ui, texture: ExternalTextureId) {
        let viewport = ui.main_viewport();
        ui.window("Actual secondary image")
            .position(
                [
                    viewport.pos()[0] + viewport.size()[0] + 40.0,
                    viewport.pos()[1],
                ],
                Condition::Always,
            )
            .size([180.0, 140.0], Condition::Always)
            .build(|| ui.image(texture.texture_id(), [64.0, 64.0]));
    }

    fn window_packet(window: &Window) -> RenderWindowPacket {
        let size = window.inner_size();
        RenderWindowPacket {
            id: WindowId::PRIMARY,
            resolution: WindowResolution::new(size.width, size.height)
                .with_scale_factor(window.scale_factor()),
            present_mode: PresentMode::AutoVsync,
        }
    }

    fn image_packet(
        generation: EditorRenderGeneration,
        target: RenderImagePacket,
        frame_index: u64,
    ) -> RenderFramePacket {
        let viewport = ViewportRect::new(0, 0, 64, 64).unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(target.image()),
            viewport,
            mapping: nara_render::ViewMapping2d::orthographic(
                nara_core::Mat3::IDENTITY,
                64.0,
                viewport,
            )
            .unwrap(),
            order: 0,
            load: nara_render::ViewLoadOp::Clear(Color::BLACK),
        });
        build_editor_render_frame_packet(generation, frame_index, &views, target)
            .unwrap()
            .unwrap()
    }
}
