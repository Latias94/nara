use std::collections::{BTreeMap, BTreeSet};

use crate::quad::WgpuQuadMaterialKey;
use nara_asset::{AssetSlotRevision, Assets, Handle};
use nara_image::{ImageAsset, ImageColorSpace, ImageFormat, PreparedImageResource};
use nara_material::{AddressMode, FilterMode, SamplerDescriptor};
use nara_render::{
    PreparedRenderResources, RenderPrepareStatus, RenderResourceKey, RenderResourceKind,
    RenderResourceSnapshot,
};
use thiserror::Error;

const RGBA8_BYTES_PER_PIXEL: u32 = 4;
const DEFAULT_UNUSED_GRACE_FRAMES: u64 = 2;

#[derive(Debug)]
pub(crate) struct WgpuSpriteTextureCache {
    policy: WgpuTextureCachePolicy,
    stats: WgpuTextureCacheStats,
    fallback_texture: Option<WgpuTextureResource>,
    fallback_bindings: BTreeMap<WgpuQuadMaterialKey, WgpuSpriteTextureBinding>,
    images: BTreeMap<RenderResourceKey, WgpuImageTextureResource>,
    image_bindings: BTreeMap<WgpuQuadMaterialKey, WgpuSpriteImageBinding>,
}

impl Default for WgpuSpriteTextureCache {
    fn default() -> Self {
        Self {
            policy: WgpuTextureCachePolicy::default(),
            stats: WgpuTextureCacheStats::default(),
            fallback_texture: None,
            fallback_bindings: BTreeMap::new(),
            images: BTreeMap::new(),
            image_bindings: BTreeMap::new(),
        }
    }
}

impl WgpuSpriteTextureCache {
    #[must_use]
    pub(crate) fn image_count(&self) -> usize {
        self.images.len()
    }

    #[must_use]
    pub(crate) fn stats(&self) -> WgpuTextureCacheStats {
        let mut stats = self.stats;
        stats.image_textures = self.images.len();
        stats.image_bindings = self.image_bindings.len();
        stats.fallback_bindings = self.fallback_bindings.len();
        stats.has_fallback_texture = self.fallback_texture.is_some();
        stats.texture_bytes = self.logical_texture_bytes();
        stats
    }

    #[must_use]
    pub(crate) fn logical_texture_bytes(&self) -> u64 {
        let image_bytes = self.images.values().fold(0_u64, |total, image| {
            total.saturating_add(image.texture.logical_bytes)
        });
        image_bytes.saturating_add(
            self.fallback_texture
                .as_ref()
                .map_or(0, |texture| texture.logical_bytes),
        )
    }

    pub(crate) fn clear(&mut self) {
        self.fallback_texture = None;
        self.fallback_bindings.clear();
        self.images.clear();
        self.image_bindings.clear();
        self.stats = WgpuTextureCacheStats::default();
    }

    pub(crate) fn begin_frame(&mut self, frame_index: u64) {
        self.stats = WgpuTextureCacheStats::default();
        self.evict_unused(frame_index);
    }

    fn evict_unused(&mut self, frame_index: u64) {
        let grace_frames = self.policy.unused_grace_frames;
        let before_images = self.images.len();
        self.images.retain(|_, image| {
            !unused_past_grace(frame_index, image.last_used_frame, grace_frames)
        });
        self.stats.evicted_image_textures = before_images.saturating_sub(self.images.len());

        let live_images = self.images.keys().copied().collect::<BTreeSet<_>>();
        let before_image_bindings = self.image_bindings.len();
        self.image_bindings.retain(|material, binding| {
            material.image.is_some()
                && live_images.contains(&material.image.unwrap())
                && !unused_past_grace(frame_index, binding.last_used_frame, grace_frames)
        });
        self.stats.evicted_image_bindings =
            before_image_bindings.saturating_sub(self.image_bindings.len());

        let before_fallback_bindings = self.fallback_bindings.len();
        self.fallback_bindings.retain(|_, binding| {
            !unused_past_grace(frame_index, binding.last_used_frame, grace_frames)
        });
        self.stats.evicted_fallback_bindings =
            before_fallback_bindings.saturating_sub(self.fallback_bindings.len());

        if self.fallback_texture.is_some() && self.fallback_bindings.is_empty() {
            self.fallback_texture = None;
            self.stats.evicted_fallback_textures = 1;
        }
    }

