use std::collections::HashMap;

use crate::gpu::{
    RenderPassContext, WgpuRenderError,
    pass::{sampler_layout_entry, texture_layout_entry},
};
use crate::image::{ImageColorSpace, ImageFormat, PreparedImageSnapshot};
use crate::material::{AddressMode, FilterMode, SamplerDescriptor};
use crate::{RenderResourceContentHash, RenderResourceKey};

const MAX_CACHED_IMAGES: usize = 1024;
const IMAGE_GRACE_FRAMES: u64 = 2;

type ImageKey = Option<(RenderResourceKey, RenderResourceContentHash)>;

pub(super) struct SpriteTextureCache {
    pub(super) layout: wgpu::BindGroupLayout,
    images: HashMap<ImageKey, SampledImage>,
    samplers: HashMap<SamplerDescriptor, wgpu::Sampler>,
}

struct SampledImage {
    view: wgpu::TextureView,
    bindings: HashMap<SamplerDescriptor, wgpu::BindGroup>,
    used_at: u64,
}

impl SpriteTextureCache {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nara_scene_texture"),
            entries: &[texture_layout_entry(0), sampler_layout_entry(1)],
        });
        Self {
            layout,
            images: HashMap::new(),
            samplers: HashMap::new(),
        }
    }

    pub(super) fn prune(&mut self, frame: u64) {
        self.images
            .retain(|_, image| frame.saturating_sub(image.used_at) <= IMAGE_GRACE_FRAMES);
    }

    pub(super) fn view(
        &mut self,
        ctx: &mut RenderPassContext<'_>,
        snapshot: &PreparedImageSnapshot,
        frame: u64,
    ) -> Result<wgpu::TextureView, WgpuRenderError> {
        Ok(resolve_image(ctx, &mut self.images, Some(snapshot), frame)?
            .view
            .clone())
    }

    pub(super) fn bind(
        &mut self,
        ctx: &mut RenderPassContext<'_>,
        snapshot: Option<&PreparedImageSnapshot>,
        sampler: SamplerDescriptor,
        frame: u64,
    ) -> Result<wgpu::BindGroup, WgpuRenderError> {
        let image = resolve_image(ctx, &mut self.images, snapshot, frame)?;
        let binding = image.bindings.entry(sampler).or_insert_with(|| {
            let sampler = self
                .samplers
                .entry(sampler)
                .or_insert_with(|| create_sampler(ctx.device, sampler));
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nara_scene_texture"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&image.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        });
        Ok(binding.clone())
    }
}

fn resolve_image<'a>(
    ctx: &mut RenderPassContext<'_>,
    images: &'a mut HashMap<ImageKey, SampledImage>,
    snapshot: Option<&PreparedImageSnapshot>,
    frame: u64,
) -> Result<&'a mut SampledImage, WgpuRenderError> {
    let key = snapshot.map(|image| (image.identity().key(), image.identity().content_hash()));
    if !images.contains_key(&key) {
        let view = upload_image(ctx, snapshot)?;
        if images.len() == MAX_CACHED_IMAGES
            && let Some(oldest) = images
                .iter()
                .min_by_key(|(_, image)| image.used_at)
                .map(|(key, _)| *key)
        {
            images.remove(&oldest);
        }
        images.insert(
            key,
            SampledImage {
                view,
                bindings: HashMap::new(),
                used_at: frame,
            },
        );
    }
    let image = images
        .get_mut(&key)
        .ok_or(WgpuRenderError::Capacity("scene texture cache"))?;
    image.used_at = frame;
    Ok(image)
}

fn upload_image(
    ctx: &mut RenderPassContext<'_>,
    snapshot: Option<&PreparedImageSnapshot>,
) -> Result<wgpu::TextureView, WgpuRenderError> {
    let (width, height, format, pixels) = match snapshot {
        Some(image) => {
            let resource = image.resource();
            let extent = resource.extent();
            let format = match (resource.format(), resource.color_space()) {
                (ImageFormat::Rgba8, ImageColorSpace::Linear) => wgpu::TextureFormat::Rgba8Unorm,
                (ImageFormat::Rgba8, ImageColorSpace::Srgb) => wgpu::TextureFormat::Rgba8UnormSrgb,
            };
            (extent.width, extent.height, format, resource.pixels())
        }
        None => (1, 1, wgpu::TextureFormat::Rgba8Unorm, &[255_u8; 4][..]),
    };
    let maximum = ctx.device.limits().max_texture_dimension_2d;
    let row_bytes = width
        .checked_mul(4)
        .filter(|_| width != 0 && height != 0 && width <= maximum && height <= maximum)
        .ok_or(WgpuRenderError::Capacity(
            "scene texture exceeds the device extent limit",
        ))?;
    if u64::from(row_bytes) * u64::from(height) != pixels.len() as u64 {
        return Err(WgpuRenderError::Validation(
            "scene image has an invalid RGBA payload".into(),
        ));
    }
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = ctx.create_sampled_texture("nara_sampled_scene_image", size, format);
    ctx.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(row_bytes),
            rows_per_image: Some(height),
        },
        size,
    );
    Ok(texture.create_view(&wgpu::TextureViewDescriptor::default()))
}

fn create_sampler(device: &wgpu::Device, sampler: SamplerDescriptor) -> wgpu::Sampler {
    let filter = |mode| match mode {
        FilterMode::Nearest => wgpu::FilterMode::Nearest,
        FilterMode::Linear => wgpu::FilterMode::Linear,
    };
    let address = |mode| match mode {
        AddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        AddressMode::Repeat => wgpu::AddressMode::Repeat,
        AddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nara_scene_sampler"),
        address_mode_u: address(sampler.address_mode_u),
        address_mode_v: address(sampler.address_mode_v),
        mag_filter: filter(sampler.mag_filter),
        min_filter: filter(sampler.min_filter),
        mipmap_filter: match sampler.mipmap_filter {
            FilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
            FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
        },
        ..Default::default()
    })
}
