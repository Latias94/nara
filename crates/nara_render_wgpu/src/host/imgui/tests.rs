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

struct OffscreenGpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl OffscreenGpu {
    fn new() -> Option<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let force_fallback_adapter = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
            Ok("1") => true,
            Ok("0") | Err(std::env::VarError::NotPresent) => false,
            value => panic!("invalid NARA_WGPU_FORCE_FALLBACK: {value:?}"),
        };
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter,
                ..Default::default()
            })) {
                Ok(adapter) => adapter,
                Err(error)
                    if std::env::var("DEAR_IMGUI_REQUIRE_WGPU_ADAPTER").as_deref() != Ok("1") =>
                {
                    eprintln!("skipping renderer lifetime test without a WGPU adapter: {error}");
                    return None;
                }
                Err(error) => panic!("the required WGPU adapter is unavailable: {error}"),
            };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        Some(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    fn scope(&self) -> super::WgpuWindowDeviceScope<'_> {
        super::WgpuWindowDeviceScope {
            instance: &self.instance,
            adapter: &self.adapter,
            device: &self.device,
            queue: &self.queue,
            surface_format: wgpu::TextureFormat::Rgba8Unorm,
            surface_extent: nara_render::Extent2d::new(128, 128).unwrap(),
            backend_instance_id: 1,
            device_epoch: 1,
        }
    }
}

fn test_image_output(
    texture: wgpu::Texture,
    backend_instance_id: u64,
    device_epoch: u64,
) -> crate::output::WgpuImageOutput {
    let target = nara_render::__private::editor_viewport_image_target(
        nara_render::RenderImage2d::allocate_handle().unwrap(),
        nara_render::Extent2d::new(texture.width(), texture.height()).unwrap(),
    )
    .unwrap();
    let receipt = crate::presented::WgpuSubmittedImageReceipt::new(
        crate::host::WgpuRenderProducerId::new(1),
        target,
        1,
        nara_render::RenderFrameSourceGeneration::try_acquire().unwrap(),
        1,
        backend_instance_id,
        device_epoch,
        1,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let descriptor = crate::WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("test_retained_image"),
            size: texture.size(),
            mip_level_count: texture.mip_level_count(),
            sample_count: texture.sample_count(),
            dimension: texture.dimension(),
            format: texture.format(),
            usage: texture.usage(),
            view_formats: &[],
        },
        transfer: if texture.format().has_srgb_suffix() {
            crate::TextureTransfer::Srgb
        } else {
            crate::TextureTransfer::Linear
        },
        alpha: crate::TextureAlpha::Premultiplied,
    };
    crate::output::WgpuImageOutput(std::sync::Arc::new(crate::output::WgpuImageStorage {
        receipt,
        resource: std::sync::Arc::new(crate::program::ProgramResourceStorage {
            backend_instance_id,
            device_epoch,
            epoch_lost: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            producer: receipt.producer_id(),
            source: receipt.source_generation(),
            frame_index: receipt.render_transaction_frame_index(),
            descriptor: descriptor.resource::<crate::WgpuWorldColor>(),
            _reservation: std::sync::Arc::new(
                std::sync::Arc::new(crate::output::OutputBudget::default())
                    .reserve(u64::from(texture.width()) * u64::from(texture.height()) * 4)
                    .unwrap(),
            ),
            native: crate::program::NativeResource::Texture { texture, view },
        }),
    }))
}

fn drain_gpu(retirement: &mut crate::retirement::WgpuGpuRetirement) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !retirement.poll().unwrap() {
        assert!(
            std::time::Instant::now() < deadline,
            "GPU retirement did not complete"
        );
        std::thread::yield_now();
    }
    assert!(retirement.poll().unwrap(), "completion must remain stable");
}

fn lifetime_context() -> dear_imgui_rs::Context {
    let mut context = dear_imgui_rs::Context::create();
    context
        .set_ini_filename(None::<std::path::PathBuf>)
        .unwrap();
    context
        .font_atlas()
        .add_font(&[dear_imgui_rs::FontSource::default_font_with_size(16.0)]);
    context.io_mut().set_display_size([128.0, 128.0]);
    context
}

#[test]
fn gpu_retirement_flushes_pending_initializer_transfers_before_renderer_shutdown() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut context = lifetime_context();
    let mut retirement = gpu.scope().gpu_retirement();
    let mut renderer = gpu
        .scope()
        .create_imgui_renderer(&mut context, None)
        .unwrap();
    // The SDK constructor queued its fallback-texture upload without submitting. This sentinel
    // shares that pending-write batch and lets the test read back an actual transferred payload.
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("nara_imgui_initializer_retirement"),
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    gpu.queue.write_buffer(&readback, 0, &[19, 37, 73, 127]);
    assert!(
        gpu.device
            .poll(wgpu::PollType::Poll)
            .unwrap()
            .is_queue_empty()
    );
    drain_gpu(&mut retirement);
    // No additional queue submission is needed to observe the completed initialization batch.
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap();
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        gpu.device.poll(wgpu::PollType::Poll).unwrap();
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "readback did not complete"
        );
        std::thread::yield_now();
    }
    assert_eq!(&*readback.get_mapped_range(..).unwrap(), &[19, 37, 73, 127]);
    readback.unmap();
    renderer.shutdown(&mut context).unwrap();
}