    pub(crate) fn fallback_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        material: WgpuQuadMaterialKey,
        frame_index: u64,
    ) -> wgpu::BindGroup {
        if self.fallback_texture.is_none() {
            self.fallback_texture = Some(create_texture_resource(
                device,
                queue,
                "nara_wgpu_sprite_fallback_texture",
                &[255, 255, 255, 255],
                1,
                1,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ));
        }

        if !self.fallback_bindings.contains_key(&material) {
            let binding = {
                let texture = self.fallback_texture.as_ref().unwrap();
                create_texture_bind_group(
                    device,
                    layout,
                    "nara_wgpu_sprite_fallback_bind_group",
                    &texture.view,
                    material.sampler,
                    frame_index,
                )
            };
            self.fallback_bindings.insert(material, binding);
        }

        let binding = self.fallback_bindings.get_mut(&material).unwrap();
        binding.last_used_frame = frame_index;
        binding.bind_group.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        material: WgpuQuadMaterialKey,
        images: &Assets<ImageAsset>,
        prepared_images: &PreparedRenderResources<PreparedImageResource>,
        frame_index: u64,
    ) -> Result<wgpu::BindGroup, WgpuSpriteTextureError> {
        let key = material
            .image
            .ok_or(WgpuSpriteTextureError::MissingMaterialImage)?;
        let (snapshot, image, upload_action) = prepare_image_texture_submission(
            key,
            images,
            prepared_images,
            self.images.get(&key).map(|existing| &existing.snapshot),
        )?;
        if upload_action.requires_upload() {
            let texture = create_image_texture_resource(device, queue, key, image)?;
            self.images.insert(
                key,
                WgpuImageTextureResource {
                    snapshot: snapshot.clone(),
                    texture,
                    last_used_frame: frame_index,
                },
            );
            match upload_action {
                TextureUploadAction::UploadNew => {
                    self.stats.uploaded_images = self.stats.uploaded_images.saturating_add(1);
                }
                TextureUploadAction::ReuploadChangedSnapshot => {
                    self.stats.reuploaded_images = self.stats.reuploaded_images.saturating_add(1);
                }
                TextureUploadAction::ReuseExisting => {}
            }
        } else if let Some(existing) = self.images.get_mut(&key) {
            existing.last_used_frame = frame_index;
            self.stats.reused_images = self.stats.reused_images.saturating_add(1);
        }

        let binding_needs_update = self
            .image_bindings
            .get(&material)
            .map(|existing| &existing.snapshot != snapshot)
            .unwrap_or(true);
        if binding_needs_update {
            let binding = {
                let texture = &self.images.get(&key).unwrap().texture;
                create_texture_bind_group(
                    device,
                    layout,
                    "nara_wgpu_sprite_image_bind_group",
                    &texture.view,
                    material.sampler,
                    frame_index,
                )
            };
            self.image_bindings.insert(
                material,
                WgpuSpriteImageBinding {
                    snapshot: snapshot.clone(),
                    binding,
                    last_used_frame: frame_index,
                },
            );
        }

        let binding = self.image_bindings.get_mut(&material).unwrap();
        binding.last_used_frame = frame_index;
        Ok(binding.binding.bind_group.clone())
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
    pub evicted_image_textures: usize,
    pub evicted_image_bindings: usize,
    pub evicted_fallback_textures: u32,
    pub evicted_fallback_bindings: usize,
    pub image_textures: usize,
    pub image_bindings: usize,
    pub fallback_bindings: usize,
    pub has_fallback_texture: bool,
    pub texture_bytes: u64,
}

