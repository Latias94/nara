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

pub(crate) fn two_image_versions() -> [nara_image::PreparedImageSnapshot; 2] {
    image_versions(1, 1)
}

pub(crate) fn image_versions(width: u32, height: u32) -> [nara_image::PreparedImageSnapshot; 2] {
    use nara_asset::{AssetEvents, AssetStates, Assets};
    use nara_image::PreparedImageSnapshot;
    let handle = nara_asset::allocate_anonymous_handle::<nara_image::ImageAsset>().unwrap();
    let mut images = Assets::<nara_image::ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    image_assets(width, height).map(|image| {
        assert!(image.product().is_none());
        assert!(image.source().is_none());
        let version = images
            .commit_loaded(handle, image, &mut states, &mut events, None, None)
            .unwrap();
        let snapshot = PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
        assert_eq!(snapshot.identity().asset_version(), version);
        assert_eq!(
            snapshot.identity().slot_revision(),
            &images.slot_revision(handle)
        );
        snapshot
    })
}

pub(crate) fn image_assets(width: u32, height: u32) -> [nara_image::ImageAsset; 2] {
    use nara_image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat};
    [[255, 0, 0, 255], [0, 255, 0, 255]].map(|color| {
        let pixels = color.repeat((width * height) as usize);
        ImageAsset::new(
            ImageExtent::new(width, height),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            pixels,
        )
        .unwrap()
    })
}

#[test]
fn primary_image_extent_admission_rejects_device_excess_and_recovers() {
    image_extent_admission_readback(false);
}

#[test]
fn auxiliary_image_extent_admission_rejects_device_excess_and_recovers() {
    image_extent_admission_readback(true);
}

fn image_extent_admission_readback(auxiliary: bool) {
    let (pixels, ()) = render_to_rgba(|device, queue, format, output_view, encoder| {
        assert_eq!(device.limits().max_texture_dimension_2d, 8_192);
        let texture_layout = create_quad_texture_bind_group_layout(device)?;
        let material_layout = create_material_bind_group_layout(device)?;
        let mut cache = WgpuSpriteTextureCache::default();
        cache.begin_frame(1);
        for width in [8_192, 8_193, 16_384] {
            let [snapshot, _] = image_versions(width, 1);
            let texture = crate::quad::WgpuQuadTextureKey::Image(snapshot.identity().key());
            let batch = image_extent_admission_batch(snapshot, auxiliary);
            let before = cache.stats();
            let result = create_quad_batch_buffers(
                device,
                queue,
                &[batch],
                &texture_layout,
                &material_layout,
                &mut cache,
                1,
            );
            if width == 8_192 {
                assert_eq!(result.unwrap().len(), 1);
            } else {
                let error = result.expect_err("CPU-valid image exceeds the requested device limit");
                assert!(
                    !error.is_deferred(),
                    "unsupported extents cannot become legal later"
                );
                assert_eq!(
                    error,
                    crate::texture::WgpuTextureError::TextureExtentUnsupported {
                        texture,
                        width,
                        height: 1,
                        maximum: 8_192,
                    },
                );
                assert_eq!(
                    cache.stats(),
                    before,
                    "rejection cannot publish or charge an image"
                );
            }
        }
        let [snapshot, _] = two_image_versions();
        let batch = image_extent_admission_batch(snapshot, auxiliary);
        let pipelines = crate::required_quad_pipeline_keys(std::slice::from_ref(&batch), format)
            .into_iter()
            .map(|key| {
                let pipeline =
                    create_quad_pipeline(device, key, &texture_layout, &material_layout)?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: key.alpha_mode,
                    material: key.material,
                    pipeline: pipeline.pipeline,
                })
            })
            .collect::<Result<Vec<_>, crate::WgpuRenderError>>()?;
        let buffers = create_quad_batch_buffers(
            device,
            queue,
            &[batch],
            &texture_layout,
            &material_layout,
            &mut cache,
            1,
        )
        .unwrap();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_image_extent_admission_recovery"),
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
        draw_quad_batch_buffers(&mut pass, &pipelines, &buffers, readback_viewport());
        Ok(())
    })
    .unwrap();
    assert_eq!(readback_pixel(&pixels, 32, 32), &[255, 0, 0, 255]);
}

