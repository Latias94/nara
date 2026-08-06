use std::sync::Arc;

use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{RenderTarget, ViewportRect};
use nara_ui_render::{UiPacketPrimitive, UiPacketRect, UiPaintPacket};
use thiserror::Error;

use crate::quad::{
    WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture, WgpuScissorRect,
};

pub(crate) fn collect_ui_quad_batches(
    packets: &[Arc<UiPaintPacket>],
    target: RenderTarget,
    primary_window_id: Option<nara_window::WindowId>,
    viewport: ViewportRect,
) -> Result<Vec<WgpuQuadBatch>, WgpuUiPacketError> {
    let Some(target_window_id) = target.window_id(primary_window_id) else {
        return Ok(Vec::new());
    };
    let mut batches = Vec::<WgpuQuadBatch>::new();
    for packet in packets
        .iter()
        .filter(|packet| packet.target().window_id(primary_window_id) == Some(target_window_id))
    {
        for primitive in packet.primitives() {
            let Some((material, texture, scissor, instance)) =
                convert_primitive(packet, primitive, viewport)?
            else {
                continue;
            };
            if let Some(batch) = batches.last_mut()
                && batch.material == material
                && batch.texture == texture
                && batch.scissor == scissor
            {
                batch.instances.push(instance);
                continue;
            }
            batches.push(WgpuQuadBatch {
                material,
                texture,
                scissor,
                instances: vec![instance],
                counts_as_sprites: false,
            });
        }
    }
    Ok(batches)
}

fn convert_primitive(
    packet: &UiPaintPacket,
    primitive: &UiPacketPrimitive,
    viewport: ViewportRect,
) -> Result<
    Option<(
        WgpuQuadMaterialKey,
        WgpuQuadTexture,
        Option<WgpuScissorRect>,
        WgpuQuadInstance,
    )>,
    WgpuUiPacketError,
> {
    let (texture, color, uv, corner_radii, texture_mode) = match primitive {
        UiPacketPrimitive::Solid {
            color,
            corner_radii,
            ..
        } => (
            WgpuQuadTexture::Fallback,
            color.components(),
            nara_ui_render::UiPacketUv::FULL,
            *corner_radii,
            0,
        ),
        UiPacketPrimitive::Image {
            image_index,
            tint,
            uv,
            corner_radii,
            ..
        } => {
            let image = packet.images().get(*image_index as usize).ok_or(
                WgpuUiPacketError::MissingImageResource {
                    index: *image_index,
                },
            )?;
            (
                WgpuQuadTexture::Image(image.snapshot().clone()),
                tint.components(),
                *uv,
                *corner_radii,
                0,
            )
        }
        UiPacketPrimitive::Glyph {
            page_index,
            color,
            uv,
            ..
        } => {
            let page = packet
                .glyph_pages()
                .get(usize::from(*page_index))
                .ok_or(WgpuUiPacketError::MissingGlyphPage { index: *page_index })?;
            (
                WgpuQuadTexture::Glyph(page.clone()),
                color.components(),
                *uv,
                [0.0; 4],
                1,
            )
        }
    };
    if corner_radii.iter().any(|radius| *radius != 0.0) {
        return Err(WgpuUiPacketError::UnsupportedCornerRadii);
    }
    let Some(instance) = ui_quad_instance(primitive.rect(), color, uv, texture_mode, viewport)
    else {
        return Ok(None);
    };
    let scissor = primitive
        .clip()
        .and_then(|clip| scissor_rect(clip, viewport));
    let material = WgpuQuadMaterialKey {
        texture: texture.key(),
        sampler: SamplerDescriptor::default(),
        alpha_mode: AlphaMode2d::Blend,
    };
    Ok(Some((material, texture, scissor, instance)))
}

fn ui_quad_instance(
    rect: UiPacketRect,
    color: [f32; 4],
    uv: nara_ui_render::UiPacketUv,
    texture_mode: u32,
    viewport: ViewportRect,
) -> Option<WgpuQuadInstance> {
    if viewport.physical_width == 0 || viewport.physical_height == 0 {
        return None;
    }
    let viewport_width = viewport.physical_width as f32;
    let viewport_height = viewport.physical_height as f32;
    let center_x = (rect.min_x() as f32 + rect.max_x() as f32) * 0.5;
    let center_y = (rect.min_y() as f32 + rect.max_y() as f32) * 0.5;
    let half_width = rect.width() as f32 * 0.5;
    let half_height = rect.height() as f32 * 0.5;
    Some(WgpuQuadInstance {
        center: [
            center_x / viewport_width * 2.0 - 1.0,
            1.0 - center_y / viewport_height * 2.0,
        ],
        x_axis: [half_width / viewport_width * 2.0, 0.0],
        y_axis: [0.0, -(half_height / viewport_height * 2.0)],
        color,
        uv_min: uv.min(),
        uv_size: uv.size(),
        texture_mode,
    })
}