#[derive(Debug)]
struct WgpuImageTextureResource {
    snapshot: RenderResourceSnapshot,
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
struct WgpuSpriteImageBinding {
    snapshot: RenderResourceSnapshot,
    binding: WgpuSpriteTextureBinding,
    last_used_frame: u64,
}

#[derive(Debug)]
struct WgpuSpriteTextureBinding {
    bind_group: wgpu::BindGroup,
    _sampler: wgpu::Sampler,
    last_used_frame: u64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum WgpuSpriteTextureError {
    #[error("sprite material has no image render resource key")]
    MissingMaterialImage,
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
    #[error("sprite texture {key:?} changed after its prepared snapshot was published")]
    PreparedSlotRevisionMismatch { key: RenderResourceKey },
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
}

impl WgpuSpriteTextureError {
    pub(crate) const fn is_transient_resource_rejection(&self) -> bool {
        matches!(self, Self::PreparedSlotRevisionMismatch { .. })
    }
}

fn prepared_image_record(
    key: RenderResourceKey,
    prepared_images: &PreparedRenderResources<PreparedImageResource>,
) -> Result<(&RenderResourceSnapshot, &PreparedImageResource), WgpuSpriteTextureError> {
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
    Ok((snapshot, prepared))
}

fn validate_prepared_slot_revision(
    key: RenderResourceKey,
    snapshot: &RenderResourceSnapshot,
    current: &AssetSlotRevision,
) -> Result<(), WgpuSpriteTextureError> {
    if snapshot.slot_revision() != current {
        return Err(WgpuSpriteTextureError::PreparedSlotRevisionMismatch { key });
    }
    Ok(())
}

fn prepare_image_texture_submission<'a>(
    key: RenderResourceKey,
    images: &'a Assets<ImageAsset>,
    prepared_images: &'a PreparedRenderResources<PreparedImageResource>,
    existing: Option<&RenderResourceSnapshot>,
) -> Result<
    (
        &'a RenderResourceSnapshot,
        &'a ImageAsset,
        TextureUploadAction,
    ),
    WgpuSpriteTextureError,
> {
    if key.kind() != RenderResourceKind::IMAGE_2D {
        return Err(WgpuSpriteTextureError::WrongResourceKind {
            key,
            kind: key.kind().as_str(),
        });
    }

    let (snapshot, prepared) = prepared_image_record(key, prepared_images)?;
    let handle = Handle::<ImageAsset>::new(key.asset_id());
    validate_prepared_slot_revision(key, snapshot, &images.slot_revision(handle))?;
    let image = images
        .get(handle)
        .ok_or(WgpuSpriteTextureError::MissingImageAsset { key })?;
    validate_prepared_image_matches_asset(key, image, prepared)?;
    Ok((snapshot, image, texture_upload_action(existing, snapshot)))
}

fn create_image_texture_resource(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    key: RenderResourceKey,
    image: &ImageAsset,
) -> Result<WgpuTextureResource, WgpuSpriteTextureError> {
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

    Ok(create_texture_resource(
        device,
        queue,
        "nara_wgpu_sprite_image_texture",
        image.pixels(),
        extent.width,
        extent.height,
        format,
    ))
}

#[allow(clippy::too_many_arguments)]
fn create_texture_resource(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &'static str,
    pixels: &[u8],
    width: u32,
    height: u32,
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
    WgpuTextureResource {
        _texture: texture,
        view,
        logical_bytes: u64::try_from(pixels.len()).unwrap_or(u64::MAX),
    }
}

fn create_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    label: &'static str,
    view: &wgpu::TextureView,
    sampler_descriptor: SamplerDescriptor,
    last_used_frame: u64,
) -> WgpuSpriteTextureBinding {
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

    WgpuSpriteTextureBinding {
        bind_group,
        _sampler: sampler,
        last_used_frame,
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

fn unused_past_grace(current_frame: u64, last_used_frame: u64, grace_frames: u64) -> bool {
    current_frame.saturating_sub(last_used_frame) > grace_frames
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextureUploadAction {
    UploadNew,
    ReuploadChangedSnapshot,
    ReuseExisting,
}

impl TextureUploadAction {
    fn requires_upload(self) -> bool {
        !matches!(self, Self::ReuseExisting)
    }
}

fn texture_upload_action(
    existing: Option<&RenderResourceSnapshot>,
    incoming: &RenderResourceSnapshot,
) -> TextureUploadAction {
    match existing {
        None => TextureUploadAction::UploadNew,
        Some(existing) if existing != incoming => TextureUploadAction::ReuploadChangedSnapshot,
        Some(_) => TextureUploadAction::ReuseExisting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::{
        ArtifactFormatVersion, ArtifactLabel, AssetId, AssetPath, AssetVersion,
        ImportArtifactDigest, ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest,
        ImportProfile, ImportSettingsHash, ImportedAssetType, ImporterId, ImporterVersion,
        SourceHash, StableAssetId,
    };
    use nara_image::{ImageExtent, ImageSourceMetadata};

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
        assert_eq!(filter_mode(FilterMode::Nearest), wgpu::FilterMode::Nearest);
        assert_eq!(filter_mode(FilterMode::Linear), wgpu::FilterMode::Linear);
        assert_eq!(
            mipmap_filter_mode(FilterMode::Nearest),
            wgpu::MipmapFilterMode::Nearest
        );
        assert_eq!(
            mipmap_filter_mode(FilterMode::Linear),
            wgpu::MipmapFilterMode::Linear
        );
        assert_eq!(
            address_mode(AddressMode::ClampToEdge),
            wgpu::AddressMode::ClampToEdge
        );
        assert_eq!(address_mode(AddressMode::Repeat), wgpu::AddressMode::Repeat);
        assert_eq!(
            address_mode(AddressMode::MirrorRepeat),
            wgpu::AddressMode::MirrorRepeat
        );
    }

    #[test]
    fn cache_policy_retains_resources_through_grace_window() {
        assert!(!unused_past_grace(10, 10, 2));
        assert!(!unused_past_grace(11, 10, 2));
        assert!(!unused_past_grace(12, 10, 2));
        assert!(unused_past_grace(13, 10, 2));
        assert!(!unused_past_grace(0, 10, 2));
    }

    #[test]
    fn cache_stats_report_current_resource_counts() {
        let mut cache = WgpuSpriteTextureCache::default();
        cache.stats.uploaded_images = 3;

        assert_eq!(
            cache.stats(),
            WgpuTextureCacheStats {
                uploaded_images: 3,
                image_textures: 0,
                image_bindings: 0,
                fallback_bindings: 0,
                has_fallback_texture: false,
                ..WgpuTextureCacheStats::default()
            }
        );

        cache.clear();
        assert_eq!(cache.stats(), WgpuTextureCacheStats::default());
    }

    #[test]
    fn texture_upload_action_tracks_snapshot_changes() {
        let handle = Handle::<()>::new(AssetId::from_raw(1));
        let mut assets = Assets::default();
        assets.insert(handle, ());
        let revision = assets.slot_revision(handle);
        let first = snapshot(1, 1, revision.clone(), b"descriptor-a");
        let same = snapshot(1, 1, revision.clone(), b"descriptor-a");
        let new_version = snapshot(1, 2, revision.clone(), b"descriptor-a");
        let new_descriptor = snapshot(1, 1, revision, b"descriptor-b");
        assert!(assets.get_mut(handle).is_some());
        let new_slot = snapshot(1, 1, assets.slot_revision(handle), b"descriptor-a");

        assert_eq!(
            texture_upload_action(None, &first),
            TextureUploadAction::UploadNew
        );
        assert_eq!(
            texture_upload_action(Some(&first), &same),
            TextureUploadAction::ReuseExisting
        );
        assert_eq!(
            texture_upload_action(Some(&first), &new_version),
            TextureUploadAction::ReuploadChangedSnapshot
        );
        assert_eq!(
            texture_upload_action(Some(&first), &new_descriptor),
            TextureUploadAction::ReuploadChangedSnapshot
        );
        assert_eq!(
            texture_upload_action(Some(&first), &new_slot),
            TextureUploadAction::ReuploadChangedSnapshot
        );
    }

    #[test]
    fn prepared_snapshot_rejects_an_asset_mutated_after_prepare() {
        let key = RenderResourceKey::new(AssetId::from_raw(9), RenderResourceKind::IMAGE_2D);
        let handle = Handle::<()>::new(key.asset_id());
        let mut assets = Assets::default();
        assets.insert(handle, ());
        let snapshot = RenderResourceSnapshot::new(
            key,
            AssetVersion::from_raw(1),
            assets.slot_revision(handle),
            ImportArtifactDigest::from_bytes(b"descriptor"),
        );

        assert_eq!(
            validate_prepared_slot_revision(key, &snapshot, &assets.slot_revision(handle)),
            Ok(())
        );
        assert!(assets.get_mut(handle).is_some());
        assert_eq!(
            validate_prepared_slot_revision(key, &snapshot, &assets.slot_revision(handle)),
            Err(WgpuSpriteTextureError::PreparedSlotRevisionMismatch { key })
        );
    }

    #[test]
    fn image_submission_rejects_stale_pixels_then_reuploads_once_and_reuses() {
        let handle = Handle::<ImageAsset>::new(AssetId::from_raw(11));
        let key = RenderResourceKey::for_asset(handle, RenderResourceKind::IMAGE_2D);
        let version = AssetVersion::from_raw(1);
        let descriptor = ImportArtifactDigest::from_bytes(b"same-descriptor");
        let mut images = Assets::default();
        images.insert(handle, test_image([255, 0, 0, 255]));
        let first_snapshot =
            RenderResourceSnapshot::new(key, version, images.slot_revision(handle), descriptor);
        let mut prepared = PreparedRenderResources::default();
        prepared.insert_ready(
            first_snapshot.clone(),
            PreparedImageResource::from_image(images.get(handle).unwrap()),
        );

        let (published, first_image, action) =
            prepare_image_texture_submission(key, &images, &prepared, None).unwrap();
        assert_eq!(first_image.pixels(), &[255, 0, 0, 255]);
        assert_eq!(action, TextureUploadAction::UploadNew);
        let cached_snapshot = published.clone();
        assert_eq!(
            prepare_image_texture_submission(key, &images, &prepared, Some(&cached_snapshot))
                .unwrap()
                .2,
            TextureUploadAction::ReuseExisting
        );

        images.insert(handle, test_image([0, 0, 255, 255]));
        assert!(matches!(
            prepare_image_texture_submission(
                key,
                &images,
                &prepared,
                Some(&cached_snapshot)
            ),
            Err(WgpuSpriteTextureError::PreparedSlotRevisionMismatch { key: actual })
                if actual == key
        ));

        let replacement_snapshot =
            RenderResourceSnapshot::new(key, version, images.slot_revision(handle), descriptor);
        prepared.insert_ready(
            replacement_snapshot.clone(),
            PreparedImageResource::from_image(images.get(handle).unwrap()),
        );
        let (published, replacement_image, action) =
            prepare_image_texture_submission(key, &images, &prepared, Some(&cached_snapshot))
                .unwrap();
        assert_eq!(replacement_image.pixels(), &[0, 0, 255, 255]);
        assert_eq!(action, TextureUploadAction::ReuploadChangedSnapshot);
        let cached_snapshot = published.clone();
        assert_eq!(
            prepare_image_texture_submission(key, &images, &prepared, Some(&cached_snapshot))
                .unwrap()
                .2,
            TextureUploadAction::ReuseExisting
        );
    }

    fn test_image(pixels: [u8; 4]) -> ImageAsset {
        let stable_id = StableAssetId::parse_str("ce2ab2f8-c58b-48e3-b94e-9465340262a1").unwrap();
        let source_hash = SourceHash::from_bytes(b"test image");
        let key = ImportArtifactKey::new(
            stable_id,
            source_hash,
            ImportDependencyDigest::empty(),
            ImporterId::new("nara-image").unwrap(),
            ImporterVersion::new(1),
            ImportSettingsHash::default(),
            ImportProfile::default(),
            ImportedAssetType::new("image").unwrap(),
            ArtifactLabel::default(),
            ArtifactFormatVersion::new(1),
        );
        let source = ImageSourceMetadata::new(
            stable_id,
            AssetPath::new("textures/test.png").unwrap(),
            source_hash,
            ImportArtifactRecord::new(key).unwrap(),
        );
        ImageAsset::new(
            source,
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            pixels.to_vec(),
        )
        .unwrap()
    }

    fn snapshot(
        asset_id: u64,
        version: u64,
        slot_revision: AssetSlotRevision,
        descriptor: &[u8],
    ) -> RenderResourceSnapshot {
        RenderResourceSnapshot::new(
            RenderResourceKey::new(AssetId::from_raw(asset_id), RenderResourceKind::IMAGE_2D),
            AssetVersion::from_raw(version),
            slot_revision,
            ImportArtifactDigest::from_bytes(descriptor),
        )
    }

    #[test]
    fn image_submission_classifies_deletion_after_prepare_as_a_transient_resource_change() {
        let handle = Handle::<ImageAsset>::new(AssetId::from_raw(12));
        let key = RenderResourceKey::for_asset(handle, RenderResourceKind::IMAGE_2D);
        let mut images = Assets::default();
        images.insert(handle, test_image([255, 255, 255, 255]));
        let snapshot = RenderResourceSnapshot::new(
            key,
            AssetVersion::from_raw(1),
            images.slot_revision(handle),
            ImportArtifactDigest::from_bytes(b"descriptor"),
        );
        let mut prepared = PreparedRenderResources::default();
        prepared.insert_ready(
            snapshot.clone(),
            PreparedImageResource::from_image(images.get(handle).unwrap()),
        );

        assert!(images.remove(handle).is_some());

        assert!(matches!(
            prepare_image_texture_submission(key, &images, &prepared, Some(&snapshot)),
            Err(WgpuSpriteTextureError::PreparedSlotRevisionMismatch { key: actual })
                if actual == key
        ));
    }
}
