use std::collections::{BTreeMap, BTreeSet};

use nara_image::{ImageColorSpace, ImageFormat, PreparedImageSnapshot};
use nara_material::{AddressMode, FilterMode, SamplerDescriptor};
use nara_render::{RenderResourceKey, RenderResourceKind, RenderResourceSnapshot};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot};
use thiserror::Error;

use crate::quad::{WgpuQuadMaterialKey, WgpuQuadTexture, WgpuQuadTextureKey};

const RGBA8_BYTES_PER_PIXEL: u32 = 4;
#[cfg(feature = "ui-submitter")]
const R8_BYTES_PER_PIXEL: u32 = 1;
const DEFAULT_UNUSED_GRACE_FRAMES: u64 = 2;
const MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME: usize = 64 * 1_024 * 1_024;

#[derive(Debug, Default)]
pub(crate) struct WgpuSpriteTextureCache {
    policy: WgpuTextureCachePolicy,
    stats: WgpuTextureCacheStats,
    fallback_texture: Option<WgpuTextureResource>,
    images: BTreeMap<RenderResourceKey, WgpuImageTextureResource>,
    #[cfg(feature = "ui-submitter")]
    glyph_pages: BTreeMap<UiGlyphAtlasPageKey, WgpuGlyphTextureResource>,
    bindings: BTreeMap<WgpuQuadMaterialKey, WgpuTextureBinding>,
    upload_bytes: usize,
}

impl WgpuSpriteTextureCache {
    #[must_use]
    pub(crate) fn image_count(&self) -> usize {
        self.images.len() + {
            #[cfg(feature = "ui-submitter")]
            {
                self.glyph_pages.len()
            }
            #[cfg(not(feature = "ui-submitter"))]
            {
                0
            }
        }
    }

    #[must_use]
    pub(crate) fn stats(&self) -> WgpuTextureCacheStats {
        let mut stats = self.stats;
        stats.image_textures = self.images.len();
        stats.image_bindings = self
            .bindings
            .keys()
            .filter(|material| matches!(material.texture, WgpuQuadTextureKey::Image(_)))
            .count();
        stats.fallback_bindings = self
            .bindings
            .keys()
            .filter(|material| material.texture == WgpuQuadTextureKey::Fallback)
            .count();
        #[cfg(feature = "ui-submitter")]
        {
            stats.glyph_textures = self.glyph_pages.len();
            stats.glyph_bindings = self
                .bindings
                .keys()
                .filter(|material| matches!(material.texture, WgpuQuadTextureKey::Glyph(_)))
                .count();
        }
        stats.has_fallback_texture = self.fallback_texture.is_some();
        stats.texture_bytes = self.logical_texture_bytes();
        stats
    }

    #[must_use]
    pub(crate) fn logical_texture_bytes(&self) -> u64 {
        let images = self.images.values().fold(0_u64, |total, image| {
            total.saturating_add(image.texture.logical_bytes)
        });
        #[cfg(feature = "ui-submitter")]
        let glyphs = self.glyph_pages.values().fold(0_u64, |total, page| {
            total.saturating_add(page.texture.logical_bytes)
        });
        #[cfg(not(feature = "ui-submitter"))]
        let glyphs = 0;
        images.saturating_add(glyphs).saturating_add(
            self.fallback_texture
                .as_ref()
                .map_or(0, |texture| texture.logical_bytes),
        )
    }

    pub(crate) fn clear(&mut self) {
        self.fallback_texture = None;
        self.images.clear();
        #[cfg(feature = "ui-submitter")]
        self.glyph_pages.clear();
        self.bindings.clear();
        self.stats = WgpuTextureCacheStats::default();
        self.upload_bytes = 0;
    }

    pub(crate) fn begin_frame(&mut self, frame_index: u64) {
        self.stats = WgpuTextureCacheStats::default();
        self.upload_bytes = 0;
        self.evict_unused(frame_index);
    }

