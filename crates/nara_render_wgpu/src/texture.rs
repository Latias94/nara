#[cfg(feature = "ui-submitter")]
use std::collections::BTreeSet;
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

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
const MAX_RESIDENT_IMAGE_VERSIONS: usize = 1_024;
const MAX_RESIDENT_IMAGE_BYTES: u64 = 256 * 1_024 * 1_024;

#[derive(Debug, Default)]
pub(crate) struct WgpuSpriteTextureCache {
    policy: WgpuTextureCachePolicy,
    stats: WgpuTextureCacheStats,
    fallback_texture: Option<WgpuTextureResource>,
    images: BTreeMap<RenderResourceKey, Vec<WgpuImageTextureEntry>>,
    resident_images: Vec<ResidentImageTexture>,
    residency_limits: WgpuImageResidencyLimits,
    #[cfg(feature = "ui-submitter")]
    glyph_pages: BTreeMap<UiGlyphAtlasPageKey, WgpuGlyphTextureResource>,
    bindings: BTreeMap<WgpuQuadMaterialKey, Vec<WgpuTextureBinding>>,
    upload_bytes: usize,
}

impl WgpuSpriteTextureCache {
    #[must_use]
    pub(crate) fn image_count(&self) -> usize {
        self.resident_image_count() + {
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
        stats.image_textures = self.resident_image_count();
        stats.image_bindings = self
            .bindings
            .iter()
            .filter(|(material, _)| matches!(material.texture, WgpuQuadTextureKey::Image(_)))
            .map(|(_, bindings)| bindings.len())
            .sum();
        stats.fallback_bindings = self
            .bindings
            .iter()
            .filter(|(material, _)| material.texture == WgpuQuadTextureKey::Fallback)
            .map(|(_, bindings)| bindings.len())
            .sum();
        #[cfg(feature = "ui-submitter")]
        {
            stats.glyph_textures = self.glyph_pages.len();
            stats.glyph_bindings = self
                .bindings
                .iter()
                .filter(|(material, _)| matches!(material.texture, WgpuQuadTextureKey::Glyph(_)))
                .map(|(_, bindings)| bindings.len())
                .sum();
        }
        stats.has_fallback_texture = self.fallback_texture.is_some();
        stats.texture_bytes = self.logical_texture_bytes();
        stats
    }

    #[must_use]
    pub(crate) fn logical_texture_bytes(&self) -> u64 {
        let images = self
            .resident_images
            .iter()
            .filter_map(|resident| resident.image.upgrade())
            .fold(0_u64, |total, image| {
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
        self.resident_images.retain_mut(|resident| {
            resident.reusable = false;
            resident.image.strong_count() > 0
        });
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
    ) -> Result<WgpuCachedTextureBinding, WgpuTextureError> {
        let actual = texture.key();
        if material.texture != actual {
            return Err(WgpuTextureError::TextureIdentityMismatch {
                expected: material.texture,
                actual,
            });
        }
        let image = match texture {
            WgpuQuadTexture::Fallback => {
                self.ensure_fallback(device, queue)?;
                None
            }
            WgpuQuadTexture::Image(snapshot) => {
                Some(self.ensure_image(device, queue, snapshot, frame_index)?)
            }
            #[cfg(feature = "ui-submitter")]
            WgpuQuadTexture::Glyph(page) => {
                self.ensure_glyph_page(device, queue, page, frame_index)?;
                None
            }
        };

        let binding_index = self.bindings.get(&material).and_then(|bindings| {
            bindings
                .iter()
                .position(|binding| match (&binding.image, &image) {
                    (Some(existing), Some(image)) => Arc::ptr_eq(existing, image),
                    (None, None) => true,
                    _ => false,
                })
        });
        let binding_index = match binding_index {
            Some(index) => index,
            None => {
                let view = match &image {
                    Some(image) => image.texture.view.clone(),
                    None => self.texture_view(actual)?.clone(),
                };
                let mut binding = create_texture_binding(
                    device,
                    layout,
                    "nara_wgpu_quad_texture_binding",
                    &view,
                    material.sampler,
                    frame_index,
                );
                binding.image = image.clone();
                let bindings = self.bindings.entry(material).or_default();
                let index = bindings.len();
                bindings.push(binding);
                index
            }
        };
        let binding = &mut self
            .bindings
            .get_mut(&material)
            .expect("binding was inserted")[binding_index];
        binding.last_used_frame = frame_index;
        Ok(WgpuCachedTextureBinding {
            bind_group: binding.bind_group.clone(),
            image,
        })
    }

    pub(crate) fn prepared_image_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        snapshot: &PreparedImageSnapshot,
        frame_index: u64,
    ) -> Result<WgpuCachedImageView, WgpuTextureError> {
        let image = self.ensure_image(device, queue, snapshot, frame_index)?;
        Ok(WgpuCachedImageView {
            view: image.texture.view.clone(),
            image,
        })
    }

    fn ensure_fallback(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), WgpuTextureError> {
        if self.fallback_texture.is_none() {
            admit_texture_extent(device, WgpuQuadTextureKey::Fallback, 1, 1)?;
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
            )?);
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
    ) -> Result<Arc<WgpuImageTextureResource>, WgpuTextureError> {
        let key = snapshot.identity().key();
        if key.kind() != RenderResourceKind::IMAGE_2D {
            return Err(WgpuTextureError::WrongImageResourceKind {
                key,
                kind: key.kind().as_str(),
            });
        }
        validate_image_snapshot(snapshot)?;
        let extent = snapshot.resource().extent();
        admit_texture_extent(
            device,
            WgpuQuadTextureKey::Image(key),
            extent.width,
            extent.height,
        )?;
        if let Some(existing) = self.images.get_mut(&key).and_then(|versions| {
            versions
                .iter_mut()
                .find(|entry| &entry.image.identity == snapshot.identity())
        }) {
            existing.last_used_frame = frame_index;
            self.stats.reused_images = self.stats.reused_images.saturating_add(1);
            return Ok(Arc::clone(&existing.image));
        }
        self.resident_images
            .retain(|resident| resident.image.strong_count() > 0);
        if let Some(image) = self
            .resident_images
            .iter()
            .filter(|resident| resident.reusable)
            .filter_map(|resident| resident.image.upgrade())
            .find(|image| &image.identity == snapshot.identity())
        {
            self.images
                .entry(key)
                .or_default()
                .push(WgpuImageTextureEntry {
                    image: Arc::clone(&image),
                    last_used_frame: frame_index,
                });
            self.stats.reused_images = self.stats.reused_images.saturating_add(1);
            return Ok(image);
        }
        let reupload = self.images.contains_key(&key);
        {
            let resource = snapshot.resource();
            let required = resource.pixels().len();
            self.admit_image_upload(key, required)?;
            self.admit_image_residency(key, required as u64)?;
            let format = image_texture_format(resource.format(), resource.color_space());
            let image = Arc::new(WgpuImageTextureResource {
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
                )?,
            });
            self.resident_images.push(ResidentImageTexture {
                image: Arc::downgrade(&image),
                reusable: true,
            });
            self.images
                .entry(key)
                .or_default()
                .push(WgpuImageTextureEntry {
                    image: Arc::clone(&image),
                    last_used_frame: frame_index,
                });
            if reupload {
                self.stats.reuploaded_images = self.stats.reuploaded_images.saturating_add(1);
            } else {
                self.stats.uploaded_images = self.stats.uploaded_images.saturating_add(1);
            }
            self.upload_bytes = self.upload_bytes.saturating_add(required);
            Ok(image)
        }
    }

