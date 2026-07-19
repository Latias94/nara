use crate::quad::{WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuScissorRect};
use nara_render::{RenderPhaseInput, RenderPhaseLabel};
use nara_ui_render::{UiBatch, UiBatches, UiInstance};

pub(crate) fn collect_ui_quad_batches(
    batches: &UiBatches,
    view_index: usize,
) -> Vec<WgpuQuadBatch> {
    batches.for_view(view_index).map(ui_quad_batch).collect()
}

pub(crate) fn append_ui_phase_inputs(batches: &UiBatches, inputs: &mut Vec<RenderPhaseInput>) {
    inputs.extend(batches.as_slice().iter().map(|batch| RenderPhaseInput {
        view_index: batch.view_index,
        phase: RenderPhaseLabel::UI,
    }));
}

fn ui_quad_batch(batch: &UiBatch) -> WgpuQuadBatch {
    WgpuQuadBatch {
        phase: RenderPhaseLabel::UI,
        material: WgpuQuadMaterialKey {
            image: batch.material.image,
            sampler: batch.material.sampler,
            alpha_mode: batch.material.alpha_mode,
        },
        scissor: batch.clip_rect.map(|clip| WgpuScissorRect {
            x: clip.x,
            y: clip.y,
            width: clip.width,
            height: clip.height,
        }),
        instances: batch.instances.iter().map(ui_quad_instance).collect(),
        counts_as_sprites: false,
    }
}

