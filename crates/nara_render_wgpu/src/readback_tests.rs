use crate::{
    quad::{
        WgpuQuadBatch, WgpuQuadPipelineDrawRef, create_material_bind_group_layout,
        create_quad_batch_buffers, create_quad_pipeline, create_quad_texture_bind_group_layout,
        draw_quad_batch_buffers,
    },
    texture::WgpuSpriteTextureCache,
};
use nara_render::ViewportRect;
use std::{sync::mpsc, time::Duration};

pub(crate) const READBACK_EXTENT: u32 = 64;
pub(crate) const READBACK_BYTES_PER_PIXEL: u32 = 4;

pub(crate) fn readback_viewport() -> ViewportRect {
    ViewportRect::new(0, 0, READBACK_EXTENT, READBACK_EXTENT).unwrap()
}

pub(crate) fn readback_pixel(pixels: &[u8], x: u32, y: u32) -> &[u8] {
    let offset = usize::try_from((y * READBACK_EXTENT + x) * READBACK_BYTES_PER_PIXEL).unwrap();
    &pixels[offset..offset + READBACK_BYTES_PER_PIXEL as usize]
}

pub(crate) fn render_batches_to_rgba(
    batches: &[WgpuQuadBatch],
) -> Result<Vec<u8>, crate::WgpuRenderError> {
    render_to_rgba(|device, queue, format, output_view, encoder| {
        let texture_layout = create_quad_texture_bind_group_layout(&device);
        let material_layout = create_material_bind_group_layout(&device);
        let pipelines = crate::required_quad_pipeline_keys(batches, format)
            .into_iter()
            .map(|key| {
                let pipeline =
                    create_quad_pipeline(&device, key, &texture_layout, &material_layout)?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: key.alpha_mode,
                    material: key.material,
                    pipeline: pipeline.pipeline,
                })
            })
            .collect::<Result<Vec<_>, crate::WgpuRenderError>>()?;
        let mut texture_cache = WgpuSpriteTextureCache::default();
        texture_cache.begin_frame(1);
        let buffers = create_quad_batch_buffers(
            &device,
            &queue,
            batches,
            &texture_layout,
            &material_layout,
            &mut texture_cache,
            1,
        )
        .unwrap();

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nara_wgpu_ui_readback_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(
                0.0,
                0.0,
                READBACK_EXTENT as f32,
                READBACK_EXTENT as f32,
                0.0,
                1.0,
            );
            draw_quad_batch_buffers(&mut pass, &pipelines, &buffers, readback_viewport());
        }
        Ok(())
    })
    .map(|(pixels, ())| pixels)
}

pub(crate) fn render_to_rgba<R>(
    render: impl FnOnce(
        &wgpu::Device,
        &wgpu::Queue,
        wgpu::TextureFormat,
        &wgpu::TextureView,
        &mut wgpu::CommandEncoder,
    ) -> Result<R, crate::WgpuRenderError>,
) -> Result<(Vec<u8>, R), crate::WgpuRenderError> {
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
    })
    .expect("quad readback requires a wgpu adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("quad readback requires a wgpu device");

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("nara_wgpu_ui_readback_target"),
        size: wgpu::Extent3d {
            width: READBACK_EXTENT,
            height: READBACK_EXTENT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let bytes_per_row = READBACK_EXTENT * READBACK_BYTES_PER_PIXEL;
    assert_eq!(bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("nara_wgpu_ui_readback_buffer"),
        size: u64::from(bytes_per_row) * u64::from(READBACK_EXTENT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("nara_wgpu_ui_readback_encoder"),
    });
    let facts = render(&device, &queue, format, &output_view, &mut encoder)?;
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &output,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(READBACK_EXTENT),
            },
        },
        wgpu::Extent3d {
            width: READBACK_EXTENT,
            height: READBACK_EXTENT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let (sender, receiver) = mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();

    let mapped = readback.get_mapped_range(..).unwrap();
    let pixels = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    Ok((pixels, facts))
}