    fn resident_image_count(&self) -> usize {
        self.resident_images
            .iter()
            .filter(|resident| resident.image.strong_count() > 0)
            .count()
    }

    fn admit_image_residency(
        &self,
        key: RenderResourceKey,
        required: u64,
    ) -> Result<(), WgpuTextureError> {
        let resident_bytes = self
            .resident_images
            .iter()
            .filter_map(|resident| resident.image.upgrade())
            .fold(0_u64, |bytes, image| {
                bytes.saturating_add(image.texture.logical_bytes)
            });
        if self.resident_image_count() >= self.residency_limits.versions
            || resident_bytes.saturating_add(required) > self.residency_limits.bytes
        {
            return Err(WgpuTextureError::ImageResidencyDeferred {
                key,
                required,
                resident_bytes,
                maximum_bytes: self.residency_limits.bytes,
                maximum_versions: self.residency_limits.versions,
            });
        }
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
        admit_texture_extent(
            device,
            WgpuQuadTextureKey::Glyph(key),
            page.width(),
            page.height(),
        )?;
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
                )?,
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
            WgpuQuadTextureKey::Image(key) => Err(WgpuTextureError::MissingImageTexture { key }),
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
        let before_images = self.images.values().map(Vec::len).sum::<usize>();
        self.images.retain(|_, versions| {
            versions.retain(|entry| !unused_past_grace(frame_index, entry.last_used_frame, grace));
            !versions.is_empty()
        });
        self.stats.evicted_image_textures =
            before_images.saturating_sub(self.images.values().map(Vec::len).sum());

