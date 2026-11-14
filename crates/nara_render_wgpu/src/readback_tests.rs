use crate::{
    output::OutputBudget,
    preparation::NativePreparation,
    quad::{
        WgpuMaterial2dLayouts, WgpuQuadBatch, WgpuQuadPipelineDrawRef,
        create_material_bind_group_layout, create_quad_batch_buffers, create_quad_pipeline,
        create_quad_texture_bind_group_layout, draw_quad_batch_buffers,
    },
    submission::{WgpuSubmissionPrefix, WgpuSubmissionReservation},
    texture::WgpuSpriteTextureCache,
};
use nara_render::ViewportRect;
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

pub(crate) const READBACK_EXTENT: u32 = 64;
pub(crate) const READBACK_BYTES_PER_PIXEL: u32 = 4;

pub(crate) fn two_image_versions() -> [nara_image::PreparedImageSnapshot; 2] {
    image_versions(1, 1)
}

pub(crate) fn image_versions(width: u32, height: u32) -> [nara_image::PreparedImageSnapshot; 2] {
    image_versions_with_handle(width, height).1
}

pub(crate) fn image_versions_with_handle(
    width: u32,
    height: u32,
) -> (
    nara_asset::Handle<nara_image::ImageAsset>,
    [nara_image::PreparedImageSnapshot; 2],
) {
    use nara_asset::{AssetEvents, AssetStates, Assets};
    use nara_image::PreparedImageSnapshot;
    let handle = nara_asset::allocate_anonymous_handle::<nara_image::ImageAsset>().unwrap();
    let mut images = Assets::<nara_image::ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let snapshots = image_assets(width, height).map(|image| {
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
    });
    (handle, snapshots)
}

#[derive(nara_material::MaterialParameters)]
pub(crate) struct TestMaterialParameters {
    #[material(id = "value")]
    pub(crate) value: nara_material::Vec4,
}

pub(crate) const TEXTURE_MATERIAL_FRAGMENT: &str = "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return textureLoad(material_texture, vec2<i32>(0, 0), 0) * material_parameters.value * tint; }";

#[derive(nara_material::MaterialParameters)]
pub(crate) struct MixedMaterialParameters {
    #[material(id = "gain")]
    pub(crate) gain: f32,
    #[material(id = "bias")]
    pub(crate) bias: nara_material::Vec3,
    #[material(id = "mode")]
    pub(crate) mode: u32,
    #[material(id = "shift")]
    pub(crate) shift: nara_material::Vec2,
}

impl Default for MixedMaterialParameters {
    fn default() -> Self {
        Self {
            gain: 0.0,
            bias: nara_material::Vec3::ZERO,
            mode: 0,
            shift: nara_material::Vec2::ZERO,
        }
    }
}

pub(crate) const MIXED_MATERIAL_FRAGMENT: &str = "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { let left = textureLoad(material_left, vec2<i32>(0, 0), 0); let right = textureLoad(material_right, vec2<i32>(0, 0), 0); if material_parameters.mode == 1u { return vec4<f32>(0.0, 0.0, 1.0, 1.0) * tint; } let color = mix(left.rgb, right.rgb, material_parameters.gain) + material_parameters.bias + vec3<f32>(material_parameters.shift, 0.0); return vec4<f32>(color, 1.0) * tint; }";

