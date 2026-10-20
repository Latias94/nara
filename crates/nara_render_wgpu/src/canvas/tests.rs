use super::*;
use crate::allocation::AllocationFailureGuard;

#[cfg(feature = "sprite-submitter")]
use nara_app::RuntimeGeneration;
use nara_app::{App, RuntimeCandidateRetirementState, RuntimeInstance};
use nara_core::Color;
#[cfg(feature = "sprite-submitter")]
use nara_core::Vec2;
#[cfg(feature = "sprite-submitter")]
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
#[cfg(feature = "sprite-submitter")]
use nara_render::{Camera2d, RenderFramePacket};
use nara_render::{
    ExtractedViews, RenderTarget, WorldCanvas, build_render_frame_packet_with_world_canvas,
};
#[cfg(feature = "sprite-submitter")]
use nara_sprite::Sprite;
#[cfg(feature = "sprite-submitter")]
use nara_sprite_render::__private::SpriteBatches;
#[cfg(feature = "sprite-submitter")]
use nara_transform::Transform2d;
use nara_window::{Window, WindowId, WindowResolution};

#[cfg(feature = "sprite-submitter")]
use crate::readback_tests::{readback_pixel, render_to_rgba};
use crate::{WgpuRenderBackend, frame::WgpuFramePayload};

#[cfg(feature = "sprite-submitter")]
struct CheckerFrame {
    generation: RuntimeGeneration,
    views: ExtractedViews,
    payload: WgpuFramePayload,
    canvas: WorldCanvas,
}

#[cfg(feature = "sprite-submitter")]
impl CheckerFrame {
    fn new() -> Self {
        let canvas = WorldCanvas::new(8, 4).unwrap();
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_transform::TransformPlugin,
            nara_render::RenderComponentsPlugin,
            nara_render::RenderPlugin {
                world_canvas: Some(canvas),
            },
            nara_image::ImagePreparePlugin,
            nara_sprite_render::SpriteRenderPlugin,
        ))
        .unwrap();
        let world = app.world_mut().unwrap();
        world.spawn(Window::new(
            "canvas readback",
            WindowResolution::new(64, 64),
        ));
        let mut entities = vec![
            world
                .spawn((
                    Camera2d {
                        target: RenderTarget::Window(WindowId::PRIMARY),
                        viewport_height: 4.0,
                        ..Default::default()
                    },
                    Transform2d::default(),
                ))
                .id(),
        ];
        for y in 0..4 {
            for x in 0..8 {
                let color = if (x + y) % 2 == 0 {
                    Color::rgb(1.0, 0.0, 0.0)
                } else {
                    Color::rgb(0.0, 1.0, 0.0)
                };
                entities.push(
                    world
                        .spawn((
                            Sprite::from_color(Vec2::ONE, color)
                                .with_alpha_mode(nara_material::AlphaMode2d::Opaque),
                            Transform2d::from_translation(Vec2::new(
                                x as f32 - 3.5,
                                1.5 - y as f32,
                            )),
                        ))
                        .id(),
                );
            }
        }
        HierarchyWriter::new(world)
            .apply_batch(
                &entities
                    .into_iter()
                    .map(|entity| HierarchyOperation::Place {
                        entity,
                        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let driven = runtime.drive(std::time::Duration::ZERO);
        let frame = Self {
            generation: runtime.generation(),
            views: runtime.world().resource::<ExtractedViews>().clone(),
            payload: WgpuFramePayload::for_editor_sprites(
                runtime.world().resource::<SpriteBatches>().clone(),
            ),
            canvas,
        };
        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
        driven.unwrap();
        frame
    }

    fn packet(&self, width: u32, height: u32, scale: f64, frame_index: u64) -> RenderFramePacket {
        let window = Window::new(
            "canvas readback",
            WindowResolution::new(width, height).with_scale_factor(scale),
        );
        build_render_frame_packet_with_world_canvas(
            self.generation,
            frame_index,
            Some(WindowId::PRIMARY),
            [&window],
            &self.views,
            Some(self.canvas),
        )
        .unwrap()
        .unwrap()
    }
}

#[test]
#[cfg(feature = "sprite-submitter")]
fn fixed_canvas_checkerboard_bars_and_native_ui_share_the_target_encoder() {
    #[allow(
        unused_mut,
        reason = "the UI packet is present only with the UI submitter"
    )]
    let mut captured = CheckerFrame::new();
    #[cfg(feature = "ui-submitter")]
    {
        captured.payload.ui_packets = vec![crate::view_probe::overlay_packet(
            RenderTarget::Window(WindowId::PRIMARY),
        )];
    }
    let packet = captured.packet(64, 64, 1.0, 1);
    let (pixels, ()) = render_to_rgba(|device, queue, format, output, encoder| {
        let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
        backend.admit_packet(packet.source_generation(), packet.frame_index())?;
        let prepared = backend.prepare_target_draw(format, &packet, &captured.payload, 1)?;
        assert_eq!(prepared.canvas.as_ref().unwrap().world_view_count, 1);
        #[cfg(feature = "ui-submitter")]
        assert_eq!(prepared.ui_packets.len(), 1);
        assert_eq!(backend.frame_transaction_stats().queue_submissions(), 0);
        assert_eq!(
            backend.frame_transaction_stats().surface_acquire_attempts(),
            0
        );
        crate::encode_target_draw(encoder, output, &prepared);
        Ok(())
    })
    .unwrap();
    assert_checkerboard(
        &pixels,
        packet.world_canvas().unwrap(),
        64,
        64,
        cfg!(feature = "ui-submitter"),
    );
    #[cfg(feature = "ui-submitter")]
    assert!(
        readback_pixel(&pixels, 29, 8)
            .iter()
            .zip([188u8, 188, 188, 255])
            .all(|(&actual, expected)| actual.abs_diff(expected) <= 1),
        "native UI must remain in the top bar at its full-target coordinates"
    );
}