#[test]
fn renderer_shutdown_preserves_rejected_state_and_releases_bindings_idempotently() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut context = lifetime_context();
    let mut retirement = gpu.scope().gpu_retirement();
    let mut renderer = gpu
        .scope()
        .create_imgui_renderer(&mut context, None)
        .unwrap();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("nara_imgui_shutdown_binding"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let display = crate::output::WgpuImageDisplay::issue(test_image_output(texture, 1, 1));
    let view = display.output().0.view().clone();
    let id = renderer.register_image(&display).unwrap();
    let binding = renderer.external_binding(id).unwrap();
    let observed_scope = dear_imgui_wgpu::WgpuFrameScope {
        context_id: context.id(),
        epoch: None,
        frame_count: 0,
    };
    renderer.last_secondary_scope = Some(observed_scope);
    let suspended = context.suspend_or_panic();
    let mut foreign = dear_imgui_rs::Context::create();
    assert!(matches!(
        renderer.shutdown(&mut foreign),
        Err(super::WgpuRenderHostError::WindowPaint { .. })
    ));
    assert!(!renderer.closed);
    assert_eq!(renderer.bindings, vec![(binding, display.clone())]);
    assert_eq!(renderer.last_secondary_scope, Some(observed_scope));
    drop(foreign);
    let mut context = suspended.activate().unwrap();
    // A real synchronous epoch retains the Context borrow until drop. Unlike detached snapshots,
    // safe SDK callers cannot pass that Context to shutdown while the epoch is outstanding.
    let super::RendererOwner::Docked(inner) = &renderer.owner else {
        unreachable!()
    };
    let pending = context
        .begin_frame()
        .try_render(inner.renderer_consumer().unwrap())
        .unwrap();
    drop(pending);
    drain_gpu(&mut retirement);
    renderer.shutdown(&mut context).unwrap();
    assert!(renderer.closed);
    assert!(renderer.bindings.is_empty());
    assert!(renderer.last_secondary_scope.is_none());
    let suspended = context.suspend_or_panic();
    let mut foreign = dear_imgui_rs::Context::create();
    assert!(matches!(
        renderer.shutdown(&mut foreign),
        Err(super::WgpuRenderHostError::WindowPaint { message })
            if message == "the renderer belongs to another Context"
    ));
    drop(foreign);
    let mut context = suspended.activate().unwrap();
    for result in [
        renderer.register_image(&display).map(|_| ()),
        renderer.unregister_external_texture(id),
    ] {
        assert!(matches!(
            result,
            Err(super::WgpuRenderHostError::WindowPaint { message })
                if message == "the renderer is closed"
        ));
    }
    // A second shutdown must not re-enter the detached SDK owner or touch its successor.
    let mut successor = gpu
        .scope()
        .create_imgui_renderer(&mut context, None)
        .unwrap();
    let successor_data = context.io().backend_renderer_user_data();
    assert!(!successor_data.is_null());
    renderer.shutdown(&mut context).unwrap();
    assert_eq!(context.io().backend_renderer_user_data(), successor_data);
    let super::RendererOwner::Docked(inner) = &mut successor.owner else {
        unreachable!()
    };
    let pending = context
        .begin_frame()
        .try_render(inner.renderer_consumer().unwrap())
        .unwrap();
    let prepared = inner.reconcile_frame(pending).unwrap();
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut leading_command_buffers = Vec::new();
    let mut sampled_viewports = Vec::new();
    let retention = crate::submission::WgpuSubmissionReservation::new(&gpu.device, &gpu.queue, ());
    let submission_prefix = retention.observe();
    let mut frame = super::WgpuWindowFrame {
        device: &gpu.device,
        queue: &gpu.queue,
        encoder: &mut encoder,
        target_view: &view,
        surface_format: wgpu::TextureFormat::Rgba8Unorm,
        surface_extent: nara_render::Extent2d::new(1, 1).unwrap(),
        viewports: &[],
        leading_command_buffers: &mut leading_command_buffers,
        sampled_viewports: &mut sampled_viewports,
        submission_prefix: &submission_prefix,
    };
    assert!(matches!(
        renderer.paint(
            super::WgpuEditorImguiFrame(super::PreparedFrame::Docked(prepared)),
            &mut frame,
            wgpu::Color::BLACK,
        ),
        Err(super::WgpuRenderHostError::WindowPaint { message })
            if message == "the renderer is closed"
    ));
    let _prefix = retention.bind_current_prefix();
    let mut successor_retirement = gpu.scope().gpu_retirement();
    drain_gpu(&mut successor_retirement);
    successor.shutdown(&mut context).unwrap();
}