fn image_extent_admission_batch(
    snapshot: nara_image::PreparedImageSnapshot,
    auxiliary: bool,
) -> WgpuQuadBatch {
    use crate::quad::{WgpuMaterial2d, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture};
    use nara_material::{AlphaMode2d, Material2dDefinition, Material2dInput, SamplerDescriptor};

    let texture = if auxiliary {
        WgpuQuadTexture::Fallback
    } else {
        WgpuQuadTexture::Image(snapshot.clone())
    };
    WgpuQuadBatch {
        material: WgpuQuadMaterialKey {
            texture: texture.key(),
            sampler: SamplerDescriptor::NEAREST_CLAMP,
            alpha_mode: AlphaMode2d::Opaque,
        },
        texture,
        scissor: None,
        instances: vec![WgpuQuadInstance {
            center: [0.0; 2],
            x_axis: [1.0, 0.0],
            y_axis: [0.0, 1.0],
            color: [1.0; 4],
            uv_min: [0.0; 2],
            uv_size: [1.0; 2],
            texture_mode: 0,
            shape_mode: 0,
            rect_size: [1.0; 2],
            corner_radius: 0.0,
            border_width: 0.0,
        }],
        counts_as_sprites: true,
        material_extension: auxiliary.then_some(WgpuMaterial2d {
            definition: Material2dDefinition {
                id: "readback.extent-admission",
                fragment_shader: "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return textureLoad(material_texture, vec2<i32>(0, 0), 0) * tint; }",
                input: Material2dInput::SPRITE_FRAGMENT_V1,
                alpha_mode: AlphaMode2d::Opaque,
            },
            parameters: [0.0; 4],
            texture: snapshot,
        }),
    }
}

#[test]
fn exact_image_versions_render_together_and_repeat_without_reupload() {
    use crate::quad::{WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture, WgpuQuadTextureKey};
    use nara_material::{AlphaMode2d, SamplerDescriptor};
    let [old, new] = two_image_versions();
    assert_eq!(old.identity().key(), new.identity().key());
    assert_ne!(old.identity(), new.identity());
    assert_ne!(
        old.identity().slot_revision(),
        new.identity().slot_revision()
    );
    assert_ne!(old.identity().content_hash(), new.identity().content_hash());
    assert_eq!(
        old.identity().asset_version(),
        nara_asset::AssetVersion::from_raw(1)
    );
    assert_eq!(
        new.identity().asset_version(),
        nara_asset::AssetVersion::from_raw(2)
    );
    assert_eq!(old.resource().pixels(), &[255, 0, 0, 255]);
    assert_eq!(new.resource().pixels(), &[0, 255, 0, 255]);
    let batches = [old.clone(), new, old]
        .into_iter()
        .enumerate()
        .map(|(index, snapshot)| WgpuQuadBatch {
            material: WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Image(snapshot.identity().key()),
                sampler: SamplerDescriptor::NEAREST_CLAMP,
                alpha_mode: AlphaMode2d::Opaque,
            },
            texture: WgpuQuadTexture::Image(snapshot),
            scissor: None,
            instances: vec![WgpuQuadInstance {
                center: [-2.0 / 3.0 + index as f32 * 2.0 / 3.0, 0.0],
                x_axis: [1.0 / 3.0, 0.0],
                y_axis: [0.0, 1.0],
                color: [1.0; 4],
                uv_min: [0.0; 2],
                uv_size: [1.0; 2],
                texture_mode: 0,
                shape_mode: 0,
                rect_size: [1.0; 2],
                corner_radius: 0.0,
                border_width: 0.0,
            }],
            counts_as_sprites: true,
            material_extension: None,
        })
        .collect::<Vec<_>>();
    let (pixels, (first, repeated)) =
        render_to_rgba(|device, queue, format, output_view, encoder| {
            let texture_layout = create_quad_texture_bind_group_layout(device)?;
            let material_layout = create_material_bind_group_layout(device)?;
            let pipelines = crate::required_quad_pipeline_keys(&batches, format)
                .into_iter()
                .map(|key| {
                    let pipeline =
                        create_quad_pipeline(device, key, &texture_layout, &material_layout)?;
                    Ok(WgpuQuadPipelineDrawRef {
                        alpha_mode: key.alpha_mode,
                        material: key.material,
                        pipeline: pipeline.pipeline,
                    })
                })
                .collect::<Result<Vec<_>, crate::WgpuRenderError>>()?;
            let mut cache = WgpuSpriteTextureCache::default();
            cache.begin_frame(1);
            let first_buffers = create_quad_batch_buffers(
                device,
                queue,
                &batches,
                &texture_layout,
                &material_layout,
                &mut cache,
                1,
            )
            .unwrap();
            let first = cache.stats();
            cache.begin_frame(2);
            let repeated_buffers = create_quad_batch_buffers(
                device,
                queue,
                &batches,
                &texture_layout,
                &material_layout,
                &mut cache,
                2,
            )
            .unwrap();
            let repeated = cache.stats();
            for buffers in [&first_buffers, &repeated_buffers] {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("nara_exact_version_readback"),
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
                draw_quad_batch_buffers(&mut pass, &pipelines, buffers, readback_viewport());
            }
            Ok((first, repeated))
        })
        .unwrap();
    assert_eq!(readback_pixel(&pixels, 10, 32), &[255, 0, 0, 255]);
    assert_eq!(readback_pixel(&pixels, 32, 32), &[0, 255, 0, 255]);
    assert_eq!(readback_pixel(&pixels, 53, 32), &[255, 0, 0, 255]);
    assert_eq!(first.image_textures, 2);
    assert_eq!(first.uploaded_images + first.reuploaded_images, 2);
    assert_eq!(repeated.uploaded_images + repeated.reuploaded_images, 0);
    assert_eq!(repeated.reused_images, 3);
}

