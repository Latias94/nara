use std::collections::{BTreeMap, BTreeSet};

use bytemuck::{Pod, Zeroable};
use nara_asset::{Assets, Handle};
use nara_image::{
    ImageAddressMode, ImageAsset, ImageColorSpace, ImageFilterMode, ImageFormat,
    ImageSamplerDescriptor, PreparedImageResource,
};
use nara_render::{
    PreparedRenderResources, RenderPrepareStatus, RenderResourceKey, RenderResourceKind,
    RenderResourceSnapshot,
};
use nara_sprite_render::{SpriteBatch, SpriteInstance};
use thiserror::Error;
use wgpu::util::DeviceExt;

const VERTICES_PER_QUAD: u32 = 6;
const RGBA8_BYTES_PER_PIXEL: u32 = 4;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
struct WgpuSpriteInstance {
    center: [f32; 2],
    x_axis: [f32; 2],
    y_axis: [f32; 2],
    color: [f32; 4],
    uv_min: [f32; 2],
    uv_size: [f32; 2],
}

const SPRITE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x2,
    3 => Float32x4,
    4 => Float32x2,
    5 => Float32x2
];

#[derive(Debug, Clone)]
pub(crate) struct WgpuSpritePipeline {
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) texture_bind_group_layout: wgpu::BindGroupLayout,
}

#[derive(Debug)]
pub(crate) struct WgpuSpriteBatchBuffer {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_count: u32,
}

#[derive(Debug, Default)]
pub(crate) struct WgpuSpriteTextureCache {
    fallback: Option<WgpuSpriteTextureBinding>,
    images: BTreeMap<RenderResourceKey, WgpuSpriteImageBinding>,
}

impl WgpuSpriteTextureCache {
    #[must_use]
    pub(crate) fn image_count(&self) -> usize {
        self.images.len()
    }

    pub(crate) fn clear(&mut self) {
        self.fallback = None;
        self.images.clear();
    }

    pub(crate) fn prune_unused(&mut self, batches: &[SpriteBatch]) {
        let used = batches
            .iter()
            .filter_map(|batch| batch.texture)
            .collect::<BTreeSet<_>>();
        self.images.retain(|key, _| used.contains(key));
    }

    fn fallback_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        if self.fallback.is_none() {
            self.fallback = Some(create_texture_binding(
                device,
                queue,
                layout,
                "nara_wgpu_sprite_fallback_texture",
                &[255, 255, 255, 255],
                1,
                1,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                ImageSamplerDescriptor::default(),
            ));
        }

        self.fallback.as_ref().unwrap().bind_group.clone()
    }

    #[allow(clippy::too_many_arguments)]
    fn image_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        key: RenderResourceKey,
        images: &Assets<ImageAsset>,
        prepared_images: &PreparedRenderResources<PreparedImageResource>,
    ) -> Result<wgpu::BindGroup, WgpuSpriteTextureError> {
        if key.kind() != RenderResourceKind::IMAGE_2D {
            return Err(WgpuSpriteTextureError::WrongResourceKind {
                key,
                kind: key.kind().as_str(),
            });
        }

        let record = prepared_images
            .get(key)
            .ok_or(WgpuSpriteTextureError::MissingPreparedResource { key })?;
        let snapshot = record.snapshot();
        let prepared = match record.status() {
            RenderPrepareStatus::Ready => record
                .resource()
                .ok_or(WgpuSpriteTextureError::MissingPreparedResource { key })?,
            RenderPrepareStatus::Failed(error) => {
                return Err(WgpuSpriteTextureError::FailedPreparedResource {
                    key,
                    message: error.message().to_string(),
                });
            }
        };

        if let Some(existing) = self.images.get(&key)
            && existing.snapshot == snapshot
        {
            return Ok(existing.binding.bind_group.clone());
        }

        let handle = Handle::<ImageAsset>::new(key.asset_id());
        let image = images
            .get(handle)
            .ok_or(WgpuSpriteTextureError::MissingImageAsset { key })?;
        validate_prepared_image_matches_asset(key, image, prepared)?;
        let binding = create_image_texture_binding(device, queue, layout, key, image)?;
        self.images
            .insert(key, WgpuSpriteImageBinding { snapshot, binding });

        Ok(self.images.get(&key).unwrap().binding.bind_group.clone())
    }
}