#[test]
#[cfg(feature = "sprite-submitter")]
fn fixed_canvas_nearest_composite_handles_odd_fractional_and_undersized_windows() {
    let captured = CheckerFrame::new();
    for (width, height, scale) in [(63, 61, 1.5), (5, 3, 1.25)] {
        let packet = captured.packet(width, height, scale, 1);
        let (pixels, ()) = render_to_rgba(|device, queue, format, output, encoder| {
            let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
            let prepared = backend.prepare_target_draw(format, &packet, &captured.payload, 1)?;
            crate::encode_target_draw(encoder, output, &prepared);
            Ok(())
        })
        .unwrap();
        assert_checkerboard(
            &pixels,
            packet.world_canvas().unwrap(),
            width,
            height,
            false,
        );
    }
}

#[cfg(feature = "sprite-submitter")]
fn assert_checkerboard(pixels: &[u8], frame: WorldCanvasFrame, width: u32, height: u32, ui: bool) {
    let rect = frame.mapping().content_rect();
    let ppp = frame.mapping().pixels_per_point();
    let min = rect.min.map(|point| point * ppp);
    let max = rect.max.map(|point| point * ppp);
    for y in 0..height {
        for x in 0..width {
            if ui && (28..36).contains(&x) && (4..12).contains(&y) {
                continue;
            }
            let center = [x as f32 + 0.5, y as f32 + 0.5];
            let expected =
                if (0..2).all(|axis| center[axis] >= min[axis] && center[axis] < max[axis]) {
                    let sx = ((center[0] - min[0]) / (max[0] - min[0]) * 8.0).floor() as u32;
                    let sy = ((center[1] - min[1]) / (max[1] - min[1]) * 4.0).floor() as u32;
                    if (sx + sy) % 2 == 0 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 255, 0, 255]
                    }
                } else {
                    [0, 0, 0, 255]
                };
            assert_eq!(
                readback_pixel(pixels, x, y),
                expected,
                "destination {width}x{height} pixel ({x}, {y}), physical content {min:?}..{max:?}, logical content {rect:?}, pixels_per_point {ppp}"
            );
        }
    }
}

