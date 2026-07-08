use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{Assets, Handle};
use nara_image::{
    ImageAddressMode, ImageAsset, ImageColorSpace, ImageFilterMode, ImageFormat,
    ImageSamplerDescriptor, PreparedImageResource,
};
use nara_render::{
    PreparedRenderResources, RenderPrepareStatus, RenderResourceKey, RenderResourceKind,
    RenderResourceSnapshot,
};
use nara_sprite_render::SpriteBatch;
use thiserror::Error;

const RGBA8_BYTES_PER_PIXEL: u32 = 4;

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

    pub(crate) fn fallback_bind_group(
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
    pub(crate) fn image_bind_group(
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

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::AssetId;

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
}