#[derive(Debug)]
struct WgpuSpriteImageBinding {
    snapshot: RenderResourceSnapshot,
    binding: WgpuSpriteTextureBinding,
}

#[derive(Debug)]
struct WgpuSpriteTextureBinding {
    bind_group: wgpu::BindGroup,
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    _sampler: wgpu::Sampler,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum WgpuSpriteTextureError {
    #[error("sprite batch referenced non-image render resource {key:?} of kind {kind}")]
    WrongResourceKind {
        key: RenderResourceKey,
        kind: &'static str,
    },
    #[error("sprite texture {key:?} has no prepared image resource")]
    MissingPreparedResource { key: RenderResourceKey },
    #[error("sprite texture {key:?} prepare failed: {message}")]
    FailedPreparedResource {
        key: RenderResourceKey,
        message: String,
    },
    #[error("sprite texture {key:?} has no loaded image asset")]
    MissingImageAsset { key: RenderResourceKey },
    #[error(
        "sprite texture {key:?} has invalid pixel data length: expected {expected} bytes, got {actual}"
    )]
    InvalidPixelDataLength {
        key: RenderResourceKey,
        expected: usize,
        actual: usize,
    },
    #[error("sprite texture {key:?} prepared extent does not match image asset")]
    PreparedExtentMismatch { key: RenderResourceKey },
    #[error("sprite texture {key:?} prepared format does not match image asset")]
    PreparedFormatMismatch { key: RenderResourceKey },
    #[error("sprite texture {key:?} prepared color space does not match image asset")]
    PreparedColorSpaceMismatch { key: RenderResourceKey },
    #[error("sprite texture {key:?} prepared sampler does not match image asset")]
    PreparedSamplerMismatch { key: RenderResourceKey },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WgpuSpriteDrawStats {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
}

pub(crate) fn create_sprite_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> WgpuSpritePipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("sprite.wgsl"));
    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nara_wgpu_sprite_texture_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nara_wgpu_sprite_pipeline_layout"),
        bind_group_layouts: &[Some(&texture_bind_group_layout)],
        immediate_size: 0,
    });
    let vertex_buffer_layout = sprite_instance_buffer_layout();
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nara_wgpu_sprite_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertex_buffer_layout)],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });

    WgpuSpritePipeline {
        format,
        pipeline,
        texture_bind_group_layout,
    }
}

pub(crate) fn create_sprite_batch_buffers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    batches: &[&SpriteBatch],
    texture_layout: &wgpu::BindGroupLayout,
    texture_cache: &mut WgpuSpriteTextureCache,
    images: Option<&Assets<ImageAsset>>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
) -> Result<Vec<WgpuSpriteBatchBuffer>, WgpuSpriteTextureError> {
    batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| {
            let bind_group = match batch.texture {
                Some(key) => texture_cache.image_bind_group(
                    device,
                    queue,
                    texture_layout,
                    key,
                    images.ok_or(WgpuSpriteTextureError::MissingImageAsset { key })?,
                    prepared_images
                        .ok_or(WgpuSpriteTextureError::MissingPreparedResource { key })?,
                )?,
                None => texture_cache.fallback_bind_group(device, queue, texture_layout),
            };
            let instances = pack_sprite_instances(&batch.instances);
            let instance_count = saturating_u32(instances.len());
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("nara_wgpu_sprite_instances"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            });

            Ok(WgpuSpriteBatchBuffer {
                buffer,
                bind_group,
                instance_count,
            })
        })
        .collect()
}

pub(crate) fn draw_sprite_batch_buffers<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    pipeline: &'pass wgpu::RenderPipeline,
    buffers: &'pass [WgpuSpriteBatchBuffer],
) {
    if buffers.is_empty() {
        return;
    }

    render_pass.set_pipeline(pipeline);
    for buffer in buffers {
        render_pass.set_bind_group(0, &buffer.bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffer.buffer.slice(..));
        render_pass.draw(0..VERTICES_PER_QUAD, 0..buffer.instance_count);
    }
}