    pub(crate) fn bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        material: WgpuQuadMaterialKey,
        texture: &WgpuQuadTexture,
        frame_index: u64,
    ) -> Result<wgpu::BindGroup, WgpuTextureError> {
        let actual = texture.key();
        if material.texture != actual {
            return Err(WgpuTextureError::TextureIdentityMismatch {
                expected: material.texture,
                actual,
            });
        }
        match texture {
            WgpuQuadTexture::Fallback => {
                self.ensure_fallback(device, queue)?;
            }
            WgpuQuadTexture::Image(snapshot) => {
                self.ensure_image(device, queue, snapshot, frame_index)?;
            }
            #[cfg(feature = "ui-submitter")]
            WgpuQuadTexture::Glyph(page) => {
                self.ensure_glyph_page(device, queue, page, frame_index)?;
            }
        }

        if !self.bindings.contains_key(&material) {
            let view = self.texture_view(actual)?.clone();
            self.bindings.insert(
                material,
                create_texture_binding(
                    device,
                    layout,
                    "nara_wgpu_quad_texture_binding",
                    &view,
                    material.sampler,
                    frame_index,
                ),
            );
        }
        let binding = self
            .bindings
            .get_mut(&material)
            .expect("the binding was inserted above");
        binding.last_used_frame = frame_index;
        Ok(binding.bind_group.clone())
    }

    fn ensure_fallback(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), WgpuTextureError> {
        if self.fallback_texture.is_none() {
            let required = RGBA8_BYTES_PER_PIXEL as usize;
            if self.upload_bytes.saturating_add(required) > MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME {
                return Err(WgpuTextureError::FallbackUploadDeferred {
                    required,
                    remaining: MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME.saturating_sub(self.upload_bytes),
                });
            }
            self.fallback_texture = Some(create_texture_resource(
                device,
                queue,
                "nara_wgpu_quad_fallback_texture",
                &[255, 255, 255, 255],
                1,
                1,
                RGBA8_BYTES_PER_PIXEL,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ));
            self.upload_bytes = self.upload_bytes.saturating_add(required);
        }
        Ok(())
    }

    fn ensure_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        snapshot: &PreparedImageSnapshot,
        frame_index: u64,
    ) -> Result<(), WgpuTextureError> {
        let key = snapshot.identity().key();
        if key.kind() != RenderResourceKind::IMAGE_2D {
            return Err(WgpuTextureError::WrongImageResourceKind {
                key,
                kind: key.kind().as_str(),
            });
        }
        validate_image_snapshot(snapshot)?;
        let action = match self.images.get(&key) {
            None => TextureUploadAction::Upload,
            Some(existing) if &existing.identity == snapshot.identity() => {
                TextureUploadAction::Reuse
            }
            Some(_) => TextureUploadAction::Reupload,
        };
        if action != TextureUploadAction::Reuse {
            let resource = snapshot.resource();
            let required = resource.pixels().len();
            self.admit_image_upload(key, required)?;
            let format = image_texture_format(resource.format(), resource.color_space());
            self.images.insert(
                key,
                WgpuImageTextureResource {
                    identity: snapshot.identity().clone(),
                    texture: create_texture_resource(
                        device,
                        queue,
                        "nara_wgpu_image_texture",
                        resource.pixels(),
                        resource.extent().width,
                        resource.extent().height,
                        RGBA8_BYTES_PER_PIXEL,
                        format,
                    ),
                    last_used_frame: frame_index,
                },
            );
            self.bindings
                .retain(|material, _| material.texture != WgpuQuadTextureKey::Image(key));
            match action {
                TextureUploadAction::Upload => {
                    self.stats.uploaded_images = self.stats.uploaded_images.saturating_add(1);
                }
                TextureUploadAction::Reupload => {
                    self.stats.reuploaded_images = self.stats.reuploaded_images.saturating_add(1);
                }
                TextureUploadAction::Reuse => unreachable!(),
            }
            self.upload_bytes = self.upload_bytes.saturating_add(required);
        } else {
            self.stats.reused_images = self.stats.reused_images.saturating_add(1);
        }
        self.images
            .get_mut(&key)
            .expect("the image texture exists after preparation")
            .last_used_frame = frame_index;
        Ok(())
    }

    #[cfg(feature = "ui-submitter")]
    fn ensure_glyph_page(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        page: &UiGlyphAtlasPageSnapshot,
        frame_index: u64,
    ) -> Result<(), WgpuTextureError> {
        let key = page.key();
        let expected = usize::try_from(page.width())
            .ok()
            .and_then(|width| {
                usize::try_from(page.height())
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(WgpuTextureError::InvalidGlyphPageLength {
                key,
                expected: usize::MAX,
                actual: page.pixels().len(),
            })?;
        if page.pixels().len() != expected {
            return Err(WgpuTextureError::InvalidGlyphPageLength {
                key,
                expected,
                actual: page.pixels().len(),
            });
        }
        if self.glyph_pages.contains_key(&key) {
            self.stats.reused_glyph_pages = self.stats.reused_glyph_pages.saturating_add(1);
            self.glyph_pages
                .get_mut(&key)
                .expect("the glyph page exists")
                .last_used_frame = frame_index;
            return Ok(());
        }
        if expected > MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME {
            return Err(WgpuTextureError::GlyphUploadLimitExceeded {
                key,
                required: expected,
                maximum: MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME,
            });
        }
        if self.upload_bytes.saturating_add(expected) > MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME {
            return Err(WgpuTextureError::GlyphUploadDeferred {
                key,
                required: expected,
                remaining: MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME.saturating_sub(self.upload_bytes),
            });
        }
        self.glyph_pages.insert(
            key,
            WgpuGlyphTextureResource {
                texture: create_texture_resource(
                    device,
                    queue,
                    "nara_wgpu_ui_glyph_atlas",
                    page.pixels(),
                    page.width(),
                    page.height(),
                    R8_BYTES_PER_PIXEL,
                    wgpu::TextureFormat::R8Unorm,
                ),
                last_used_frame: frame_index,
            },
        );
        self.upload_bytes = self.upload_bytes.saturating_add(expected);
        self.stats.uploaded_glyph_pages = self.stats.uploaded_glyph_pages.saturating_add(1);
        Ok(())
    }

    fn admit_image_upload(
        &self,
        key: RenderResourceKey,
        required: usize,
    ) -> Result<(), WgpuTextureError> {
        if required > MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME {
            return Err(WgpuTextureError::ImageUploadLimitExceeded {
                key,
                required,
                maximum: MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME,
            });
        }
        if self.upload_bytes.saturating_add(required) > MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME {
            return Err(WgpuTextureError::ImageUploadDeferred {
                key,
                required,
                remaining: MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME.saturating_sub(self.upload_bytes),
            });
        }
        Ok(())
    }

    fn texture_view(
        &self,
        key: WgpuQuadTextureKey,
    ) -> Result<&wgpu::TextureView, WgpuTextureError> {
        match key {
            WgpuQuadTextureKey::Fallback => self
                .fallback_texture
                .as_ref()
                .map(|texture| &texture.view)
                .ok_or(WgpuTextureError::MissingFallbackTexture),
            WgpuQuadTextureKey::Image(key) => self
                .images
                .get(&key)
                .map(|image| &image.texture.view)
                .ok_or(WgpuTextureError::MissingImageTexture { key }),
            #[cfg(feature = "ui-submitter")]
            WgpuQuadTextureKey::Glyph(key) => self
                .glyph_pages
                .get(&key)
                .map(|page| &page.texture.view)
                .ok_or(WgpuTextureError::MissingGlyphTexture { key }),
        }
    }

    fn evict_unused(&mut self, frame_index: u64) {
        let grace = self.policy.unused_grace_frames;
        let before_images = self.images.len();
        self.images
            .retain(|_, image| !unused_past_grace(frame_index, image.last_used_frame, grace));
        self.stats.evicted_image_textures = before_images.saturating_sub(self.images.len());

        #[cfg(feature = "ui-submitter")]
        {
            let before_glyphs = self.glyph_pages.len();
            self.glyph_pages
                .retain(|_, page| !unused_past_grace(frame_index, page.last_used_frame, grace));
            self.stats.evicted_glyph_textures =
                before_glyphs.saturating_sub(self.glyph_pages.len());
        }

        let live_images = self.images.keys().copied().collect::<BTreeSet<_>>();
        #[cfg(feature = "ui-submitter")]
        let live_glyphs = self.glyph_pages.keys().copied().collect::<BTreeSet<_>>();
        let before_bindings = self.bindings.len();
        self.bindings.retain(|material, binding| {
            let texture_live = match material.texture {
                WgpuQuadTextureKey::Fallback => true,
                WgpuQuadTextureKey::Image(key) => live_images.contains(&key),
                #[cfg(feature = "ui-submitter")]
                WgpuQuadTextureKey::Glyph(key) => live_glyphs.contains(&key),
            };
            texture_live && !unused_past_grace(frame_index, binding.last_used_frame, grace)
        });
        self.stats.evicted_image_bindings = before_bindings.saturating_sub(self.bindings.len());

        let has_fallback_binding = self
            .bindings
            .keys()
            .any(|material| material.texture == WgpuQuadTextureKey::Fallback);
        if self.fallback_texture.is_some() && !has_fallback_binding {
            self.fallback_texture = None;
            self.stats.evicted_fallback_textures = 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextureUploadAction {
    Upload,
    Reupload,
    Reuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgpuTextureCachePolicy {
    pub unused_grace_frames: u64,
}

impl Default for WgpuTextureCachePolicy {
    fn default() -> Self {
        Self {
            unused_grace_frames: DEFAULT_UNUSED_GRACE_FRAMES,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuTextureCacheStats {
    pub uploaded_images: u32,
    pub reuploaded_images: u32,
    pub reused_images: u32,
    pub uploaded_glyph_pages: u32,
    pub reused_glyph_pages: u32,
    pub evicted_image_textures: usize,
    pub evicted_glyph_textures: usize,
    pub evicted_image_bindings: usize,
    pub evicted_fallback_textures: u32,
    pub evicted_fallback_bindings: usize,
    pub image_textures: usize,
    pub image_bindings: usize,
    pub glyph_textures: usize,
    pub glyph_bindings: usize,
    pub fallback_bindings: usize,
    pub has_fallback_texture: bool,
    pub texture_bytes: u64,
}

#[derive(Debug)]
struct WgpuImageTextureResource {
    identity: RenderResourceSnapshot,
    texture: WgpuTextureResource,
    last_used_frame: u64,
}

#[cfg(feature = "ui-submitter")]
#[derive(Debug)]
struct WgpuGlyphTextureResource {
    texture: WgpuTextureResource,
    last_used_frame: u64,
}

#[derive(Debug)]
struct WgpuTextureResource {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    logical_bytes: u64,
}

#[derive(Debug)]
struct WgpuTextureBinding {
    bind_group: wgpu::BindGroup,
    _sampler: wgpu::Sampler,
    last_used_frame: u64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum WgpuTextureError {
    #[error("quad material texture identity mismatch: expected {expected:?}, got {actual:?}")]
    TextureIdentityMismatch {
        expected: WgpuQuadTextureKey,
        actual: WgpuQuadTextureKey,
    },
    #[error("image snapshot {key:?} has non-image resource kind {kind}")]
    WrongImageResourceKind {
        key: RenderResourceKey,
        kind: &'static str,
    },
    #[error("image snapshot {key:?} has invalid pixel length: expected {expected}, got {actual}")]
    InvalidImageLength {
        key: RenderResourceKey,
        expected: usize,
        actual: usize,
    },
    #[error("glyph page {key:?} has invalid pixel length: expected {expected}, got {actual}")]
    #[cfg(feature = "ui-submitter")]
    InvalidGlyphPageLength {
        key: UiGlyphAtlasPageKey,
        expected: usize,
        actual: usize,
    },
    #[error("fallback texture upload requires {required} bytes with only {remaining} remaining")]
    FallbackUploadDeferred { required: usize, remaining: usize },
    #[error("image {key:?} upload requires {required} bytes; per-frame maximum is {maximum}")]
    ImageUploadLimitExceeded {
        key: RenderResourceKey,
        required: usize,
        maximum: usize,
    },
    #[error("image {key:?} upload requires {required} bytes with only {remaining} remaining")]
    ImageUploadDeferred {
        key: RenderResourceKey,
        required: usize,
        remaining: usize,
    },
    #[error("glyph page {key:?} upload requires {required} bytes; per-frame maximum is {maximum}")]
    #[cfg(feature = "ui-submitter")]
    GlyphUploadLimitExceeded {
        key: UiGlyphAtlasPageKey,
        required: usize,
        maximum: usize,
    },
    #[error("glyph page {key:?} upload requires {required} bytes with only {remaining} remaining")]
    #[cfg(feature = "ui-submitter")]
    GlyphUploadDeferred {
        key: UiGlyphAtlasPageKey,
        required: usize,
        remaining: usize,
    },
    #[error("fallback texture was not prepared")]
    MissingFallbackTexture,
    #[error("image texture {key:?} was not prepared")]
    MissingImageTexture { key: RenderResourceKey },
    #[error("glyph texture {key:?} was not prepared")]
    #[cfg(feature = "ui-submitter")]
    MissingGlyphTexture { key: UiGlyphAtlasPageKey },
}

impl WgpuTextureError {
    pub(crate) const fn is_deferred(&self) -> bool {
        if matches!(
            self,
            Self::FallbackUploadDeferred { .. } | Self::ImageUploadDeferred { .. }
        ) {
            return true;
        }
        #[cfg(feature = "ui-submitter")]
        if matches!(self, Self::GlyphUploadDeferred { .. }) {
            return true;
        }
        false
    }
}

fn validate_image_snapshot(snapshot: &PreparedImageSnapshot) -> Result<(), WgpuTextureError> {
    let resource = snapshot.resource();
    let expected = usize::try_from(resource.extent().width)
        .ok()
        .and_then(|width| {
            usize::try_from(resource.extent().height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(RGBA8_BYTES_PER_PIXEL as usize))
        .ok_or(WgpuTextureError::InvalidImageLength {
            key: snapshot.identity().key(),
            expected: usize::MAX,
            actual: resource.pixels().len(),
        })?;
    if resource.pixels().len() != expected {
        return Err(WgpuTextureError::InvalidImageLength {
            key: snapshot.identity().key(),
            expected,
            actual: resource.pixels().len(),
        });
    }
    Ok(())
}

fn image_texture_format(format: ImageFormat, color_space: ImageColorSpace) -> wgpu::TextureFormat {
    match (format, color_space) {
        (ImageFormat::Rgba8, ImageColorSpace::Srgb) => wgpu::TextureFormat::Rgba8UnormSrgb,
        (ImageFormat::Rgba8, ImageColorSpace::Linear) => wgpu::TextureFormat::Rgba8Unorm,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_texture_resource(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &'static str,
    pixels: &[u8],
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
    format: wgpu::TextureFormat,
) -> WgpuTextureResource {
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
            bytes_per_row: Some(width.saturating_mul(bytes_per_pixel)),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    WgpuTextureResource {
        _texture: texture,
        view,
        logical_bytes: u64::try_from(pixels.len()).unwrap_or(u64::MAX),
    }
}

fn create_texture_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    label: &'static str,
    view: &wgpu::TextureView,
    sampler_descriptor: SamplerDescriptor,
    last_used_frame: u64,
) -> WgpuTextureBinding {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nara_wgpu_quad_sampler"),
        address_mode_u: address_mode(sampler_descriptor.address_mode_u),
        address_mode_v: address_mode(sampler_descriptor.address_mode_v),
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter_mode(sampler_descriptor.mag_filter),
        min_filter: filter_mode(sampler_descriptor.min_filter),
        mipmap_filter: mipmap_filter_mode(sampler_descriptor.mipmap_filter),
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    WgpuTextureBinding {
        bind_group,
        _sampler: sampler,
        last_used_frame,
    }
}

fn unused_past_grace(frame_index: u64, last_used_frame: u64, grace_frames: u64) -> bool {
    frame_index.saturating_sub(last_used_frame) > grace_frames
}

fn filter_mode(filter_mode: FilterMode) -> wgpu::FilterMode {
    match filter_mode {
        FilterMode::Nearest => wgpu::FilterMode::Nearest,
        FilterMode::Linear => wgpu::FilterMode::Linear,
    }
}

fn mipmap_filter_mode(filter_mode: FilterMode) -> wgpu::MipmapFilterMode {
    match filter_mode {
        FilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
        FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
    }
}

fn address_mode(address_mode: AddressMode) -> wgpu::AddressMode {
    match address_mode {
        AddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        AddressMode::Repeat => wgpu::AddressMode::Repeat,
        AddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::AssetId;

    #[test]
    fn cache_policy_retains_two_presented_epochs() {
        assert!(!unused_past_grace(3, 1, 2));
        assert!(unused_past_grace(4, 1, 2));
    }

    #[test]
    fn clearing_resets_stats_and_owned_resources() {
        let mut cache = WgpuSpriteTextureCache::default();
        cache.stats.uploaded_images = 3;

        cache.clear();

        assert_eq!(cache.stats(), WgpuTextureCacheStats::default());
        assert_eq!(cache.logical_texture_bytes(), 0);
    }

    #[test]
    fn image_upload_observes_prior_shared_upload_work() {
        let key = RenderResourceKey::new(AssetId::from_raw(7), RenderResourceKind::IMAGE_2D);
        let mut cache = WgpuSpriteTextureCache {
            upload_bytes: MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME - 4,
            ..Default::default()
        };

        assert!(matches!(
            cache.admit_image_upload(key, 8),
            Err(WgpuTextureError::ImageUploadDeferred {
                required: 8,
                remaining: 4,
                ..
            })
        ));
        assert!(matches!(
            cache.admit_image_upload(key, MAX_TEXTURE_UPLOAD_BYTES_PER_FRAME + 1),
            Err(WgpuTextureError::ImageUploadLimitExceeded { .. })
        ));

        cache.begin_frame(2);
        assert_eq!(cache.upload_bytes, 0);
        assert!(cache.admit_image_upload(key, 8).is_ok());
    }
}
