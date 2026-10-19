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

#[test]
fn owned_renderer_records_exact_external_binding_revisions_and_excludes_clipped_images() {
    use super::{RendererOwner, WgpuEditorDeviceScope};
    use dear_imgui_rs::{Context, FontSource};
    use dear_imgui_wgpu::{FramebufferExtent, WgpuDrawDependencies};

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let force_fallback_adapter = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
        Ok("1") => true,
        Ok("0") | Err(std::env::VarError::NotPresent) => false,
        value => panic!("invalid NARA_WGPU_FORCE_FALLBACK: {value:?}"),
    };
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) if std::env::var("DEAR_IMGUI_REQUIRE_WGPU_ADAPTER").as_deref() != Ok("1") => {
            eprintln!("skipping renderer binding test without a WGPU adapter: {error}");
            return;
        }
        Err(error) => panic!("the required WGPU adapter is unavailable: {error}"),
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut context = Context::create();
    context
        .set_ini_filename(None::<std::path::PathBuf>)
        .unwrap();
    context
        .font_atlas()
        .add_font(&[FontSource::default_font_with_size(16.0)]);
    context.io_mut().set_display_size([128.0, 128.0]);
    let scope = WgpuEditorDeviceScope {
        instance: &instance,
        adapter: &adapter,
        device: &device,
        queue: &queue,
        surface_format: wgpu::TextureFormat::Rgba8Unorm,
        surface_extent: nara_render::Extent2d::new(128, 128).unwrap(),
        backend_instance_id: 1,
        device_epoch: 1,
    };
    let mut renderer = scope.create_imgui_renderer(&mut context, None).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("nara_imgui_binding_test"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let canonical = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let alias = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("nara_imgui_binding_output"),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        ..wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::empty(),
            view_formats: &[],
        }
    });
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let id = renderer.register_external_texture(&canonical).unwrap();
    let first_binding = renderer.external_binding(id).unwrap();
    let render = |context: &mut Context,
                  renderer: &mut super::WgpuEditorImguiRenderer,
                  clipped: bool|
     -> WgpuDrawDependencies {
        let frame = context.begin_frame();
        {
            let draw_list = frame.ui().get_foreground_draw_list();
            let draw_image = || {
                draw_list.add_image(
                    id.texture_id(),
                    [8.0, 8.0],
                    [40.0, 40.0],
                    [0.0, 0.0],
                    [1.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0],
                );
            };
            if clipped {
                draw_list.with_clip_rect([160.0, 160.0], [180.0, 180.0], draw_image);
            } else {
                draw_image();
            }
        }
        let RendererOwner::Docked(inner) = &mut renderer.owner else {
            unreachable!()
        };
        let consumer = inner.renderer_consumer().unwrap();
        let pending = frame.try_render(consumer).unwrap();
        let prepared = inner.reconcile_frame(pending).unwrap();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let recorded = {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            inner
                .render_reconciled(prepared, &mut pass, FramebufferExtent::new(128, 128))
                .unwrap()
        };
        queue.submit([encoder.finish()]);
        recorded.dependencies
    };
    let initial = render(&mut context, &mut renderer, false);
    assert_eq!(initial.external_bindings, vec![first_binding]);
    assert!(renderer.samples(&initial, &canonical));
    assert!(!renderer.samples(&initial, &alias));
    renderer.update_external_texture(id, &alias).unwrap();
    let replacement = renderer.external_binding(id).unwrap();
    assert_eq!(replacement.texture, first_binding.texture);
    assert!(replacement.revision > first_binding.revision);
    assert!(!renderer.samples(&initial, &canonical));
    assert!(
        !renderer.samples(&initial, &alias),
        "old mapping evidence cannot certify a replacement"
    );
    let updated = render(&mut context, &mut renderer, false);
    assert_eq!(updated.external_bindings, vec![replacement]);
    assert!(renderer.samples(&updated, &alias));
    assert!(!renderer.samples(&updated, &canonical));
    let clipped = render(&mut context, &mut renderer, true);
    assert!(clipped.external_bindings.is_empty());
    assert!(!renderer.samples(&clipped, &alias));
    renderer.unregister_external_texture(id).unwrap();
    assert!(!renderer.samples(&updated, &alias));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    renderer.shutdown(&mut context).unwrap();
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
        let mut app = BarePrimaryApp {
            result: None,
            #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
            instance_budget_probe: false,
        };
        event_loop.run_app(&mut app).unwrap();
        app.result.expect("the primary lifecycle must run").unwrap();
    }

    #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[test]
    #[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
    fn editor_targets_share_submitted_scope_budget_and_recover_next_frame() {
        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        let event_loop = builder.build().unwrap();
        let mut app = BarePrimaryApp {
            result: None,
            instance_budget_probe: true,
        };
        event_loop.run_app(&mut app).unwrap();
        app.result
            .expect("the Editor budget trajectory must run")
            .unwrap();
    }

    struct BarePrimaryApp {
        result: Option<Result<(), String>>,
        #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
        instance_budget_probe: bool,
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
                #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
                if self.instance_budget_probe {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        verify_instance_budget(&mut execution, packet)
                    }))
                    .map_err(|_| "the Editor instance budget assertions failed".to_owned())??;
                }
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

    #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn verify_instance_budget(
        execution: &mut WgpuEditorExecution,
        window: RenderWindowPacket,
    ) -> Result<(), String> {
        use crate::editor_execution::{WgpuEditorExecutionError, WgpuEditorViewId};
        use nara_ecs::Entity;
        use nara_render::{
            __private::{
                EditorRenderGeneration, build_editor_render_frame_packet,
                editor_viewport_image_target,
            },
            Color, Extent2d, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget,
            ViewportRect,
        };

        let generation = EditorRenderGeneration::try_acquire().unwrap();
        let instance_bytes = std::mem::size_of::<crate::quad::WgpuQuadInstance>() as u64;
        let mut targets = Vec::new();
        execution
            .begin_editor_frame(window, 1)
            .map_err(|error| error.to_string())?;
        execution.backend.set_instance_budget_limit(instance_bytes);
        for index in 1..=2 {
            let image = RenderImage2d::allocate_handle().unwrap();
            let target =
                editor_viewport_image_target(image, Extent2d::new(64, 64).unwrap()).unwrap();
            let viewport = ViewportRect::new(0, 0, 64, 64).unwrap();
            let mut views = ExtractedViews::default();
            views.push(ExtractedView {
                camera_entity: Entity::PLACEHOLDER,
                target: RenderTarget::Image(image),
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
            let packet = build_editor_render_frame_packet(generation, 1, &views, target)
                .unwrap()
                .unwrap();
            let view_id = WgpuEditorViewId::new(index);
            let (reservation, ()) = execution
                .reserve_viewport_target_with_texture(
                    view_id,
                    window,
                    packet.source_generation(),
                    target,
                    |_, _| (),
                )
                .map_err(|error| error.to_string())?;
            execution
                .activate_viewport_target(reservation)
                .map_err(|error| error.to_string())?;
            targets.push((view_id, target, views, packet));
        }

        let (first_view, first_target, first_views, first_packet) = targets.remove(0);
        let first = execution
            .submit_authoring_ui_image(
                first_view,
                window,
                first_packet,
                crate::view_probe::overlay_packet(RenderTarget::Image(first_target.image())),
            )
            .map_err(|error| error.to_string())?;
        assert_eq!(
            execution
                .frame_transaction_stats()
                .candidate_instance_buffer_demand_bytes(),
            0
        );
        assert_eq!(
            execution
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            instance_bytes
        );
        let (second_view, second_target, second_views, second_packet) = targets.remove(0);
        let error = execution
            .submit_authoring_ui_image(
                second_view,
                window,
                second_packet,
                crate::view_probe::overlay_packet(RenderTarget::Image(second_target.image())),
            )
            .expect_err("a second target must share the first submitted target's demand");
        assert!(matches!(
            error,
            WgpuEditorExecutionError::Backend(crate::WgpuRenderError::ContentRejected { .. })
        ));
        assert_eq!(execution.frame_transaction_stats().queue_submissions(), 1);
        assert_eq!(
            execution
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            instance_bytes
        );
        assert_eq!(
            execution.views[&first_view]
                .target
                .as_ref()
                .unwrap()
                .last_submitted_frame_index,
            Some(first.render_transaction_frame_index())
        );

        execution
            .wait_for_idle()
            .map_err(|error| error.to_string())?;
        assert_eq!(
            execution
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            instance_bytes,
            "GPU completion cannot refund this frame's submitted demand"
        );
        execution
            .begin_editor_frame(window, 2)
            .map_err(|error| error.to_string())?;
        execution
            .backend
            .set_instance_budget_limit(2 * instance_bytes);
        assert_eq!(
            execution
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            0
        );
        let next_first_packet =
            build_editor_render_frame_packet(generation, 2, &first_views, first_target)
                .unwrap()
                .unwrap();
        execution
            .submit_authoring_ui_image(
                first_view,
                window,
                next_first_packet,
                crate::view_probe::overlay_packet(RenderTarget::Image(first_target.image())),
            )
            .map_err(|error| error.to_string())?;
        let next_packet =
            build_editor_render_frame_packet(generation, 2, &second_views, second_target)
                .unwrap()
                .unwrap();
        execution
            .submit_authoring_ui_image(
                second_view,
                window,
                next_packet,
                crate::view_probe::overlay_packet(RenderTarget::Image(second_target.image())),
            )
            .map_err(|error| error.to_string())?;
        assert_eq!(execution.frame_transaction_stats().queue_submissions(), 2);
        assert_eq!(
            execution
                .frame_transaction_stats()
                .candidate_instance_buffer_demand_bytes(),
            0
        );
        assert_eq!(
            execution
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            2 * instance_bytes
        );
        Ok(())
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
    use dear_imgui_wgpu::{ExternalTextureId, WgpuFrameStage};
    use dear_imgui_winit::{HiDpiMode, WinitPlatform};
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
        PreparedFrame, WgpuEditorExecution, WgpuEditorExecutionError, WgpuEditorImguiRenderer,
        WgpuSubmittedImageReceipt,
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
        renderer: WgpuEditorImguiRenderer,
        execution: WgpuEditorExecution,
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
            let mut renderer = execution
                .with_device(packet, |scope| {
                    scope
                        .create_imgui_renderer(&mut context, Some(&platform))
                        .unwrap()
                })
                .unwrap();
            let (foreign_device, foreign_queue) = pollster::block_on(
                namespace
                    .adapter
                    .request_device(&wgpu::DeviceDescriptor::default()),
            )
            .unwrap();
            let generation = EditorRenderGeneration::try_acquire().unwrap();
            let target = editor_viewport_image_target(
                RenderImage2d::allocate_handle().unwrap(),
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
            let texture = renderer.register_external_texture(&canonical).unwrap();
            Self {
                window,
                context,
                platform,
                renderer,
                execution,
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
            let mut outcomes = Vec::new();
            let prepared = self
                .execution
                .prepare_imgui_frame(
                    packet,
                    &mut self.renderer,
                    event_loop,
                    frame,
                    &[self.image],
                    &mut outcomes,
                )
                .unwrap();
            let report = match &prepared.0 {
                PreparedFrame::Native(main) => main.secondary_report().clone(),
                PreparedFrame::Docked(_) => panic!("the fixture must own a native route"),
            };
            let presented = report
                .observations()
                .iter()
                .filter(|observed| observed.stage == WgpuFrameStage::PresentationRequested)
                .collect::<Vec<_>>();
            if presented.is_empty() {
                return false;
            }
            assert_eq!(presented.len(), 1);
            assert_eq!(outcomes.len(), 1);
            let window = outcomes[0].window().unwrap();
            assert_eq!(window.surface_generation(), None);
            assert_eq!(
                window.delegated_native_instance(),
                Some(presented[0].target.instance_id.get().get())
            );
            let snapshot = self
                .platform
                .window_snapshots()
                .unwrap()
                .into_iter()
                .find(|snapshot| snapshot.target.instance_id == presented[0].target.instance_id)
                .unwrap();
            assert_ne!(snapshot.window_id, self.window.id());
            assert_eq!(
                self.execution
                    .imgui_window_id(snapshot.target.instance_id)
                    .unwrap(),
                window.window_id()
            );
            assert_eq!(
                self.execution
                    .frame_transaction_stats()
                    .surface_acquire_attempts(),
                0
            );
            assert_eq!(self.execution.frame_transaction_stats().presents(), 1);
            let dependencies = &presented[0].dependencies;
            if !self.renderer.samples(dependencies, &self.canonical)
                && !self.renderer.samples(dependencies, &self.alias)
            {
                assert!(outcomes[0].composites.is_empty());
                return false;
            }
            match self.phase {
                Phase::Exact => {
                    assert!(self.renderer.samples(dependencies, &self.canonical));
                    assert_eq!(outcomes[0].composites.len(), 1);
                    assert_eq!(outcomes[0].composites[0].image(), self.image);
                    let accepted = (
                        self.execution.last_presentation_generation,
                        self.execution.frame_transaction_stats(),
                    );
                    let mut replayed = Vec::new();
                    assert_toolkit_rejection(
                        self.execution.join_imgui_secondary_presentations(
                            &mut self.renderer,
                            self.frame_index,
                            report,
                            &[self.image],
                            &mut replayed,
                        ),
                        "already consumed",
                    );
                    assert!(replayed.is_empty());
                    assert_eq!(
                        (
                            self.execution.last_presentation_generation,
                            self.execution.frame_transaction_stats()
                        ),
                        accepted
                    );
                    drop(prepared);
                    // Reject an actual foreign device/queue before any toolkit frame is rendered.
                    for foreign_queue in [false, true] {
                        self.platform
                            .prepare_frame(&mut self.context, &self.window)
                            .unwrap();
                        let frame = self.context.begin_frame();
                        let device = self.renderer.device.clone();
                        let queue = self.renderer.queue.clone();
                        if foreign_queue {
                            self.renderer.queue = self.foreign_queue.clone();
                        } else {
                            self.renderer.device = self.foreign_device.clone();
                        }
                        let mut rejected = Vec::new();
                        let result = self.execution.prepare_imgui_frame(
                            packet,
                            &mut self.renderer,
                            event_loop,
                            frame,
                            &[self.image],
                            &mut rejected,
                        );
                        assert!(
                            matches!(&result, Err(WgpuEditorExecutionError::ToolkitPaint { message })
                            if message.contains("another GPU device or queue"))
                        );
                        drop(result);
                        assert!(rejected.is_empty());
                        self.renderer.device = device;
                        self.renderer.queue = queue;
                        self.complete_rejected_frame(event_loop);
                    }
                    self.renderer
                        .update_external_texture(self.texture, &self.alias)
                        .unwrap();
                    self.phase = Phase::Alias;
                }
                Phase::Alias => {
                    assert!(self.renderer.samples(dependencies, &self.alias));
                    assert!(!self.renderer.samples(dependencies, &self.canonical));
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
                    self.platform
                        .prepare_frame(&mut self.context, &self.window)
                        .unwrap();
                    let frame = self.context.begin_frame();
                    let mut rejected = Vec::new();
                    assert!(matches!(
                        self.execution.prepare_imgui_frame(
                            packet,
                            &mut self.renderer,
                            event_loop,
                            frame,
                            &[old],
                            &mut rejected
                        ),
                        Err(WgpuEditorExecutionError::StaleImageSubmission)
                    ));
                    assert!(rejected.is_empty());
                    self.complete_rejected_frame(event_loop);
                    self.renderer
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

        /// A rejected renderer admission consumes no route preparation, so the native platform
        /// watermark still needs the normal update step before another multi-viewport frame.
        fn complete_rejected_frame(&mut self, event_loop: &ActiveEventLoop) {
            self.context.end_frame();
            let (updated, faults) = self
                .platform
                .with_event_loop(event_loop, |_| self.context.update_platform_windows())
                .into_parts();
            assert_eq!(updated, Some(()));
            assert!(
                faults.is_empty(),
                "rejected frame platform update failed: {faults:?}"
            );
        }

        fn shutdown(&mut self) -> Result<(), String> {
            self.context.end_frame();
            let idle = self.execution.wait_for_idle();
            let renderer = self.renderer.shutdown(&mut self.context);
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

    fn assert_toolkit_rejection(result: Result<(), WgpuEditorExecutionError>, expected: &str) {
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