pub(crate) fn sprite_batch_draw_stats(batches: &[&SpriteBatch]) -> WgpuSpriteDrawStats {
    let mut stats = WgpuSpriteDrawStats::default();
    for batch in batches {
        let instances = saturating_u32(batch.instances.len());
        if instances == 0 {
            continue;
        }

        stats.draw_calls = stats.draw_calls.saturating_add(1);
        stats.sprites = stats.sprites.saturating_add(instances);
    }
    stats
}

fn sprite_instance_buffer_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<WgpuSpriteInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &SPRITE_INSTANCE_ATTRIBUTES,
    }
}

fn pack_sprite_instances(instances: &[SpriteInstance]) -> Vec<WgpuSpriteInstance> {
    instances
        .iter()
        .map(|instance| WgpuSpriteInstance {
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
        })
        .collect()
}

fn create_image_texture_binding(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    key: RenderResourceKey,
    image: &ImageAsset,
) -> Result<WgpuSpriteTextureBinding, WgpuSpriteTextureError> {
    let format = texture_format(key, image.format(), image.color_space())?;
    let extent = image.extent();
    let expected = extent
        .pixel_count()
        .checked_mul(u64::from(RGBA8_BYTES_PER_PIXEL))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .unwrap_or(usize::MAX);
    let actual = image.pixels().len();
    if actual != expected {
        return Err(WgpuSpriteTextureError::InvalidPixelDataLength {
            key,
            expected,
            actual,
        });
    }

    Ok(create_texture_binding(
        device,
        queue,
        layout,
        "nara_wgpu_sprite_image_texture",
        image.pixels(),
        extent.width,
        extent.height,
        format,
        image.sampler(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn create_texture_binding(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    label: &'static str,
    pixels: &[u8],
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    sampler_descriptor: ImageSamplerDescriptor,
) -> WgpuSpriteTextureBinding {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * RGBA8_BYTES_PER_PIXEL),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nara_wgpu_sprite_sampler"),
        address_mode_u: address_mode(sampler_descriptor.address_mode_u),
        address_mode_v: address_mode(sampler_descriptor.address_mode_v),
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter_mode(sampler_descriptor.mag_filter),
        min_filter: filter_mode(sampler_descriptor.min_filter),
        mipmap_filter: mipmap_filter_mode(sampler_descriptor.mipmap_filter),
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("nara_wgpu_sprite_texture_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    WgpuSpriteTextureBinding {
        bind_group,
        _texture: texture,
        _view: view,
        _sampler: sampler,
    }
}

fn validate_prepared_image_matches_asset(
    key: RenderResourceKey,
    image: &ImageAsset,
    prepared: &PreparedImageResource,
) -> Result<(), WgpuSpriteTextureError> {
    if prepared.extent() != image.extent() {
        return Err(WgpuSpriteTextureError::PreparedExtentMismatch { key });
    }
    if prepared.format() != image.format() {
        return Err(WgpuSpriteTextureError::PreparedFormatMismatch { key });
    }
    if prepared.color_space() != image.color_space() {
        return Err(WgpuSpriteTextureError::PreparedColorSpaceMismatch { key });
    }
    if prepared.sampler() != image.sampler() {
        return Err(WgpuSpriteTextureError::PreparedSamplerMismatch { key });
    }
    Ok(())
}

fn texture_format(
    _key: RenderResourceKey,
    format: ImageFormat,
    color_space: ImageColorSpace,
) -> Result<wgpu::TextureFormat, WgpuSpriteTextureError> {
    Ok(match (format, color_space) {
        (ImageFormat::Rgba8, ImageColorSpace::Srgb) => wgpu::TextureFormat::Rgba8UnormSrgb,
        (ImageFormat::Rgba8, ImageColorSpace::Linear) => wgpu::TextureFormat::Rgba8Unorm,
    })
}

fn filter_mode(filter_mode: ImageFilterMode) -> wgpu::FilterMode {
    match filter_mode {
        ImageFilterMode::Nearest => wgpu::FilterMode::Nearest,
        ImageFilterMode::Linear => wgpu::FilterMode::Linear,
    }
}

fn mipmap_filter_mode(filter_mode: ImageFilterMode) -> wgpu::MipmapFilterMode {
    match filter_mode {
        ImageFilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
        ImageFilterMode::Linear => wgpu::MipmapFilterMode::Linear,
    }
}

fn address_mode(address_mode: ImageAddressMode) -> wgpu::AddressMode {
    match address_mode {
        ImageAddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        ImageAddressMode::Repeat => wgpu::AddressMode::Repeat,
        ImageAddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::AssetId;
    use nara_core::{Color, Vec2};
    use nara_render::{RenderPhaseLabel, RenderTarget};
    use nara_sprite_render::TextureUvRect;

    fn batch(instances: Vec<SpriteInstance>) -> SpriteBatch {
        SpriteBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 0,
            sort_key: 0,
            texture: None,
            instances,
        }
    }

    #[test]
    fn instance_packing_preserves_center_axes_color_and_uv_order() {
        let packed = pack_sprite_instances(&[SpriteInstance {
            center: Vec2::new(0.25, -0.5),
            x_axis: Vec2::new(0.1, 0.2),
            y_axis: Vec2::new(-0.3, 0.4),
            color: Color::rgba(0.3, 0.4, 0.5, 0.6),
            uv: TextureUvRect::new(Vec2::new(0.25, 0.5), Vec2::new(0.5, 0.25)),
        }]);

        assert_eq!(
            packed,
            vec![WgpuSpriteInstance {
                center: [0.25, -0.5],
                x_axis: [0.1, 0.2],
                y_axis: [-0.3, 0.4],
                color: [0.3, 0.4, 0.5, 0.6],
                uv_min: [0.25, 0.5],
                uv_size: [0.5, 0.25],
            }]
        );
    }

    #[test]
    fn texture_format_matches_image_color_space() {
        let key = RenderResourceKey::new(AssetId::from_raw(1), RenderResourceKind::IMAGE_2D);

        assert_eq!(
            texture_format(key, ImageFormat::Rgba8, ImageColorSpace::Srgb),
            Ok(wgpu::TextureFormat::Rgba8UnormSrgb)
        );
        assert_eq!(
            texture_format(key, ImageFormat::Rgba8, ImageColorSpace::Linear),
            Ok(wgpu::TextureFormat::Rgba8Unorm)
        );
    }

    #[test]
    fn sampler_modes_map_to_wgpu_modes() {
        assert_eq!(
            filter_mode(ImageFilterMode::Nearest),
            wgpu::FilterMode::Nearest
        );
        assert_eq!(
            filter_mode(ImageFilterMode::Linear),
            wgpu::FilterMode::Linear
        );
        assert_eq!(
            mipmap_filter_mode(ImageFilterMode::Nearest),
            wgpu::MipmapFilterMode::Nearest
        );
        assert_eq!(
            mipmap_filter_mode(ImageFilterMode::Linear),
            wgpu::MipmapFilterMode::Linear
        );
        assert_eq!(
            address_mode(ImageAddressMode::ClampToEdge),
            wgpu::AddressMode::ClampToEdge
        );
        assert_eq!(
            address_mode(ImageAddressMode::Repeat),
            wgpu::AddressMode::Repeat
        );
        assert_eq!(
            address_mode(ImageAddressMode::MirrorRepeat),
            wgpu::AddressMode::MirrorRepeat
        );
    }

    #[test]
    fn sprite_draw_stats_ignore_empty_batches() {
        let sprite = SpriteInstance::axis_aligned(Vec2::ZERO, Vec2::splat(0.5), Color::WHITE);
        let empty = batch(Vec::new());
        let one = batch(vec![sprite]);
        let two = batch(vec![sprite, sprite]);

        assert_eq!(
            sprite_batch_draw_stats(&[&empty, &one, &two]),
            WgpuSpriteDrawStats {
                draw_calls: 2,
                sprites: 3,
            }
        );
    }

    #[test]
    fn sprite_instance_layout_matches_shader_contract() {
        let layout = sprite_instance_buffer_layout();

        assert_eq!(
            layout.array_stride,
            std::mem::size_of::<WgpuSpriteInstance>() as wgpu::BufferAddress
        );
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
        assert_eq!(layout.attributes.len(), 6);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[1].offset, 8);
        assert_eq!(layout.attributes[2].offset, 16);
        assert_eq!(layout.attributes[3].offset, 24);
        assert_eq!(layout.attributes[4].offset, 40);
        assert_eq!(layout.attributes[5].offset, 48);
    }
}
