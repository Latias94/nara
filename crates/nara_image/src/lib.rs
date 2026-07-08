//! Backend-neutral image assets and PNG-first importing.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_app::{App, CoreStage, Plugin};
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetPath, AssetStates, Assets, ImportArtifactDigest,
    ImportArtifactPathError, ImportArtifactRecord, ImportError, ImportRequest, ImportedAssetType,
    Importer, ImporterDescriptor, ImporterDescriptorError, ImporterId, ImporterSelectionError,
    ImporterVersion, LoadState, SourceExtension, SourceHash, StableAssetId,
};
use nara_ecs::{Res, ResMut, Resource};
use nara_render::{
    PreparedRenderResources, RenderPrepareApplyResult, RenderPrepareInvalidationReason,
    RenderPrepareInvalidations, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageFormat {
    Rgba8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageColorSpace {
    Srgb,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageExtent {
    pub width: u32,
    pub height: u32,
}

impl ImageExtent {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn pixel_count(self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageFilterMode {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageAddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageSamplerDescriptor {
    pub min_filter: ImageFilterMode,
    pub mag_filter: ImageFilterMode,
    pub mipmap_filter: ImageFilterMode,
    pub address_mode_u: ImageAddressMode,
    pub address_mode_v: ImageAddressMode,
}

impl Default for ImageSamplerDescriptor {
    fn default() -> Self {
        Self {
            min_filter: ImageFilterMode::Linear,
            mag_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageSourceMetadata {
    stable_id: StableAssetId,
    path: AssetPath,
    source_hash: SourceHash,
    artifact: ImportArtifactRecord,
}

impl ImageSourceMetadata {
    #[must_use]
    pub fn new(
        stable_id: StableAssetId,
        path: AssetPath,
        source_hash: SourceHash,
        artifact: ImportArtifactRecord,
    ) -> Self {
        Self {
            stable_id,
            path,
            source_hash,
            artifact,
        }
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub const fn artifact(&self) -> &ImportArtifactRecord {
        &self.artifact
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageAsset {
    source: ImageSourceMetadata,
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    sampler: ImageSamplerDescriptor,
    pixels: Vec<u8>,
}

impl ImageAsset {
    #[must_use]
    pub fn new(
        source: ImageSourceMetadata,
        extent: ImageExtent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        sampler: ImageSamplerDescriptor,
        pixels: Vec<u8>,
    ) -> Self {
        Self {
            source,
            extent,
            format,
            color_space,
            sampler,
            pixels,
        }
    }

    #[must_use]
    pub const fn source(&self) -> &ImageSourceMetadata {
        &self.source
    }

    #[must_use]
    pub const fn extent(&self) -> ImageExtent {
        self.extent
    }

    #[must_use]
    pub const fn format(&self) -> ImageFormat {
        self.format
    }

    #[must_use]
    pub const fn color_space(&self) -> ImageColorSpace {
        self.color_space
    }

    #[must_use]
    pub const fn sampler(&self) -> ImageSamplerDescriptor {
        self.sampler
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageImportedAsset {
    artifact: ImportArtifactRecord,
    image: ImageAsset,
}

impl ImageImportedAsset {
    #[must_use]
    pub fn new(artifact: ImportArtifactRecord, image: ImageAsset) -> Self {
        Self { artifact, image }
    }

    #[must_use]
    pub const fn artifact(&self) -> &ImportArtifactRecord {
        &self.artifact
    }

    #[must_use]
    pub const fn image(&self) -> &ImageAsset {
        &self.image
    }

    #[must_use]
    pub fn into_image(self) -> ImageAsset {
        self.image
    }
}

#[derive(Debug, Clone)]
pub struct ImageImporter {
    descriptor: ImporterDescriptor,
    color_space: ImageColorSpace,
    sampler: ImageSamplerDescriptor,
}

impl Default for ImageImporter {
    fn default() -> Self {
        Self::new().expect("built-in image importer descriptor is valid")
    }
}

impl ImageImporter {
    pub fn new() -> Result<Self, ImporterDescriptorError> {
        Ok(Self {
            descriptor: ImporterDescriptor::new(
                ImporterId::new("nara_image_png").expect("built-in importer id is valid"),
                ImporterVersion::new(1),
                [SourceExtension::new("png").expect("built-in source extension is valid")],
                ImportedAssetType::new("nara_image.image").expect("built-in output type is valid"),
                ArtifactFormatVersion::new(1),
            )?,
            color_space: ImageColorSpace::Srgb,
            sampler: ImageSamplerDescriptor::default(),
        })
    }

    #[must_use]
    pub fn with_color_space(mut self, color_space: ImageColorSpace) -> Self {
        self.color_space = color_space;
        self
    }

    #[must_use]
    pub fn with_sampler(mut self, sampler: ImageSamplerDescriptor) -> Self {
        self.sampler = sampler;
        self
    }

    pub fn import_image(
        &self,
        request: ImportRequest<'_>,
    ) -> Result<ImageImportedAsset, ImageImportError> {
        let extension = SourceExtension::from_asset_path(request.source().path())
            .map_err(ImageImportError::Selection)?;
        let png_extension =
            SourceExtension::new("png").expect("built-in source extension is valid");
        if extension != png_extension {
            return Err(ImageImportError::UnsupportedFormat { extension });
        }

        let artifact = self.import_record(&request)?;
        let dynamic =
            image::load_from_memory_with_format(request.source_bytes(), image::ImageFormat::Png)
                .map_err(|error| ImageImportError::Decode(error.to_string()))?;
        let rgba = dynamic.to_rgba8();
        let (width, height) = rgba.dimensions();
        let pixels = rgba.into_raw();
        let expected_len = ImageExtent::new(width, height).pixel_count() as usize * 4;
        if pixels.len() != expected_len {
            return Err(ImageImportError::Decode(format!(
                "decoded RGBA8 byte length is {}, expected {expected_len}",
                pixels.len()
            )));
        }

        let image = ImageAsset::new(
            ImageSourceMetadata::new(
                request.source().stable_id(),
                request.source().path().clone(),
                request.source_hash(),
                artifact.clone(),
            ),
            ImageExtent::new(width, height),
            ImageFormat::Rgba8,
            self.color_space,
            self.sampler,
            pixels,
        );

        Ok(ImageImportedAsset::new(artifact, image))
    }

    fn import_record(
        &self,
        request: &ImportRequest<'_>,
    ) -> Result<ImportArtifactRecord, ImageImportError> {
        let key = request.artifact_key(&self.descriptor, ArtifactLabel::default());
        ImportArtifactRecord::new(key).map_err(ImageImportError::ArtifactPath)
    }
}

impl Importer for ImageImporter {
    fn descriptor(&self) -> &ImporterDescriptor {
        &self.descriptor
    }

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifactRecord, ImportError> {
        self.import_record(&request)
            .map_err(|error| ImportError::ImporterFailed(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageImportError {
    Selection(ImporterSelectionError),
    UnsupportedFormat { extension: SourceExtension },
    ArtifactPath(ImportArtifactPathError),
    Decode(String),
}

impl Display for ImageImportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Selection(error) => Display::fmt(error, formatter),
            Self::UnsupportedFormat { extension } => {
                write!(
                    formatter,
                    "unsupported image source extension '.{extension}'"
                )
            }
            Self::ArtifactPath(error) => Display::fmt(error, formatter),
            Self::Decode(error) => write!(formatter, "failed to decode image: {error}"),
        }
    }
}

impl Error for ImageImportError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImageResource {
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    sampler: ImageSamplerDescriptor,
    source_hash: SourceHash,
    artifact_hash: ImportArtifactDigest,
    pixel_len: usize,
}

impl PreparedImageResource {
    #[must_use]
    pub fn from_image(image: &ImageAsset) -> Self {
        Self {
            extent: image.extent(),
            format: image.format(),
            color_space: image.color_space(),
            sampler: image.sampler(),
            source_hash: image.source().source_hash(),
            artifact_hash: image.source().artifact().key().digest(),
            pixel_len: image.pixels().len(),
        }
    }

    #[must_use]
    pub const fn extent(&self) -> ImageExtent {
        self.extent
    }

    #[must_use]
    pub const fn format(&self) -> ImageFormat {
        self.format
    }

    #[must_use]
    pub const fn color_space(&self) -> ImageColorSpace {
        self.color_space
    }

    #[must_use]
    pub const fn sampler(&self) -> ImageSamplerDescriptor {
        self.sampler
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub const fn artifact_hash(&self) -> ImportArtifactDigest {
        self.artifact_hash
    }

    #[must_use]
    pub const fn pixel_len(&self) -> usize {
        self.pixel_len
    }
}

#[derive(Debug, Default, Resource)]
pub struct ImagePrepareStats {
    pub prepared: u32,
    pub skipped_missing_state: u32,
    pub skipped_not_loaded: u32,
    pub stale_results: u32,
}

#[derive(Debug, Default)]
pub struct ImagePreparePlugin;

impl Plugin for ImagePreparePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<ImageAsset>>();
        app.init_resource::<AssetStates>();
        app.init_resource::<PreparedRenderResources<PreparedImageResource>>();
        app.init_resource::<RenderPrepareInvalidations>();
        app.init_resource::<ImagePrepareStats>();
        app.add_systems(CoreStage::Prepare, prepare_images);
    }
}

pub fn prepare_images(
    images: Res<Assets<ImageAsset>>,
    states: Res<AssetStates>,
    mut prepared_images: ResMut<PreparedRenderResources<PreparedImageResource>>,
    mut invalidations: ResMut<RenderPrepareInvalidations>,
    mut stats: ResMut<ImagePrepareStats>,
) {
    *stats = ImagePrepareStats::default();

    for (handle, image) in images.iter() {
        let Some(state) = states.state(handle.id()) else {
            stats.skipped_missing_state += 1;
            continue;
        };
        if state.load_state() != &LoadState::Loaded {
            stats.skipped_not_loaded += 1;
            continue;
        }

        let key = image_resource_key(handle);
        let snapshot =
            RenderResourceSnapshot::new(key, state.version(), image_descriptor_hash(image));

        prepared_images.invalidate_if_snapshot_changed(
            snapshot,
            &mut invalidations,
            RenderPrepareInvalidationReason::DescriptorChanged,
        );

        if !prepared_images.needs_prepare(snapshot) {
            continue;
        }

        match prepared_images.insert_ready(snapshot, PreparedImageResource::from_image(image)) {
            RenderPrepareApplyResult::Applied => stats.prepared += 1,
            RenderPrepareApplyResult::DiscardedStale { .. } => stats.stale_results += 1,
        }
    }
}

#[must_use]
pub fn image_resource_key(handle: nara_asset::Handle<ImageAsset>) -> RenderResourceKey {
    RenderResourceKey::for_asset(handle, RenderResourceKind::IMAGE_2D)
}

#[must_use]
pub fn image_descriptor_hash(image: &ImageAsset) -> ImportArtifactDigest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&image.source().artifact().key().digest().as_bytes());
    bytes.extend_from_slice(&image.source().source_hash().as_bytes());
    bytes.extend_from_slice(&image.extent().width.to_le_bytes());
    bytes.extend_from_slice(&image.extent().height.to_le_bytes());
    bytes.push(image_format_tag(image.format()));
    bytes.push(image_color_space_tag(image.color_space()));
    bytes.push(filter_mode_tag(image.sampler().min_filter));
    bytes.push(filter_mode_tag(image.sampler().mag_filter));
    bytes.push(filter_mode_tag(image.sampler().mipmap_filter));
    bytes.push(address_mode_tag(image.sampler().address_mode_u));
    bytes.push(address_mode_tag(image.sampler().address_mode_v));
    bytes.extend_from_slice(&(image.pixels().len() as u64).to_le_bytes());
    ImportArtifactDigest::from_bytes(bytes)
}

fn image_format_tag(format: ImageFormat) -> u8 {
    match format {
        ImageFormat::Rgba8 => 1,
    }
}

fn image_color_space_tag(color_space: ImageColorSpace) -> u8 {
    match color_space {
        ImageColorSpace::Srgb => 1,
        ImageColorSpace::Linear => 2,
    }
}

fn filter_mode_tag(filter_mode: ImageFilterMode) -> u8 {
    match filter_mode {
        ImageFilterMode::Nearest => 1,
        ImageFilterMode::Linear => 2,
    }
}

fn address_mode_tag(address_mode: ImageAddressMode) -> u8 {
    match address_mode {
        ImageAddressMode::ClampToEdge => 1,
        ImageAddressMode::Repeat => 2,
        ImageAddressMode::MirrorRepeat => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;
    use nara_app::App;
    use nara_asset::{
        AssetEvents, AssetRecord, AssetServer, AssetSourceKind, AssetStates, Assets,
        ImportDependencyDigest, ImportProfile, ImportSettingsHash, ImporterRegistry,
    };

    fn stable_id() -> StableAssetId {
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
    }

    fn image_record(path: &str) -> AssetRecord {
        AssetRecord::new(
            stable_id(),
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
        )
    }

    fn request<'a>(record: &'a AssetRecord, source_bytes: &'a [u8]) -> ImportRequest<'a> {
        ImportRequest::new(
            record,
            source_bytes,
            ImportDependencyDigest::empty(),
            ImportSettingsHash::default(),
            ImportProfile::default(),
        )
    }

    fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        encoder
            .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        bytes
    }

    #[test]
    fn png_import_produces_rgba8_image_asset() {
        let record = image_record("textures/player.png");
        let bytes = rgba_png(
            2,
            1,
            &[
                255, 0, 0, 255, //
                0, 255, 0, 128,
            ],
        );

        let imported = ImageImporter::default()
            .import_image(request(&record, &bytes))
            .unwrap();

        assert_eq!(imported.image().extent(), ImageExtent::new(2, 1));
        assert_eq!(imported.image().format(), ImageFormat::Rgba8);
        assert_eq!(imported.image().color_space(), ImageColorSpace::Srgb);
        assert_eq!(imported.image().pixels().len(), 8);
        assert_eq!(imported.image().source().stable_id(), stable_id());
        assert_eq!(
            imported.image().source().path().as_str(),
            "textures/player.png"
        );
        assert!(
            imported
                .artifact()
                .artifact_path()
                .as_str()
                .starts_with(".nara/import-cache/nara_image_png/default/nara_image.image/")
        );
    }

    #[test]
    fn unsupported_image_extension_returns_import_diagnostic() {
        let record = image_record("textures/player.jpg");
        let error = ImageImporter::default()
            .import_image(request(&record, b"not a png"))
            .unwrap_err();

        assert!(matches!(
            error,
            ImageImportError::UnsupportedFormat { extension } if extension.as_str() == "jpg"
        ));
    }

    #[test]
    fn importer_registry_selects_png_importer_by_extension() {
        let mut registry = ImporterRegistry::new();
        registry.register(ImageImporter::default()).unwrap();
        let record = image_record("textures/player.png");
        let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);

        let artifact = registry.import(request(&record, &bytes)).unwrap();

        assert_eq!(
            artifact.key().output_asset_type().as_str(),
            "nara_image.image"
        );
    }

    #[test]
    fn imported_image_stores_under_reserved_handle() {
        let record = image_record("textures/player.png");
        let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
        let imported = ImageImporter::default()
            .import_image(request(&record, &bytes))
            .unwrap();
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let mut images = Assets::<ImageAsset>::default();

        assert_eq!(images.insert(handle, imported.into_image()), None);

        let stored = images.get(handle).unwrap();
        assert_eq!(stored.extent(), ImageExtent::new(1, 1));
        assert_eq!(server.stable_id(handle.id()), Some(stable_id()));
    }

    #[test]
    fn prepare_system_writes_backend_neutral_image_resource() {
        let record = image_record("textures/player.png");
        let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
        let imported = ImageImporter::default()
            .import_image(request(&record, &bytes))
            .unwrap();
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let mut images = Assets::<ImageAsset>::default();
        let mut states = AssetStates::default();
        let mut asset_events = AssetEvents::default();
        images
            .commit_loaded(
                handle,
                imported.into_image(),
                &mut states,
                &mut asset_events,
                None,
                None,
            )
            .unwrap();
        let mut app = App::new();
        app.add_plugin(nara_render::RenderPlugin).unwrap();
        app.add_plugin(ImagePreparePlugin).unwrap();
        app.world_mut().insert_resource(images);
        app.world_mut().insert_resource(states);

        app.update();

        let prepared = app
            .world()
            .resource::<PreparedRenderResources<PreparedImageResource>>();
        let resource = prepared.get_ready(image_resource_key(handle)).unwrap();
        assert_eq!(resource.extent(), ImageExtent::new(1, 1));
        assert_eq!(resource.pixel_len(), 4);
        assert_eq!(app.world().resource::<ImagePrepareStats>().prepared, 1);
    }

    #[test]
    fn descriptor_hash_changes_when_sampler_changes() {
        let record = image_record("textures/player.png");
        let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
        let image = ImageImporter::default()
            .import_image(request(&record, &bytes))
            .unwrap()
            .into_image();
        let mut changed = image.clone();
        changed.sampler = ImageSamplerDescriptor {
            min_filter: ImageFilterMode::Nearest,
            ..changed.sampler()
        };

        assert_ne!(
            image_descriptor_hash(&image),
            image_descriptor_hash(&changed)
        );
    }
}