pub(crate) fn mixed_material_library(id: &str) -> nara_material::Material2dLibrary {
    let mut library = nara_material::Material2dLibrary::default();
    library
        .declare(
            nara_material::Material2dDeclaration::from_parameters::<MixedMaterialParameters>(
                id,
                1,
                nara_material::AlphaMode2d::Opaque,
                vec![
                    nara_material::MaterialTextureDeclaration::new("right", "material_right", 7)
                        .unwrap(),
                    nara_material::MaterialTextureDeclaration::new("left", "material_left", 2)
                        .unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    library
}

pub(crate) fn mixed_material_binding(
    generation: &nara_material::Material2dGeneration,
    parameters: &MixedMaterialParameters,
    left: nara_asset::Handle<nara_image::ImageAsset>,
    right: nara_asset::Handle<nara_image::ImageAsset>,
) -> nara_material::Material2dBinding {
    generation
        .bind(
            parameters,
            std::collections::BTreeMap::from([
                ("left".to_owned(), left),
                ("right".to_owned(), right),
            ]),
        )
        .unwrap()
}

pub(crate) fn material_batch(
    binding: nara_material::Material2dBinding,
    textures: Vec<(u32, nara_image::PreparedImageSnapshot)>,
) -> WgpuQuadBatch {
    let texture = crate::quad::WgpuQuadTexture::Fallback;
    WgpuQuadBatch {
        material: crate::quad::WgpuQuadMaterialKey {
            texture: texture.key(),
            sampler: nara_material::SamplerDescriptor::NEAREST_CLAMP,
            alpha_mode: binding.generation().declaration().alpha_mode(),
        },
        texture,
        scissor: None,
        instances: vec![crate::quad::WgpuQuadInstance {
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
        material_extension: Some(crate::quad::WgpuMaterial2d { binding, textures }),
    }
}

pub(crate) fn material_library(
    id: &str,
    alpha_mode: nara_material::AlphaMode2d,
) -> nara_material::Material2dLibrary {
    let mut library = nara_material::Material2dLibrary::default();
    library
        .declare(
            nara_material::Material2dDeclaration::from_parameters::<TestMaterialParameters>(
                id,
                1,
                alpha_mode,
                vec![
                    nara_material::MaterialTextureDeclaration::new(
                        "texture",
                        "material_texture",
                        1,
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    library
}

pub(crate) fn publish_material(
    library: &mut nara_material::Material2dLibrary,
    id: &str,
    fragment: &str,
) -> nara_material::Material2dGeneration {
    let path = nara_asset::AssetPath::new("native-material/fragment.wgsl").unwrap();
    let prepared = library
        .prepare_update(id, std::slice::from_ref(&path), |_| Ok(Arc::from(fragment)))
        .unwrap();
    library
        .publish(&prepared, |_| Ok(Arc::from(fragment)))
        .unwrap()
}

pub(crate) fn material_generation(
    id: &str,
    alpha_mode: nara_material::AlphaMode2d,
    fragment: &str,
) -> nara_material::Material2dGeneration {
    publish_material(&mut material_library(id, alpha_mode), id, fragment)
}

pub(crate) fn material_binding(
    generation: &nara_material::Material2dGeneration,
    parameters: [f32; 4],
    texture: nara_asset::Handle<nara_image::ImageAsset>,
) -> nara_material::Material2dBinding {
    generation
        .bind(
            &TestMaterialParameters {
                value: nara_material::Vec4::from_array(parameters),
            },
            std::collections::BTreeMap::from([("texture".to_owned(), texture)]),
        )
        .unwrap()
}

pub(crate) fn material_layouts(
    device: &wgpu::Device,
    batches: &[WgpuQuadBatch],
) -> Result<WgpuMaterial2dLayouts, crate::texture::WgpuTextureError> {
    let mut layouts = WgpuMaterial2dLayouts::new();
    for material in batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .filter_map(|batch| batch.material_extension.as_ref())
    {
        let declaration = material.binding.generation().declaration();
        if let std::collections::btree_map::Entry::Vacant(entry) =
            layouts.entry(declaration.binding_digest())
        {
            entry.insert(create_material_bind_group_layout(device, declaration)?);
        }
    }
    Ok(layouts)
}

pub(crate) fn pipeline_material_layout<'a>(
    layouts: &'a WgpuMaterial2dLayouts,
    key: &crate::quad::WgpuQuadPipelineKey,
) -> Option<&'a wgpu::BindGroupLayout> {
    layouts.get(&key.material.as_ref()?.declaration().binding_digest())
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
    let (pixels, ()) = render_to_rgba(|device, _, format, output_view, encoder, preparation| {
        assert_eq!(device.limits().max_texture_dimension_2d, 8_192);
        let texture_layout = create_quad_texture_bind_group_layout(device)?;
        let mut cache = WgpuSpriteTextureCache::default();
        cache.begin_frame(1);
        for width in [8_192, 8_193, 16_384] {
            let (handle, [snapshot, _]) = image_versions_with_handle(width, 1);
            let texture = crate::quad::WgpuQuadTextureKey::Image(snapshot.identity().key());
            let batch = image_extent_admission_batch(handle, snapshot, auxiliary);
            let layouts = material_layouts(device, std::slice::from_ref(&batch))?;
            let before = cache.stats();
            let result = create_quad_batch_buffers(
                preparation,
                &[batch],
                &texture_layout,
                &layouts,
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
        let (handle, [snapshot, _]) = image_versions_with_handle(1, 1);
        let batch = image_extent_admission_batch(handle, snapshot, auxiliary);
        let layouts = material_layouts(device, std::slice::from_ref(&batch))?;
        let pipelines = crate::required_quad_pipeline_keys(std::slice::from_ref(&batch), format)
            .into_iter()
            .map(|key| {
                let pipeline = create_quad_pipeline(
                    device,
                    key.clone(),
                    &texture_layout,
                    pipeline_material_layout(&layouts, &key),
                )?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: key.alpha_mode,
                    material: key.material.clone(),
                    pipeline: pipeline.pipeline,
                })
            })
            .collect::<Result<Vec<_>, crate::WgpuRenderError>>()?;
        let buffers = create_quad_batch_buffers(
            preparation,
            &[batch],
            &texture_layout,
            &layouts,
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
    handle: nara_asset::Handle<nara_image::ImageAsset>,
    snapshot: nara_image::PreparedImageSnapshot,
    auxiliary: bool,
) -> WgpuQuadBatch {
    use crate::quad::{WgpuMaterial2d, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture};
    use nara_material::{AlphaMode2d, SamplerDescriptor};

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
            binding: material_binding(
                &material_generation(
                    "readback.extent-admission",
                    AlphaMode2d::Opaque,
                    TEXTURE_MATERIAL_FRAGMENT,
                ),
                [1.0; 4],
                handle,
            ),
            textures: vec![(1, snapshot)],
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
        render_to_rgba(|device, _, format, output_view, encoder, preparation| {
            let texture_layout = create_quad_texture_bind_group_layout(device)?;
            let material_layouts = WgpuMaterial2dLayouts::new();
            let pipelines = crate::required_quad_pipeline_keys(&batches, format)
                .into_iter()
                .map(|key| {
                    let pipeline =
                        create_quad_pipeline(device, key.clone(), &texture_layout, None)?;
                    Ok(WgpuQuadPipelineDrawRef {
                        alpha_mode: key.alpha_mode,
                        material: key.material.clone(),
                        pipeline: pipeline.pipeline,
                    })
                })
                .collect::<Result<Vec<_>, crate::WgpuRenderError>>()?;
            let mut cache = WgpuSpriteTextureCache::default();
            cache.begin_frame(1);
            let first_buffers = create_quad_batch_buffers(
                preparation,
                &batches,
                &texture_layout,
                &material_layouts,
                &mut cache,
                1,
            )
            .unwrap();
            let first = cache.stats();
            cache.begin_frame(2);
            let repeated_buffers = create_quad_batch_buffers(
                preparation,
                &batches,
                &texture_layout,
                &material_layouts,
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
                let mut batch = image_extent_admission_batch(
                    tileset
                        .material()
                        .image
                        .expect("the retained TileSet names its Image"),
                    snapshot,
                    false,
                );
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
    render_to_rgba(|device, _, format, output_view, encoder, preparation| {
        let texture_layout = create_quad_texture_bind_group_layout(device)?;
        let layouts = material_layouts(device, batches)?;
        let pipelines = crate::required_quad_pipeline_keys(batches, format)
            .into_iter()
            .map(|key| {
                let pipeline = create_quad_pipeline(
                    device,
                    key.clone(),
                    &texture_layout,
                    pipeline_material_layout(&layouts, &key),
                )?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: key.alpha_mode,
                    material: key.material.clone(),
                    pipeline: pipeline.pipeline,
                })
            })
            .collect::<Result<Vec<_>, crate::WgpuRenderError>>()?;
        let mut texture_cache = WgpuSpriteTextureCache::default();
        texture_cache.begin_frame(1);
        let buffers = create_quad_batch_buffers(
            preparation,
            batches,
            &texture_layout,
            &layouts,
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
        &mut NativePreparation<'_>,
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
        &mut NativePreparation<'_>,
    ) -> Result<R, crate::WgpuRenderError>,
) -> Result<(Vec<u8>, R), crate::WgpuRenderError> {
    ReadbackGpu::with_limits(limits).render_to_rgba(render)
}

pub(crate) struct ReadbackGpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) budget: Arc<OutputBudget>,
}

impl ReadbackGpu {
    pub(crate) fn new() -> Self {
        Self::with_limits(wgpu::Limits::default())
    }

    pub(crate) fn with_limits(limits: wgpu::Limits) -> Self {
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
        Self {
            device,
            queue,
            budget: Arc::new(OutputBudget::default()),
        }
    }

    /// The successful closure submits its own encoder as its final Queue operation.
    /// Failed preparation still flushes earlier uploads into the same original reservation.
    pub(crate) fn with_preparation<R>(
        &self,
        prepare: impl FnOnce(
            &mut NativePreparation<'_>,
        ) -> Result<(R, wgpu::SubmissionIndex), crate::WgpuRenderError>,
    ) -> (Result<R, crate::WgpuRenderError>, WgpuSubmissionPrefix) {
        let mut reservation =
            WgpuSubmissionReservation::new(&self.device, &self.queue, Arc::clone(&self.budget));
        let result = prepare(&mut NativePreparation::new(
            &self.device,
            &self.queue,
            &self.budget,
            &mut reservation,
            &mut Default::default(),
        ));
        match result {
            Ok((facts, index)) => (Ok(facts), reservation.submitted(index)),
            Err(error) => {
                let index = self.queue.submit([]);
                (Err(error), reservation.submitted(index))
            }
        }
    }

    pub(crate) fn wait_for_prefix(&self, prefix: &WgpuSubmissionPrefix) {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: prefix.submission_index().cloned(),
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        assert!(
            prefix.is_complete(),
            "original readback custody must settle"
        );
    }

    pub(crate) fn render_to_rgba<R>(
        &self,
        render: impl FnOnce(
            &wgpu::Device,
            &wgpu::Queue,
            wgpu::TextureFormat,
            &wgpu::TextureView,
            &mut wgpu::CommandEncoder,
            &mut NativePreparation<'_>,
        ) -> Result<R, crate::WgpuRenderError>,
    ) -> Result<(Vec<u8>, R), crate::WgpuRenderError> {
        let device = &self.device;
        let queue = &self.queue;
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
        let (facts, prefix) = self.with_preparation(|preparation| {
            let facts = render(
                device,
                queue,
                format,
                &output_view,
                &mut encoder,
                preparation,
            )?;
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
            Ok((facts, queue.submit([encoder.finish()])))
        });
        let facts = match facts {
            Ok(facts) => facts,
            Err(error) => {
                self.wait_for_prefix(&prefix);
                return Err(error);
            }
        };

        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            sender.send(result).unwrap();
        });
        self.wait_for_prefix(&prefix);
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
}