        #[cfg(feature = "ui-submitter")]
        {
            let before_glyphs = self.glyph_pages.len();
            self.glyph_pages
                .retain(|_, page| !unused_past_grace(frame_index, page.last_used_frame, grace));
            self.stats.evicted_glyph_textures =
                before_glyphs.saturating_sub(self.glyph_pages.len());
        }

        #[cfg(feature = "ui-submitter")]
        let live_glyphs = self.glyph_pages.keys().copied().collect::<BTreeSet<_>>();
        let binding_counts = |bindings: &BTreeMap<WgpuQuadMaterialKey, Vec<WgpuTextureBinding>>| {
            let mut images = 0_usize;
            let mut fallback = 0_usize;
            for (material, bindings) in bindings {
                match material.texture {
                    WgpuQuadTextureKey::Image(_) => images += bindings.len(),
                    WgpuQuadTextureKey::Fallback => fallback += bindings.len(),
                    #[cfg(feature = "ui-submitter")]
                    WgpuQuadTextureKey::Glyph(_) => {}
                }
            }
            (images, fallback)
        };
        let (before_images, before_fallback) = binding_counts(&self.bindings);
        self.bindings.retain(|material, bindings| {
            let texture_live = match material.texture {
                WgpuQuadTextureKey::Fallback => true,
                WgpuQuadTextureKey::Image(_) => true,
                #[cfg(feature = "ui-submitter")]
                WgpuQuadTextureKey::Glyph(key) => live_glyphs.contains(&key),
            };
            bindings.retain(|binding| {
                texture_live && !unused_past_grace(frame_index, binding.last_used_frame, grace)
            });
            !bindings.is_empty()
        });
        let (after_images, after_fallback) = binding_counts(&self.bindings);
        self.stats.evicted_image_bindings = before_images.saturating_sub(after_images);
        self.stats.evicted_fallback_bindings = before_fallback.saturating_sub(after_fallback);
        self.resident_images
            .retain(|resident| resident.image.strong_count() > 0);

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

#[derive(Debug)]
struct WgpuImageResidencyLimits {
    versions: usize,
    bytes: u64,
}

impl Default for WgpuImageResidencyLimits {
    fn default() -> Self {
        Self {
            versions: MAX_RESIDENT_IMAGE_VERSIONS,
            bytes: MAX_RESIDENT_IMAGE_BYTES,
        }
    }
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
}

#[derive(Debug)]
struct ResidentImageTexture {
    image: Weak<WgpuImageTextureResource>,
    reusable: bool,
}

#[derive(Debug)]
struct WgpuImageTextureEntry {
    image: Arc<WgpuImageTextureResource>,
    last_used_frame: u64,
}

#[derive(Debug)]
pub(crate) struct WgpuCachedTextureBinding {
    pub(crate) bind_group: wgpu::BindGroup,
    image: Option<Arc<WgpuImageTextureResource>>,
}

#[derive(Debug)]
pub(crate) struct WgpuCachedImageView {
    pub(crate) view: wgpu::TextureView,
    image: Arc<WgpuImageTextureResource>,
}

impl WgpuCachedTextureBinding {
    pub(crate) fn retain_until_submission_complete(&self, queue: &wgpu::Queue) {
        if let Some(image) = &self.image {
            let image = Arc::clone(image);
            queue.on_submitted_work_done(move || drop(image));
        }
    }
}

impl WgpuCachedImageView {
    pub(crate) fn retain_until_submission_complete(&self, queue: &wgpu::Queue) {
        let image = Arc::clone(&self.image);
        queue.on_submitted_work_done(move || drop(image));
    }
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
    image: Option<Arc<WgpuImageTextureResource>>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum WgpuTextureError {
    #[error(
        "texture {texture:?} extent {width}x{height} is unsupported by the active device; dimensions must be nonzero and at most {maximum}"
    )]
    TextureExtentUnsupported {
        texture: WgpuQuadTextureKey,
        width: u32,
        height: u32,
        maximum: u32,
    },
    #[error("texture {label} allocation failed: {message}")]
    TextureAllocationFailed {
        label: &'static str,
        message: String,
    },
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
    #[error(
        "image {key:?} residency requires {required} bytes with {resident_bytes} resident; limits are {maximum_bytes} bytes and {maximum_versions} versions"
    )]
    ImageResidencyDeferred {
        key: RenderResourceKey,
        required: u64,
        resident_bytes: u64,
        maximum_bytes: u64,
        maximum_versions: usize,
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
            Self::FallbackUploadDeferred { .. }
                | Self::ImageUploadDeferred { .. }
                | Self::ImageResidencyDeferred { .. }
                | Self::TextureAllocationFailed { .. }
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

fn admit_texture_extent(
    device: &wgpu::Device,
    texture: WgpuQuadTextureKey,
    width: u32,
    height: u32,
) -> Result<(), WgpuTextureError> {
    let maximum = device.limits().max_texture_dimension_2d;
    if width == 0 || height == 0 || width > maximum || height > maximum {
        return Err(WgpuTextureError::TextureExtentUnsupported {
            texture,
            width,
            height,
            maximum,
        });
    }
    Ok(())
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
) -> Result<WgpuTextureResource, WgpuTextureError> {
    let allocation_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
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
    if let Some(error) = pollster::block_on(allocation_scope.pop()) {
        return Err(WgpuTextureError::TextureAllocationFailed {
            label,
            message: error.to_string(),
        });
    }
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
    Ok(WgpuTextureResource {
        _texture: texture,
        view,
        logical_bytes: u64::try_from(pixels.len()).unwrap_or(u64::MAX),
    })
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
        image: None,
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

    #[test]
    fn rejected_image_extent_preserves_upload_and_residency_capacity() {
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _| {
            let mut cache = WgpuSpriteTextureCache {
                residency_limits: WgpuImageResidencyLimits {
                    versions: 1,
                    bytes: 8_192 * 4,
                },
                ..Default::default()
            };
            for (width, height) in [(8_193, 1), (16_384, 1), (1, 8_193), (1, 16_384)] {
                let [snapshot, _] = crate::readback_tests::image_versions(width, height);
                assert_eq!(
                    cache.ensure_image(device, queue, &snapshot, 1).unwrap_err(),
                    WgpuTextureError::TextureExtentUnsupported {
                        texture: WgpuQuadTextureKey::Image(snapshot.identity().key()),
                        width,
                        height,
                        maximum: 8_192,
                    },
                );
                assert_eq!(cache.upload_bytes, 0);
                assert_eq!(cache.logical_texture_bytes(), 0);
                assert_eq!(cache.stats(), WgpuTextureCacheStats::default());
            }
            let [legal, _] = crate::readback_tests::image_versions(8_192, 1);
            cache.ensure_image(device, queue, &legal, 1).unwrap();
            assert_eq!(cache.upload_bytes, 8_192 * 4);
            assert_eq!(cache.logical_texture_bytes(), 8_192 * 4);
            assert_eq!(cache.stats().image_textures, 1);
            assert_eq!(cache.stats().uploaded_images, 1);
            Ok(())
        })
        .unwrap();
    }

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

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn independent_glyph_atlases_upload_separately_and_replay_reuses_the_same_page() {
        use nara_render::RenderTarget;
        use nara_ui_kernel::{
            UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiLayer, UiLocalKey, UiNode,
            UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme,
        };
        use nara_ui_render::{UiPreparedImages, UiPresentationPlanner};
        use nara_window::WindowId;

        let plan_page = |text: &str| {
            let snapshot = UiAuthoringSnapshotBuilder::new(1)
                .with_root(
                    UiRoot::new(
                        UiRootKey::runtime("glyph-cache", "root").unwrap(),
                        UiLayer::Base,
                        UiStyle::fill(),
                    )
                    .with_child(
                        UiNode::new(UiStyle::absolute(4.0, 4.0, 56.0, 40.0))
                            .with_key(UiLocalKey::runtime("glyph-cache", "label").unwrap())
                            .with_text(UiText::new(text, 20.0).unwrap()),
                    ),
                )
                .build()
                .unwrap();
            let environment =
                UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 64.0, 64.0, 1.0, 1, 1, 1, 1)
                    .unwrap();
            let frame =
                UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
                    .unwrap()
                    .compile(&snapshot, environment, UiTheme::default())
                    .unwrap();
            UiPresentationPlanner::default()
                .plan(
                    frame,
                    RenderTarget::Window(WindowId::PRIMARY),
                    &UiPreparedImages::default(),
                )
                .unwrap()
                .glyph_pages()[0]
                .clone()
        };
        let first = plan_page("A");
        let second = plan_page("B");
        assert_ne!(first.pixels(), second.pixels());
        assert_ne!(first.key(), second.key());

        let (_, cache) = crate::readback_tests::render_to_rgba(|device, queue, _, _, _| {
            let mut cache = WgpuSpriteTextureCache::default();
            cache.begin_frame(1);
            cache.ensure_glyph_page(device, queue, &first, 1).unwrap();
            cache.ensure_glyph_page(device, queue, &second, 1).unwrap();
            assert_eq!(cache.glyph_pages.len(), 2);
            assert_eq!(cache.stats().uploaded_glyph_pages, 2);
            assert_eq!(cache.stats().reused_glyph_pages, 0);

            cache.ensure_glyph_page(device, queue, &first, 1).unwrap();
            assert_eq!(cache.glyph_pages.len(), 2);
            assert_eq!(cache.stats().uploaded_glyph_pages, 2);
            assert_eq!(cache.stats().reused_glyph_pages, 1);
            Ok(cache)
        })
        .unwrap();
        assert_eq!(cache.stats().glyph_textures, 2);
    }

    #[test]
    fn image_upload_observes_prior_shared_upload_work() {
        let key = RenderResourceKey::new(
            nara_asset::allocate_anonymous_handle::<()>().unwrap().id(),
            RenderResourceKind::IMAGE_2D,
        );
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

    #[test]
    fn image_residency_capacity_preserves_existing_versions_and_counts_actual_holders() {
        let [first, second] = crate::readback_tests::two_image_versions();
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _| {
            let layout = crate::quad::create_quad_texture_bind_group_layout(device);
            for limits in [
                WgpuImageResidencyLimits {
                    versions: 1,
                    bytes: 8,
                },
                WgpuImageResidencyLimits {
                    versions: 2,
                    bytes: 4,
                },
            ] {
                let mut cache = WgpuSpriteTextureCache {
                    residency_limits: limits,
                    ..Default::default()
                };
                let material = WgpuQuadMaterialKey {
                    texture: WgpuQuadTextureKey::Image(first.identity().key()),
                    sampler: SamplerDescriptor::NEAREST_CLAMP,
                    alpha_mode: nara_material::AlphaMode2d::Opaque,
                };
                cache.begin_frame(1);
                let held = cache
                    .bind_group(
                        device,
                        queue,
                        &layout,
                        material,
                        &WgpuQuadTexture::Image(first.clone()),
                        1,
                    )
                    .unwrap();
                assert!(matches!(
                    cache.ensure_image(device, queue, &second, 1),
                    Err(WgpuTextureError::ImageResidencyDeferred { .. })
                ));
                assert_eq!(cache.stats().image_textures, 1);
                assert_eq!(
                    cache.stats().uploaded_images + cache.stats().reuploaded_images,
                    1
                );
                cache.begin_frame(4);
                assert!(cache.images.is_empty());
                assert!(cache.bindings.is_empty());
                assert_eq!(cache.stats().image_textures, 1);
                assert_eq!(cache.logical_texture_bytes(), 4);
                assert!(matches!(
                    cache.ensure_image(device, queue, &second, 4),
                    Err(WgpuTextureError::ImageResidencyDeferred { .. })
                ));
                cache.ensure_image(device, queue, &first, 4).unwrap();
                assert_eq!(cache.stats().reused_images, 1);
                assert_eq!(
                    cache.stats().uploaded_images + cache.stats().reuploaded_images,
                    0
                );
                cache.clear();
                assert_eq!(cache.logical_texture_bytes(), 4);
                drop(held);
                assert_eq!(cache.logical_texture_bytes(), 0);
                cache.begin_frame(8);
                cache.ensure_image(device, queue, &second, 8).unwrap();
                assert_eq!(cache.stats().image_textures, 1);
                assert_eq!(cache.logical_texture_bytes(), 4);
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn clearing_image_cache_retires_namespace_without_forgetting_live_allocations() {
        let [snapshot, _] = crate::readback_tests::two_image_versions();
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _| {
            let layout = crate::quad::create_quad_texture_bind_group_layout(device);
            let material = WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Image(snapshot.identity().key()),
                sampler: SamplerDescriptor::NEAREST_CLAMP,
                alpha_mode: nara_material::AlphaMode2d::Opaque,
            };
            let mut cache = WgpuSpriteTextureCache {
                residency_limits: WgpuImageResidencyLimits {
                    versions: 1,
                    bytes: 4,
                },
                ..Default::default()
            };
            let old_binding = cache
                .bind_group(
                    device,
                    queue,
                    &layout,
                    material,
                    &WgpuQuadTexture::Image(snapshot.clone()),
                    1,
                )
                .unwrap();
            cache.clear();
            assert_eq!(cache.stats().image_textures, 1);
            assert!(matches!(
                cache.ensure_image(device, queue, &snapshot, 2),
                Err(WgpuTextureError::ImageResidencyDeferred { .. })
            ));
            cache.residency_limits = WgpuImageResidencyLimits {
                versions: 2,
                bytes: 8,
            };
            let new_binding = cache
                .bind_group(
                    device,
                    queue,
                    &layout,
                    material,
                    &WgpuQuadTexture::Image(snapshot),
                    2,
                )
                .unwrap();
            assert!(!Arc::ptr_eq(
                old_binding.image.as_ref().unwrap(),
                new_binding.image.as_ref().unwrap()
            ));
            assert_eq!(cache.stats().image_textures, 2);
            assert_eq!(cache.stats().reused_images, 0);
            assert_eq!(cache.stats().uploaded_images, 1);
            assert_eq!(cache.logical_texture_bytes(), 8);
            drop(old_binding);
            assert_eq!(cache.logical_texture_bytes(), 4);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn fallback_binding_eviction_does_not_count_as_an_image_binding() {
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _| {
            let layout = crate::quad::create_quad_texture_bind_group_layout(device);
            let material = WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Fallback,
                sampler: SamplerDescriptor::NEAREST_CLAMP,
                alpha_mode: nara_material::AlphaMode2d::Opaque,
            };
            let mut cache = WgpuSpriteTextureCache::default();
            cache
                .bind_group(
                    device,
                    queue,
                    &layout,
                    material,
                    &WgpuQuadTexture::Fallback,
                    1,
                )
                .unwrap();
            cache.begin_frame(4);
            assert_eq!(cache.stats().evicted_image_bindings, 0);
            assert_eq!(cache.stats().evicted_fallback_bindings, 1);
            Ok(())
        })
        .unwrap();
    }
}