#[test]
fn processed_tileset_captures_render_exact_versions_after_native_retirement() {
    use nara_asset::{
        AssetEvents, AssetProductRef, AssetServer, AssetStates, Assets, ImportedProductId,
        RuntimeAssetCodec, StableAssetId,
    };
    use nara_core::Vec2;
    use nara_image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageRuntimeCodec};
    use nara_material::{AlphaMode2d, SamplerDescriptor};
    use nara_tilemap::{
        TileAtlasLayout, TileIndex, TileSet, TileSetAssets, TileSetDescriptor,
        TileSetPublicationInput, TileSetRuntimeCodec,
    };
    use std::sync::Arc;

    for selection in [
        ImportedProductId::PRIMARY,
        ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap(),
    ] {
        let product = |uuid| {
            AssetProductRef::new(StableAssetId::parse_str(uuid).unwrap(), selection).unwrap()
        };
        let descriptor = TileSetDescriptor::new(
            product("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f"),
            product("b73f0f16-09e8-4265-b090-b689b41c197e"),
            TileAtlasLayout::grid(Vec2::ONE, 2, 1),
        )
        .unwrap()
        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
        .with_alpha_mode(AlphaMode2d::Opaque);
        let decoded = TileSetRuntimeCodec::decode_runtime(
            &TileSetRuntimeCodec::encode_runtime(&descriptor).unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, descriptor);
        let captures = {
            let mut server = AssetServer::new();
            let handle = server
                .reserve_product::<TileSet>(decoded.product())
                .unwrap();
            let image_handle = server
                .reserve_product::<ImageAsset>(decoded.image_product())
                .unwrap();
            let mut images = Assets::default();
            let mut tilesets = Assets::default();
            let mut states = AssetStates::default();
            let mut events = AssetEvents::default();
            let mut owner = TileSetAssets::default();
            let mut receipt = None;
            let captures = [
                [0, 0, 255, 255, 255, 0, 0, 255],
                [255, 0, 255, 255, 0, 255, 0, 255],
            ]
            .map(|pixels| {
                let image = ImageAsset::from_product(
                    decoded.image_product(),
                    ImageExtent::new(2, 1),
                    ImageFormat::Rgba8,
                    ImageColorSpace::Srgb,
                    pixels.to_vec(),
                )
                .unwrap();
                let acquisition = ImageRuntimeCodec::decode_runtime(
                    &ImageRuntimeCodec::encode_runtime(&image).unwrap(),
                )
                .unwrap();
                let acquisition = Arc::new(acquisition);
                // Concrete Arc custody models acquisition here; base and decoded-output cost
                // custody are qualified separately by the independent consumer.
                let image = acquisition.share_with_retention_owner(Arc::clone(&acquisition));
                let mut input = TileSetPublicationInput::from_product(
                    handle,
                    decoded,
                    image_handle,
                    image.share_retained().unwrap(),
                );
                if let Some(previous) = receipt.take() {
                    input = input.with_receipt(previous);
                }
                let prepared = owner
                    .prepare_publications(vec![input], &[], &server, None, &tilesets, &states)
                    .unwrap();
                images
                    .commit_loaded(image_handle, image, &mut states, &mut events, None, None)
                    .unwrap();
                receipt = prepared
                    .commit(&server, &images, &mut tilesets, &mut states, &mut events)
                    .pop();
                let tileset = tilesets.get(handle).unwrap().clone();
                let content = tileset.image_content().unwrap();
                assert_eq!(content.product(), decoded.image_product());
                assert!(content.source().is_none());
                assert!(std::ptr::eq(
                    content.pixels(),
                    images.get(image_handle).unwrap().pixels()
                ));
                let snapshot = content.prepared_snapshot();
                assert!(std::ptr::eq(content.pixels(), snapshot.resource().pixels()));
                (tileset, snapshot, Arc::downgrade(&acquisition))
            });
            owner
                .prepare_publications(
                    Vec::new(),
                    &[receipt.unwrap()],
                    &server,
                    None,
                    &tilesets,
                    &states,
                )
                .unwrap()
                .commit(&server, &images, &mut tilesets, &mut states, &mut events);
            assert!(
                images
                    .remove_with_state(image_handle, &mut states, &mut events)
                    .unwrap()
                    .is_some()
            );
            assert!(owner.is_empty() && tilesets.is_empty() && images.is_empty());
            captures
        };
        let [
            (old, old_prepared, old_owner),
            (new, new_prepared, new_owner),
        ] = captures;
        assert_eq!(old_prepared.identity().key(), new_prepared.identity().key());
        assert_ne!(old_prepared.identity(), new_prepared.identity());
        let batches = [(old, old_prepared), (new, new_prepared)]
            .into_iter()
            .enumerate()
            .map(|(index, (tileset, snapshot))| {
                let region = tileset.normalized_region(TileIndex(1)).unwrap();
                assert_eq!(region.min, Vec2::new(0.5, 0.0));
                let mut batch = image_extent_admission_batch(snapshot, false);
                batch.material.sampler = tileset.material().sampler;
                batch.material.alpha_mode = tileset.material().alpha_mode;
                batch.instances[0].center = [-0.5 + index as f32, 0.0];
                batch.instances[0].x_axis = [0.5, 0.0];
                batch.instances[0].uv_min = region.min.to_array();
                batch.instances[0].uv_size = region.size.to_array();
                batch
            })
            .collect::<Vec<_>>();
        // Only the prepared captures remain: native stores, owner and TileSet copies are gone.
        assert!(old_owner.upgrade().is_some() && new_owner.upgrade().is_some());
        let pixels = render_batches_to_rgba(&batches).unwrap();
        for y in 0..READBACK_EXTENT {
            for x in 0..READBACK_EXTENT {
                let expected = if x < READBACK_EXTENT / 2 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                assert_eq!(
                    readback_pixel(&pixels, x, y),
                    &expected,
                    "{selection:?} at ({x}, {y})"
                );
            }
        }
        drop(batches);
        assert!(old_owner.upgrade().is_none() && new_owner.upgrade().is_none());
    }
}

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
        let texture_layout = create_quad_texture_bind_group_layout(&device)?;
        let material_layout = create_material_bind_group_layout(&device)?;
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
    render_to_rgba_with_limits(wgpu::Limits::default(), render)
}

pub(crate) fn render_to_rgba_with_limits<R>(
    limits: wgpu::Limits,
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
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))
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