fn ui_quad_instance(instance: &UiInstance) -> WgpuQuadInstance {
    WgpuQuadInstance {
        center: instance.center.to_array(),
        x_axis: instance.x_axis.to_array(),
        y_axis: instance.y_axis.to_array(),
        color: [
            instance.color.r,
            instance.color.g,
            instance.color.b,
            instance.color.a,
        ],
        uv_min: instance.uv.min.to_array(),
        uv_size: instance.uv.size.to_array(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UI_PIXEL_CHILD_MARKER: &str = "NARA_UI_PIXEL_TEST_CHILD";
    use crate::{
        quad::{
            WgpuQuadPipelineDrawRef, WgpuQuadPipelineKey, create_quad_batch_buffers,
            create_quad_pipeline, create_quad_texture_bind_group_layout,
            draw_quad_batch_buffers_for_phase,
        },
        texture::WgpuSpriteTextureCache,
    };
    use nara_core::{Color, Vec2};
    use nara_material::{AlphaMode2d, SamplerDescriptor};
    use nara_render::{RenderTarget, ViewportRect};
    use nara_ui::UiRect;
    use nara_ui_render::{UiClipRect, UiColorKey, UiMaterialKey, UiTextureRect};

    #[test]
    fn ui_adapter_does_not_require_sprite_domain_types() {
        let batch = UiBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            order: 0,
            z_index: 0,
            material: UiMaterialKey {
                image: None,
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Blend,
                tint: UiColorKey::from_color(Color::WHITE),
            },
            clip_rect: Some(
                UiClipRect::from_rect_clamped(
                    UiRect::new(Vec2::new(-10.0, 5.0), Vec2::new(150.0, 80.0)),
                    nara_render::ViewportRect::new(0, 0, 100, 60).unwrap(),
                )
                .unwrap(),
            ),
            instances: vec![UiInstance {
                center: Vec2::ZERO,
                x_axis: Vec2::X,
                y_axis: Vec2::Y,
                color: Color::WHITE,
                uv: UiTextureRect::FULL,
            }],
        };

        let converted = ui_quad_batch(&batch);
        assert_eq!(converted.phase, RenderPhaseLabel::UI);
        assert!(!converted.counts_as_sprites);
        assert_eq!(converted.instances.len(), 1);
        assert_eq!(
            converted.scissor,
            Some(WgpuScissorRect {
                x: 0,
                y: 5,
                width: 100,
                height: 55,
            })
        );
    }

    #[test]
    fn production_quad_pipeline_applies_clamped_ui_scissor_to_pixels() {
        let mut child = std::process::Command::new(
            std::env::current_exe().expect("the unit-test executable should have a path"),
        )
        .args([
            "--ignored",
            "--exact",
            "ui::tests::production_quad_pipeline_applies_clamped_ui_scissor_to_pixels_child",
            "--nocapture",
        ])
        .env(UI_PIXEL_CHILD_MARKER, "1")
        .spawn()
        .expect("the production UI pixel test child should start");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert!(
                        status.success(),
                        "the production UI pixel test child failed"
                    );
                    break;
                }
                Ok(None) => {}
                Err(poll_error) => {
                    let reap = terminate_and_reap(&mut child);
                    panic!(
                        "production UI pixel test child polling failed: {poll_error}; reap={reap:?}"
                    );
                }
            }
            if std::time::Instant::now() >= deadline {
                let status = terminate_and_reap(&mut child)
                    .expect("the timed-out production UI pixel test child should be reaped");
                panic!("production UI pixel test exceeded its hard deadline: {status}");
            }
            std::thread::park_timeout(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "runs only inside the parent-owned hard-deadline process"]
    fn production_quad_pipeline_applies_clamped_ui_scissor_to_pixels_child() {
        assert_eq!(
            std::env::var_os(UI_PIXEL_CHILD_MARKER).as_deref(),
            Some(std::ffi::OsStr::new("1")),
            "the ignored GPU child must run only under the bounded parent wrapper"
        );
        run_production_quad_pipeline_ui_scissor_pixel_test();
    }

    fn terminate_and_reap(
        child: &mut std::process::Child,
    ) -> Result<std::process::ExitStatus, String> {
        let kill_error = child.kill().err();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::park_timeout(std::time::Duration::from_millis(10));
                }
                Ok(None) => {
                    return Err(match kill_error {
                        Some(kill_error) => {
                            format!("kill={kill_error}; child remained live past the reap deadline")
                        }
                        None => "child remained live past the reap deadline".to_owned(),
                    });
                }
                Err(poll_error) => {
                    return Err(match kill_error {
                        Some(kill_error) => format!("kill={kill_error}; poll={poll_error}"),
                        None => format!("poll={poll_error}"),
                    });
                }
            }
        }
    }

    #[test]
    fn terminate_helper_reaps_a_child_that_exits_before_kill() {
        let mut child = std::process::Command::new(
            std::env::current_exe().expect("the unit-test executable should have a path"),
        )
        .args(["--exact", "no-test-has-this-name"])
        .spawn()
        .expect("the short-lived child should start");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let exited = loop {
            if let Some(status) = child
                .try_wait()
                .expect("the child should remain observable")
            {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the short-lived child did not exit"
            );
            std::thread::park_timeout(std::time::Duration::from_millis(10));
        };

        let status = terminate_and_reap(&mut child).expect("the exited child should be reaped");
        assert_eq!(status, exited);
    }

    fn run_production_quad_pipeline_ui_scissor_pixel_test() {
        const GPU_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
        const WIDTH: u32 = 64;
        const HEIGHT: u32 = 48;
        const BYTES_PER_PIXEL: u32 = 4;
        const BYTES_PER_ROW: u32 = WIDTH * BYTES_PER_PIXEL;

        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .or_else(|_| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: true,
                ..Default::default()
            }))
        });
        let adapter = adapter.expect("production UI pixel readback requires a wgpu adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("production UI pixel readback requires a wgpu device");

        let viewport = ViewportRect::new(8, 4, 40, 32).unwrap();
        let clip = UiClipRect::from_rect_clamped(
            UiRect::new(Vec2::new(-10.0, 12.0), Vec2::new(80.0, 18.0)),
            viewport,
        )
        .unwrap();
        assert_eq!(
            clip,
            UiClipRect {
                x: 8,
                y: 12,
                width: 40,
                height: 18,
            },
            "the UI queue must clamp an oversized clip to the admitted viewport",
        );

        let batch = ui_quad_batch(&UiBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            order: 0,
            z_index: 0,
            material: UiMaterialKey {
                image: None,
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Opaque,
                tint: UiColorKey::from_color(Color::rgb(1.0, 0.0, 0.0)),
            },
            clip_rect: Some(clip),
            instances: vec![UiInstance::axis_aligned(
                Vec2::ZERO,
                Vec2::ONE,
                Color::rgb(1.0, 0.0, 0.0),
            )],
        });

        let format = wgpu::TextureFormat::Rgba8Unorm;
        let texture_layout = create_quad_texture_bind_group_layout(&device);
        let pipeline = create_quad_pipeline(
            &device,
            WgpuQuadPipelineKey {
                format,
                alpha_mode: AlphaMode2d::Opaque,
            },
            &texture_layout,
        );
        let mut texture_cache = WgpuSpriteTextureCache::default();
        let buffers = create_quad_batch_buffers(
            &device,
            &queue,
            &[batch],
            &texture_layout,
            &mut texture_cache,
            None,
            None,
            1,
        )
        .unwrap();
        let pipelines = [WgpuQuadPipelineDrawRef {
            alpha_mode: AlphaMode2d::Opaque,
            pipeline: pipeline.pipeline,
        }];

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nara_ui_scissor_readback_target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("nara_ui_scissor_readback_buffer"),
            size: u64::from(BYTES_PER_ROW) * u64::from(HEIGHT),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("nara_ui_scissor_readback_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nara_ui_scissor_readback_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
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
            pass.set_viewport(
                viewport.physical_x as f32,
                viewport.physical_y as f32,
                viewport.physical_width as f32,
                viewport.physical_height as f32,
                0.0,
                1.0,
            );
            draw_quad_batch_buffers_for_phase(
                &mut pass,
                &pipelines,
                &buffers,
                RenderPhaseLabel::UI,
                viewport,
            );
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(BYTES_PER_ROW),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(GPU_WAIT_TIMEOUT),
            })
            .expect("production UI pixel readback timed out while polling the device");
        receiver
            .recv_timeout(GPU_WAIT_TIMEOUT)
            .expect("production UI pixel readback map callback timed out")
            .expect("production UI pixel readback mapping failed");
        let pixels = slice.get_mapped_range().unwrap();
        let rgba = |x: u32, y: u32| {
            let start = (y * BYTES_PER_ROW + x * BYTES_PER_PIXEL) as usize;
            &pixels[start..start + BYTES_PER_PIXEL as usize]
        };

        assert_eq!(rgba(16, 16), &[255, 0, 0, 255], "inside the clip");
        assert_eq!(rgba(16, 8), &[0, 0, 0, 255], "inside viewport, above clip");
        assert_eq!(rgba(4, 16), &[0, 0, 0, 255], "outside viewport and clip");
        assert_eq!(rgba(16, 32), &[0, 0, 0, 255], "below the clip");
        drop(pixels);
        readback.unmap();
    }
}