#[test]
fn failed_native_attach_returns_live_docked_renderer_custody() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut context = lifetime_context();
    let mut platform = dear_imgui_winit::WinitPlatform::new(&mut context).unwrap();
    let mut retirement = gpu.scope().gpu_retirement();
    // No native runtime was enabled: public SDK attach fails transactionally before OS work.
    let failure = gpu
        .scope()
        .create_imgui_renderer(&mut context, Some(&platform))
        .err()
        .expect("an inactive native platform cannot accept a renderer route");
    let message = failure.error().to_string();
    assert_eq!(failure.to_string(), message);
    assert!(format!("{failure:?}").contains("retains_renderer: true"));
    let (error, renderer) = failure.into_parts();
    assert_eq!(error.to_string(), message);
    let mut renderer = renderer.expect("attach failure must return the original SDK renderer");
    assert!(matches!(&renderer.owner, super::RendererOwner::Docked(_)));
    assert!(!renderer.closed);
    assert!(!context.io().backend_renderer_user_data().is_null());
    let super::RendererOwner::Docked(inner) = &renderer.owner else {
        unreachable!()
    };
    assert_eq!(
        inner.renderer_consumer().unwrap().context_id(),
        context.id()
    );
    drain_gpu(&mut retirement);
    renderer.shutdown(&mut context).unwrap();
    platform.shutdown(&mut context).unwrap();
}

#[test]
fn gpu_retirement_outlives_scope_when_sdk_constructor_returns_no_renderer() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut context = lifetime_context();
    let mut scope = gpu.scope();
    let mut retirement = scope.gpu_retirement();
    scope.surface_format = wgpu::TextureFormat::Rgba8Uint;
    let failure = scope
        .create_imgui_renderer(&mut context, None)
        .err()
        .expect("the SDK must reject an unblendable render target");
    let (_, renderer) = failure.into_parts();
    assert!(renderer.is_none());
    assert!(context.io().backend_renderer_user_data().is_null());
    drop(gpu);
    drain_gpu(&mut retirement);
}

#[test]
fn gpu_retirement_discard_destroys_device_but_still_drains_accepted_submissions() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut context = lifetime_context();
    let mut retirement = gpu.scope().gpu_retirement();
    let mut renderer = gpu
        .scope()
        .create_imgui_renderer(&mut context, None)
        .unwrap();
    gpu.queue.submit([]);
    let (sender, receiver) = std::sync::mpsc::channel();
    gpu.queue
        .on_submitted_work_done(move || sender.send(()).unwrap());
    retirement.begin(true);
    retirement.begin(false);
    drain_gpu(&mut retirement);
    receiver
        .try_recv()
        .expect("accepted work must finish before retirement");
    renderer.shutdown(&mut context).unwrap();
}

#[test]
fn gpu_retirement_flush_tolerates_device_loss_before_submission() {
    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut context = lifetime_context();
    let mut retirement = gpu.scope().gpu_retirement();
    let mut renderer = gpu
        .scope()
        .create_imgui_renderer(&mut context, None)
        .unwrap();
    retirement.begin(false);
    // Deterministically exercise loss between branch selection and the flush attempt. WGPU
    // reports this rejected submission through loss handling, not its fatal validation handler.
    gpu.device.destroy();
    drain_gpu(&mut retirement);
    renderer.shutdown(&mut context).unwrap();
}