#[test]
#[cfg(feature = "sprite-submitter")]
fn canvas_resize_reuses_raster_and_configuration_retirement_releases_it() {
    let captured = CheckerFrame::new();
    render_to_rgba(|device, queue, format, _, _| {
        let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
        let first = backend.prepare_target_draw(
            format,
            &captured.packet(64, 64, 1.0, 1),
            &captured.payload,
            1,
        )?;
        let resources = Arc::downgrade(&first.canvas.as_ref().unwrap().resources);
        let resized = backend.prepare_target_draw(
            format,
            &captured.packet(63, 61, 1.5, 2),
            &captured.payload,
            2,
        )?;
        assert!(Arc::ptr_eq(
            &first.canvas.as_ref().unwrap().resources,
            &resized.canvas.as_ref().unwrap().resources
        ));
        drop(first);
        drop(resized);
        assert!(resources.upgrade().is_some());
        backend.clear_gpu_resources(crate::surface::SurfaceDropReason::BackendCleanup)?;
        assert!(resources.upgrade().is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg(feature = "sprite-submitter")]
fn canvas_allocation_failures_do_not_submit_and_valid_retry_recovers() {
    let captured = CheckerFrame::new();
    render_to_rgba(|device, queue, format, output, encoder| {
        for label in [
            "nara_world_canvas_layout",
            "nara_world_canvas_texture",
            "nara_world_canvas_view",
            "nara_world_canvas_binding",
            "nara_world_canvas_mapping",
            "nara_world_canvas_shader",
            "nara_world_canvas_pipeline_layout",
            "nara_world_canvas_pipeline",
        ] {
            let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
            let packet = captured.packet(64, 64, 1.0, 1);
            let mut canvas_cache = WorldCanvasCache::default();
            let baseline = backend.prepare_target_draw_with_canvas_cache(
                format,
                &packet,
                &captured.payload,
                1,
                &mut canvas_cache,
            )?;
            let retained = Arc::clone(&baseline.canvas.as_ref().unwrap().resources);
            drop(baseline);
            let failure = AllocationFailureGuard::at(label);
            let error = backend
                .prepare_target_draw_with_canvas_cache(
                    wgpu::TextureFormat::Rgba8Unorm,
                    &packet,
                    &captured.payload,
                    1,
                    &mut canvas_cache,
                )
                .unwrap_err();
            assert!(
                matches!(error, WgpuRenderError::UploadDeferred { .. }),
                "{label}: {error}"
            );
            assert!(failure.was_consumed(), "{label}");
            assert!(
                Arc::ptr_eq(canvas_cache.resources.as_ref().unwrap(), &retained),
                "{label}: failure must retain the complete prior canvas allocation"
            );
            assert_eq!(backend.frame_transaction_stats().queue_submissions(), 0);
            assert_eq!(
                backend.frame_transaction_stats().surface_acquire_attempts(),
                0
            );
            assert_eq!(
                backend
                    .frame_transaction_stats()
                    .candidate_instance_buffer_demand_bytes(),
                0
            );
            drop(failure);
            let prepared = backend.prepare_target_draw_with_canvas_cache(
                format,
                &packet,
                &captured.payload,
                1,
                &mut canvas_cache,
            )?;
            crate::encode_target_draw(encoder, output, &prepared);
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn clear_only_canvas_uses_checked_base_backend_without_submitters() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let generation = runtime.generation();
    let mut retirement = runtime.begin_retirement();
    while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
        retirement.drive_retirement();
    }
    let window = Window::new("clear canvas", WindowResolution::new(64, 64));
    let mut views = ExtractedViews::default();
    let viewport = nara_render::ViewportRect::new(0, 0, 8, 4).unwrap();
    views.push(nara_render::ExtractedView {
        camera_entity: nara_ecs::Entity::PLACEHOLDER,
        target: RenderTarget::Window(WindowId::PRIMARY),
        viewport,
        mapping: nara_render::ViewMapping2d::orthographic(nara_core::Mat3::IDENTITY, 4.0, viewport)
            .unwrap(),
        order: 0,
        load: nara_render::ViewLoadOp::Clear(Color::rgb(1.0, 0.0, 0.0)),
    });
    let packet = build_render_frame_packet_with_world_canvas(
        generation,
        1,
        Some(WindowId::PRIMARY),
        [&window],
        &views,
        Some(WorldCanvas::new(8, 4).unwrap()),
    )
    .unwrap()
    .unwrap();
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("canvas readback requires an adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("canvas readback requires a device");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("canvas_clear_readback"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
    let mut cache = WorldCanvasCache::default();
    let prepared = backend
        .prepare_target_draw_with_canvas_cache(
            format,
            &packet,
            &WgpuFramePayload::default(),
            1,
            &mut cache,
        )
        .unwrap();
    let retained = Arc::downgrade(&prepared.canvas.as_ref().unwrap().resources);
    let original_key = cache.resources.as_ref().unwrap().key;
    let excess = device
        .limits()
        .max_texture_dimension_2d
        .checked_add(1)
        .unwrap();
    let invalid = build_render_frame_packet_with_world_canvas(
        generation,
        2,
        Some(WindowId::PRIMARY),
        [&window],
        &views,
        Some(WorldCanvas::new(excess, 4).unwrap()),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        backend.prepare_target_draw_with_canvas_cache(
            format,
            &invalid,
            &WgpuFramePayload::default(),
            2,
            &mut cache
        ),
        Err(WgpuRenderError::ContentRejected { .. })
    ));
    assert_eq!(cache.resources.as_ref().unwrap().key, original_key);
    let failed = AllocationFailureGuard::at("nara_world_canvas_texture");
    let mut empty = WorldCanvasCache::default();
    assert!(matches!(
        backend.prepare_target_draw_with_canvas_cache(
            format,
            &packet,
            &WgpuFramePayload::default(),
            3,
            &mut empty
        ),
        Err(WgpuRenderError::UploadDeferred { .. })
    ));
    assert!(failed.was_consumed());
    assert!(empty.resources.is_none());
    drop(failed);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("canvas_clear_readback_buffer"),
        size: 3 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    crate::encode_target_draw(&mut encoder, &output, &prepared);
    for (index, (x, y)) in [(0, 0), (32, 32), (0, 63)].into_iter().enumerate() {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: index as u64 * 256,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
    assert_eq!(backend.frame_transaction_stats().queue_submissions(), 0);
    assert_eq!(
        backend.frame_transaction_stats().surface_acquire_attempts(),
        0
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = readback.get_mapped_range(..).unwrap();
    for (offset, expected) in [
        (0, [0, 0, 0, 255]),
        (256, [255, 0, 0, 255]),
        (512, [0, 0, 0, 255]),
    ] {
        assert_eq!(&bytes[offset..offset + 4], &expected);
    }
    drop(bytes);
    readback.unmap();
    drop(prepared);
    cache.clear();
    assert!(retained.upgrade().is_none());
}