fn scissor_rect(rect: UiPacketRect, viewport: ViewportRect) -> Option<WgpuScissorRect> {
    let left = rect.min_x().max(0) as u32;
    let top = rect.min_y().max(0) as u32;
    let right = (rect.max_x().max(0) as u32).min(viewport.physical_width);
    let bottom = (rect.max_y().max(0) as u32).min(viewport.physical_height);
    if right <= left || bottom <= top {
        return None;
    }
    Some(WgpuScissorRect {
        x: viewport.physical_x.saturating_add(left),
        y: viewport.physical_y.saturating_add(top),
        width: right - left,
        height: bottom - top,
    })
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(crate) enum WgpuUiPacketError {
    #[error("UI paint packet references missing image resource index {index}")]
    MissingImageResource { index: u32 },
    #[error("UI paint packet references missing glyph-atlas page {index}")]
    MissingGlyphPage { index: u16 },
    #[error("the first-party WGPU UI submitter does not yet support rounded rectangles")]
    UnsupportedCornerRadii,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    use nara_asset::{
        ArtifactFormatVersion, ArtifactLabel, AssetId, AssetPath, AssetVersion, Assets, Handle,
        ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
        ImportSettingsHash, ImportedAssetType, ImporterId, ImporterVersion, SourceHash,
        StableAssetId,
    };
    use nara_image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageSourceMetadata,
        PreparedImageResource, PreparedImageSnapshot, image_descriptor_hash, image_resource_key,
    };
    use nara_render::RenderResourceSnapshot;
    use nara_text::bundled_font_face;
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiImageId, UiLayer,
        UiLocalKey, UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
    };
    use nara_window::WindowId;

    use crate::{
        quad::{
            WgpuQuadPipelineDrawRef, WgpuQuadPipelineKey, create_quad_batch_buffers,
            create_quad_pipeline, create_quad_texture_bind_group_layout, draw_quad_batch_buffers,
        },
        texture::WgpuSpriteTextureCache,
    };

    #[test]
    fn physical_ui_rect_projects_into_the_target_viewport() {
        let instance = ui_quad_instance(
            UiPacketRect::new(0, 0, 50, 25).unwrap(),
            [1.0; 4],
            nara_ui_render::UiPacketUv::FULL,
            0,
            ViewportRect::new(10, 20, 100, 50).unwrap(),
        )
        .unwrap();

        assert_eq!(instance.center, [-0.5, 0.5]);
        assert_eq!(instance.x_axis, [0.5, 0.0]);
        assert_eq!(instance.y_axis, [0.0, -0.5]);
    }

    #[test]
    fn scissor_is_clamped_and_offset_by_the_viewport() {
        let scissor = scissor_rect(
            UiPacketRect::new(-5, 10, 40, 70).unwrap(),
            ViewportRect::new(10, 20, 100, 50).unwrap(),
        )
        .unwrap();

        assert_eq!(scissor.x, 10);
        assert_eq!(scissor.y, 30);
        assert_eq!(scissor.width, 40);
        assert_eq!(scissor.height, 40);
    }

    #[test]
    fn glyph_packet_renders_nonblank_gpu_pixels() {
        let packet = text_packet();
        let viewport = readback_viewport();
        // The Host route is explicit while the extracted camera keeps the primary-window alias.
        // Both names resolve to the same presentation target and must share one GPU path.
        let target = RenderTarget::PrimaryWindow;
        let mut batches =
            collect_ui_quad_batches(&[packet], target, Some(WindowId::PRIMARY), viewport).unwrap();
        batches.retain(|batch| matches!(&batch.texture, WgpuQuadTexture::Glyph(_)));
        assert!(
            !batches.is_empty(),
            "the UI packet should contain glyph work"
        );

        let pixels = render_batches_to_rgba(&batches);
        let nonblank_pixels = pixels
            .chunks_exact(READBACK_BYTES_PER_PIXEL as usize)
            .filter(|pixel| pixel[3] != 0)
            .count();
        assert!(nonblank_pixels > 0, "the rendered glyph must affect pixels");
        assert!(
            nonblank_pixels < (READBACK_EXTENT * READBACK_EXTENT) as usize,
            "the glyph must not fill the entire target"
        );
    }

    #[test]
    fn image_packet_renders_the_captured_snapshot_pixels() {
        let packet = image_packet();
        let viewport = readback_viewport();
        let target = RenderTarget::Window(WindowId::PRIMARY);
        let mut batches =
            collect_ui_quad_batches(&[packet], target, Some(WindowId::PRIMARY), viewport).unwrap();
        batches.retain(|batch| matches!(&batch.texture, WgpuQuadTexture::Image(_)));
        assert!(
            !batches.is_empty(),
            "the UI packet should contain image work"
        );

        let pixels = render_batches_to_rgba(&batches);
        assert!(
            pixels
                .chunks_exact(READBACK_BYTES_PER_PIXEL as usize)
                .any(|pixel| pixel[3] > 200 && pixel[0] > 200 && pixel[1] < 64 && pixel[2] > 80)
        );
    }

    const READBACK_EXTENT: u32 = 64;
    const READBACK_BYTES_PER_PIXEL: u32 = 4;

    fn readback_viewport() -> ViewportRect {
        ViewportRect::new(0, 0, READBACK_EXTENT, READBACK_EXTENT).unwrap()
    }

    fn render_batches_to_rgba(batches: &[WgpuQuadBatch]) -> Vec<u8> {
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
        .expect("UI readback requires a wgpu adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("UI readback requires a wgpu device");

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
        let texture_layout = create_quad_texture_bind_group_layout(&device);
        let pipeline = create_quad_pipeline(
            &device,
            WgpuQuadPipelineKey {
                format,
                alpha_mode: AlphaMode2d::Blend,
            },
            &texture_layout,
        );
        let pipelines = [WgpuQuadPipelineDrawRef {
            alpha_mode: AlphaMode2d::Blend,
            pipeline: pipeline.pipeline.clone(),
        }];
        let mut texture_cache = WgpuSpriteTextureCache::default();
        texture_cache.begin_frame(1);
        let buffers = create_quad_batch_buffers(
            &device,
            &queue,
            &batches,
            &texture_layout,
            &mut texture_cache,
            1,
        )
        .unwrap();

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
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nara_wgpu_ui_readback_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
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
        pixels
    }

    fn text_packet() -> Arc<UiPaintPacket> {
        let node = UiNode::new(UiStyle::absolute(4.0, 4.0, 56.0, 40.0))
            .with_key(UiLocalKey::runtime("wgpu-glyph-readback", "label").unwrap())
            .with_text(UiText::new("Nara", 20.0).unwrap());
        let frame = compile_frame("wgpu-glyph-readback", node);
        nara_ui_render::UiPaintPacketBuilder::default()
            .build(
                frame,
                RenderTarget::Window(WindowId::PRIMARY),
                &nara_ui_render::UiPreparedImages::default(),
            )
            .unwrap()
    }

    fn image_packet() -> Arc<UiPaintPacket> {
        let image_id = UiImageId::new("project://ui/readback.png").unwrap();
        let node = UiNode::new(UiStyle::absolute(8.0, 8.0, 32.0, 32.0))
            .with_key(UiLocalKey::runtime("wgpu-image-readback", "image").unwrap())
            .with_visual(UiVisual::image(
                image_id.clone(),
                UiColor::rgba(1.0, 1.0, 1.0, 1.0),
            ));
        let frame = compile_frame("wgpu-image-readback", node);
        let mut prepared = nara_ui_render::UiPreparedImages::default();
        prepared.insert(image_id, captured_image_snapshot());
        nara_ui_render::UiPaintPacketBuilder::default()
            .build(frame, RenderTarget::Window(WindowId::PRIMARY), &prepared)
            .unwrap()
    }

    fn compile_frame(namespace: &str, node: UiNode) -> Arc<nara_ui_kernel::UiFrame> {
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime(namespace, "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(node),
            )
            .build()
            .unwrap();
        let environment = UiEnvironment::new(
            UiSurfaceId::new(1).unwrap(),
            READBACK_EXTENT as f32,
            READBACK_EXTENT as f32,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        UiCompiler::new(bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap()
    }

    fn captured_image_snapshot() -> PreparedImageSnapshot {
        let stable_id = StableAssetId::parse_str("d7961f69-0e25-41d5-8893-0ef85ab19615").unwrap();
        let source_hash = SourceHash::from_bytes(b"wgpu image readback fixture");
        let artifact = ImportArtifactRecord::new(ImportArtifactKey::new(
            stable_id,
            source_hash,
            ImportDependencyDigest::empty(),
            ImporterId::new("nara-wgpu-test").unwrap(),
            ImporterVersion::new(1),
            ImportSettingsHash::default(),
            ImportProfile::default(),
            ImportedAssetType::new("image").unwrap(),
            ArtifactLabel::default(),
            ArtifactFormatVersion::new(1),
        ))
        .unwrap();
        let image = ImageAsset::new(
            ImageSourceMetadata::new(
                stable_id,
                AssetPath::new("ui/readback.png").unwrap(),
                source_hash,
                artifact,
            ),
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![240, 16, 120, 255],
        )
        .unwrap();
        let handle = Handle::new(AssetId::from_raw(17));
        let mut images = Assets::<ImageAsset>::default();
        assert!(images.insert(handle, image).is_none());
        let image = images.get(handle).unwrap();
        let identity = RenderResourceSnapshot::new(
            image_resource_key(handle),
            AssetVersion::from_raw(1),
            images.slot_revision(handle),
            image_descriptor_hash(image),
        );
        PreparedImageSnapshot::new(identity, PreparedImageResource::from_image(image))
    }
}