#[test]
fn owned_renderer_records_exact_external_binding_revisions_and_excludes_clipped_images() {
    use super::{RendererOwner, WgpuWindowDeviceScope};
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
    let scope = WgpuWindowDeviceScope {
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
    for (backend_instance_id, device_epoch) in [(2, 1), (1, 2)] {
        let foreign = crate::output::WgpuImageDisplay::issue(test_image_output(
            texture.clone(),
            backend_instance_id,
            device_epoch,
        ));
        assert!(matches!(
            renderer.register_image(&foreign),
            Err(super::WgpuRenderHostError::StaleImageSubmission)
        ));
        assert!(renderer.bindings.is_empty());
    }
    let image = test_image_output(texture, 1, 1);
    let designated = crate::output::WgpuImageDisplay::issue(image.clone());
    let mirror = crate::output::WgpuImageDisplay::issue(image.clone());
    let unregistered = crate::output::WgpuImageDisplay::issue(image.clone());
    assert_eq!(designated.output(), mirror.output());
    assert_ne!(designated.occurrence(), mirror.occurrence());
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
    let id = renderer.register_image(&designated).unwrap();
    let mirror_id = renderer.register_image(&mirror).unwrap();
    assert_ne!(id, mirror_id);
    let first_binding = renderer.external_binding(id).unwrap();
    let mirror_binding = renderer.external_binding(mirror_id).unwrap();
    let render = |context: &mut Context,
                  renderer: &mut super::WgpuEditorImguiRenderer,
                  images: &[(dear_imgui_wgpu::ExternalTextureId, bool)]|
     -> WgpuDrawDependencies {
        let frame = context.begin_frame();
        {
            let draw_list = frame.ui().get_foreground_draw_list();
            for &(texture, clipped) in images {
                let draw_image = || {
                    draw_list.add_image(
                        texture.texture_id(),
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
    let initial = render(&mut context, &mut renderer, &[(id, false)]);
    assert_eq!(initial.external_bindings, vec![first_binding]);
    assert!(renderer.samples(&initial, &designated.clone()));
    assert!(!renderer.samples(&initial, &mirror));
    assert!(!renderer.samples(&initial, &unregistered));
    let both = render(
        &mut context,
        &mut renderer,
        &[(id, false), (mirror_id, false)],
    );
    assert_eq!(both.external_bindings.len(), 2);
    assert!(both.external_bindings.contains(&first_binding));
    assert!(both.external_bindings.contains(&mirror_binding));
    assert!(renderer.samples(&both, &designated));
    assert!(renderer.samples(&both, &mirror));
    let mirror_only = render(
        &mut context,
        &mut renderer,
        &[(id, true), (mirror_id, false)],
    );
    assert_eq!(mirror_only.external_bindings, vec![mirror_binding]);
    assert!(!renderer.samples(&mirror_only, &designated));
    assert!(renderer.samples(&mirror_only, &mirror));
    // Rebinding to another occurrence of the same pixels still advances the SDK revision.
    renderer.update_image(id, &mirror).unwrap();
    let replacement = renderer.external_binding(id).unwrap();
    assert_eq!(replacement.texture, first_binding.texture);
    assert!(replacement.revision > first_binding.revision);
    assert!(!renderer.samples(&initial, &designated));
    assert!(
        !renderer.samples(&initial, &mirror),
        "old mapping evidence cannot certify a replacement"
    );
    let updated = render(&mut context, &mut renderer, &[(id, false)]);
    assert_eq!(updated.external_bindings, vec![replacement]);
    assert!(renderer.samples(&updated, &mirror));
    assert!(!renderer.samples(&updated, &designated));
    let mismatched_output = crate::output::WgpuImageDisplay {
        output: crate::output::WgpuImageOutput(std::sync::Arc::new(
            crate::output::WgpuImageStorage {
                receipt: test_image_output(image.0.texture().clone(), 1, 1).receipt(),
                resource: std::sync::Arc::clone(&image.0.resource),
            },
        )),
        occurrence: mirror.occurrence(),
    };
    assert!(!renderer.samples(&updated, &mismatched_output));
    let clipped = render(&mut context, &mut renderer, &[(id, true)]);
    assert!(clipped.external_bindings.is_empty());
    assert!(!renderer.samples(&clipped, &mirror));
    renderer.unregister_external_texture(id).unwrap();
    assert!(!renderer.samples(&updated, &mirror));
    assert!(renderer.samples(&mirror_only, &mirror));
    renderer.unregister_external_texture(mirror_id).unwrap();
    assert!(!renderer.samples(&mirror_only, &mirror));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    renderer.shutdown(&mut context).unwrap();
}

#[test]
fn owned_renderer_paint_distinguishes_same_window_occurrences_and_retains_registered_outputs() {
    use crate::output::WgpuImageDisplay;

    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut context = lifetime_context();
    let mut renderer = gpu
        .scope()
        .create_imgui_renderer(&mut context, None)
        .unwrap();
    let descriptor = wgpu::TextureDescriptor {
        label: Some("nara_imgui_occurrence_source"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    let image = test_image_output(gpu.device.create_texture(&descriptor), 1, 1);
    let retained = std::sync::Arc::downgrade(&image.0);
    let designated = WgpuImageDisplay::issue(image.clone());
    let mirror = WgpuImageDisplay::issue(image.clone());
    let unregistered = WgpuImageDisplay::issue(image.clone());
    let designated_texture = renderer.register_image(&designated).unwrap();
    let mirror_texture = renderer.register_image(&mirror).unwrap();
    assert_ne!(designated_texture, mirror_texture);
    let viewports = [designated.clone(), mirror.clone(), unregistered.clone()];
    let destination = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("nara_imgui_occurrence_destination"),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        ..descriptor
    });
    let destination_view = destination.create_view(&wgpu::TextureViewDescriptor::default());
    let mut presentation_generation = 0;
    for (show_designated, show_mirror, expected) in [
        (false, true, vec![mirror.occurrence()]),
        (
            true,
            true,
            vec![designated.occurrence(), mirror.occurrence()],
        ),
        (true, false, vec![designated.occurrence()]),
        (false, false, Vec::new()),
    ] {
        let frame = context.begin_frame();
        {
            let draw_list = frame.ui().get_foreground_draw_list();
            for (texture, visible, x) in [
                (designated_texture, show_designated, 8.0),
                (mirror_texture, show_mirror, 56.0),
            ] {
                let draw_image = || {
                    draw_list.add_image(
                        texture.texture_id(),
                        [x, 8.0],
                        [x + 32.0, 40.0],
                        [0.0, 0.0],
                        [1.0, 1.0],
                        [1.0, 1.0, 1.0, 1.0],
                    );
                };
                if visible {
                    draw_image();
                } else {
                    draw_list.with_clip_rect([160.0, 160.0], [180.0, 180.0], draw_image);
                }
            }
        }
        let super::RendererOwner::Docked(inner) = &mut renderer.owner else {
            unreachable!()
        };
        let pending = frame
            .try_render(inner.renderer_consumer().unwrap())
            .unwrap();
        let prepared = inner.reconcile_frame(pending).unwrap();
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let mut leading_command_buffers = Vec::new();
        let mut samples = Vec::with_capacity(viewports.len());
        let retention =
            crate::submission::WgpuSubmissionReservation::new(&gpu.device, &gpu.queue, ());
        let submission_prefix = retention.observe();
        renderer
            .paint(
                super::WgpuEditorImguiFrame(super::PreparedFrame::Docked(prepared)),
                &mut super::WgpuWindowFrame {
                    device: &gpu.device,
                    queue: &gpu.queue,
                    encoder: &mut encoder,
                    target_view: &destination_view,
                    surface_format: wgpu::TextureFormat::Rgba8Unorm,
                    surface_extent: nara_render::Extent2d::new(128, 128).unwrap(),
                    viewports: &viewports,
                    leading_command_buffers: &mut leading_command_buffers,
                    sampled_viewports: &mut samples,
                    submission_prefix: &submission_prefix,
                },
                wgpu::Color::BLACK,
            )
            .unwrap();
        let _prefix = retention.submitted(gpu.queue.submit([encoder.finish()]));
        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.occurrence)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(samples.iter().all(|sample| sample.image == image.receipt()));
        let composites = super::join_composite_receipts(
            &mut presentation_generation,
            &viewports,
            &samples,
            super::WgpuWindowPresentedReceipt::new(WindowId::PRIMARY, 1, 1, 1, 1),
        )
        .unwrap();
        assert_eq!(
            composites
                .iter()
                .map(|composite| composite.occurrence())
                .collect::<Vec<_>>(),
            expected
        );
    }
    let mut retirement = gpu.scope().gpu_retirement();
    drain_gpu(&mut retirement);
    drop(viewports);
    drop(unregistered);
    drop(designated);
    drop(mirror);
    drop(image);
    assert!(
        retained.upgrade().is_some(),
        "SDK bindings must retain the exact output lease"
    );
    renderer
        .unregister_external_texture(designated_texture)
        .unwrap();
    assert!(
        retained.upgrade().is_some(),
        "the mirror owns its own retained lease"
    );
    renderer
        .unregister_external_texture(mirror_texture)
        .unwrap();
    assert!(
        retained.upgrade().is_none(),
        "the final unregister releases the output lease"
    );
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

    use super::super::WgpuRenderHost;

    #[test]
    #[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
    fn configured_primary_without_imgui_retires_before_first_present() {
        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        let event_loop = builder.build().unwrap();
        let mut app = BarePrimaryApp {
            result: None,
            #[cfg(all(feature = "stock-sprite", feature = "stock-ui"))]
            instance_budget_probe: false,
        };
        event_loop.run_app(&mut app).unwrap();
        app.result.expect("the primary lifecycle must run").unwrap();
    }

    #[cfg(all(feature = "stock-sprite", feature = "stock-ui"))]
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
        #[cfg(all(feature = "stock-sprite", feature = "stock-ui"))]
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
                let mut execution = WgpuRenderHost::new(
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
                #[cfg(all(feature = "stock-sprite", feature = "stock-ui"))]
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

    #[cfg(all(feature = "stock-sprite", feature = "stock-ui"))]
    fn verify_instance_budget(
        execution: &mut WgpuRenderHost,
        window: RenderWindowPacket,
    ) -> Result<(), String> {
        use crate::host::WgpuRenderHostError;
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
            .begin_window_frame(window, 1)
            .map_err(|error| error.to_string())?;
        execution.backend.set_instance_budget_limit(instance_bytes);
        for _ in 1..=2 {
            let image = RenderImage2d::allocate_handle().unwrap();
            let target =
                editor_viewport_image_target(image, Extent2d::new(64, 64).unwrap()).unwrap();
            let viewport = ViewportRect::new(0, 0, 64, 64).unwrap();
            let mut views = ExtractedViews::default();
            views.push(ExtractedView {
                id: nara_render::RenderViewId::try_acquire().unwrap(),
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
            let producer = execution.create_producer().unwrap();
            let reservation = execution
                .reserve_viewport_target(&producer, window, packet.source_generation(), target)
                .map_err(|error| error.to_string())?;
            execution
                .activate_image_target(&producer, reservation)
                .map_err(|error| error.to_string())?;
            targets.push((producer, target, views, packet));
        }

        let (first_view, first_target, first_views, first_packet) = targets.remove(0);
        let first = execution
            .submit(
                &first_view,
                &crate::host::tests::test_stock_capture(
                    first_packet,
                    crate::WgpuStockCapture::new().with_ui(
                        vec![crate::view_probe::overlay_packet(RenderTarget::Image(
                            first_target.image(),
                        ))],
                        None,
                        None,
                    ),
                ),
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
            .submit(
                &second_view,
                &crate::host::tests::test_stock_capture(
                    second_packet,
                    crate::WgpuStockCapture::new().with_ui(
                        vec![crate::view_probe::overlay_packet(RenderTarget::Image(
                            second_target.image(),
                        ))],
                        None,
                        None,
                    ),
                ),
            )
            .expect_err("a second target must share the first submitted target's demand");
        assert!(matches!(
            error,
            WgpuRenderHostError::Backend(crate::WgpuRenderError::ContentRejected { .. })
        ));
        assert_eq!(execution.frame_transaction_stats().queue_submissions(), 1);
        assert_eq!(
            execution
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            instance_bytes
        );
        assert_eq!(
            execution.views[&first_view.id()].admission.last_frame_index,
            Some(first.receipt().render_transaction_frame_index())
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
            .begin_window_frame(window, 2)
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
            .submit(
                &first_view,
                &crate::host::tests::test_stock_capture(
                    next_first_packet,
                    crate::WgpuStockCapture::new().with_ui(
                        vec![crate::view_probe::overlay_packet(RenderTarget::Image(
                            first_target.image(),
                        ))],
                        None,
                        None,
                    ),
                ),
            )
            .map_err(|error| error.to_string())?;
        let next_packet =
            build_editor_render_frame_packet(generation, 2, &second_views, second_target)
                .unwrap()
                .unwrap();
        execution
            .submit(
                &second_view,
                &crate::host::tests::test_stock_capture(
                    next_packet,
                    crate::WgpuStockCapture::new().with_ui(
                        vec![crate::view_probe::overlay_packet(RenderTarget::Image(
                            second_target.image(),
                        ))],
                        None,
                        None,
                    ),
                ),
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
        RenderImagePacket, RenderTarget, RenderViewId, RenderWindowPacket, ViewportRect,
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
        PreparedFrame, WgpuEditorImguiRenderer, WgpuRenderHost, WgpuRenderHostError,
    };
    use crate::output::{WgpuImageDisplay, WgpuImageOutput};

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
        execution: WgpuRenderHost,
        producer: crate::host::WgpuRenderProducer,
        foreign_device: wgpu::Device,
        foreign_queue: wgpu::Queue,
        texture: ExternalTextureId,
        mirror_texture: ExternalTextureId,
        alias: wgpu::TextureView,
        display: WgpuImageDisplay,
        mirror: WgpuImageDisplay,
        successor: Option<WgpuImageOutput>,
        generation: EditorRenderGeneration,
        view_id: RenderViewId,
        target: RenderImagePacket,
        frame_index: u64,
        phase: Phase,
        context: Context,
    }

    #[derive(Clone, Copy)]
    enum Phase {
        Exact,
        Alias,
        RetainedAfterAdvance,
        RetainedAfterClose,
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
            let mut execution = WgpuRenderHost::new(
                WindowId::PRIMARY,
                WindowHandleProvider::new(Arc::clone(&window)),
            )
            .unwrap();
            let packet = window_packet(&window);
            let namespace = execution
                .backend
                .prepare_editor_window(
                    execution
                        .handles
                        .as_ref()
                        .expect("registered window authority"),
                    packet,
                )
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
            let view_id = RenderViewId::try_acquire().unwrap();
            let image_packet = image_packet(generation, view_id, target, 1);
            execution.begin_window_frame(packet, 1).unwrap();
            let producer = execution.create_producer().unwrap();
            let reservation = execution
                .reserve_viewport_target(
                    &producer,
                    packet,
                    image_packet.source_generation(),
                    target,
                )
                .unwrap();
            execution
                .activate_image_target(&producer, reservation)
                .unwrap();
            let image = execution
                .submit(
                    &producer,
                    &crate::host::tests::test_stock_input(image_packet),
                )
                .unwrap();
            let alias = image
                .0
                .texture()
                .create_view(&wgpu::TextureViewDescriptor::default());
            assert_ne!(&alias, image.0.view());
            let display = execution.display_image(image.clone()).unwrap();
            let mirror = execution.display_image(image).unwrap();
            let texture = renderer.register_image(&display).unwrap();
            let mirror_texture = renderer.register_image(&mirror).unwrap();
            assert_ne!(texture, mirror_texture);
            Self {
                window,
                context,
                platform,
                renderer,
                execution,
                producer,
                foreign_device,
                foreign_queue,
                texture,
                mirror_texture,
                alias,
                display,
                mirror,
                successor: None,
                generation,
                view_id,
                target,
                frame_index: 1,
                phase: Phase::Exact,
            }
        }

        fn advance(&mut self, event_loop: &ActiveEventLoop) -> bool {
            self.frame_index += 1;
            let packet = window_packet(&self.window);
            self.execution
                .begin_window_frame(packet, self.frame_index)
                .unwrap();
            self.platform
                .prepare_frame(&mut self.context, &self.window)
                .unwrap();
            let frame = self.context.begin_frame();
            draw_secondary_images(
                frame.ui(),
                self.texture,
                self.mirror_texture,
                matches!(self.phase, Phase::RetainedAfterClose),
            );
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
                    &[self.display.clone(), self.mirror.clone()],
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
            if !self.renderer.samples(dependencies, &self.mirror) {
                assert!(outcomes[0].composites.is_empty());
                return false;
            }
            match self.phase {
                Phase::Exact => {
                    assert!(self.renderer.samples(dependencies, &self.display));
                    assert_eq!(outcomes[0].composites.len(), 2);
                    assert_eq!(outcomes[0].composites[0].image(), self.display.receipt());
                    assert_eq!(outcomes[0].composites[1].image(), self.mirror.receipt());
                    assert_eq!(
                        outcomes[0].composites[0].occurrence(),
                        self.display.occurrence()
                    );
                    assert_eq!(
                        outcomes[0].composites[1].occurrence(),
                        self.mirror.occurrence()
                    );
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
                            &[self.display.clone(), self.mirror.clone()],
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
                            &[self.display.clone(), self.mirror.clone()],
                            &mut rejected,
                        );
                        assert!(
                            matches!(&result, Err(WgpuRenderHostError::WindowPaint { message })
                            if message.contains("another GPU device or queue"))
                        );
                        drop(result);
                        assert!(rejected.is_empty());
                        self.renderer.device = device;
                        self.renderer.queue = queue;
                        self.complete_rejected_frame(event_loop);
                    }
                    // A raw SDK mutation is deliberately outside the occurrence bridge. Even an
                    // alias of the same storage must not certify its original display binding.
                    let super::super::RendererOwner::Native(route) = &mut self.renderer.owner
                    else {
                        unreachable!()
                    };
                    route
                        .update_external_texture(self.texture, &self.alias)
                        .unwrap();
                    self.phase = Phase::Alias;
                }
                Phase::Alias => {
                    assert!(
                        dependencies
                            .external_bindings
                            .contains(&self.renderer.external_binding(self.texture).unwrap())
                    );
                    assert!(!self.renderer.samples(dependencies, &self.display));
                    assert_eq!(outcomes[0].composites.len(), 1);
                    assert_eq!(
                        outcomes[0].composites[0].occurrence(),
                        self.mirror.occurrence()
                    );
                    drop(prepared);
                    let successor = self
                        .execution
                        .submit(
                            &self.producer,
                            &crate::host::tests::test_stock_input(image_packet(
                                self.generation,
                                self.view_id,
                                self.target,
                                2,
                            )),
                        )
                        .unwrap();
                    assert_eq!(
                        self.display.receipt().target_generation(),
                        successor.receipt().target_generation()
                    );
                    assert_ne!(
                        self.display.receipt().output_version(),
                        successor.receipt().output_version()
                    );
                    assert_ne!(self.display.output().0.view(), successor.0.view());
                    self.successor = Some(successor);
                    let stale = WgpuImageDisplay::issue(super::test_image_output(
                        self.display.output().0.texture().clone(),
                        self.renderer.backend_instance_id,
                        self.renderer.device_epoch + 1,
                    ));
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
                            &[stale],
                            &mut rejected
                        ),
                        Err(WgpuRenderHostError::StaleImageSubmission)
                    ));
                    assert!(rejected.is_empty());
                    self.complete_rejected_frame(event_loop);
                    self.renderer
                        .update_image(self.texture, &self.display)
                        .unwrap();
                    self.phase = Phase::RetainedAfterAdvance;
                }
                Phase::RetainedAfterAdvance => {
                    assert!(self.renderer.samples(dependencies, &self.display));
                    assert_eq!(outcomes[0].composites.len(), 2);
                    for composite in outcomes[0].composites() {
                        assert_eq!(composite.image(), self.display.receipt());
                        assert_ne!(
                            composite.image(),
                            self.successor.as_ref().unwrap().receipt()
                        );
                    }
                    drop(prepared);
                    let view_id = self.display.receipt().producer_id();
                    assert!(self.execution.retire_producer(&self.producer).is_some());
                    assert!(!self.execution.views.contains_key(&view_id));
                    self.phase = Phase::RetainedAfterClose;
                }
                Phase::RetainedAfterClose => {
                    assert!(!self.renderer.samples(dependencies, &self.display));
                    assert_eq!(outcomes[0].composites.len(), 1);
                    assert_eq!(outcomes[0].composites[0].image(), self.mirror.receipt());
                    assert_eq!(
                        outcomes[0].composites[0].occurrence(),
                        self.mirror.occurrence()
                    );
                    drop(prepared);
                    self.verify_failed_paint_flush(packet);
                    return true;
                }
            }
            false
        }

        fn verify_failed_paint_flush(&mut self, window: RenderWindowPacket) {
            let output = self.mirror.output().clone();
            let buffer = self
                .execution
                .with_image(&output, |device, _, _| {
                    device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("failed-window-paint-upload"),
                        size: 16,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    })
                })
                .unwrap();
            let presentation = self.execution.last_presentation_generation;
            let before = self.execution.frame_transaction_stats().presents();
            let error = self
                .execution
                .present_window(
                    window,
                    self.frame_index,
                    std::slice::from_ref(&self.mirror),
                    |frame| {
                        frame.encode(|_, queue, _, _| queue.write_buffer(&buffer, 0, &[7; 16]));
                        Err(WgpuRenderHostError::window_paint(
                            "injected paint rejection after upload",
                        ))
                    },
                    || panic!("a rejected paint cannot reach native present"),
                )
                .unwrap_err();
            assert!(matches!(error, WgpuRenderHostError::WindowPaint { .. }));
            assert_eq!(self.execution.last_presentation_generation, presentation);
            assert_eq!(self.execution.frame_transaction_stats().presents(), before);
            let prefix = self
                .execution
                .native_prefix
                .as_ref()
                .expect("failed paint retains its own native prefix")
                .clone();
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            buffer.map_async(wgpu::MapMode::Read, .., move |result| {
                sender.send(result).unwrap();
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.execution
                    .with_image(&output, |device, _, _| device.poll(wgpu::PollType::Poll))
                    .unwrap()
                    .unwrap();
                match receiver.try_recv() {
                    Ok(result) => {
                        result.unwrap();
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => assert!(Instant::now() < deadline),
                    Err(error) => panic!("failed-paint readback disappeared: {error}"),
                }
                std::thread::yield_now();
            }
            assert_eq!(&*buffer.get_mapped_range(..).unwrap(), &[7; 16]);
            buffer.unmap();
            assert!(prefix.poll().unwrap());
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

    fn assert_toolkit_rejection(result: Result<(), WgpuRenderHostError>, expected: &str) {
        assert!(matches!(
            result,
            Err(WgpuRenderHostError::WindowPaint { message }) if message.contains(expected)
        ));
    }

    fn draw_secondary_images(
        ui: &Ui,
        designated: ExternalTextureId,
        mirror: ExternalTextureId,
        clip_designated: bool,
    ) {
        let viewport = ui.main_viewport();
        ui.window("Actual secondary image")
            .position(
                [
                    viewport.pos()[0] + viewport.size()[0] + 40.0,
                    viewport.pos()[1],
                ],
                Condition::Always,
            )
            .size([180.0, 220.0], Condition::Always)
            .build(|| {
                if clip_designated {
                    ui.get_window_draw_list().with_clip_rect(
                        [100_000.0, 100_000.0],
                        [100_001.0, 100_001.0],
                        || ui.image(designated.texture_id(), [64.0, 64.0]),
                    );
                } else {
                    ui.image(designated.texture_id(), [64.0, 64.0]);
                }
                ui.image(mirror.texture_id(), [64.0, 64.0]);
            });
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
        view_id: RenderViewId,
        target: RenderImagePacket,
        frame_index: u64,
    ) -> RenderFramePacket {
        let viewport = ViewportRect::new(0, 0, 64, 64).unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: view_id,
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
